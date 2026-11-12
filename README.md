# Keyboard Control for Redragon K708-RGB-PRO (Linux)

A Linux desktop app (Rust + egui) to control the Redragon K708-RGB-PRO keyboard:
lighting effects and per-key colours, key remapping, macros, polling rate, Windows-key lock,
key debounce, profiles, and the screen clock (12/24 hour, sync with system time, custom time).

The USB protocol was worked out from the vendor's Windows driver; the details, with the
function addresses they come from, are in [MECHANICS.md](MECHANICS.md).

## Install (Arch Linux)

```sh
./scripts/install/arch/install.sh          # system-wide, into /usr/local (asks for sudo)
./scripts/install/arch/install.sh --user   # just for you, into ~/.local
```

Builds a release binary, installs `k708-control` with a menu entry and icon, and adds a udev
rule so your user can talk to the keyboard (USB ids `320f:511b` cable, `320f:511c` 2.4G
receiver; skipped when the same rule is already installed). Unplug and replug the keyboard
once afterwards.

Uninstall (use the same `--user` flag you installed with):

```sh
./scripts/install/arch/uninstall.sh                  # keep saved profiles
./scripts/install/arch/uninstall.sh --purge          # also delete ~/.config/k708-control
./scripts/install/arch/uninstall.sh --keep-udev      # leave the udev rule in place
```

Use `PREFIX=/usr` with both scripts to install under `/usr` instead.

## Build and run from source

```sh
cargo build --release
./target/release/k708-control
```

Other options:

- `--page lighting|keys|macros|settings|screen|log` opens a page directly
- `--screenshot file.png` draws 120 frames, saves a picture and exits
- `--test-clock` sets the keyboard clock to the system time and reads it back (no window)

## Notes

- Not affiliated with Redragon. No vendor images or files are included.
- 12-hour clock mode is experimental: the vendor app only ever sends 24-hour time.
