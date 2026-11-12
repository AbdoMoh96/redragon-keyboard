//! Static data: keyboard models, light modes, key layout and key codes.
//! Layout coordinates and default key assignments come from the original's
//! keyboarddevice.xml and DefaultData/Keyboard.json (see MECHANICS.md).

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Model {
    pub name: &'static str,
    pub subtitle: &'static str,
    /// USB (vendor, product) ids this model can show up with.
    pub ids: &'static [(u16, u16)],
    pub verified: bool,
}

pub const MODELS: [Model; 2] = [
    Model {
        name: "K708-RGB-PRO",
        subtitle: "75% wireless, LCD screen + knob",
        // Device table in sub_402a10 (stride 0x9c, 11 entries; matched in sub_4643c0).
        // Keyboard.json's 0x0C45:0x1007 is a stale default the code never uses.
        ids: &[
            (12815, 20763), // 0x320F:0x511B wired
            (12815, 20764), // 0x320F:0x511C 2.4G receiver
        ],
        verified: true,
    },
    Model {
        name: "Other Redragon keyboard",
        subtitle: "same protocol, enter its USB ids",
        ids: &[],
        verified: false,
    },
];

/// The 2.4G receiver: smaller packets and a shorter write timeout (sub_484790, sub_4810b0).
pub const DONGLE_PID: u16 = 20764;
/// Firmware below this (bcdDevice) gets the "version is too low" prompt for 0x511B (sub_478530).
pub const MIN_FIRMWARE: u16 = 262; // 0x0106

pub fn ids_text(ids: &[(u16, u16)]) -> String {
    ids.iter().map(|(v, p)| format!("{v:04x}:{p:04x}")).collect::<Vec<_>>().join(", ")
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightMode {
    pub value: u8,
    pub name: &'static str,
    pub uses_color: bool,
    pub uses_direction: bool,
}

const fn lm(value: u8, name: &'static str, uses_color: bool, uses_direction: bool) -> LightMode {
    LightMode { value, name, uses_color, uses_direction }
}

/// Visible modes of group 0 in default_light.json, names from lan_en.xml.
pub const LIGHT_MODES: [LightMode; 22] = [
    lm(1, "Normal", true, false),
    lm(2, "Breath", true, false),
    lm(3, "Spectrum", false, false),
    lm(4, "Traverse", true, true),
    lm(5, "Rain", true, true),
    lm(6, "Ripples", true, false),
    lm(7, "Stars", true, false),
    lm(8, "Reaction", true, false),
    lm(9, "Stream", true, true),
    lm(10, "Corrugated", true, true),
    lm(11, "Cartoon", true, false),
    lm(12, "Wave", true, true),
    lm(13, "Serpentine", true, true),
    lm(14, "Roll", true, true),
    lm(15, "Flowers", true, false),
    lm(16, "Scan", true, true),
    lm(17, "Surmount", true, false),
    lm(18, "Speed", true, false),
    lm(19, "Custom", false, false),
    lm(20, "Off", false, false),
    lm(21, "Audio wave", true, false),
    lm(22, "Light and shadow", false, false),
];

pub fn mode_by_value(v: u8) -> LightMode {
    LIGHT_MODES.iter().copied().find(|m| m.value == v).unwrap_or(LIGHT_MODES[0])
}

/// Direction names from lan_en.xml (light_set_*_text).
pub const DIRECTIONS: [&str; 8] = [
    "From left to right",
    "From right to left",
    "Top to bottom",
    "Bottom-up",
    "From the inside",
    "Outside",
    "Clockwise",
    "Anticlockwise",
];

/// Favourite colours from color_option_info.config.
pub const PALETTE: [[u8; 3]; 13] = [
    [0xFF, 0x00, 0x00], [0xFF, 0x72, 0x00], [0xFF, 0xF0, 0x05], [0x00, 0xD7, 0x0F],
    [0x00, 0x99, 0xFF], [0x31, 0x53, 0xFF], [0x5E, 0x01, 0xD2], [0xFF, 0x16, 0xA9],
    [0xFF, 0x00, 0x8A], [0xFF, 0xA8, 0x00], [0x8D, 0xFF, 0x8D], [0x3D, 0xEF, 0xFF],
    [0xFF, 0xFF, 0xFF],
];

pub const POLLING_HZ: [u32; 4] = [1000, 500, 250, 125];
pub const PROFILES: usize = 3;
pub const FRAME_INTERVAL_MS: u16 = 100;

#[derive(Clone, Copy, Debug)]
pub struct KeyCap {
    pub index: usize,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub label: &'static str,
}

/// The layout coordinates sit inside this box of the original picture.
pub const LAYOUT_X0: f32 = 100.0;
pub const LAYOUT_Y0: f32 = 16.0;
pub const LAYOUT_X1: f32 = 576.0;
pub const LAYOUT_Y1: f32 = 202.0;

pub const TYPE_KEY: u32 = 0x20;
pub const TYPE_MACRO: u32 = 0x70;
pub const DISABLED: u32 = 0x200000;

pub fn assign_key(modifiers: u8, usage: u8) -> u32 {
    TYPE_KEY << 16 | (modifiers as u32) << 8 | usage as u32
}

pub fn assign_macro(index: u8) -> u32 {
    TYPE_MACRO << 16 | (index as u32) << 8
}

pub const MOD_NAMES: [(u8, &str); 8] = [
    (0x01, "Ctrl"), (0x02, "Shift"), (0x04, "Alt"), (0x08, "Win"),
    (0x10, "RCtrl"), (0x20, "RShift"), (0x40, "RAlt"), (0x80, "RWin"),
];

/// HID keyboard usages offered in the remap list (names from lan_en.xml).
pub const USAGES: &[(u8, &str)] = &[
    (0x04, "A"), (0x05, "B"), (0x06, "C"), (0x07, "D"), (0x08, "E"), (0x09, "F"), (0x0A, "G"),
    (0x0B, "H"), (0x0C, "I"), (0x0D, "J"), (0x0E, "K"), (0x0F, "L"), (0x10, "M"), (0x11, "N"),
    (0x12, "O"), (0x13, "P"), (0x14, "Q"), (0x15, "R"), (0x16, "S"), (0x17, "T"), (0x18, "U"),
    (0x19, "V"), (0x1A, "W"), (0x1B, "X"), (0x1C, "Y"), (0x1D, "Z"),
    (0x1E, "1"), (0x1F, "2"), (0x20, "3"), (0x21, "4"), (0x22, "5"), (0x23, "6"), (0x24, "7"),
    (0x25, "8"), (0x26, "9"), (0x27, "0"),
    (0x28, "Enter"), (0x29, "Esc"), (0x2A, "Backspace"), (0x2B, "Tab"), (0x2C, "Space"),
    (0x2D, "-"), (0x2E, "="), (0x2F, "["), (0x30, "]"), (0x31, "\\"), (0x33, ";"), (0x34, "'"),
    (0x35, "`"), (0x36, ","), (0x37, "."), (0x38, "/"), (0x39, "Caps Lock"),
    (0x3A, "F1"), (0x3B, "F2"), (0x3C, "F3"), (0x3D, "F4"), (0x3E, "F5"), (0x3F, "F6"),
    (0x40, "F7"), (0x41, "F8"), (0x42, "F9"), (0x43, "F10"), (0x44, "F11"), (0x45, "F12"),
    (0x46, "Print Scr"), (0x47, "Scroll Lock"), (0x48, "Pause"), (0x49, "Insert"), (0x4A, "Home"),
    (0x4B, "Page Up"), (0x4C, "Delete"), (0x4D, "End"), (0x4E, "Page Down"), (0x4F, "Right"),
    (0x50, "Left"), (0x51, "Down"), (0x52, "Up"), (0x53, "Num Lock"), (0x54, "Num /"),
    (0x55, "Num *"), (0x56, "Num -"), (0x57, "Num +"), (0x58, "Num Enter"), (0x59, "Num 1"),
    (0x5A, "Num 2"), (0x5B, "Num 3"), (0x5C, "Num 4"), (0x5D, "Num 5"), (0x5E, "Num 6"),
    (0x5F, "Num 7"), (0x60, "Num 8"), (0x61, "Num 9"), (0x62, "Num 0"), (0x63, "Decimal"),
    (0x64, "K45 (\\|)"), (0x65, "App"),
];

pub fn usage_name(u: u8) -> &'static str {
    USAGES.iter().find(|(c, _)| *c == u).map(|(_, n)| *n).unwrap_or("?")
}

/// Short text describing an assignment value.
pub fn describe(a: u32) -> String {
    let t = a >> 16;
    let m = ((a >> 8) & 0xff) as u8;
    let k = (a & 0xff) as u8;
    match t {
        0x20 if m == 0 && k == 0 => "Off".into(),
        0x20 => {
            let mut parts: Vec<String> =
                MOD_NAMES.iter().filter(|(b, _)| m & b != 0).map(|(_, n)| n.to_string()).collect();
            if k != 0 {
                parts.push(usage_name(k).to_string());
            }
            parts.join("+")
        }
        0x70 | 0x71 => format!("Macro {}", m as u32 + 1),
        0xA0 => "Fn / special".into(),
        0xD0 => "Profile / light key".into(),
        _ => format!("{a:06X}"),
    }
}

/// Number of key slots the original works with (loops to 0xaa in sub_4a0e70).
pub const KEY_SLOTS: usize = 170;

/// Built-in default key table at 0x6ad5c8 (170 slots, 0xTTMMKK), used by sub_4a0e70
/// to find which hardware key each slot is, by matching the keyboard's own key table.
pub const DEFAULT_ASSIGN: [u32; KEY_SLOTS] = [
    0x200029, 0x20003a, 0x20003b, 0x20003c, 0x20003d, 0x20003e, 0x20003f, 0x200040,
    0x200041, 0x200042, 0x200043, 0x200044, 0x200045, 0x200046, 0x200047, 0x200048,
    0xa01b00, 0xa02400, 0x200000, 0x200000, 0x200000, 0x200035, 0x20001e, 0x20001f,
    0x200020, 0x200021, 0x200022, 0x200023, 0x200024, 0x200025, 0x200026, 0x200027,
    0x20002d, 0x20002e, 0x20002a, 0x200049, 0x20004a, 0x20004b, 0x200053, 0x200054,
    0x200055, 0x200056, 0x20002b, 0x200014, 0x20001a, 0x200008, 0x200015, 0x200017,
    0x20001c, 0x200018, 0x20000c, 0x200012, 0x200013, 0x20002f, 0x200030, 0x200031,
    0x20004c, 0x20004d, 0x20004e, 0x20005f, 0x200060, 0x200061, 0x200032, 0x200039,
    0x200004, 0x200016, 0x200007, 0x200009, 0x20000a, 0x20000b, 0x20000d, 0x20000e,
    0x20000f, 0x200033, 0x200034, 0x200028, 0x20005c, 0x20005d, 0x20005e, 0x20008a,
    0x20008b, 0x200090, 0x200091, 0x200088, 0x200200, 0x20001d, 0x20001b, 0x200006,
    0x200019, 0x200005, 0x200011, 0x200010, 0x200036, 0x200037, 0x200038, 0x202000,
    0x200052, 0x200059, 0x20005a, 0x20005b, 0x200087, 0x200089, 0x200064, 0x200085,
    0x200000, 0x200100, 0x200800, 0x200400, 0x20002c, 0x204000, 0xa00100, 0x200065,
    0x201000, 0x200050, 0x200051, 0x20004f, 0x200062, 0x200063, 0x200058, 0x200057,
    0xd0a201, 0xd0a202, 0xd0a203, 0xd0a204, 0xd0a205, 0x208000, 0x200000, 0x200000,
    0x200000, 0x200000, 0x200000, 0x200000, 0x200000, 0x200000, 0x200000, 0x200000,
    0x200000, 0x200000, 0x200000, 0x200000, 0x200000, 0x200000, 0x200000, 0x200000,
    0x200000, 0x200000, 0x200000, 0x200000, 0x200000, 0x200000, 0x200000, 0x200000,
    0x200000, 0x200000, 0x200000, 0x200000, 0x200000, 0x200000, 0x200000, 0x200000,
    0x200000, 0x200000, 0x200000, 0x200000, 0x200000, 0x200000, 0x200000, 0x200000,
    0x200000, 0x200000,
];

pub const LAYOUT: [KeyCap; 81] = [
    KeyCap { index: 0, x: 107.0, y: 24.0, w: 21.0, h: 22.0, label: "Esc" },
    KeyCap { index: 1, x: 140.0, y: 24.0, w: 20.0, h: 22.0, label: "F1" },
    KeyCap { index: 2, x: 169.0, y: 24.0, w: 20.0, h: 22.0, label: "F2" },
    KeyCap { index: 3, x: 197.0, y: 24.0, w: 21.0, h: 22.0, label: "F3" },
    KeyCap { index: 4, x: 225.0, y: 23.0, w: 20.0, h: 23.0, label: "F4" },
    KeyCap { index: 5, x: 258.0, y: 24.0, w: 20.0, h: 22.0, label: "F5" },
    KeyCap { index: 6, x: 287.0, y: 24.0, w: 21.0, h: 22.0, label: "F6" },
    KeyCap { index: 7, x: 315.0, y: 24.0, w: 20.0, h: 22.0, label: "F7" },
    KeyCap { index: 8, x: 343.0, y: 24.0, w: 21.0, h: 22.0, label: "F8" },
    KeyCap { index: 9, x: 376.0, y: 24.0, w: 20.0, h: 22.0, label: "F9" },
    KeyCap { index: 10, x: 405.0, y: 24.0, w: 20.0, h: 22.0, label: "F10" },
    KeyCap { index: 11, x: 433.0, y: 24.0, w: 20.0, h: 22.0, label: "F11" },
    KeyCap { index: 12, x: 461.0, y: 24.0, w: 21.0, h: 22.0, label: "F12" },
    KeyCap { index: 21, x: 107.0, y: 57.0, w: 21.0, h: 22.0, label: "`" },
    KeyCap { index: 22, x: 136.0, y: 57.0, w: 21.0, h: 22.0, label: "1" },
    KeyCap { index: 23, x: 165.0, y: 57.0, w: 20.0, h: 22.0, label: "2" },
    KeyCap { index: 24, x: 193.0, y: 57.0, w: 19.0, h: 22.0, label: "3" },
    KeyCap { index: 25, x: 220.0, y: 57.0, w: 22.0, h: 22.0, label: "4" },
    KeyCap { index: 26, x: 249.0, y: 57.0, w: 21.0, h: 22.0, label: "5" },
    KeyCap { index: 27, x: 278.0, y: 57.0, w: 20.0, h: 22.0, label: "6" },
    KeyCap { index: 28, x: 307.0, y: 57.0, w: 20.0, h: 22.0, label: "7" },
    KeyCap { index: 29, x: 335.0, y: 57.0, w: 21.0, h: 22.0, label: "8" },
    KeyCap { index: 30, x: 363.0, y: 57.0, w: 20.0, h: 22.0, label: "9" },
    KeyCap { index: 31, x: 390.0, y: 57.0, w: 22.0, h: 22.0, label: "0" },
    KeyCap { index: 32, x: 420.0, y: 57.0, w: 19.0, h: 22.0, label: "-" },
    KeyCap { index: 33, x: 447.0, y: 57.0, w: 22.0, h: 22.0, label: "=" },
    KeyCap { index: 34, x: 476.0, y: 57.0, w: 50.0, h: 22.0, label: "Bksp" },
    KeyCap { index: 35, x: 545.0, y: 57.0, w: 21.0, h: 22.0, label: "Ins" },
    KeyCap { index: 42, x: 108.0, y: 84.0, w: 34.0, h: 23.0, label: "Tab" },
    KeyCap { index: 43, x: 150.0, y: 84.0, w: 20.0, h: 23.0, label: "Q" },
    KeyCap { index: 44, x: 179.0, y: 84.0, w: 19.0, h: 23.0, label: "W" },
    KeyCap { index: 45, x: 208.0, y: 84.0, w: 19.0, h: 23.0, label: "E" },
    KeyCap { index: 46, x: 236.0, y: 84.0, w: 20.0, h: 23.0, label: "R" },
    KeyCap { index: 47, x: 264.0, y: 84.0, w: 21.0, h: 23.0, label: "T" },
    KeyCap { index: 48, x: 292.0, y: 84.0, w: 21.0, h: 23.0, label: "Y" },
    KeyCap { index: 49, x: 321.0, y: 84.0, w: 20.0, h: 23.0, label: "U" },
    KeyCap { index: 50, x: 349.0, y: 84.0, w: 19.0, h: 23.0, label: "I" },
    KeyCap { index: 51, x: 377.0, y: 84.0, w: 21.0, h: 23.0, label: "O" },
    KeyCap { index: 52, x: 406.0, y: 84.0, w: 19.0, h: 23.0, label: "P" },
    KeyCap { index: 53, x: 434.0, y: 84.0, w: 19.0, h: 23.0, label: "[" },
    KeyCap { index: 54, x: 462.0, y: 84.0, w: 20.0, h: 23.0, label: "]" },
    KeyCap { index: 55, x: 491.0, y: 84.0, w: 33.0, h: 23.0, label: "\\" },
    KeyCap { index: 56, x: 545.0, y: 84.0, w: 21.0, h: 23.0, label: "Del" },
    KeyCap { index: 63, x: 108.0, y: 113.0, w: 42.0, h: 22.0, label: "Caps" },
    KeyCap { index: 64, x: 158.0, y: 113.0, w: 20.0, h: 22.0, label: "A" },
    KeyCap { index: 65, x: 185.0, y: 113.0, w: 21.0, h: 22.0, label: "S" },
    KeyCap { index: 66, x: 214.0, y: 113.0, w: 20.0, h: 22.0, label: "D" },
    KeyCap { index: 67, x: 242.0, y: 113.0, w: 21.0, h: 22.0, label: "F" },
    KeyCap { index: 68, x: 271.0, y: 113.0, w: 21.0, h: 22.0, label: "G" },
    KeyCap { index: 69, x: 298.0, y: 113.0, w: 22.0, h: 22.0, label: "H" },
    KeyCap { index: 70, x: 327.0, y: 113.0, w: 21.0, h: 22.0, label: "J" },
    KeyCap { index: 71, x: 356.0, y: 113.0, w: 20.0, h: 22.0, label: "K" },
    KeyCap { index: 72, x: 385.0, y: 113.0, w: 20.0, h: 22.0, label: "L" },
    KeyCap { index: 73, x: 413.0, y: 113.0, w: 20.0, h: 22.0, label: ";" },
    KeyCap { index: 74, x: 442.0, y: 113.0, w: 19.0, h: 22.0, label: "'" },
    KeyCap { index: 75, x: 469.0, y: 113.0, w: 55.0, h: 22.0, label: "Enter" },
    KeyCap { index: 37, x: 545.0, y: 113.0, w: 21.0, h: 22.0, label: "PgUp" },
    KeyCap { index: 84, x: 107.0, y: 141.0, w: 57.0, h: 22.0, label: "Shift" },
    KeyCap { index: 85, x: 171.0, y: 141.0, w: 21.0, h: 22.0, label: "Z" },
    KeyCap { index: 86, x: 200.0, y: 141.0, w: 20.0, h: 22.0, label: "X" },
    KeyCap { index: 87, x: 228.0, y: 141.0, w: 20.0, h: 22.0, label: "C" },
    KeyCap { index: 88, x: 256.0, y: 141.0, w: 21.0, h: 22.0, label: "V" },
    KeyCap { index: 89, x: 285.0, y: 141.0, w: 20.0, h: 22.0, label: "B" },
    KeyCap { index: 90, x: 313.0, y: 141.0, w: 21.0, h: 22.0, label: "N" },
    KeyCap { index: 91, x: 340.0, y: 141.0, w: 22.0, h: 22.0, label: "M" },
    KeyCap { index: 92, x: 370.0, y: 141.0, w: 20.0, h: 22.0, label: "," },
    KeyCap { index: 93, x: 399.0, y: 141.0, w: 20.0, h: 22.0, label: "." },
    KeyCap { index: 94, x: 426.0, y: 141.0, w: 21.0, h: 22.0, label: "/" },
    KeyCap { index: 95, x: 456.0, y: 141.0, w: 41.0, h: 22.0, label: "Shift" },
    KeyCap { index: 96, x: 510.0, y: 144.0, w: 21.0, h: 24.0, label: "Up" },
    KeyCap { index: 58, x: 545.0, y: 141.0, w: 21.0, h: 22.0, label: "PgDn" },
    KeyCap { index: 105, x: 107.0, y: 170.0, w: 29.0, h: 22.0, label: "Ctrl" },
    KeyCap { index: 106, x: 143.0, y: 170.0, w: 28.0, h: 22.0, label: "Win" },
    KeyCap { index: 107, x: 179.0, y: 170.0, w: 27.0, h: 22.0, label: "Alt" },
    KeyCap { index: 108, x: 215.0, y: 170.0, w: 168.0, h: 22.0, label: "Space" },
    KeyCap { index: 109, x: 391.0, y: 170.0, w: 21.0, h: 22.0, label: "Alt" },
    KeyCap { index: 110, x: 419.0, y: 170.0, w: 21.0, h: 22.0, label: "Fn" },
    KeyCap { index: 112, x: 448.0, y: 170.0, w: 21.0, h: 22.0, label: "Ctrl" },
    KeyCap { index: 113, x: 482.0, y: 174.0, w: 21.0, h: 22.0, label: "Left" },
    KeyCap { index: 114, x: 510.0, y: 174.0, w: 21.0, h: 22.0, label: "Down" },
    KeyCap { index: 115, x: 539.0, y: 174.0, w: 21.0, h: 22.0, label: "Right" },
];
