//! Historique du presse-papiers : stockage sur disque et chiffrement des
//! éléments protégés.
//!
//! - Les éléments ordinaires sont dans `history.json` (texte) et `images/`.
//! - Les éléments protégés sont chiffrés (AES-256-GCM) dans `protected/`.
//!   La clé est rangée dans le Trousseau macOS : le contenu ne quitte jamais
//!   le disque en clair.

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Nombre maximal d'éléments non épinglés gardés dans l'historique.
pub const MAX_HISTORY: usize = 200;
/// Taille maximale d'un texte enregistré (au-delà, il est ignoré).
const MAX_TEXT_BYTES: usize = 2 * 1024 * 1024;
/// Taille maximale d'une image (en pixels).
const MAX_IMAGE_PIXELS: usize = 40_000_000;
/// Largeur maximale des miniatures.
const THUMB_SIZE: u32 = 320;
/// Longueur de l'aperçu de texte envoyé à l'interface.
const PREVIEW_CHARS: usize = 600;

const KEYCHAIN_SERVICE: &str = "fr.copiep.app";
const KEYCHAIN_ACCOUNT: &str = "cle-elements-proteges";

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Text,
    Image,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Item {
    pub id: String,
    pub kind: Kind,
    /// Texte en clair (absent pour les images et les éléments protégés).
    #[serde(default)]
    pub text: Option<String>,
    /// Miniature PNG en base64 (absente pour les éléments protégés).
    #[serde(default)]
    pub thumb: Option<String>,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    /// Empreinte à clé du contenu (sert à éviter les doublons).
    pub hash: String,
    pub created: u64,
    pub last_used: u64,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub pinned_at: u64,
    #[serde(default)]
    pub protected: bool,
    /// Nombre de caractères (texte) : affiché aussi pour les éléments protégés.
    #[serde(default)]
    pub chars: usize,
}

/// Ce que l'interface reçoit pour chaque élément.
#[derive(Serialize, Clone)]
pub struct ItemView {
    pub id: String,
    pub kind: Kind,
    pub preview: Option<String>,
    pub thumb: Option<String>,
    pub width: u32,
    pub height: u32,
    pub pinned: bool,
    pub protected: bool,
    pub revealed: bool,
    pub time: u64,
    pub chars: usize,
}

/// Contenu réel d'un élément (pour le remettre dans le presse-papiers).
pub enum Content {
    Text(String),
    /// Image RGBA.
    Image { width: usize, height: usize, rgba: Vec<u8> },
}

#[derive(Serialize, Deserialize, Default)]
struct HistoryFile {
    version: u32,
    items: Vec<Item>,
}

pub struct Store {
    dir: PathBuf,
    key: [u8; 32],
    items: Vec<Item>,
    counter: u64,
}

impl Store {
    pub fn open(dir: PathBuf) -> Store {
        let _ = fs::create_dir_all(dir.join("images"));
        let _ = fs::create_dir_all(dir.join("protected"));
        let key = load_or_create_key(&dir);
        let items = fs::read(dir.join("history.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<HistoryFile>(&b).ok())
            .map(|h| h.items)
            .unwrap_or_default();
        let mut store = Store { dir, key, items, counter: 0 };
        store.sort();
        store
    }

    fn save(&self) {
        let file = HistoryFile { version: 1, items: self.items.clone() };
        if let Ok(json) = serde_json::to_vec(&file) {
            let tmp = self.dir.join("history.json.tmp");
            if fs::write(&tmp, json).is_ok() {
                let _ = fs::rename(&tmp, self.dir.join("history.json"));
            }
        }
    }

    fn new_id(&mut self) -> String {
        self.counter += 1;
        format!("{:x}{:04x}", now_ms(), self.counter % 0xffff)
    }

    /// Empreinte à clé : sans la clé du Trousseau, impossible de deviner
    /// un mot de passe protégé à partir de son empreinte.
    fn keyed_hash(&self, kind: Kind, bytes: &[u8]) -> String {
        let mut h = Sha256::new();
        h.update(self.key);
        h.update([kind as u8]);
        h.update(bytes);
        let digest = h.finalize();
        digest[..16].iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Épinglés d'abord (le plus récemment épinglé en haut), puis le plus récent.
    fn sort(&mut self) {
        self.items.sort_by(|a, b| match (a.pinned, b.pinned) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (true, true) => b.pinned_at.cmp(&a.pinned_at),
            (false, false) => b.last_used.cmp(&a.last_used),
        });
    }

    fn trim(&mut self) {
        let mut unpinned = 0;
        let mut removed = Vec::new();
        self.items.retain(|it| {
            if it.pinned {
                return true;
            }
            unpinned += 1;
            if unpinned > MAX_HISTORY {
                removed.push(it.id.clone());
                false
            } else {
                true
            }
        });
        for id in removed {
            self.remove_files(&id);
        }
    }

    fn remove_files(&self, id: &str) {
        let _ = fs::remove_file(self.image_path(id));
        let _ = fs::remove_file(self.protected_path(id));
    }

    fn image_path(&self, id: &str) -> PathBuf {
        self.dir.join("images").join(format!("{id}.png"))
    }

    fn protected_path(&self, id: &str) -> PathBuf {
        self.dir.join("protected").join(format!("{id}.bin"))
    }

    fn find(&self, id: &str) -> Option<usize> {
        self.items.iter().position(|it| it.id == id)
    }

    pub fn is_protected(&self, id: &str) -> bool {
        self.find(id).map(|i| self.items[i].protected).unwrap_or(false)
    }

    /// Remonte un élément déjà connu ; renvoie vrai s'il existait.
    fn bump_existing(&mut self, hash: &str) -> bool {
        if let Some(i) = self.items.iter().position(|it| it.hash == hash) {
            self.items[i].last_used = now_ms();
            self.sort();
            self.save();
            return true;
        }
        false
    }

    /// Ajoute un texte copié. Renvoie vrai si l'historique a changé.
    pub fn add_text(&mut self, text: String) -> bool {
        if text.trim().is_empty() || text.len() > MAX_TEXT_BYTES {
            return false;
        }
        let hash = self.keyed_hash(Kind::Text, text.as_bytes());
        if self.bump_existing(&hash) {
            return true;
        }
        let id = self.new_id();
        let t = now_ms();
        let chars = text.chars().count();
        self.items.push(Item {
            id,
            kind: Kind::Text,
            text: Some(text),
            thumb: None,
            width: 0,
            height: 0,
            hash,
            created: t,
            last_used: t,
            pinned: false,
            pinned_at: 0,
            protected: false,
            chars,
        });
        self.sort();
        self.trim();
        self.save();
        true
    }

    /// Ajoute une image RGBA copiée. Renvoie vrai si l'historique a changé.
    pub fn add_image(&mut self, width: usize, height: usize, rgba: &[u8]) -> bool {
        if width == 0 || height == 0 || width * height > MAX_IMAGE_PIXELS {
            return false;
        }
        if rgba.len() != width * height * 4 {
            return false;
        }
        let hash = self.keyed_hash(Kind::Image, rgba);
        if self.bump_existing(&hash) {
            return true;
        }
        let Some(img) = image::RgbaImage::from_raw(width as u32, height as u32, rgba.to_vec()) else {
            return false;
        };
        let id = self.new_id();
        if img.save_with_format(self.image_path(&id), image::ImageFormat::Png).is_err() {
            return false;
        }
        let thumb = make_thumb(&img);
        let t = now_ms();
        self.items.push(Item {
            id,
            kind: Kind::Image,
            text: None,
            thumb,
            width: width as u32,
            height: height as u32,
            hash,
            created: t,
            last_used: t,
            pinned: false,
            pinned_at: 0,
            protected: false,
            chars: 0,
        });
        self.sort();
        self.trim();
        self.save();
        true
    }

    /// Liste pour l'interface. Le contenu des éléments protégés n'est
    /// envoyé que si la session est déverrouillée.
    pub fn views(&self, unlocked: bool) -> Vec<ItemView> {
        self.items
            .iter()
            .map(|it| {
                let (preview, thumb, revealed) = if it.protected {
                    if unlocked {
                        match self.decrypt_item(it) {
                            Some(Content::Text(t)) => (Some(preview_of(&t)), None, true),
                            Some(Content::Image { width, height, rgba }) => {
                                let thumb = image::RgbaImage::from_raw(width as u32, height as u32, rgba)
                                    .and_then(|img| make_thumb(&img));
                                (None, thumb, true)
                            }
                            None => (None, None, false),
                        }
                    } else {
                        (None, None, false)
                    }
                } else {
                    (it.text.as_deref().map(preview_of), it.thumb.clone(), true)
                };
                ItemView {
                    id: it.id.clone(),
                    kind: it.kind,
                    preview,
                    thumb,
                    width: it.width,
                    height: it.height,
                    pinned: it.pinned,
                    protected: it.protected,
                    revealed,
                    time: it.last_used,
                    chars: it.chars,
                }
            })
            .collect()
    }

    /// Contenu complet d'un élément (déchiffré si besoin).
    pub fn content(&self, id: &str) -> Option<Content> {
        let it = &self.items[self.find(id)?];
        if it.protected {
            return self.decrypt_item(it);
        }
        match it.kind {
            Kind::Text => it.text.clone().map(Content::Text),
            Kind::Image => {
                let img = image::open(self.image_path(&it.id)).ok()?.to_rgba8();
                let (w, h) = img.dimensions();
                Some(Content::Image { width: w as usize, height: h as usize, rgba: img.into_raw() })
            }
        }
    }

    /// Remonte un élément non épinglé en tête (après un collage).
    pub fn touch(&mut self, id: &str) {
        if let Some(i) = self.find(id) {
            self.items[i].last_used = now_ms();
            self.sort();
            self.save();
        }
    }

    pub fn set_pinned(&mut self, id: &str, pinned: bool) -> bool {
        let Some(i) = self.find(id) else { return false };
        let it = &mut self.items[i];
        it.pinned = pinned;
        if pinned {
            it.pinned_at = now_ms();
        } else {
            it.last_used = now_ms();
        }
        if !pinned && it.protected {
            // Désépingler retire aussi la protection.
            let _ = self.set_protected(id, false);
            return true;
        }
        self.sort();
        self.trim();
        self.save();
        true
    }

    /// Protéger chiffre le contenu (et épingle l'élément) ; retirer la
    /// protection le remet en clair.
    pub fn set_protected(&mut self, id: &str, protect: bool) -> bool {
        let Some(i) = self.find(id) else { return false };
        if self.items[i].protected == protect {
            return true;
        }
        if protect {
            let Some(content) = self.content(id) else { return false };
            let plain = encode_content(&content);
            let Some(blob) = encrypt(&self.key, &plain) else { return false };
            if fs::write(self.protected_path(id), blob).is_err() {
                return false;
            }
            let _ = fs::remove_file(self.image_path(id));
            let it = &mut self.items[i];
            it.text = None;
            it.thumb = None;
            it.protected = true;
            if !it.pinned {
                it.pinned = true;
                it.pinned_at = now_ms();
            }
        } else {
            let Some(content) = self.decrypt_item(&self.items[i]) else { return false };
            match content {
                Content::Text(t) => self.items[i].text = Some(t),
                Content::Image { width, height, rgba } => {
                    let Some(img) = image::RgbaImage::from_raw(width as u32, height as u32, rgba) else {
                        return false;
                    };
                    if img.save_with_format(self.image_path(id), image::ImageFormat::Png).is_err() {
                        return false;
                    }
                    self.items[i].thumb = make_thumb(&img);
                }
            }
            let _ = fs::remove_file(self.protected_path(id));
            self.items[i].protected = false;
        }
        self.sort();
        self.save();
        true
    }

    pub fn delete(&mut self, id: &str) -> bool {
        let Some(i) = self.find(id) else { return false };
        self.items.remove(i);
        self.remove_files(id);
        self.save();
        true
    }

    /// Vide l'historique en gardant les éléments épinglés.
    pub fn clear_unpinned(&mut self) {
        let removed: Vec<String> = self.items.iter().filter(|it| !it.pinned).map(|it| it.id.clone()).collect();
        self.items.retain(|it| it.pinned);
        for id in removed {
            self.remove_files(&id);
        }
        self.save();
    }

    fn decrypt_item(&self, it: &Item) -> Option<Content> {
        let blob = fs::read(self.protected_path(&it.id)).ok()?;
        let plain = decrypt(&self.key, &blob)?;
        decode_content(&plain)
    }
}

fn preview_of(text: &str) -> String {
    let mut s: String = text.chars().take(PREVIEW_CHARS).collect();
    if text.chars().count() > PREVIEW_CHARS {
        s.push('…');
    }
    s
}

fn make_thumb(img: &image::RgbaImage) -> Option<String> {
    let (w, h) = img.dimensions();
    let scale = (THUMB_SIZE as f32 / w.max(h) as f32).min(1.0);
    let tw = ((w as f32 * scale).round() as u32).max(1);
    let th = ((h as f32 * scale).round() as u32).max(1);
    let thumb = image::imageops::thumbnail(img, tw, th);
    let mut buf = Cursor::new(Vec::new());
    thumb.write_to(&mut buf, image::ImageFormat::Png).ok()?;
    Some(format!("data:image/png;base64,{}", B64.encode(buf.into_inner())))
}

/// Format interne : 1 octet de type, puis les données.
fn encode_content(c: &Content) -> Vec<u8> {
    match c {
        Content::Text(t) => {
            let mut v = vec![b'T'];
            v.extend_from_slice(t.as_bytes());
            v
        }
        Content::Image { width, height, rgba } => {
            let mut v = vec![b'I'];
            v.extend_from_slice(&(*width as u32).to_le_bytes());
            v.extend_from_slice(&(*height as u32).to_le_bytes());
            v.extend_from_slice(rgba);
            v
        }
    }
}

fn decode_content(b: &[u8]) -> Option<Content> {
    match b.first()? {
        b'T' => String::from_utf8(b[1..].to_vec()).ok().map(Content::Text),
        b'I' if b.len() >= 9 => {
            let width = u32::from_le_bytes(b[1..5].try_into().ok()?) as usize;
            let height = u32::from_le_bytes(b[5..9].try_into().ok()?) as usize;
            let rgba = b[9..].to_vec();
            (rgba.len() == width * height * 4).then_some(Content::Image { width, height, rgba })
        }
        _ => None,
    }
}

fn encrypt(key: &[u8; 32], plain: &[u8]) -> Option<Vec<u8>> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let mut out = nonce.to_vec();
    out.extend(cipher.encrypt(&nonce, plain).ok()?);
    Some(out)
}

fn decrypt(key: &[u8; 32], blob: &[u8]) -> Option<Vec<u8>> {
    if blob.len() < 12 {
        return None;
    }
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    cipher.decrypt(Nonce::from_slice(&blob[..12]), &blob[12..]).ok()
}

/// Clé de chiffrement : dans le Trousseau macOS. Si le Trousseau est
/// indisponible (ex. Linux sans service de clés), repli sur un fichier.
fn load_or_create_key(dir: &Path) -> [u8; 32] {
    let decode = |s: &str| -> Option<[u8; 32]> { B64.decode(s.trim()).ok()?.try_into().ok() };

    if let Ok(entry) = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT) {
        match entry.get_password() {
            Ok(s) => {
                if let Some(k) = decode(&s) {
                    return k;
                }
            }
            Err(keyring::Error::NoEntry) => {
                let mut k = [0u8; 32];
                rand::thread_rng().fill_bytes(&mut k);
                if entry.set_password(&B64.encode(k)).is_ok() && entry.get_password().ok().as_deref().and_then(decode) == Some(k) {
                    return k;
                }
            }
            Err(e) => log::warn!("Trousseau indisponible : {e}"),
        }
    }

    let path = dir.join("key.bin");
    if let Some(k) = fs::read_to_string(&path).ok().as_deref().and_then(decode) {
        return k;
    }
    let mut k = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut k);
    let _ = fs::write(&path, B64.encode(k));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
    }
    k
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> Store {
        let dir = std::env::temp_dir().join(format!("copiep-test-{}", now_ms() ^ rand::random::<u64>()));
        Store::open(dir)
    }

    #[test]
    fn protect_roundtrip_and_dedupe() {
        let mut s = temp_store();
        assert!(s.add_text("motdepasse-123".into()));
        assert!(s.add_text("autre".into()));
        let id = s.items.iter().find(|i| i.text.as_deref() == Some("motdepasse-123")).unwrap().id.clone();

        assert!(s.set_protected(&id, true));
        let it = &s.items[s.find(&id).unwrap()];
        assert!(it.protected && it.pinned && it.text.is_none());
        // Rien en clair sur le disque.
        let json = fs::read_to_string(s.dir.join("history.json")).unwrap();
        assert!(!json.contains("motdepasse-123"));
        let blob = fs::read(s.protected_path(&id)).unwrap();
        assert!(!String::from_utf8_lossy(&blob).contains("motdepasse-123"));

        // Verrouillé : pas d'aperçu.
        let v = s.views(false);
        assert!(v.iter().find(|x| x.id == id).unwrap().preview.is_none());
        // Déverrouillé : aperçu.
        let v = s.views(true);
        assert_eq!(v.iter().find(|x| x.id == id).unwrap().preview.as_deref(), Some("motdepasse-123"));

        // Recopier le même secret ne crée pas de doublon en clair.
        let n = s.items.len();
        assert!(s.add_text("motdepasse-123".into()));
        assert_eq!(s.items.len(), n);

        // Épinglés en tête.
        assert_eq!(s.items[0].id, id);

        assert!(s.set_protected(&id, false));
        assert!(matches!(s.content(&id), Some(Content::Text(t)) if t == "motdepasse-123"));
    }

    #[test]
    fn image_protect_roundtrip() {
        let mut s = temp_store();
        let rgba = vec![200u8; 4 * 4 * 4];
        assert!(s.add_image(4, 4, &rgba));
        let id = s.items[0].id.clone();
        assert!(s.items[0].thumb.is_some());
        assert!(s.set_protected(&id, true));
        assert!(!s.image_path(&id).exists());
        assert!(s.set_protected(&id, false));
        assert!(matches!(s.content(&id), Some(Content::Image { width: 4, height: 4, .. })));
    }

    #[test]
    fn trim_keeps_pinned() {
        let mut s = temp_store();
        s.add_text("garde".into());
        let id = s.items[0].id.clone();
        s.set_pinned(&id, true);
        for i in 0..(MAX_HISTORY + 10) {
            s.add_text(format!("x{i}"));
        }
        assert!(s.find(&id).is_some());
        assert_eq!(s.items.iter().filter(|i| !i.pinned).count(), MAX_HISTORY);
    }
}
