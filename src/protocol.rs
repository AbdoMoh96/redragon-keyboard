//! Packet building for the Redragon K708 protocol.
//! Every number here comes from MECHANICS.md (function addresses listed there).

pub const REPORT_SIZE: usize = 64;
pub const REPORT_ID: u8 = 0x04;
pub const HEADER: usize = 8;
/// Data bytes per packet over the cable (sub_484790, mode 1).
pub const CHUNK_WIRED: usize = 56; // 0x38
/// Data bytes per packet through the 2.4G receiver (sub_484790, mode 2).
pub const CHUNK_DONGLE: usize = 24; // 0x18

pub const CMD_BEGIN: u8 = 0x01;
pub const CMD_END: u8 = 0x02;
pub const CMD_READ_INFO: u8 = 0x03;
pub const CMD_READ_CONFIG: u8 = 0x05;
pub const CMD_WRITE_CONFIG: u8 = 0x06;
pub const CMD_READ_KEYMAP: u8 = 0x07;
pub const CMD_WRITE_KEYMAP: u8 = 0x09;
pub const CMD_REALTIME: u8 = 0x12;
pub const CMD_WRITE_MACROS: u8 = 0x15;
pub const CMD_READ_LEDMAP: u8 = 0x1B;

pub const READ_INFO_LEN: usize = 37; // 0x25
/// Info byte 5 = number of hardware keys, byte 6 = macro space in 128-byte blocks (sub_49eab0, sub_49fd10).
pub const INFO_KEYS: usize = 5;
pub const INFO_MACRO_BLOCKS: usize = 6;
pub const MACRO_BLOCK: usize = 128;
/// Key table length limit (sub_484760: N*3 must be <= 0x200).
pub const KEYMAP_MAX: usize = 512;
pub const CONFIG_LEN: usize = 49; // 0x31
pub const PROFILE_STRIDE: u16 = 64; // 0x40

pub const BRIGHTNESS_MAX: u8 = 5;
pub const SPEED_MAX: u8 = 5;
pub const PC_DRIVEN_MODE: u8 = 254; // 0xFE
pub const DEBOUNCE_MAX: u8 = 4;

pub const MACRO_MAGIC: u16 = 21930; // 0x55AA
pub const MACRO_HEADER: usize = 16;
pub const MACRO_MAX_ACTIONS: usize = 250;
pub const MACRO_MIN_INTERVAL_MS: u16 = 10;
/// Event byte 2: Type, with bit 7 set for "down" (sub_44ef70).
pub const MACRO_DOWN_BIT: u8 = 128;
/// Type value for a keyboard key (estimated: no sample macro in the original's data).
pub const MACRO_TYPE_KEY: u8 = 1;

/// One 64-byte output report: id, checksum, command, length, offset, status, data.
pub fn packet(cmd: u8, offset: u16, len: usize, data: &[u8]) -> [u8; REPORT_SIZE] {
    let mut p = [0u8; REPORT_SIZE];
    p[0] = REPORT_ID;
    p[3] = cmd;
    p[4] = len.min(REPORT_SIZE - HEADER) as u8;
    p[5] = (offset & 0xff) as u8;
    p[6] = (offset >> 8) as u8;
    p[7] = 0;
    let n = data.len().min(REPORT_SIZE - HEADER);
    p[HEADER..HEADER + n].copy_from_slice(&data[..n]);
    let sum = checksum(&p);
    p[1] = (sum & 0xff) as u8;
    p[2] = (sum >> 8) as u8;
    p
}

/// Sum of bytes 3..63 as a 16-bit value.
pub fn checksum(p: &[u8; REPORT_SIZE]) -> u16 {
    p[3..].iter().fold(0u16, |a, &b| a.wrapping_add(b as u16))
}

/// A simple command with no data (begin / end configure).
pub fn simple(cmd: u8) -> [u8; REPORT_SIZE] {
    packet(cmd, 0, 0, &[])
}

/// Read requests for `len` bytes, one per chunk (sub_482430, sub_482ea0, sub_483140).
pub fn reads(cmd: u8, len: usize, chunk: usize) -> Vec<(usize, [u8; REPORT_SIZE])> {
    reads_at(cmd, 0, len, chunk)
}

/// Read requests starting at `base` (sub_49ec50 reads the config at profile × 0x40).
pub fn reads_at(cmd: u8, base: u16, len: usize, chunk: usize) -> Vec<(usize, [u8; REPORT_SIZE])> {
    (0..len)
        .step_by(chunk)
        .map(|off| {
            let n = chunk.min(len - off);
            (n, packet(cmd, base + off as u16, n, &[]))
        })
        .collect()
}

fn from_bcd(b: u8) -> u32 {
    (b >> 4) as u32 * 10 + (b & 0x0f) as u32
}

/// The clock stored in a settings block (bytes 0x23..0x29, BCD).
pub fn block_clock(block: &[u8]) -> Option<Clock> {
    if block.len() < 0x2a {
        return None;
    }
    Some(Clock {
        sec: from_bcd(block[0x23]),
        min: from_bcd(block[0x24]),
        hour: from_bcd(block[0x25]),
        weekday: from_bcd(block[0x26]),
        day: from_bcd(block[0x27]),
        month: from_bcd(block[0x28]),
        year: 2000 + from_bcd(block[0x29]),
    })
}

/// Split a write of `data` at `base` into chunked packets (sub_483e00).
pub fn chunked(cmd: u8, base: u16, data: &[u8], chunk: usize) -> Vec<[u8; REPORT_SIZE]> {
    data.chunks(chunk)
        .enumerate()
        .map(|(i, c)| packet(cmd, base + (i * chunk) as u16, c.len(), c))
        .collect()
}

/// Reply status byte 7: 0xFF and 0xFE are errors (-102 / -103 in the original).
pub fn reply_error(reply: &[u8]) -> Option<&'static str> {
    match reply.get(7) {
        Some(0xFF) => Some("keyboard answered with error 0xFF"),
        Some(0xFE) => Some("keyboard answered with error 0xFE"),
        _ => None,
    }
}

fn bcd(v: u32) -> u8 {
    (((v / 10) % 10) << 4 | (v % 10)) as u8
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clock {
    pub sec: u32,
    pub min: u32,
    pub hour: u32,
    pub weekday: u32,
    pub day: u32,
    pub month: u32,
    pub year: u32,
}

impl Clock {
    /// A clock for a chosen date and time; weekday 0 = Sunday, as tm_wday in sub_4a0990.
    pub fn from_date_time(year: u32, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> Self {
        let day = day.clamp(1, days_in_month(year, month));
        Clock { sec, min, hour, weekday: weekday(year, month, day), day, month, year }
    }

    /// 12-hour form of the hour (1..12). The original only sends 0..23 (no 12/24 field
    /// exists in the block); this is the app's own option.
    pub fn as_12_hour(mut self) -> Self {
        self.hour = match self.hour % 12 {
            0 => 12,
            h => h,
        };
        self
    }
}

pub fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Day of week, 0 = Sunday (Sakamoto's method).
pub fn weekday(year: u32, month: u32, day: u32) -> u32 {
    const T: [u32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    (y + y / 4 - y / 100 + y / 400 + T[(month.clamp(1, 12) - 1) as usize] + day) % 7
}

/// The lighting/settings fields that go into the 49-byte config block (sub_4a0990).
#[derive(Clone, Copy, Debug)]
pub struct ConfigBlock {
    pub profile: u8,
    pub mode: u8,
    pub brightness: u8,
    pub speed: u8,
    pub direction: u8,
    pub multicolor: bool,
    pub color: [u8; 3],
    pub surmount_mode: u8,
    pub w_swap: bool,
    pub six_key: bool,
    pub win_lock: bool,
    pub polling: u8,
    pub custom_group: u8,
    pub frame_interval: u16,
    pub debounce: u8,
}

impl ConfigBlock {
    pub fn bytes(&self, clock: Clock) -> [u8; CONFIG_LEN] {
        let mut b = [0u8; CONFIG_LEN];
        b[0x00] = self.profile;
        b[0x01] = if self.mode == 21 || self.mode == 22 { PC_DRIVEN_MODE } else { self.mode };
        b[0x02] = self.brightness.min(BRIGHTNESS_MAX);
        b[0x03] = SPEED_MAX - self.speed.min(SPEED_MAX);
        b[0x04] = self.direction;
        b[0x05] = self.multicolor as u8;
        b[0x06] = self.color[0];
        b[0x07] = self.color[1];
        b[0x08] = self.color[2];
        b[0x10] = 0;
        b[0x12] = self.surmount_mode;
        b[0x13] = self.w_swap as u8;
        b[0x14] = self.six_key as u8;
        b[0x15] = self.win_lock as u8;
        b[0x16] = self.polling.min(3);
        b[0x19] = self.custom_group;
        b[0x23] = bcd(clock.sec);
        b[0x24] = bcd(clock.min);
        b[0x25] = bcd(clock.hour);
        b[0x26] = bcd(clock.weekday);
        b[0x27] = bcd(clock.day);
        b[0x28] = bcd(clock.month);
        b[0x29] = bcd(clock.year % 100);
        b[0x2b] = (self.frame_interval & 0xff) as u8;
        b[0x2c] = (self.frame_interval >> 8) as u8;
        b[0x2d] = self.debounce.min(DEBOUNCE_MAX);
        b[0x2f] = 1;
        b
    }
}

/// Full write sequence for the lighting/settings block.
pub fn config_write(cfg: &ConfigBlock, clock: Clock, chunk: usize) -> Vec<[u8; REPORT_SIZE]> {
    let mut v = vec![simple(CMD_BEGIN)];
    v.extend(chunked(CMD_WRITE_CONFIG, cfg.profile as u16 * PROFILE_STRIDE, &cfg.bytes(clock), chunk));
    v.push(simple(CMD_END));
    v
}

fn assign_bytes(a: u32) -> [u8; 3] {
    [(a >> 16) as u8, (a >> 8) as u8, a as u8]
}

/// What the keyboard told us when connecting (sub_49eab0).
#[derive(Clone, Debug, Default)]
pub struct DeviceTables {
    pub info: Vec<u8>,
    /// The keyboard's own key table: 3 bytes [type, modifier, code] per hardware key (cmd 0x07).
    pub keymap: Vec<u8>,
    /// LED number of each hardware key (cmd 0x1B).
    pub ledmap: Vec<u8>,
    /// Hardware key of each of the 170 slots, or None (sub_4a0e70).
    pub slot_hw: Vec<Option<usize>>,
}

impl DeviceTables {
    pub fn keys(&self) -> usize {
        self.info.get(INFO_KEYS).copied().unwrap_or(0) as usize
    }

    pub fn macro_space(&self) -> usize {
        self.info.get(INFO_MACRO_BLOCKS).copied().unwrap_or(0) as usize * MACRO_BLOCK
    }

    /// Slot i is the first hardware key whose 3 bytes equal default[i] (sub_4a0e70).
    pub fn build_slots(&mut self, default: &[u32]) {
        let n = self.keys().min(self.keymap.len() / 3);
        self.slot_hw = default
            .iter()
            .map(|&a| {
                let want = assign_bytes(a);
                (0..n).find(|&k| self.keymap[k * 3..k * 3 + 3] == want)
            })
            .collect();
    }

    pub fn slot_led(&self, slot: usize) -> Option<usize> {
        let hw = (*self.slot_hw.get(slot)?)?;
        self.ledmap.get(hw).map(|&l| l as usize).filter(|&l| l < 0xff)
    }
}

/// Key table to write: start from the keyboard's own table, then put each slot's
/// assignment at its hardware key (sub_49f700).
pub fn keymap_bytes(tables: &DeviceTables, assign: &[u32]) -> Vec<u8> {
    let mut buf = tables.keymap.clone();
    buf.truncate(KEYMAP_MAX);
    for (slot, a) in assign.iter().enumerate() {
        if let Some(Some(hw)) = tables.slot_hw.get(slot) {
            if hw * 3 + 3 <= buf.len() {
                buf[hw * 3..hw * 3 + 3].copy_from_slice(&assign_bytes(*a));
            }
        }
    }
    buf
}

/// cmd 0x09 at offset (0 << 9) + i; the original refuses any other table number (sub_482710).
pub fn keymap_write(tables: &DeviceTables, assign: &[u32], chunk: usize) -> Vec<[u8; REPORT_SIZE]> {
    let mut v = vec![simple(CMD_BEGIN)];
    v.extend(chunked(CMD_WRITE_KEYMAP, 0, &keymap_bytes(tables, assign), chunk));
    v.push(simple(CMD_END));
    v
}

/// One frame of per-key colours for the PC-driven modes (sub_4a0ff0 → cmd 0x12):
/// N*3 bytes, at LED index L: (R*A)>>8, (G*A)>>8, (B*A)>>8.
pub fn realtime_frame(tables: &DeviceTables, colors: &[[u8; 4]], chunk: usize) -> Vec<[u8; REPORT_SIZE]> {
    let mut buf = vec![0u8; tables.keys() * 3];
    for (slot, c) in colors.iter().enumerate() {
        if let Some(l) = tables.slot_led(slot) {
            if l * 3 + 3 <= buf.len() {
                let a = c[3] as u16;
                buf[l * 3] = ((c[0] as u16 * a) >> 8) as u8;
                buf[l * 3 + 1] = ((c[1] as u16 * a) >> 8) as u8;
                buf[l * 3 + 2] = ((c[2] as u16 * a) >> 8) as u8;
            }
        }
    }
    chunked(CMD_REALTIME, 0, &buf, chunk)
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct MacroEvent {
    pub usage: u8,
    pub down: bool,
    pub delay_ms: u16,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Macro {
    pub name: String,
    pub events: Vec<MacroEvent>,
}

/// Macro blob (sub_49fd10): 16-byte header (0x55AA, size, count), u16 offset table,
/// then per macro: u16 event count, 2 reserved bytes, 4 bytes per event
/// [delay lo, delay hi, type | down bit, key code] (sub_44ef70).
pub fn macro_blob(macros: &[Macro]) -> Vec<u8> {
    let mut b = vec![0u8; MACRO_HEADER];
    let table_at = b.len();
    b.resize(table_at + macros.len() * 2, 0);
    for (i, m) in macros.iter().enumerate() {
        let off = b.len() as u16;
        b[table_at + i * 2] = (off & 0xff) as u8;
        b[table_at + i * 2 + 1] = (off >> 8) as u8;
        let n = m.events.len().min(MACRO_MAX_ACTIONS) as u16;
        b.extend([(n & 0xff) as u8, (n >> 8) as u8, 0, 0]);
        for e in m.events.iter().take(MACRO_MAX_ACTIONS) {
            let d = e.delay_ms.max(MACRO_MIN_INTERVAL_MS);
            let kind = MACRO_TYPE_KEY | if e.down { MACRO_DOWN_BIT } else { 0 };
            b.extend([(d & 0xff) as u8, (d >> 8) as u8, kind, e.usage]);
        }
    }
    let size = b.len() as u16;
    b[0] = (MACRO_MAGIC & 0xff) as u8;
    b[1] = (MACRO_MAGIC >> 8) as u8;
    b[2] = (size & 0xff) as u8;
    b[3] = (size >> 8) as u8;
    b[4] = macros.len() as u8;
    b[5] = 0;
    b
}

pub fn macro_write(macros: &[Macro], chunk: usize) -> Vec<[u8; REPORT_SIZE]> {
    let mut v = vec![simple(CMD_BEGIN)];
    v.extend(chunked(CMD_WRITE_MACROS, 0, &macro_blob(macros), chunk));
    v.push(simple(CMD_END));
    v
}

pub fn hex(p: &[u8]) -> String {
    p.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begin_packet() {
        let p = simple(CMD_BEGIN);
        assert_eq!(&p[..8], &[4, 1, 0, 1, 0, 0, 0, 0]);
    }

    #[test]
    fn speed_is_inverted() {
        let c = ConfigBlock {
            profile: 0, mode: 1, brightness: 5, speed: 2, direction: 0, multicolor: false,
            color: [255, 0, 0], surmount_mode: 0, w_swap: false, six_key: true, win_lock: false,
            polling: 0, custom_group: 0, frame_interval: 100, debounce: 0,
        };
        let clk = Clock { sec: 5, min: 42, hour: 13, weekday: 3, day: 12, month: 11, year: 2026 };
        let b = c.bytes(clk);
        assert_eq!(b[3], 3);
        assert_eq!(b[0x24], 0x42);
        assert_eq!(b[0x29], 0x26);
        assert_eq!(config_write(&c, clk, CHUNK_WIRED).len(), 3);
        assert_eq!(config_write(&c, clk, CHUNK_DONGLE).len(), 5);
    }

    #[test]
    fn reads_back_keyboard_clock() {
        // Settings block read from the person's keyboard (cmd 0x05), clock 05:33:01 Thu 12 Nov 2026.
        let block = [
            0x00, 0x01, 0x05, 0x03, 0x00, 0x00, 0x5e, 0x01, 0xd2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x01, 0x33, 0x05, 0x04, 0x12, 0x11, 0x26, 0x00, 0x64, 0, 0, 0, 0x01, 0,
        ];
        let c = block_clock(&block).unwrap();
        assert_eq!((c.hour, c.min, c.sec, c.weekday, c.day, c.month, c.year), (5, 33, 1, 4, 12, 11, 2026));
    }

    #[test]
    fn clock_helpers() {
        let c = Clock::from_date_time(2026, 11, 12, 13, 5, 0);
        assert_eq!(c.weekday, 4); // Thursday
        assert_eq!(c.as_12_hour().hour, 1);
        assert_eq!(Clock::from_date_time(2026, 11, 12, 0, 0, 0).as_12_hour().hour, 12);
        assert_eq!(Clock::from_date_time(2024, 2, 31, 0, 0, 0).day, 29);
    }

    #[test]
    fn slots_match_keyboard_table() {
        // keyboard with 3 keys: B, Esc, A
        let mut t = DeviceTables {
            info: vec![0, 0, 0, 0, 0, 3, 1],
            keymap: vec![0x20, 0, 0x05, 0x20, 0, 0x29, 0x20, 0, 0x04],
            ledmap: vec![7, 8, 9],
            slot_hw: vec![],
        };
        t.build_slots(&[0x200029, 0x200004, 0x200099]);
        assert_eq!(t.slot_hw, vec![Some(1), Some(2), None]);
        let out = keymap_bytes(&t, &[0x200029, 0x200105, 0]);
        assert_eq!(out, vec![0x20, 0, 0x05, 0x20, 0, 0x29, 0x20, 0x01, 0x05]);
        assert_eq!(t.slot_led(0), Some(8));
    }
}
