#!/bin/bash
# CopieP — compile et installe l'app sur ce Mac (puce Apple M).
# Double-cliquez ce fichier. Si macOS le bloque : clic droit → Ouvrir.
#
# Première fois : 20 à 30 min (installation des outils et de Qt).
# Ensuite : 1 à 2 min (seuls les fichiers modifiés sont recompilés).

set -euo pipefail
cd "$(dirname "$0")"
SRC="$PWD"
BUILD="$HOME/Library/Caches/copiep-build"
STAGE="$BUILD/install"
APP_NAME="CopyQ.app"

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

# --- 1. Outils Apple ---------------------------------------------------------
step "Vérification des outils Apple"
if ! xcode-select -p >/dev/null 2>&1; then
    xcode-select --install || true
    pause_exit "Une fenêtre d'installation des outils Apple vient de s'ouvrir.
Terminez-la, puis double-cliquez à nouveau ce fichier."
fi

# --- 2. Homebrew ---------------------------------------------------------------
step "Vérification de Homebrew"
if ! command -v brew >/dev/null 2>&1 && [ -x /opt/homebrew/bin/brew ]; then
    eval "$(/opt/homebrew/bin/brew shellenv)"
fi
if ! command -v brew >/dev/null 2>&1; then
    echo "Installation de Homebrew (votre mot de passe de session va être demandé)…"
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
    eval "$(/opt/homebrew/bin/brew shellenv)"
    grep -q 'brew shellenv' "$HOME/.zprofile" 2>/dev/null \
        || echo 'eval "$(/opt/homebrew/bin/brew shellenv)"' >> "$HOME/.zprofile"
fi

# --- 3. Dépendances ------------------------------------------------------------
step "Installation de Qt, CMake et Ninja (long la première fois)"
for pkg in qt cmake ninja; do
    brew list "$pkg" >/dev/null 2>&1 || brew install "$pkg"
done
QT_PREFIX="$(brew --prefix qt)"

# --- 4. Mise à jour du code (si le dossier vient de git) -------------------------
if [ -d "$SRC/.git" ] && command -v git >/dev/null 2>&1; then
    step "Récupération de la dernière version du code"
    git -C "$SRC" pull --ff-only || echo "(mise à jour ignorée, on compile la version présente)"
fi

# --- 5. Compilation --------------------------------------------------------------
step "Compilation de CopieP"
cmake -S "$SRC" -B "$BUILD" -G Ninja \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_PREFIX_PATH="$QT_PREFIX" \
    -DCMAKE_OSX_ARCHITECTURES=arm64 \
    -DCMAKE_OSX_DEPLOYMENT_TARGET=13 \
    -DCMAKE_INSTALL_PREFIX="$STAGE" \
    -DCMAKE_CXX_FLAGS="-Wno-deprecated-declarations" \
    -DWITH_TESTS=OFF \
    -DWITH_QCA_ENCRYPTION=OFF \
    -DWITH_KEYCHAIN=OFF \
    -DWITH_AUDIO=OFF \
    -DWITH_NATIVE_NOTIFICATIONS=OFF \
    >/dev/null
cmake --build "$BUILD"

step "Préparation de l'application"
rm -rf "$STAGE"
cmake --install "$BUILD" >/dev/null

APP="$(/usr/bin/find "$STAGE" -maxdepth 2 -name "$APP_NAME" -type d | head -1)"
[ -n "$APP" ] || pause_exit "Application introuvable après la compilation."

xattr -cr "$APP"
codesign --force --deep --sign - "$APP" >/dev/null 2>&1

# --- 6. Installation -------------------------------------------------------------
step "Installation dans Applications"
osascript -e 'tell application "CopyQ" to quit' >/dev/null 2>&1 || true
pkill -x CopyQ >/dev/null 2>&1 || true
sleep 1
rm -rf "/Applications/$APP_NAME"
cp -R "$APP" "/Applications/$APP_NAME"
open "/Applications/$APP_NAME"

trap - ERR
pause_exit "✅ CopieP est installé et lancé.

Après chaque installation, macOS peut retirer l'autorisation d'Accessibilité :
si Option + V ou le collage automatique ne marchent plus, allez dans
Réglages Système → Confidentialité et sécurité → Accessibilité,
retirez CopyQ avec « – », puis ajoutez-le à nouveau." 0
