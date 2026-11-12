//! Talking to the keyboard through Linux hidraw nodes.

use crate::data;
use crate::protocol::{self, DeviceTables, REPORT_SIZE};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Write timeout over the cable / through the receiver, read timeout (sub_4810b0).
pub const WRITE_TIMEOUT_WIRED_MS: i32 = 1000; // 0x3e8
pub const WRITE_TIMEOUT_DONGLE_MS: i32 = 500; // 0x1f4
pub const READ_TIMEOUT_MS: i32 = 5000; // 0x1388

/// The settings channel in the original's device table (sub_402a10): interface 1,
/// collection 4, usage page 0xFF1C, usage 0x92 — report id 4.
pub const CHANNEL_USAGE_PAGE: u32 = 65308; // 0xFF1C
pub const CHANNEL_USAGE: u32 = 146; // 0x92

#[derive(Clone, Debug)]
pub struct HidNode {
    pub path: PathBuf,
    pub vid: u16,
    pub pid: u16,
    pub name: String,
    /// The node carries the settings channel (report id 4 in usage page 0xFF1C / usage 0x92).
    pub command_channel: bool,
    pub can_open: bool,
    /// USB bcdDevice (firmware version), when sysfs has it.
    pub firmware: Option<u16>,
}

impl HidNode {
    pub fn looks_redragon(&self) -> bool {
        self.name.to_lowercase().contains("redragon") || self.command_channel
    }

    pub fn is_dongle(&self) -> bool {
        self.pid == data::DONGLE_PID
    }
}

/// Walks a HID report descriptor: true when a collection on usage page 0xFF1C,
/// usage 0x92 has an output report with id 4 — the channel the original opens.
pub fn has_command_channel(desc: &[u8]) -> bool {
    let mut i = 0;
    let mut usage_page: u32 = 0;
    let mut usage: u32 = 0;
    let mut collection_usage: Option<(u32, u32)> = None;
    let mut report_id: u32 = 0;
    while i < desc.len() {
        let prefix = desc[i];
        if prefix == 0xFE {
            let len = *desc.get(i + 1).unwrap_or(&0) as usize;
            i += 3 + len;
            continue;
        }
        let size = match prefix & 0x03 {
            3 => 4,
            n => n as usize,
        };
        let mut value: u32 = 0;
        for b in 0..size {
            value |= (*desc.get(i + 1 + b).unwrap_or(&0) as u32) << (8 * b);
        }
        match prefix & 0xFC {
            0x04 => usage_page = value,
            0x08 => usage = value,
            0x84 => report_id = value,
            0xA0 => collection_usage = Some((usage_page, usage)),
            0x90 if report_id == protocol::REPORT_ID as u32
                && collection_usage == Some((CHANNEL_USAGE_PAGE, CHANNEL_USAGE)) =>
            {
                return true
            }
            _ => {}
        }
        i += 1 + size;
    }
    false
}

/// Every hidraw node on the system with its USB ids, read from sysfs.
pub fn scan() -> Vec<HidNode> {
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir("/sys/class/hidraw") else { return out };
    for e in dir.flatten() {
        let node = e.file_name().to_string_lossy().to_string();
        let Ok(text) = std::fs::read_to_string(e.path().join("device/uevent")) else { continue };
        let mut vid = 0;
        let mut pid = 0;
        let mut name = String::new();
        for line in text.lines() {
            if let Some(id) = line.strip_prefix("HID_ID=") {
                let parts: Vec<&str> = id.split(':').collect();
                if parts.len() == 3 {
                    vid = u32::from_str_radix(parts[1], 16).unwrap_or(0) as u16;
                    pid = u32::from_str_radix(parts[2], 16).unwrap_or(0) as u16;
                }
            } else if let Some(n) = line.strip_prefix("HID_NAME=") {
                name = n.to_string();
            }
        }
        let command_channel = std::fs::read(e.path().join("device/report_descriptor"))
            .map(|d| has_command_channel(&d))
            .unwrap_or(false);
        // hidraw → hid device → USB interface → USB device
        let firmware = std::fs::read_to_string(e.path().join("device/../../bcdDevice"))
            .ok()
            .and_then(|s| u16::from_str_radix(s.trim(), 16).ok());
        let path = PathBuf::from("/dev").join(node);
        let can_open = OpenOptions::new().read(true).write(true).open(&path).is_ok();
        out.push(HidNode { path, vid, pid, name, command_channel, can_open, firmware });
    }
    out.sort_by(|a, b| b.command_channel.cmp(&a.command_channel).then(a.path.cmp(&b.path)));
    out
}

fn wait(fd: i32, events: i16, ms: i32) -> bool {
    let mut p = libc::pollfd { fd, events, revents: 0 };
    let r = unsafe { libc::poll(&mut p, 1, ms) };
    r > 0 && (p.revents & events) != 0
}

pub struct Keyboard {
    file: File,
    pub chunk: usize,
    write_timeout: i32,
    skipped: usize,
}

impl Keyboard {
    pub fn open(node: &HidNode) -> std::io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(&node.path)?;
        let (chunk, write_timeout) = if node.is_dongle() {
            (protocol::CHUNK_DONGLE, WRITE_TIMEOUT_DONGLE_MS)
        } else {
            (protocol::CHUNK_WIRED, WRITE_TIMEOUT_WIRED_MS)
        };
        Ok(Self { file, chunk, write_timeout, skipped: 0 })
    }

    /// Throw away reports already waiting (key presses, late replies) before a new command.
    fn drain(&mut self) -> usize {
        let fd = self.file.as_raw_fd();
        let mut n = 0;
        let mut buf = [0u8; 256];
        while wait(fd, libc::POLLIN, 0) && self.file.read(&mut buf).is_ok() {
            n += 1;
        }
        n
    }

    /// Write one report and wait up to 5 s for its reply.
    ///
    /// On Windows the original reads from the vendor collection only (col04), so every report
    /// it gets is a reply. Linux's hidraw node is the whole interface: it also carries the
    /// keyboard's own key presses (report ids 1, 2, 3, 6) and replies to other programs, so
    /// those are skipped. A reply echoes the command, length and offset (bytes 3–6).
    pub fn transfer(&mut self, packet: &[u8; REPORT_SIZE]) -> Result<[u8; REPORT_SIZE], String> {
        self.skipped += self.drain();
        let fd = self.file.as_raw_fd();
        if !wait(fd, libc::POLLOUT, self.write_timeout) {
            return Err("timed out writing to the keyboard".into());
        }
        self.file.write_all(packet).map_err(|e| format!("write failed: {e}"))?;
        let deadline = Instant::now() + Duration::from_millis(READ_TIMEOUT_MS as u64);
        let mut wrong_replies = 0;
        loop {
            let left = deadline.saturating_duration_since(Instant::now()).as_millis() as i32;
            if left <= 0 || !wait(fd, libc::POLLIN, left) {
                break;
            }
            let mut buf = [0u8; REPORT_SIZE];
            let n = self.file.read(&mut buf).map_err(|e| format!("read failed: {e}"))?;
            if n < protocol::HEADER || buf[0] != protocol::REPORT_ID {
                self.skipped += 1; // key press or other report on the same interface
                continue;
            }
            if buf[3..7] != packet[3..7] {
                wrong_replies += 1; // reply to something else
                self.skipped += 1;
                continue;
            }
            if let Some(err) = protocol::reply_error(&buf) {
                return Err(err.into());
            }
            return Ok(buf);
        }
        Err(format!(
            "the keyboard did not answer command 0x{:02x} within {} s ({} other reports seen)",
            packet[3],
            READ_TIMEOUT_MS / 1000,
            wrong_replies
        ))
    }

    /// Reports skipped since the last call (for the USB log).
    pub fn take_skipped(&mut self) -> usize {
        std::mem::take(&mut self.skipped)
    }

    /// Read the settings block of a profile back (sub_49ec50: cmd 0x05, 0x31 bytes at profile × 0x40).
    pub fn read_config(&mut self, profile: u8, log: &mut Vec<String>) -> Result<Vec<u8>, String> {
        self.send_all(&[protocol::simple(protocol::CMD_BEGIN)], log)?;
        let mut out = Vec::new();
        for (n, p) in protocol::reads_at(protocol::CMD_READ_CONFIG, profile as u16 * protocol::PROFILE_STRIDE, protocol::CONFIG_LEN, self.chunk) {
            log.push(format!("→ {}", protocol::hex(&p[..16])));
            let r = self.transfer(&p)?;
            log.push(format!("← {}", protocol::hex(&r[..16])));
            out.extend_from_slice(&r[protocol::HEADER..protocol::HEADER + n]);
        }
        self.send_all(&[protocol::simple(protocol::CMD_END)], log)?;
        Ok(out)
    }

    pub fn send_all(&mut self, packets: &[[u8; REPORT_SIZE]], log: &mut Vec<String>) -> Result<(), String> {
        for p in packets {
            log.push(format!("→ {}", protocol::hex(&p[..16])));
            let r = self.transfer(p);
            let skipped = self.take_skipped();
            if skipped > 0 {
                log.push(format!("  (skipped {skipped} other report(s): key presses or replies to another program)"));
            }
            let r = r?;
            log.push(format!("← {}", protocol::hex(&r[..16])));
        }
        Ok(())
    }

    fn read(&mut self, cmd: u8, len: usize, log: &mut Vec<String>) -> Result<Vec<u8>, String> {
        let mut out = Vec::with_capacity(len);
        for (n, p) in protocol::reads(cmd, len, self.chunk) {
            log.push(format!("→ {}", protocol::hex(&p[..16])));
            let r = self.transfer(&p)?;
            log.push(format!("← {}", protocol::hex(&r[..16])));
            out.extend_from_slice(&r[protocol::HEADER..protocol::HEADER + n]);
        }
        Ok(out)
    }

    /// The original's connect sequence (sub_49eab0): begin, device info, the keyboard's
    /// own key table (cmd 0x07, N*3 bytes), LED numbers (cmd 0x1B, N bytes), end.
    pub fn read_tables(&mut self, log: &mut Vec<String>) -> Result<DeviceTables, String> {
        self.send_all(&[protocol::simple(protocol::CMD_BEGIN)], log)?;
        let mut t = DeviceTables { info: self.read(protocol::CMD_READ_INFO, protocol::READ_INFO_LEN, log)?, ..Default::default() };
        let n = t.keys();
        if n == 0 || n * 3 > protocol::KEYMAP_MAX {
            return Err(format!("keyboard reported {n} keys"));
        }
        t.keymap = self.read(protocol::CMD_READ_KEYMAP, n * 3, log)?;
        t.ledmap = self.read(protocol::CMD_READ_LEDMAP, n, log)?;
        self.send_all(&[protocol::simple(protocol::CMD_END)], log)?;
        t.build_slots(&data::DEFAULT_ASSIGN);
        Ok(t)
    }
}

/// udev rules (one line per USB id) that let a normal user open the keyboard.
pub fn udev_rule(ids: &[(u16, u16)]) -> String {
    ids.iter()
        .map(|(vid, pid)| {
            format!(
                "SUBSYSTEM==\"hidraw\", ATTRS{{idVendor}}==\"{vid:04x}\", ATTRS{{idProduct}}==\"{pid:04x}\", MODE=\"0660\", TAG+=\"uaccess\""
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// One command that installs the rules and reloads udev.
pub fn udev_install_command(ids: &[(u16, u16)]) -> String {
    format!(
        "printf '%s\\n' '{}' | sudo tee /etc/udev/rules.d/70-redragon.rules && sudo udevadm control --reload && sudo udevadm trigger",
        udev_rule(ids).replace('\n', "' '")
    )
}

#[cfg(test)]
mod tests {
    use super::has_command_channel;

    // Report descriptors read from the person's keyboard (USB 320f:511b).
    const IF1_VENDOR_PART: &[u8] = &[
        0x05, 0x0c, 0x09, 0x01, 0xa1, 0x01, 0x85, 0x03, 0x15, 0x00, 0x26, 0xff, 0x1f, 0x19, 0x00, 0x2a,
        0xff, 0x1f, 0x75, 0x10, 0x95, 0x01, 0x81, 0x00, 0xc0, 0x06, 0x1c, 0xff, 0x09, 0x92, 0xa1, 0x01,
        0x85, 0x04, 0x19, 0x00, 0x2a, 0xff, 0x00, 0x15, 0x00, 0x26, 0xff, 0x00, 0x75, 0x08, 0x95, 0x3f,
        0x91, 0x00, 0x19, 0x00, 0x2a, 0xff, 0x00, 0x81, 0x00, 0xc0,
    ];
    const IF2: &[u8] = &[
        0x06, 0xef, 0xff, 0x09, 0x00, 0xa1, 0x01, 0x85, 0x05, 0x15, 0x00, 0x26, 0xff, 0x00, 0x95, 0x3f,
        0x75, 0x08, 0x09, 0x01, 0x81, 0x02, 0x09, 0x02, 0x91, 0x02, 0x09, 0x03, 0xb1, 0x02, 0xc0,
    ];
    const IF0_BOOT: &[u8] = &[
        0x05, 0x01, 0x09, 0x06, 0xa1, 0x01, 0x05, 0x07, 0x19, 0xe0, 0x29, 0xe7, 0x15, 0x00, 0x25, 0x01,
        0x75, 0x01, 0x95, 0x08, 0x81, 0x02, 0x95, 0x05, 0x75, 0x01, 0x05, 0x08, 0x19, 0x01, 0x29, 0x05,
        0x91, 0x02, 0xc0,
    ];

    #[test]
    fn finds_report_4_channel() {
        assert!(has_command_channel(IF1_VENDOR_PART));
        assert!(!has_command_channel(IF2));
        assert!(!has_command_channel(IF0_BOOT));
    }
}
