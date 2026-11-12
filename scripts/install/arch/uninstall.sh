#!/usr/bin/env bash
# Remove Keyboard Control installed by install.sh.
#
#   ./scripts/install/arch/uninstall.sh            remove from /usr/local, plus the udev rule
#   ./scripts/install/arch/uninstall.sh --user     remove from ~/.local, plus the udev rule
#   add --purge to also delete your saved profiles (~/.config/k708-control)
#   add --keep-udev to leave the udev rule in place
#   PREFIX=/usr ./scripts/install/arch/uninstall.sh
set -euo pipefail

APP=k708-control
UDEV_RULE=/etc/udev/rules.d/70-redragon.rules
PURGE=0
KEEP_UDEV=0
for arg in "$@"; do
    case "$arg" in
        --user) PREFIX="${PREFIX:-$HOME/.local}" ;;
        --purge) PURGE=1 ;;
        --keep-udev) KEEP_UDEV=1 ;;
        *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
done
PREFIX="${PREFIX:-/usr/local}"

say() { printf '\033[1;31m==>\033[0m %s\n' "$*"; }

if [[ $EUID -eq 0 ]]; then
    echo "error: run this as your normal user (it asks for sudo when needed)" >&2
    exit 1
fi
if [[ -w "$PREFIX" ]]; then FILE_SUDO=""; else FILE_SUDO=sudo; fi

files=(
    "$PREFIX/bin/$APP"
    "$PREFIX/share/applications/$APP.desktop"
    "$PREFIX/share/icons/hicolor/scalable/apps/$APP.svg"
)
for f in "${files[@]}"; do
    if [[ -e "$f" ]]; then
        say "Removing $f"
        $FILE_SUDO rm -f -- "$f"
    fi
done
command -v update-desktop-database >/dev/null && $FILE_SUDO update-desktop-database -q "$PREFIX/share/applications" || true
if [[ -f "$PREFIX/share/icons/hicolor/index.theme" ]] && command -v gtk-update-icon-cache >/dev/null; then
    $FILE_SUDO gtk-update-icon-cache -q -t "$PREFIX/share/icons/hicolor" 2>/dev/null || true
fi

if ((KEEP_UDEV == 0)) && [[ -e "$UDEV_RULE" ]]; then
    say "Removing udev rule $UDEV_RULE"
    sudo rm -f -- "$UDEV_RULE"
    sudo udevadm control --reload || true
fi

config="${XDG_CONFIG_HOME:-$HOME/.config}/k708-control"
if ((PURGE)); then
    if [[ -d "$config" ]]; then
        say "Removing saved settings $config"
        rm -rf -- "$config"
    fi
elif [[ -d "$config" ]]; then
    say "Kept your saved settings in $config (use --purge to delete them)"
fi

say "Uninstalled."
