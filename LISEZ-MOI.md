# CopieP

Historique du presse-papiers pour Mac (puce Apple), façon Windows 11, en français.
Application Tauri (Rust + HTML/CSS/JS).

L'ancienne version basée sur CopyQ (C++/Qt) est gardée dans la branche `copyq-qt`.

## Ce que fait CopieP

- **⌥V** ouvre le panneau près du pointeur, par-dessus n'importe quelle app, même en plein écran.
  **⇧⌘V** et **⌃⌥V** font pareil et marchent aussi dans les champs de mot de passe, où macOS
  bloque ⌥V (il y tape « ◊ »). En contrepartie, ⇧⌘V ne sert plus à « coller sans mise en forme »
  dans les apps qui l'utilisaient.
- **Clic sur l'icône** de la barre de menus : le panneau s'ouvre juste en dessous.
  Le menu (démarrage automatique, vider l'historique, quitter) est au clic droit.
- **Double-clic** ou **Entrée** : colle l'élément dans l'app où vous étiez.
- Au survol d'un élément :
  - 📌 **épingler** : l'élément reste en haut et n'est jamais effacé ;
  - **empreinte** : épingler **et protéger**. Le contenu est masqué et chiffré sur le disque.
    Touch ID (ou le mot de passe du Mac) est demandé pour l'afficher, le coller,
    le désépingler ou le supprimer. Il se reverrouille à la fermeture du panneau.
  - 🗑 **supprimer**.
- Un élément protégé collé est effacé du presse-papiers au bout de 90 secondes, s'il y est encore.
- Les mots de passe copiés depuis un gestionnaire de mots de passe (1Password, Bitwarden…)
  ne sont pas enregistrés, comme le veut la convention macOS.
- 200 éléments gardés, plus les épinglés. Texte et images.

### Raccourcis dans le panneau

| Touche | Action |
|---|---|
| ↑ ↓ | Choisir |
| Entrée / double-clic | Coller |
| ⌘C | Copier sans coller |
| ⌘1 … ⌘9 | Coller l'élément n° 1 à 9 |
| ⌘P | Épingler |
| ⌘L | Épingler et protéger |
| ⌘⌫ | Supprimer |
| Taper du texte | Rechercher |
| Échap | Fermer |

## Installer ou mettre à jour

1. Récupérez le code : `git clone https://github.com/gardy-doob/CopieP.git`
   (ou bouton vert **Code → Download ZIP** sur GitHub, puis décompressez).
2. Double-cliquez **`INSTALLER-SUR-CE-MAC.command`**. Si macOS bloque : clic droit → Ouvrir.

Le script installe ce qui manque (outils Apple, Rust, outil Tauri), compile, remplace l'app
dans Applications et la lance. Première fois : 10 à 20 min. Ensuite : environ 1 min.
Avec un dossier cloné par git, il récupère aussi la dernière version du code.

**Après chaque installation**, réactivez CopieP dans Réglages Système → Confidentialité et
sécurité → **Accessibilité** (bouton « Autoriser » dans le panneau). macOS retire
l'autorisation à chaque nouvelle compilation, car la signature de l'app change.

Après une mise à jour, macOS peut aussi demander le mot de passe de session pour laisser
CopieP lire sa clé dans le **Trousseau** : cliquez « Toujours autoriser ». Si vous refusez,
rien n'est perdu, mais les éléments protégés restent inaccessibles jusqu'au prochain
lancement où vous autorisez.

## Où sont les données

- Historique : `~/Library/Application Support/fr.copiep.app/`
- Éléments protégés : chiffrés (AES-256-GCM) dans le sous-dossier `protected/`.
  La clé est dans le **Trousseau** macOS (« fr.copiep.app »).

## Développement

```
cargo install tauri-cli --version "^2" --locked   # une fois
cargo tauri dev
```

- `src/` : interface (HTML, CSS, JS, sans étape de compilation)
- `src-tauri/src/lib.rs` : panneau, barre de menus, raccourcis, collage
- `src-tauri/src/store.rs` : historique et chiffrement (tests : `cargo test`)
- `src-tauri/src/platform/mac.rs` : Touch ID, presse-papiers, ⌘V, fenêtre par-dessus le plein écran
