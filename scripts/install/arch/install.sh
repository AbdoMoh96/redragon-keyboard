#!/usr/bin/env bash
# Install Keyboard Control (Redragon K708-RGB-PRO) on Arch Linux.
#
#   ./scripts/install/arch/install.sh            build and install to /usr/local
#   ./scripts/install/arch/install.sh --user     install to ~/.local (no sudo for the app files)
#   PREFIX=/usr ./scripts/install/arch/install.sh
#
# Builds as the current user and uses sudo only where it is needed: for system folders and
# for the udev rule that lets your user talk to the keyboard (skipped when already in place).
set -euo pipefail

APP=k708-control
NAME="Keyboard Control"
if [[ "${1:-}" == "--user" ]]; then
    PREFIX="${PREFIX:-$HOME/.local}"
fi
PREFIX="${PREFIX:-/usr/local}"
UDEV_RULE=/etc/udev/rules.d/70-redragon.rules
# USB ids from the original driver's device table (sub_402a10): cable and 2.4G receiver.
USB_IDS=("320f:511b" "320f:511c")

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"

say() { printf '\033[1;31m==>\033[0m %s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

if [[ $EUID -eq 0 ]]; then
    die "run this as your normal user (it asks for sudo when needed)"
fi
command -v pacman >/dev/null || die "this script is for Arch Linux (pacman not found)"

# sudo only when the target folder isn't writable by you.
mkdir -p "$PREFIX" 2>/dev/null || true
if [[ -w "$PREFIX" ]]; then
    FILE_SUDO=""
else
    command -v sudo >/dev/null || die "sudo is needed to install into $PREFIX (or use --user)"
    FILE_SUDO=sudo
fi

say "Checking build tools"
missing=()
command -v cargo >/dev/null || missing+=(rust)
command -v cc >/dev/null || missing+=(gcc)
pkg-config --version >/dev/null 2>&1 || missing+=(pkgconf)
if ((${#missing[@]})); then
    say "Installing: ${missing[*]}"
    sudo pacman -S --needed --noconfirm "${missing[@]}"
fi

say "Building (release)"
cargo build --release --manifest-path "$REPO/Cargo.toml"
BIN="$REPO/target/release/$APP"
[[ -x "$BIN" ]] || die "build did not produce $BIN"

say "Installing to $PREFIX"
$FILE_SUDO install -Dm755 "$BIN" "$PREFIX/bin/$APP"
$FILE_SUDO install -Dm644 "$REPO/assets/$APP.svg" "$PREFIX/share/icons/hicolor/scalable/apps/$APP.svg"
tmp_desktop="$(mktemp)"
cat >"$tmp_desktop" <<EOF
[Desktop Entry]
Type=Application
Name=$NAME
GenericName=Keyboard settings
Comment=Lighting, keys, macros and screen clock for Redragon keyboards
Exec=$PREFIX/bin/$APP
Icon=$APP
Terminal=false
Categories=Settings;HardwareSettings;
Keywords=redragon;keyboard;rgb;k708;
EOF
$FILE_SUDO install -Dm644 "$tmp_desktop" "$PREFIX/share/applications/$APP.desktop"
rm -f "$tmp_desktop"

tmp_rule="$(mktemp)"
for id in "${USB_IDS[@]}"; do
    printf 'SUBSYSTEM=="hidraw", ATTRS{idVendor}=="%s", ATTRS{idProduct}=="%s", MODE="0660", TAG+="uaccess"\n' \
        "${id%%:*}" "${id##*:}" >>"$tmp_rule"
done
if [[ -f "$UDEV_RULE" ]] && cmp -s "$tmp_rule" "$UDEV_RULE"; then
    say "udev rule $UDEV_RULE already in place"
else
    say "Adding udev rule $UDEV_RULE"
    sudo install -Dm644 "$tmp_rule" "$UDEV_RULE"
    sudo udevadm control --reload
    sudo udevadm trigger --subsystem-match=hidraw
fi
rm -f "$tmp_rule"

command -v update-desktop-database >/dev/null && $FILE_SUDO update-desktop-database -q "$PREFIX/share/applications" || true
if [[ -f "$PREFIX/share/icons/hicolor/index.theme" ]] && command -v gtk-update-icon-cache >/dev/null; then
    $FILE_SUDO gtk-update-icon-cache -q -t "$PREFIX/share/icons/hicolor" 2>/dev/null || true
fi

say "Done. Start \"$NAME\" from your app menu or run: $PREFIX/bin/$APP"
say "If the keyboard was already plugged in, unplug and replug it once."
