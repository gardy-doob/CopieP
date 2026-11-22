//! CopieP : historique du presse-papiers pour Mac, façon Windows 11.

mod platform;
mod store;

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use store::{Content, ItemView, Store};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, State, WebviewWindow, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

/// Délai avant d'effacer du presse-papiers un élément protégé collé.
const PROTECTED_CLEAR_DELAY: Duration = Duration::from_secs(90);
const POLL_INTERVAL: Duration = Duration::from_millis(350);
/// Argument passé au lancement automatique (pas de panneau à l'ouverture).
const AUTOSTART_ARG: &str = "--au-demarrage";

struct AppState {
    store: Mutex<Store>,
    /// Éléments protégés visibles (jusqu'à la fermeture du panneau).
    unlocked: AtomicBool,
    /// Touch ID en cours : ne pas fermer le panneau quand il perd le focus.
    authenticating: AtomicBool,
    /// Application où coller (au premier plan à l'ouverture du panneau).
    previous_app: Mutex<Option<i32>>,
    /// Ce que CopieP vient d'écrire lui-même (à ne pas réenregistrer).
    ignore_change: AtomicI64,
    ignore_text: Mutex<Option<String>>,
    /// Dernière fermeture du panneau (pour que le clic sur l'icône de la
    /// barre de menus le ferme au lieu de le rouvrir aussitôt).
    last_hidden: Mutex<Option<Instant>>,
}

const KEYCHAIN_REFUSED: &str =
    "Accès au Trousseau refusé : quittez puis relancez CopieP et autorisez l’accès pour utiliser les éléments protégés.";

/// Erreur si la clé des éléments protégés n'a pas pu être lue.
fn check_key(app: &AppHandle) -> Result<(), String> {
    if state(app).store.lock().unwrap().key_available() {
        Ok(())
    } else {
        Err(KEYCHAIN_REFUSED.into())
    }
}

fn state(app: &AppHandle) -> State<'_, AppState> {
    app.state::<AppState>()
}

fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("main")
}

fn notify_changed(app: &AppHandle) {
    let _ = app.emit("history-changed", ());
}

// ---------------------------------------------------------------------------
// Surveillance du presse-papiers

fn start_monitor(app: AppHandle) {
    thread::spawn(move || {
        let mut clipboard = None;
        let mut last_change = platform::clipboard_change_count();
        let mut last_text: Option<String> = None;
        let mut last_image_sig: Option<(usize, usize, u64)> = None;

        loop {
            thread::sleep(POLL_INTERVAL);

            // Sur Mac : on ne lit le presse-papiers que s'il a changé.
            let change = platform::clipboard_change_count();
            if change.is_some() {
                if change == last_change {
                    continue;
                }
                last_change = change;
                let st = state(&app);
                if change == Some(st.ignore_change.load(Ordering::SeqCst)) {
                    // Écrit par CopieP : ne pas garder le texte en mémoire.
                    *st.ignore_text.lock().unwrap() = None;
                    continue;
                }
                if platform::clipboard_is_concealed() {
                    continue;
                }
            }

            if clipboard.is_none() {
                clipboard = arboard::Clipboard::new().ok();
            }
            let Some(cb) = clipboard.as_mut() else { continue };

            let changed = if let Ok(text) = cb.get_text() {
                if change.is_none() && last_text.as_deref() == Some(text.as_str()) {
                    false
                } else {
                    last_text = Some(text.clone());
                    last_image_sig = None;
                    let st = state(&app);
                    let ignored = {
                        let mut ign = st.ignore_text.lock().unwrap();
                        if ign.as_deref() == Some(text.as_str()) {
                            *ign = None;
                            true
                        } else {
                            false
                        }
                    };
                    !ignored && st.store.lock().unwrap().add_text(text)
                }
            } else if let Ok(img) = cb.get_image() {
                let sig = (img.width, img.height, quick_sig(&img.bytes));
                if change.is_none() && last_image_sig == Some(sig) {
                    false
                } else {
                    last_image_sig = Some(sig);
                    last_text = None;
                    state(&app).store.lock().unwrap().add_image(img.width, img.height, &img.bytes)
                }
            } else {
                false
            };

            if changed {
                notify_changed(&app);
            }
        }
    });
}

/// Signature rapide d'une image (pour la détection hors Mac).
fn quick_sig(bytes: &[u8]) -> u64 {
    let step = (bytes.len() / 4096).max(1);
    bytes.iter().step_by(step).fold(bytes.len() as u64, |h, b| h.wrapping_mul(31).wrapping_add(*b as u64))
}

/// Met un contenu dans le presse-papiers sans qu'il soit réenregistré.
fn write_clipboard(app: &AppHandle, content: &Content, concealed: bool) -> Result<(), String> {
    let st = state(app);
    match content {
        Content::Text(text) => {
            *st.ignore_text.lock().unwrap() = Some(text.clone());
            let ok = if concealed {
                platform::write_text_concealed(text)
            } else {
                arboard::Clipboard::new().and_then(|mut c| c.set_text(text.clone())).is_ok()
            };
            if !ok {
                return Err("Impossible d'écrire dans le presse-papiers".into());
            }
        }
        Content::Image { width, height, rgba } => {
            let data = arboard::ImageData {
                width: *width,
                height: *height,
                bytes: std::borrow::Cow::Borrowed(rgba.as_slice()),
            };
            arboard::Clipboard::new()
                .and_then(|mut c| c.set_image(data))
                .map_err(|e| format!("Impossible d'écrire l'image : {e}"))?;
        }
    }
    if let Some(change) = platform::clipboard_change_count() {
        st.ignore_change.store(change, Ordering::SeqCst);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Panneau

enum Place {
    /// Près du pointeur (raccourci ⌥V).
    Cursor,
    /// Sous l'icône de la barre de menus (x au centre, y en bas de l'icône).
    Below(f64, f64),
}

fn place_window(app: &AppHandle, window: &WebviewWindow, place: Place) {
    let Ok(size) = window.outer_size() else { return };
    let (w, h) = (size.width as f64, size.height as f64);

    let (ax, ay) = match place {
        Place::Cursor => match app.cursor_position() {
            Ok(p) => (p.x, p.y),
            Err(_) => return,
        },
        Place::Below(x, y) => (x, y),
    };

    let monitor = app
        .monitor_from_point(ax, ay)
        .ok()
        .flatten()
        .or_else(|| window.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let mp = monitor.position();
    let ms = monitor.size();
    let (left, top) = (mp.x as f64, mp.y as f64);
    let (right, bottom) = (left + ms.width as f64, top + ms.height as f64);
    let margin = 8.0 * monitor.scale_factor();

    let mut x = match place {
        Place::Cursor => ax - 24.0 * monitor.scale_factor(),
        Place::Below(..) => ax - w / 2.0,
    };
    let mut y = ay + 10.0 * monitor.scale_factor();
    if y + h > bottom - margin {
        // Pas assez de place sous le pointeur : au-dessus.
        y = ay - h - 10.0 * monitor.scale_factor();
    }
    x = x.clamp(left + margin, (right - w - margin).max(left + margin));
    y = y.clamp(top + margin, (bottom - h - margin).max(top + margin));
    let _ = window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

fn show_panel(app: &AppHandle, place: Place) {
    let Some(window) = main_window(app) else { return };
    if window.is_visible().unwrap_or(false) {
        hide_panel(app, true);
        return;
    }
    let st = state(app);
    // Toujours verrouillé à l'ouverture.
    st.unlocked.store(false, Ordering::SeqCst);
    *st.previous_app.lock().unwrap() = platform::frontmost_app_pid();
    place_window(app, &window, place);

    #[cfg(target_os = "macos")]
    if let Ok(ns) = window.ns_window() {
        platform::setup_panel(ns);
    }

    let _ = window.show();
    let _ = window.set_focus();
    let _ = app.emit("panel-shown", ());
}

/// Ferme le panneau et reverrouille les éléments protégés.
fn hide_panel(app: &AppHandle, give_back_focus: bool) {
    let st = state(app);
    let was_unlocked = st.unlocked.swap(false, Ordering::SeqCst);
    if let Some(window) = main_window(app) {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
            *st.last_hidden.lock().unwrap() = Some(Instant::now());
            if give_back_focus {
                if let Some(pid) = *st.previous_app.lock().unwrap() {
                    platform::activate_app(pid);
                }
            }
        }
    }
    let _ = app.emit("panel-hidden", ());
    if was_unlocked {
        notify_changed(app);
    }
}

/// Demande Touch ID si les éléments protégés sont verrouillés.
fn ensure_unlocked(app: &AppHandle) -> bool {
    let st = state(app);
    if st.unlocked.load(Ordering::SeqCst) {
        return true;
    }
    if st.authenticating.swap(true, Ordering::SeqCst) {
        return false;
    }
    // macOS affiche « CopieP essaie de … ».
    let ok = platform::authenticate("déverrouiller vos éléments protégés");
    st.authenticating.store(false, Ordering::SeqCst);
    let visible = main_window(app).map(|w| w.is_visible().unwrap_or(false)).unwrap_or(false);
    // Si le panneau a été fermé pendant la demande, il reste verrouillé.
    if ok && visible {
        st.unlocked.store(true, Ordering::SeqCst);
        notify_changed(app);
    }
    if visible {
        if let Some(window) = main_window(app) {
            let _ = window.set_focus();
        }
    }
    ok
}

// ---------------------------------------------------------------------------
// Commandes appelées par l'interface

// (async : hors du fil principal, pour ne pas figer l'interface pendant
// qu'une grosse image est en cours d'enregistrement.)
#[tauri::command(async)]
fn list_items(app: AppHandle) -> Vec<ItemView> {
    let st = state(&app);
    let unlocked = st.unlocked.load(Ordering::SeqCst);
    let views = st.store.lock().unwrap().views(unlocked);
    views
}

#[tauri::command]
async fn unlock(app: AppHandle) -> Result<bool, String> {
    check_key(&app)?;
    Ok(ensure_unlocked(&app))
}

/// `mode` : "paste" (coller dans l'app précédente) ou "copy" (copier seulement).
#[tauri::command]
async fn activate_item(app: AppHandle, id: String, mode: String) -> Result<(), String> {
    let st = state(&app);
    let protected = st.store.lock().unwrap().is_protected(&id);
    if protected {
        check_key(&app)?;
        if !ensure_unlocked(&app) {
            return Err("Déverrouillage annulé".into());
        }
    }
    let content = st.store.lock().unwrap().content(&id).ok_or("Élément illisible")?;
    write_clipboard(&app, &content, protected)?;
    if !protected {
        st.store.lock().unwrap().touch(&id);
        notify_changed(&app);
    }

    let paste = mode == "paste";
    hide_panel(&app, true);

    if paste {
        thread::sleep(Duration::from_millis(180));
        if !platform::send_paste() {
            log::warn!("Collage impossible : autorisation d'Accessibilité manquante");
        }
    }

    if protected {
        // Comme un gestionnaire de mots de passe : on efface au bout de 90 s.
        let token = platform::clipboard_change_count();
        let secret = match &content {
            Content::Text(t) => Some(t.clone()),
            Content::Image { .. } => None,
        };
        thread::spawn(move || {
            thread::sleep(PROTECTED_CLEAR_DELAY);
            let unchanged = match token {
                Some(t) => platform::clipboard_change_count() == Some(t),
                None => secret.is_some()
                    && arboard::Clipboard::new().and_then(|mut c| c.get_text()).ok() == secret,
            };
            if unchanged {
                platform::clear_clipboard();
            }
        });
    }
    Ok(())
}

#[tauri::command]
async fn set_pinned(app: AppHandle, id: String, pinned: bool) -> Result<(), String> {
    let st = state(&app);
    let protected = st.store.lock().unwrap().is_protected(&id);
    if !pinned && protected {
        check_key(&app)?;
        if !ensure_unlocked(&app) {
            return Err("Déverrouillage annulé".into());
        }
    }
    let ok = st.store.lock().unwrap().set_pinned(&id, pinned);
    notify_changed(&app);
    if ok {
        Ok(())
    } else {
        Err("Impossible de désépingler cet élément".into())
    }
}

#[tauri::command]
async fn set_protected(app: AppHandle, id: String, protect: bool) -> Result<(), String> {
    check_key(&app)?;
    if !protect && !ensure_unlocked(&app) {
        return Err("Déverrouillage annulé".into());
    }
    let ok = state(&app).store.lock().unwrap().set_protected(&id, protect);
    notify_changed(&app);
    if ok {
        Ok(())
    } else {
        Err("Impossible de modifier la protection".into())
    }
}

#[tauri::command]
async fn delete_item(app: AppHandle, id: String) -> Result<(), String> {
    let st = state(&app);
    let protected = st.store.lock().unwrap().is_protected(&id);
    if protected && !ensure_unlocked(&app) {
        return Err("Déverrouillage annulé".into());
    }
    st.store.lock().unwrap().delete(&id);
    notify_changed(&app);
    Ok(())
}

#[tauri::command]
fn clear_history(app: AppHandle) {
    state(&app).store.lock().unwrap().clear_unpinned();
    notify_changed(&app);
}

#[tauri::command]
fn hide(app: AppHandle) {
    hide_panel(&app, true);
}

#[derive(Serialize)]
struct Status {
    accessibility: bool,
    unlocked: bool,
    mac: bool,
}

#[tauri::command]
fn status(app: AppHandle) -> Status {
    Status {
        accessibility: platform::accessibility_trusted(false),
        unlocked: state(&app).unlocked.load(Ordering::SeqCst),
        mac: cfg!(target_os = "macos"),
    }
}

#[tauri::command]
fn open_accessibility_settings() {
    platform::accessibility_trusted(true);
}

// ---------------------------------------------------------------------------
// Icône de la barre de menus

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Ouvrir CopieP (⌥V)", true, None::<&str>)?;
    let autostart_enabled = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart = CheckMenuItem::with_id(app, "autostart", "Ouvrir au démarrage du Mac", true, autostart_enabled, None::<&str>)?;
    let clear = MenuItem::with_id(app, "clear", "Vider l’historique (garder les épinglés)", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quitter CopieP", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &PredefinedMenuItem::separator(app)?,
            &autostart,
            &clear,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    let autostart_item = autostart.clone();

    TrayIconBuilder::with_id("main")
        .icon(icon)
        .icon_as_template(true)
        .tooltip("CopieP")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "open" => show_panel(app, Place::Cursor),
            "autostart" => {
                let launcher = app.autolaunch();
                let enabled = launcher.is_enabled().unwrap_or(false);
                let _ = if enabled { launcher.disable() } else { launcher.enable() };
                let _ = autostart_item.set_checked(launcher.is_enabled().unwrap_or(false));
            }
            "clear" => {
                state(app).store.lock().unwrap().clear_unpinned();
                notify_changed(app);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { rect, button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                let app = tray.app_handle();
                // Le clic vient de fermer le panneau (perte de focus) : ne
                // pas le rouvrir, pour que l'icône serve aussi à le fermer.
                let just_hidden = state(app)
                    .last_hidden
                    .lock()
                    .unwrap()
                    .map(|t| t.elapsed() < Duration::from_millis(400))
                    .unwrap_or(false);
                if just_hidden {
                    return;
                }
                let scale = app
                    .primary_monitor()
                    .ok()
                    .flatten()
                    .map(|m| m.scale_factor())
                    .unwrap_or(1.0);
                let pos = rect.position.to_physical::<f64>(scale);
                let size = rect.size.to_physical::<f64>(scale);
                show_panel(app, Place::Below(pos.x + size.width / 2.0, pos.y + size.height));
            }
        })
        .build(app)?;
    Ok(())
}

// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let shortcuts = [
        // ⌥V, comme Windows + V.
        Shortcut::new(Some(Modifiers::ALT), Code::KeyV),
        // ⌃⌥V : marche aussi dans les champs de mot de passe, où macOS
        // bloque les raccourcis qui n'utilisent que ⌥.
        Shortcut::new(Some(Modifiers::ALT | Modifiers::CONTROL), Code::KeyV),
    ];

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![AUTOSTART_ARG]),
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        show_panel(app, Place::Cursor);
                    }
                })
                .build(),
        )
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let dir = app.path().app_data_dir().expect("dossier de données introuvable");
            app.manage(AppState {
                store: Mutex::new(Store::open(dir)),
                unlocked: AtomicBool::new(false),
                authenticating: AtomicBool::new(false),
                previous_app: Mutex::new(None),
                ignore_change: AtomicI64::new(i64::MIN),
                ignore_text: Mutex::new(None),
                last_hidden: Mutex::new(None),
            });

            let handle = app.handle().clone();
            for shortcut in shortcuts {
                if let Err(e) = handle.global_shortcut().register(shortcut) {
                    log::warn!("Raccourci {shortcut:?} indisponible : {e}");
                }
            }

            #[cfg(target_os = "macos")]
            if let Some(window) = main_window(&handle) {
                if let Ok(ns) = window.ns_window() {
                    platform::setup_panel(ns);
                }
            }

            build_tray(&handle)?;
            start_monitor(handle.clone());

            // Ouvrir le panneau au premier lancement pour montrer que ça marche.
            let at_login = std::env::args().any(|a| a == AUTOSTART_ARG);
            if !at_login && std::env::var_os("COPIEP_HIDDEN").is_none() {
                show_panel(&handle, Place::Cursor);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::Focused(false) = event {
                let app = window.app_handle();
                let st = state(app);
                if !st.authenticating.load(Ordering::SeqCst) && std::env::var_os("COPIEP_KEEP_OPEN").is_none() {
                    hide_panel(app, false);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_items,
            unlock,
            activate_item,
            set_pinned,
            set_protected,
            delete_item,
            clear_history,
            hide,
            status,
            open_accessibility_settings
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de CopieP");
}
