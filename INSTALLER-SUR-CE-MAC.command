#!/bin/bash
# CopieP — compile et installe l'app sur ce Mac (puce Apple M).
# Double-cliquez ce fichier. Si macOS le bloque : clic droit → Ouvrir.
#
# Première fois : 10 à 20 min (installation de Rust et de l'outil Tauri).
# Ensuite : environ 1 min.

set -euo pipefail
cd "$(dirname "$0")"
SRC="$PWD"
APP_NAME="CopieP.app"
BUNDLE_ID="fr.copiep.app"
# Cache gardé même si vous décompressez une nouvelle version ailleurs.
export CARGO_TARGET_DIR="$HOME/Library/Caches/copiep-build"

pause_exit() {
    echo
    echo "$1"
    echo
    read -r -p "Appuyez sur Entrée pour fermer…" _ || true
    exit "${2:-1}"
}
trap 'pause_exit "❌ Une étape a échoué (voir le message ci-dessus). Envoyez une capture à Claude."' ERR

step() { echo; echo "▶ $1"; }

[ "$(uname -m)" = "arm64" ] || pause_exit "Ce script est prévu pour les Mac à puce Apple (M1, M2, M3, M4…)."

# --- 1. Outils Apple -------------------------------------------------------------
step "Vérification des outils Apple"
if ! xcode-select -p >/dev/null 2>&1; then
    xcode-select --install || true
    pause_exit "Une fenêtre d'installation des outils Apple vient de s'ouvrir.
Terminez-la, puis double-cliquez à nouveau ce fichier."
fi

# --- 2. Dernière version du code (si le dossier vient de git) -------------------
# Fait en premier : si la mise à jour modifie ce script, on le relance pour
# exécuter la nouvelle version (bash lit le fichier au fur et à mesure).
if [ -z "${COPIEP_A_JOUR:-}" ] && [ -d "$SRC/.git" ]; then
    step "Récupération de la dernière version du code"
    git -C "$SRC" pull --ff-only || echo "(mise à jour ignorée, on compile la version présente)"
    COPIEP_A_JOUR=1 exec /bin/bash "$SRC/INSTALLER-SUR-CE-MAC.command"
fi

# --- 3. Rust -------------------------------------------------------------------------
step "Vérification de Rust"
[ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"
if ! command -v cargo >/dev/null 2>&1; then
    echo "Installation de Rust (quelques minutes)…"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    source "$HOME/.cargo/env"
fi

# --- 4. Outil Tauri ------------------------------------------------------------------
step "Vérification de l'outil Tauri"
if ! cargo tauri --version >/dev/null 2>&1; then
    echo "Installation de l'outil Tauri (5 à 10 min, une seule fois)…"
    cargo install tauri-cli --version "^2" --locked
fi

# --- 5. Compilation ------------------------------------------------------------------
step "Compilation de CopieP"
cargo tauri build --bundles app

APP="$CARGO_TARGET_DIR/release/bundle/macos/$APP_NAME"
[ -d "$APP" ] || pause_exit "Application introuvable après la compilation."

# --- 6. Installation -----------------------------------------------------------------
step "Installation dans Applications"
# L'ancien CopyQ utilise aussi ⌥V : on le ferme pour éviter le conflit.
osascript -e 'quit app "CopyQ"' >/dev/null 2>&1 || true
osascript -e 'quit app "CopieP"' >/dev/null 2>&1 || true
pkill -x CopieP >/dev/null 2>&1 || true
pkill -x copiep >/dev/null 2>&1 || true
sleep 1

rm -rf "/Applications/$APP_NAME"
cp -R "$APP" "/Applications/$APP_NAME"
xattr -cr "/Applications/$APP_NAME"

# Chaque compilation change la signature de l'app : macOS garde une ancienne
# autorisation d'Accessibilité qui ne marche plus. On la retire pour que
# l'app la redemande proprement.
tccutil reset Accessibility "$BUNDLE_ID" >/dev/null 2>&1 || true

open "/Applications/$APP_NAME"

trap - ERR
MSG="✅ CopieP est installé et lancé (icône dans la barre de menus, en haut).

Dernière étape : pour que le double-clic colle dans vos apps, cliquez
« Autoriser » dans le panneau, puis activez CopieP dans
Réglages Système → Confidentialité et sécurité → Accessibilité.
(À refaire après chaque mise à jour.)"
if [ -d "/Applications/CopyQ.app" ]; then
    MSG="$MSG

L'ancien CopyQ est encore dans Applications : vous pouvez le mettre à la
Corbeille (et le retirer des « Ouverture » dans Réglages Système → Général)."
fi
pause_exit "$MSG" 0
