# Redragon K708-RGB-PRO — mechanics sheet

Source: `Redragon K708-RGB-PRO.exe`, the program inside the NSIS installer
(`redragon_k708-rgb-pro.exe` → `app.7z`), plus its `DefaultData/*.json` and `Skin/*.xml`.
Addresses are virtual addresses in that program, read with `native_function_pseudocode`.
Values marked **estimated** were looked for in the code and not settled.

## Device table (sub_402a10, matched in sub_4643c0)

11 entries (count at 0x6ea5a4), stride 0x9c. Matched on vendor, product, interface
(`mi_`) and collection (`col`) of the Windows HID path.

| Vendor:Product | Interface / collection | Usage page / usage | Meaning |
|---|---|---|---|
| 0x320F:0x511B | 1 / 4 | 0xFF1C / 0x92 | K708 over the cable — settings channel |
| 0x320F:0x511B | 1 / 3 | 0x000C (consumer) | K708 media keys |
| 0x320F:0x511C | 1 / 4 | 0xFF1C / 0x92 | K708 through the 2.4G receiver |
| 0x320F:0x221C, 0x221D, 0x1004 | | | other Redragon devices (mouse, mouse pad) |

* sub_402300 lists the device kinds: keyboard = 0x320F:0x511B.
* `DefaultData/Keyboard.json` says 0x0C45:0x1007; no code uses those numbers (stale default).
* The person's keyboard reports 0x320F:0x511B, and its interface 1 report descriptor has
  usage page 0xFF1C / usage 0x92 with report id 4, 63-byte output and input (runtime check).
* sub_478530: when product = 0x511B and firmware (bcdDevice) < 0x0106, the original shows
  "The current version is too low, please upgrade to the latest version first!".
  The person's keyboard has firmware 0x0202.

## Transport (sub_4810b0, sub_481bc0, sub_4cc6b0, sub_484790)

* Each command is one 64-byte report (id 4) written with `WriteFile`, then a 64-byte reply
  is read with `ReadFile`; the read is retried twice when reply byte 3 ≠ sent byte 3.
* Cable: write timeout 1000 ms (0x3e8), packet data 56 bytes (0x38).
* Receiver (mode 2, second handle at device+0x2f4): write timeout 500 ms (0x1f4), packet
  data 24 bytes (0x18).
* Reply timeout 5000 ms (0x1388).
* On Windows the original opens collection 4 only, so every report it reads is a reply. On Linux
  the hidraw node is all of interface 1: it also carries key presses (report ids 1, 2, 3, 6) and
  replies to other programs. Tested on the person's keyboard: a reply echoes bytes 3–6 (command,
  length, offset), so the app drains waiting reports before each command, skips reports that
  are not id 4 or don't echo its header, and waits the full 5 s.
* Tested on the person's keyboard (0x320F:0x511B, firmware 0x0202): info byte 5 = 128 keys,
  byte 6 = 24 blocks (3072 bytes of macro space); settings block read/write and clock read-back work.

### Packet layout (sub_481df0, sub_483e00)

| Byte | Meaning |
|---|---|
| 0 | report id = 0x04 |
| 1, 2 | checksum = sum of bytes 3..63, little-endian 16-bit |
| 3 | command |
| 4 | data length in this packet |
| 5, 6 | offset, little-endian |
| 7 | 0 on send; reply status: 0xFF → error -102, 0xFE → error -103 |
| 8.. | data (reads: reply data starts at byte 8) |

### Commands

| Cmd | Name | Function | Notes |
|---|---|---|---|
| 0x01 | begin configure | sub_481df0 | |
| 0x02 | end configure | sub_482000 | `Sleep(10)` before sending |
| 0x03 | read device info | sub_482430 | 37 (0x25) bytes |
| 0x05 | read config | sub_484080 | |
| 0x06 | write config | sub_483e00 | |
| 0x07 | read key table | sub_482ea0 | N×3 bytes, N = info byte 5 |
| 0x09 | write key table | sub_482710 | offset (table << 9) + i; table must be 0 |
| 0x12 | per-key colour frame | sub_483b50 | N×3 bytes, offset 0 |
| 0x15 | write macros | sub_4829d0 | |
| 0x1B | read LED numbers | sub_483140 | N bytes, one per hardware key |
| 0x0B, 0x17, 0x1A, 0x1C, 0x21, 0x23, 0xAA | screen upload, reset, link check | | not used by this app |

## Connecting (sub_49eab0)

1. begin (0x01)
2. read info (0x03, 37 bytes): byte 5 = number of hardware keys N, byte 6 = macro space
   in 128-byte blocks; `sub_484760` refuses N×3 > 0x200.
3. read the keyboard's own key table (0x07, N×3 bytes).
4. read LED numbers (0x1B, N bytes).
5. end (0x02).
6. sub_4a0e70: for each of 170 (0xaa) key slots, the hardware key is the **first**
   hardware key whose 3 bytes equal the built-in default table at 0x6ad5c8
   (3 × u32 per slot: type, modifier, code). The slot's LED number = LED table[hardware key].

## Key table (sub_49f700 → cmd 0x09)

* Buffer starts as a copy of the keyboard's own key table (N×3 bytes from step 3).
* For each slot with a hardware key: bytes [type, modifier, code] at hardware key × 3.
  Byte order settled by the table compare in sub_4a0e70.
* Assignment value `0xTTMMKK`: 0x20 keyboard (MM = modifier bits 0x01 LCtrl, 0x02 LShift,
  0x04 LAlt, 0x08 LWin, 0x10 RCtrl, 0x20 RShift, 0x40 RAlt, 0x80 RWin; KK = HID usage),
  0xA0 Fn/special, 0xD0 profile/light keys, 0x70/0x71 macro (checked in sub_49f700).
* Off = 0x200000. Default table: 170 slots at 0x6ad5c8 (differs from Keyboard.json in slots
  79–83, 100, 101, 103: international keys 0x8A, 0x8B, 0x90, 0x91, 0x88, 0x87, 0x89, 0x85).

## Per-key colours (sub_4a0ff0 → cmd 0x12)

* Sent only while the light mode is 0x15 or 0x16 (Audio wave / Light and shadow), which
  go to the keyboard as mode byte 0xFE.
* Buffer N×3 bytes; for each slot with an LED number L < 0xFF:
  `buf[L*3+c] = (colour[c] × alpha) >> 8` for R, G, B.
* After a failed send the original waits 200 ms (`Sleep(0xc8)`).

## Lighting / settings block (sub_4a0990)

Written with cmd 0x06, length 0x31 (49), offset = profile × 0x40, between 0x01 and 0x02.
Field names from sub_44a990 (`LightInfo`) and sub_451a80 (`SystemStages`).

| Byte | Field | Rule |
|---|---|---|
| 0x00 | active profile | argument |
| 0x01 | light mode | mode value; 0x15 and 0x16 → 0xFE |
| 0x02 | brightness | `Light`, 0..5 |
| 0x03 | speed | **5 − Speed**, Speed 0..5 |
| 0x04 | direction | `Fx` |
| 0x05 | rainbow | `MultiColor` |
| 0x06..0x08 | colour | `Red`, `Green`, `Blue` |
| 0x10 | 0 | |
| 0x12 | Surmount style | `ZZCCLedMode` |
| 0x13 | W flag | `WFlag` |
| 0x14 | 6-key / N-key | `Key6Flag` |
| 0x15 | Windows-key lock | `WinFlag` |
| 0x16 | polling rate | 0 = 1000 Hz, 1 = 500, 2 = 250, 3 = 125 |
| 0x19 | custom group | `CustomGroup` |
| 0x23..0x25 | clock sec, min, hour | BCD, from `localtime` (tm fields 0, 4, 8) |
| 0x26 | weekday | BCD, 0 = Sunday (tm_wday) |
| 0x27, 0x28 | day, month | BCD (tm_mday, tm_mon + 1) |
| 0x29 | year % 100 | BCD |
| 0x2b, 0x2c | frame interval | device+0x2840, LE16 (JSON `FrameIntervalTime` = 100) |
| 0x2d | key debounce | `KeyShake` 0..4 |
| 0x2f | 1 | constant |

Defaults (Keyboard.json): mode 10, Light 5, Speed 2, colour 255/255/255, MultiColor on;
Repeat 16, Key6Flag 1, PollingRate 0, WFlag 0, WinFlag 0, KeyShake 0.

## Light modes (default_light.json group 0, names from lan_en.xml)

1 Normal, 2 Breath, 3 Spectrum, 4 Traverse, 5 Rain, 6 Ripples, 7 Stars, 8 Reaction,
9 Stream, 10 Corrugated, 11 Cartoon, 12 Wave, 13 Serpentine, 14 Roll, 15 Flowers, 16 Scan,
17 Surmount, 18 Speed, 19 Custom, 20 Off, 21 Audio wave, 22 Light and shadow.

## Macros (sub_49fd10 → cmd 0x15)

* Blob: u16 0x55AA, u16 total size, u16 macro count (16-byte header), u16 offset per
  macro; each macro: u16 event count, 2 bytes, then 4 bytes per event.
* Event (sub_44ef70, JSON `Delay`, `Type`, `Action`, `Button`):
  [Delay lo, Delay hi, Type | 0x80 when Action = "down", Button].
* Room on the keyboard = info byte 6 × 128 bytes. Max 250 actions per macro.
* Type value for a keyboard key: **estimated** 1 (no sample macro in the shipped data).

## Screen clock

* The only clock data is in the settings block: bytes 0x23–0x29 (sec, min, hour, weekday,
  day, month, year % 100, all BCD) plus 0x2f = 1, sent with every settings write (sub_4a0990).
  Reading the block back (sub_49ec50: cmd 0x05, 1 byte at 0 = active profile, then 0x31 bytes at
  profile × 0x40) shows no 12/24-hour field; bytes 0x22 and 0x2e only add to a frame counter.
* The original always sends the system time from `localtime` with the hour 0–23:
  sub_4b6200 year = tm_year + 1900 (block keeps year % 100), sub_4b6260 = tm_wday + 1, then
  sub_4a0990 subtracts 1 (0 = Sunday); hour/min/sec/day/month from tm fields 8/4/0/0xc/0x10+1.
* When the original sends it: sub_4a0990 is vtable slot 0x294 of the keyboard class
  (vtable 0x6b73e8, set at 0x49c19a). It is called from every settings handler (20+ sites,
  e.g. sub_46c9a0 at 0x46d0ad) and from message 0xC11 in sub_460e90 (0x461d89, profile 0),
  which sub_45a490 / sub_48cd50 send after the language is changed.
* Not on connect: the connect sequence sub_49eab0 writes nothing. No timer re-sends the clock:
  the SetTimer in sub_460e90 is id 0x408, 10 ms, on WM_RBUTTONUP (0x205).
* The block always goes to the active profile's offset (profile × 0x40) with byte 0 = active
  profile, so the clock travels with whichever profile is written.
* This app adds (its own choices, **estimated**, not in the original): a 12-hour option that
  sends the hour as 1–12, a chosen date/time kept as an offset from the system time, a sync right
  after connecting, and a re-sync every 60 s while connected (all optional in the Screen clock tab).
* Weekday for a chosen date: Sakamoto's method, 0 = Sunday (matches tm_wday).

## App start and install (sub_480a30, sub_45a390, sub_459680)

* sub_480a30 (start-up): `RegisterWindowMessageW("Redragon K708-RGB-PRO")`, then
  `CreateMutexW(.., TRUE, "Redragon K708-RGB-PRO")`. If the mutex already exists
  (GetLastError = 0xB7) it calls `BroadcastSystemMessageW` with that message so the running copy
  comes to the front, and exits: **one copy at a time**. Window class/title "Redragon K708-RGB-PRO",
  created 800 × 572 (0x320 × 0x23c) then sized by the skin to 1000 × 640.
* sub_45a390: the only registry write is `HKCU\SOFTWARE\Redragon K708-RGB-PRO\LANGUAGE` (DWORD).
  No Run key: the original does not start itself with Windows.
* sub_459680: puts an icon in the system tray when the main window is made (icon resource 0x6b).
* This app: same one-copy rule through a Unix socket named after the mutex in `$XDG_RUNTIME_DIR`
  (a second start brings the window forward and exits; screenshot runs are exempt). No autostart,
  like the original. The tray icon is **not built** (needs a desktop tray library).
* Install/uninstall scripts (`scripts/install/arch/`) copy the binary, a menu entry, an icon and
  the udev rule for 0x320F:0x511B / 0x320F:0x511C; settings live in `~/.config/k708-control`.

## Still estimated

* Macro `Type` for keyboard keys (1).
* Number of profiles shown (3): the offset rule profile × 0x40 is from the code, the count is not.
* Saving per-key colours into the keyboard's own Custom mode (19): its upload command was not found;
  per-key colours here use the live PC-driven path.
