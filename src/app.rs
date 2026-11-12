//! The window: model dashboard, then lighting / keys / macros / settings pages.

use crate::data::{self, KeyCap, LightMode};
use crate::device::{self, HidNode, Keyboard};
use crate::protocol::{self, Clock, ConfigBlock, DeviceTables, Macro, MacroEvent};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Margin, Pos2, Rect, RichText, Sense, Stroke,
    StrokeKind, Vec2,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

const BG: Color32 = Color32::from_rgb(0x1b, 0x1d, 0x1f);
const PANEL: Color32 = Color32::from_rgb(0x24, 0x27, 0x29);
const PANEL_HI: Color32 = Color32::from_rgb(0x2e, 0x32, 0x35);
const EDGE: Color32 = Color32::from_rgb(0x3d, 0x42, 0x45);
const ACCENT: Color32 = Color32::from_rgb(0xe0, 0x1b, 0x24);
const ACCENT_DIM: Color32 = Color32::from_rgb(0x7a, 0x14, 0x18);
const TEXT: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xe8);
const MUTED: Color32 = Color32::from_rgb(0x9a, 0xa0, 0xa4);
const OK: Color32 = Color32::from_rgb(0x3c, 0xc8, 0x6e);
const KEYCAP: Color32 = Color32::from_rgb(0x2a, 0x2c, 0x2e);

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Lighting {
    pub mode: u8,
    pub brightness: u8,
    pub speed: u8,
    pub direction: u8,
    pub multicolor: bool,
    pub color: [u8; 3],
    pub surmount_mode: u8,
    pub custom_group: u8,
}

impl Default for Lighting {
    fn default() -> Self {
        // DefaultData/Keyboard.json LightInfo
        Self {
            mode: 10,
            brightness: 5,
            speed: 2,
            direction: 0,
            multicolor: true,
            color: [255, 255, 255],
            surmount_mode: 0,
            custom_group: 0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct System {
    pub polling: u8,
    pub win_lock: bool,
    pub w_swap: bool,
    pub six_key: bool,
    pub debounce: u8,
}

impl Default for System {
    fn default() -> Self {
        // DefaultData/Keyboard.json SystemStages
        Self { polling: 0, win_lock: false, w_swap: false, six_key: true, debounce: 0 }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub light: Lighting,
    pub system: System,
    pub keys: Vec<u32>,
    /// Per-key colours for the PC-driven mode, one per key slot.
    #[serde(default)]
    pub key_colors: Vec<[u8; 3]>,
}

impl Profile {
    fn new(i: usize) -> Self {
        Self {
            name: format!("Profile {}", i + 1),
            light: Lighting::default(),
            system: System::default(),
            keys: data::DEFAULT_ASSIGN.to_vec(),
            key_colors: vec![[255, 255, 255]; data::KEY_SLOTS],
        }
    }

    /// Older saves had 126 key slots; pad to the original's 170.
    fn fix_sizes(&mut self) {
        while self.keys.len() < data::KEY_SLOTS {
            self.keys.push(data::DEFAULT_ASSIGN[self.keys.len()]);
        }
        self.key_colors.resize(data::KEY_SLOTS, [255, 255, 255]);
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Saved {
    pub model: usize,
    pub custom_vid: String,
    pub custom_pid: String,
    pub active: usize,
    pub profiles: Vec<Profile>,
    pub macros: Vec<Macro>,
    #[serde(default)]
    pub screen: ScreenClock,
}

/// Screen clock choices (app-wide, not per profile).
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ScreenClock {
    /// Send the hour as 1..12 (the original only sends 0..23).
    pub hour12: bool,
    /// Set the clock on connect and every minute while connected.
    pub auto_sync: bool,
}

impl Default for ScreenClock {
    fn default() -> Self {
        Self { hour12: false, auto_sync: true }
    }
}

pub const CLOCK_RESYNC_SECONDS: f64 = 60.0;

impl Default for Saved {
    fn default() -> Self {
        Self {
            model: 0,
            custom_vid: "320f".into(),
            custom_pid: "".into(),
            active: 0,
            profiles: (0..data::PROFILES).map(Profile::new).collect(),
            macros: vec![Macro { name: "Macro 1".into(), events: vec![] }],
            screen: ScreenClock::default(),
        }
    }
}

fn config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("k708-control").join("settings.json")
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Dashboard,
    Device,
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Lighting,
    Keys,
    Macros,
    Settings,
    Screen,
    Log,
}

struct JobResult {
    label: String,
    result: Result<String, String>,
    log: Vec<String>,
    tables: Option<DeviceTables>,
    /// Clock read back from the keyboard's settings block.
    keyboard_clock: Option<Clock>,
}

pub struct ControlApp {
    saved: Saved,
    page: Page,
    tab: Tab,
    nodes: Vec<HidNode>,
    others: Vec<HidNode>,
    show_udev: bool,
    last_scan: f64,
    node: Option<usize>,
    keyboard: Arc<Mutex<Option<Keyboard>>>,
    connected: bool,
    busy: bool,
    status: String,
    log: Vec<String>,
    tx: Sender<JobResult>,
    rx: Receiver<JobResult>,
    selected_key: Option<usize>,
    remap_mods: u8,
    remap_usage: u8,
    macro_sel: usize,
    recording: bool,
    last_event_time: f64,
    fixed_delay: Option<u16>,
    shot: Option<PathBuf>,
    frames: u32,
    shot_requested: bool,
    started: f64,
    tables: Option<DeviceTables>,
    /// Clock last read back from the keyboard, with the egui time it was read.
    keyboard_clock: Option<(Clock, f64)>,
    chunk: usize,
    painting: bool,
    /// Seconds between the chosen screen time and the system time (0 = system time).
    clock_offset: i64,
    /// Date and time typed in the Screen clock tab: year, month, day, hour, minute, second.
    custom_time: [u32; 6],
    last_clock_sync: f64,
    /// egui time of the current frame.
    frames_time: f64,
    /// A second copy was started: bring this window to the front (sub_480a30 broadcast).
    show_requests: Option<Receiver<()>>,
}

impl ControlApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        shot: Option<PathBuf>,
        start: Option<String>,
        show_requests: Option<Receiver<()>>,
    ) -> Self {
        style(&cc.egui_ctx);
        let mut saved: Saved = if shot.is_some() {
            Saved::default()
        } else {
            std::fs::read_to_string(config_path())
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default()
        };
        saved.profiles.iter_mut().for_each(Profile::fix_sizes);
        let (tx, rx) = channel();
        let mut app = Self {
            saved,
            page: Page::Dashboard,
            tab: Tab::Lighting,
            nodes: Vec::new(),
            others: Vec::new(),
            show_udev: false,
            last_scan: 0.0,
            node: None,
            keyboard: Arc::new(Mutex::new(None)),
            connected: false,
            busy: false,
            status: "Choose your keyboard model, then connect.".into(),
            log: Vec::new(),
            tx,
            rx,
            selected_key: None,
            remap_mods: 0,
            remap_usage: 0x04,
            macro_sel: 0,
            recording: false,
            last_event_time: 0.0,
            fixed_delay: None,
            shot,
            frames: 0,
            shot_requested: false,
            started: 0.0,
            tables: None,
            chunk: protocol::CHUNK_WIRED,
            painting: false,
            clock_offset: 0,
            custom_time: {
                let c = Self::local_clock(0);
                [c.year, c.month, c.day, c.hour, c.min, c.sec]
            },
            last_clock_sync: 0.0,
            frames_time: 0.0,
            keyboard_clock: None,
            show_requests,
        };
        app.rescan();
        if let Some(s) = start.as_deref() {
            app.page = Page::Device;
            app.tab = match s {
                "keys" => Tab::Keys,
                "macros" => Tab::Macros,
                "settings" => Tab::Settings,
                "screen" => Tab::Screen,
                "log" => Tab::Log,
                _ => Tab::Lighting,
            };
            if app.tab == Tab::Keys {
                app.selected_key = Some(44);
                let a = app.profile().keys[44];
                app.remap_mods = ((a >> 8) & 0xff) as u8;
                app.remap_usage = (a & 0xff) as u8;
            }
        }
        app
    }

    fn model(&self) -> data::Model {
        data::MODELS[self.saved.model.min(data::MODELS.len() - 1)]
    }

    fn ids(&self) -> Vec<(u16, u16)> {
        let m = self.model();
        if m.verified {
            m.ids.to_vec()
        } else {
            vec![(
                u16::from_str_radix(self.saved.custom_vid.trim(), 16).unwrap_or(0),
                u16::from_str_radix(self.saved.custom_pid.trim(), 16).unwrap_or(0),
            )]
        }
    }

    /// Ids to put in the permission rule: the model's ids plus any keyboard actually found.
    fn rule_ids(&self) -> Vec<(u16, u16)> {
        let mut ids = self.ids();
        for n in self.nodes.iter().chain(self.others.iter()) {
            if !ids.contains(&(n.vid, n.pid)) {
                ids.push((n.vid, n.pid));
            }
        }
        ids
    }

    fn rescan(&mut self) {
        let ids = self.ids();
        let all = device::scan();
        self.nodes = all.iter().filter(|n| ids.contains(&(n.vid, n.pid))).cloned().collect();
        // Redragon-looking devices that don't match the chosen model, offered as a shortcut.
        self.others = all
            .into_iter()
            .filter(|n| !ids.contains(&(n.vid, n.pid)) && n.looks_redragon() && n.command_channel)
            .collect();
        let keep = self.node.and_then(|i| self.nodes.get(i)).map(|n| n.path.clone());
        self.node = keep
            .and_then(|p| self.nodes.iter().position(|n| n.path == p))
            .or(if self.nodes.is_empty() { None } else { Some(0) });
    }

    fn use_other(&mut self, i: usize) {
        let n = self.others[i].clone();
        self.saved.model = data::MODELS.iter().position(|m| !m.verified).unwrap_or(0);
        self.saved.custom_vid = format!("{:04x}", n.vid);
        self.saved.custom_pid = format!("{:04x}", n.pid);
        self.save();
        self.rescan();
        self.node = self.nodes.iter().position(|m| m.path == n.path);
    }

    fn save(&self) {
        if self.shot.is_some() {
            return;
        }
        let p = config_path();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(t) = serde_json::to_string_pretty(&self.saved) {
            let _ = std::fs::write(p, t);
        }
    }

    fn profile(&self) -> &Profile {
        &self.saved.profiles[self.saved.active]
    }

    fn profile_mut(&mut self) -> &mut Profile {
        let i = self.saved.active;
        &mut self.saved.profiles[i]
    }

    /// Run packets on a worker thread so the window never freezes.
    fn run(&mut self, label: &str, packets: Vec<[u8; protocol::REPORT_SIZE]>) {
        if !self.connected {
            self.status = "Not connected: the change is saved and will be sent after connecting.".into();
            return;
        }
        if self.busy {
            self.status = "Still sending the last change…".into();
            return;
        }
        self.busy = true;
        self.status = format!("{label}…");
        let kb = self.keyboard.clone();
        let tx = self.tx.clone();
        let label = label.to_string();
        std::thread::spawn(move || {
            let mut log = Vec::new();
            let result = match kb.lock() {
                Ok(mut g) => match g.as_mut() {
                    Some(k) => k.send_all(&packets, &mut log).map(|_| "done".to_string()),
                    None => Err("not connected".into()),
                },
                Err(_) => Err("device lock failed".into()),
            };
            let _ = tx.send(JobResult { label, result, log, tables: None, keyboard_clock: None });
        });
    }

    fn connect(&mut self) {
        let Some(i) = self.node else {
            self.status = "No matching keyboard found. Plug it in with the USB cable and press Rescan.".into();
            return;
        };
        let node = self.nodes[i].clone();
        match Keyboard::open(&node) {
            Ok(k) => {
                self.chunk = k.chunk;
                *self.keyboard.lock().unwrap() = Some(k);
                self.connected = true;
                self.tables = None;
                self.last_clock_sync = f64::MIN;
                self.page = Page::Device;
                self.status = format!("Opened {}", node.path.display());
                self.busy = true;
                let kb = self.keyboard.clone();
                let tx = self.tx.clone();
                // sub_478530: firmware below 1.06 on the wired keyboard gets an update prompt.
                let old_fw = node.pid == data::MODELS[0].ids[0].1
                    && node.firmware.is_some_and(|v| v < data::MIN_FIRMWARE);
                std::thread::spawn(move || {
                    let mut log = Vec::new();
                    let (result, tables) = match kb.lock().unwrap().as_mut() {
                        Some(k) => match k.read_tables(&mut log) {
                            Ok(t) => {
                                let mapped = t.slot_hw.iter().filter(|s| s.is_some()).count();
                                let mut msg = format!("{} keys, {} key slots matched, {} bytes of macro space", t.keys(), mapped, t.macro_space());
                                if old_fw {
                                    msg.push_str(". The current version is too low, please upgrade to the latest version first!");
                                }
                                (Ok(msg), Some(t))
                            }
                            Err(e) => (Err(e), None),
                        },
                        None => (Err("not connected".into()), None),
                    };
                    let _ = tx.send(JobResult { label: "Reading keyboard".into(), result, log, tables, keyboard_clock: None });
                });
            }
            Err(e) => {
                self.show_udev = e.kind() == std::io::ErrorKind::PermissionDenied;
                self.status = if self.show_udev {
                    format!(
                        "Found the keyboard ({:04x}:{:04x}) but Linux only lets root open {}. Run the permission command shown under Connect once, then press Rescan.",
                        node.vid,
                        node.pid,
                        node.path.display()
                    )
                } else {
                    format!("Could not open {}: {e}", node.path.display())
                };
            }
        }
    }

    fn disconnect(&mut self) {
        *self.keyboard.lock().unwrap() = None;
        self.connected = false;
        self.tables = None;
        self.status = "Disconnected.".into();
    }

    /// Local time now, shifted by `offset` seconds (same tm fields as sub_4a0990).
    fn local_clock(offset: i64) -> Clock {
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        let now = unsafe { libc::time(std::ptr::null_mut()) } + offset as libc::time_t;
        unsafe { libc::localtime_r(&now, &mut tm) };
        Clock {
            sec: tm.tm_sec as u32,
            min: tm.tm_min as u32,
            hour: tm.tm_hour as u32,
            weekday: tm.tm_wday as u32,
            day: tm.tm_mday as u32,
            month: tm.tm_mon as u32 + 1,
            year: tm.tm_year as u32 + 1900,
        }
    }

    /// The time shown on the screen: system time or the chosen time, still running.
    fn screen_time(&self) -> Clock {
        Self::local_clock(self.clock_offset)
    }

    /// What goes into the settings block: the screen time, in 12-hour form if chosen.
    fn clock(&self) -> Clock {
        let c = self.screen_time();
        if self.saved.screen.hour12 { c.as_12_hour() } else { c }
    }

    /// Seconds from now to a typed local date and time.
    fn offset_to(t: [u32; 6]) -> i64 {
        let c = Clock::from_date_time(t[0], t[1], t[2], t[3], t[4], t[5]);
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        tm.tm_year = c.year as i32 - 1900;
        tm.tm_mon = c.month as i32 - 1;
        tm.tm_mday = c.day as i32;
        tm.tm_hour = c.hour as i32;
        tm.tm_min = c.min as i32;
        tm.tm_sec = c.sec as i32;
        tm.tm_isdst = -1;
        let target = unsafe { libc::mktime(&mut tm) };
        let now = unsafe { libc::time(std::ptr::null_mut()) };
        (target - now) as i64
    }

    /// Send the settings block with the screen time, then read the block back to show
    /// what the keyboard stored.
    fn sync_clock(&mut self, label: &str) {
        self.last_clock_sync = self.frames_time;
        self.save();
        let packets = protocol::config_write(&self.config_block(), self.clock(), self.chunk);
        self.clock_job(label, packets);
    }

    /// Only read the keyboard's stored clock (a test that changes nothing).
    fn read_keyboard_clock(&mut self) {
        self.clock_job("Reading the keyboard clock", vec![]);
    }

    fn clock_job(&mut self, label: &str, packets: Vec<[u8; protocol::REPORT_SIZE]>) {
        if !self.connected {
            self.status = "Connect the keyboard first.".into();
            return;
        }
        if self.busy {
            self.status = "Still sending the last change…".into();
            return;
        }
        self.busy = true;
        self.status = format!("{label}…");
        let kb = self.keyboard.clone();
        let tx = self.tx.clone();
        let label = label.to_string();
        let profile = self.saved.active as u8;
        std::thread::spawn(move || {
            let mut log = Vec::new();
            let mut keyboard_clock = None;
            let result = match kb.lock() {
                Ok(mut g) => match g.as_mut() {
                    Some(k) => k
                        .send_all(&packets, &mut log)
                        .and_then(|_| k.read_config(profile, &mut log))
                        .map(|block| {
                            keyboard_clock = protocol::block_clock(&block);
                            match keyboard_clock {
                                Some(c) => format!("keyboard clock is {:02}:{:02}:{:02}, {}/{}/{}", c.hour, c.min, c.sec, c.day, c.month, c.year),
                                None => "done".into(),
                            }
                        }),
                    None => Err("not connected".into()),
                },
                Err(_) => Err("device lock failed".into()),
            };
            let _ = tx.send(JobResult { label, result, log, tables: None, keyboard_clock });
        });
    }

    fn config_block(&self) -> ConfigBlock {
        let p = self.profile();
        ConfigBlock {
            profile: self.saved.active as u8,
            mode: p.light.mode,
            brightness: p.light.brightness,
            speed: p.light.speed,
            direction: p.light.direction,
            multicolor: p.light.multicolor,
            color: p.light.color,
            surmount_mode: p.light.surmount_mode,
            w_swap: p.system.w_swap,
            six_key: p.system.six_key,
            win_lock: p.system.win_lock,
            polling: p.system.polling,
            custom_group: p.light.custom_group,
            frame_interval: data::FRAME_INTERVAL_MS,
            debounce: p.system.debounce,
        }
    }

    fn send_config(&mut self, label: &str) {
        self.save();
        let mut packets = protocol::config_write(&self.config_block(), self.clock(), self.chunk);
        if matches!(self.profile().light.mode, 21 | 22) {
            packets.extend(self.colour_frame());
        }
        self.run(label, packets);
    }

    fn colour_frame(&self) -> Vec<[u8; protocol::REPORT_SIZE]> {
        let Some(t) = &self.tables else { return vec![] };
        let colors: Vec<[u8; 4]> = self.profile().key_colors.iter().map(|c| [c[0], c[1], c[2], 255]).collect();
        protocol::realtime_frame(t, &colors, self.chunk)
    }

    fn send_colours(&mut self) {
        self.save();
        if self.tables.is_none() {
            self.status = "Connect first: per-key colours need the keyboard's LED numbers.".into();
            return;
        }
        let packets = self.colour_frame();
        self.run("Sending key colours", packets);
    }

    fn send_keys(&mut self) {
        self.save();
        let Some(t) = &self.tables else {
            self.status = "Saved. Key changes are sent after connecting (the app needs the keyboard's own key table first).".into();
            return;
        };
        let packets = protocol::keymap_write(t, &self.profile().keys, self.chunk);
        self.run("Sending key layout", packets);
    }

    fn macros_fit(&mut self) -> bool {
        let size = protocol::macro_blob(&self.saved.macros).len();
        match &self.tables {
            Some(t) if t.macro_space() > 0 && size > t.macro_space() => {
                self.status = format!("Macros take {size} bytes but the keyboard has room for {}. Remove some actions.", t.macro_space());
                false
            }
            _ => true,
        }
    }

    fn send_macros(&mut self) {
        self.save();
        if !self.macros_fit() {
            return;
        }
        let packets = protocol::macro_write(&self.saved.macros, self.chunk);
        self.run("Sending macros", packets);
    }

    fn send_everything(&mut self) {
        self.save();
        let mut packets = protocol::config_write(&self.config_block(), self.clock(), self.chunk);
        if let Some(t) = &self.tables {
            packets.extend(protocol::keymap_write(t, &self.profile().keys, self.chunk));
        }
        if self.macros_fit() {
            packets.extend(protocol::macro_write(&self.saved.macros, self.chunk));
        }
        if matches!(self.profile().light.mode, 21 | 22) {
            packets.extend(self.colour_frame());
        }
        self.run("Sending the whole profile", packets);
    }
}

fn style(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = Color32::from_rgb(0x14, 0x15, 0x16);
    v.selection.bg_fill = ACCENT;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.widgets.inactive.weak_bg_fill = PANEL_HI;
    v.widgets.inactive.bg_fill = PANEL_HI;
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(0x3a, 0x3f, 0x42);
    v.widgets.active.weak_bg_fill = ACCENT_DIM;
    v.slider_trailing_fill = true;
    v.override_text_color = Some(TEXT);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = Vec2::new(8.0, 7.0);
        s.spacing.button_padding = Vec2::new(10.0, 5.0);
    });
}

fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(PANEL)
        .stroke(Stroke::new(1.0, EDGE))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        });
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::new(4.0, 16.0), Sense::hover());
        ui.painter().rect_filled(r, CornerRadius::same(1), ACCENT);
        ui.label(RichText::new(text).size(15.0).strong().color(TEXT));
    });
}

fn accent_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(text).color(Color32::WHITE).strong())
            .fill(ACCENT)
            .corner_radius(CornerRadius::same(4))
            .min_size(Vec2::new(0.0, 28.0)),
    )
}

fn hsv(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let c = v * s;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let (r, g, b) = match h as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

fn hash(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xffff) as f32 / 65535.0
}

/// Preview colour of one key for the chosen effect (a drawing, not the keyboard's firmware).
fn preview_color(l: &Lighting, mode: LightMode, k: &KeyCap, t: f32) -> Color32 {
    let nx = (k.x - data::LAYOUT_X0) / (data::LAYOUT_X1 - data::LAYOUT_X0);
    let ny = (k.y - data::LAYOUT_Y0) / (data::LAYOUT_Y1 - data::LAYOUT_Y0);
    let speed = 0.25 + l.speed as f32 * 0.35;
    let base = [l.color[0] as f32 / 255.0, l.color[1] as f32 / 255.0, l.color[2] as f32 / 255.0];
    let rainbow = |p: f32| hsv(p, 1.0, 1.0);
    let colorize = |p: f32| if l.multicolor || !mode.uses_color { rainbow(p) } else { base };
    let dir_pos = match l.direction {
        1 => 1.0 - nx,
        2 => ny,
        3 => 1.0 - ny,
        4 => 1.0 - ((nx - 0.5).hypot(ny - 0.5) * 2.0),
        5 => (nx - 0.5).hypot(ny - 0.5) * 2.0,
        6 => (ny - 0.5).atan2(nx - 0.5) / std::f32::consts::TAU,
        7 => -(ny - 0.5).atan2(nx - 0.5) / std::f32::consts::TAU,
        _ => nx,
    };
    let (c, level) = match mode.value {
        1 => (colorize(nx), 1.0),
        2 => (colorize(t * 0.05), 0.5 + 0.5 * (t * speed * 2.0).sin()),
        3 => (rainbow(t * speed * 0.15), 1.0),
        4 | 9 | 10 | 12 | 13 | 14 => (colorize(dir_pos - t * speed * 0.3), 1.0),
        16 => {
            let band = ((dir_pos - (t * speed * 0.4).fract()).abs() < 0.08) as i32 as f32;
            (colorize(dir_pos), 0.15 + 0.85 * band)
        }
        5 => {
            let col = (nx * 14.0) as u32;
            let drop = (ny - (t * speed * 0.6 + hash(col)).fract()).abs();
            (colorize(hash(col)), (1.0 - drop * 4.0).clamp(0.08, 1.0))
        }
        6 | 15 => {
            let r = (nx - 0.5).hypot((ny - 0.5) * 0.4);
            let w = ((r - (t * speed * 0.4).fract()).abs() * 8.0).min(1.0);
            (colorize(r), 1.0 - w * 0.85)
        }
        7 | 11 => {
            let tw = (t * speed * 1.5 + hash(k.index as u32) * 6.28).sin();
            (colorize(hash(k.index as u32 + 7)), (tw * 0.5 + 0.5).powf(3.0))
        }
        8 | 17 | 18 => (colorize(nx), 0.2 + 0.8 * hash((t * speed * 2.0) as u32 * 131 + k.index as u32).powf(6.0)),
        19 => (rainbow(hash(k.index as u32)), 1.0),
        20 => ([0.0; 3], 0.0),
        21 => {
            let h = 0.5 + 0.45 * (nx * 9.0 + t * 3.0).sin() * (t * 1.7).cos();
            (colorize(nx), if 1.0 - ny < h { 1.0 } else { 0.1 })
        }
        _ => (rainbow(nx * 0.6 + t * 0.03), 0.85),
    };
    let b = l.brightness as f32 / protocol::BRIGHTNESS_MAX as f32 * level;
    Color32::from_rgb((c[0] * b * 255.0) as u8, (c[1] * b * 255.0) as u8, (c[2] * b * 255.0) as u8)
}

/// Draws the 81-key layout. Returns the clicked key index, if any.
fn keyboard_view(
    ui: &mut egui::Ui,
    light: &Lighting,
    painted: Option<&[[u8; 3]]>,
    keys: Option<&[u32]>,
    selected: Option<usize>,
    t: f32,
    height: f32,
) -> Option<usize> {
    let w = ui.available_width();
    let lw = data::LAYOUT_X1 - data::LAYOUT_X0;
    let lh = data::LAYOUT_Y1 - data::LAYOUT_Y0;
    let scale = (w / (lw + 60.0)).min(height / lh);
    let size = Vec2::new((lw + 60.0) * scale, lh * scale);
    let (outer, _) = ui.allocate_exact_size(Vec2::new(w, size.y + 8.0), Sense::hover());
    let origin = Pos2::new(outer.center().x - size.x / 2.0, outer.top() + 4.0);
    let painter = ui.painter_at(outer);
    let body = Rect::from_min_size(origin, size);
    painter.rect_filled(body, CornerRadius::same(14), Color32::from_rgb(0x10, 0x11, 0x12));
    painter.rect_stroke(body, CornerRadius::same(14), Stroke::new(1.5, EDGE), StrokeKind::Inside);
    let mode = data::mode_by_value(light.mode);
    let to_screen = |x: f32, y: f32| {
        Pos2::new(origin.x + (x - data::LAYOUT_X0 + 4.0) * scale, origin.y + (y - data::LAYOUT_Y0) * scale)
    };
    // screen and knob (top right, as on the K708)
    let scr = Rect::from_min_size(to_screen(492.0, 22.0), Vec2::new(34.0 * scale, 24.0 * scale));
    painter.rect_filled(scr, CornerRadius::same(3), Color32::from_rgb(0x06, 0x22, 0x3a));
    painter.text(scr.center(), Align2::CENTER_CENTER, "LCD", FontId::proportional(8.0 * scale.max(1.0)), MUTED);
    let knob = to_screen(552.0, 35.0);
    painter.circle_filled(knob, 11.0 * scale, Color32::from_rgb(0x30, 0x33, 0x35));
    painter.circle_stroke(knob, 11.0 * scale, Stroke::new(1.0, EDGE));
    let mut clicked = None;
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let pressed = ui.input(|i| i.pointer.primary_clicked());
    for k in data::LAYOUT.iter() {
        let r = Rect::from_min_size(to_screen(k.x, k.y), Vec2::new(k.w * scale, k.h * scale)).shrink(0.5);
        let glow = match painted.and_then(|p| p.get(k.index)) {
            Some(c) => {
                let b = light.brightness as f32 / protocol::BRIGHTNESS_MAX as f32;
                Color32::from_rgb((c[0] as f32 * b) as u8, (c[1] as f32 * b) as u8, (c[2] as f32 * b) as u8)
            }
            None => preview_color(light, mode, k, t),
        };
        painter.rect_filled(r.expand(1.5 * scale), CornerRadius::same(4), glow.gamma_multiply(0.55));
        painter.rect_filled(r, CornerRadius::same(3), KEYCAP);
        let hovered = pointer.is_some_and(|p| r.contains(p));
        let is_sel = selected == Some(k.index);
        if is_sel || hovered {
            painter.rect_stroke(
                r,
                CornerRadius::same(3),
                Stroke::new(if is_sel { 2.0 } else { 1.0 }, if is_sel { ACCENT } else { MUTED }),
                StrokeKind::Outside,
            );
        }
        let (label, col) = match keys {
            Some(a) => {
                let v = a.get(k.index).copied().unwrap_or(0);
                let def = data::DEFAULT_ASSIGN[k.index];
                if v != def {
                    (data::describe(v), ACCENT)
                } else {
                    (k.label.to_string(), TEXT)
                }
            }
            None => (k.label.to_string(), glow.lerp_to_gamma(Color32::WHITE, 0.55)),
        };
        let fs = (7.5 * scale).clamp(7.0, 13.0);
        painter.text(r.center(), Align2::CENTER_CENTER, label, FontId::proportional(fs), col);
        if hovered && pressed {
            clicked = Some(k.index);
        }
    }
    clicked
}

/// egui key → HID usage, for macro recording.
fn hid_usage(key: egui::Key) -> Option<u8> {
    use egui::Key::*;
    let letters = [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z];
    if let Some(i) = letters.iter().position(|k| *k == key) {
        return Some(0x04 + i as u8);
    }
    let nums = [Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9, Num0];
    if let Some(i) = nums.iter().position(|k| *k == key) {
        return Some(0x1E + i as u8);
    }
    let fkeys = [F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12];
    if let Some(i) = fkeys.iter().position(|k| *k == key) {
        return Some(0x3A + i as u8);
    }
    Some(match key {
        Enter => 0x28,
        Escape => 0x29,
        Backspace => 0x2A,
        Tab => 0x2B,
        Space => 0x2C,
        Minus => 0x2D,
        Equals => 0x2E,
        OpenBracket => 0x2F,
        CloseBracket => 0x30,
        Backslash => 0x31,
        Semicolon => 0x33,
        Quote => 0x34,
        Backtick => 0x35,
        Comma => 0x36,
        Period => 0x37,
        Slash => 0x38,
        Insert => 0x49,
        Home => 0x4A,
        PageUp => 0x4B,
        Delete => 0x4C,
        End => 0x4D,
        PageDown => 0x4E,
        ArrowRight => 0x4F,
        ArrowLeft => 0x50,
        ArrowDown => 0x51,
        ArrowUp => 0x52,
        _ => return None,
    })
}

impl ControlApp {
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("top")
            .exact_size(56.0)
            .frame(egui::Frame::new().fill(Color32::from_rgb(0x12, 0x13, 0x14)).inner_margin(Margin::symmetric(18, 10)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(30.0, 30.0), Sense::hover());
                    ui.painter().circle_filled(r.center(), 14.0, ACCENT);
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, "K", FontId::proportional(17.0), Color32::WHITE);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("KEYBOARD CONTROL").size(17.0).strong().color(TEXT));
                        ui.label(RichText::new(format!("Redragon {}  ·  Linux", self.model().name)).size(11.0).color(MUTED));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (txt, col) = if self.connected { ("Connected", OK) } else { ("Not connected", MUTED) };
                        ui.label(RichText::new(txt).color(col));
                        let (dot, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                        ui.painter().circle_filled(dot.center(), 4.5, col);
                        ui.add_space(10.0);
                        if self.page == Page::Device && ui.button("< Dashboard").clicked() {
                            self.page = Page::Dashboard;
                        }
                    });
                });
            });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status")
            .exact_size(30.0)
            .frame(egui::Frame::new().fill(Color32::from_rgb(0x12, 0x13, 0x14)).inner_margin(Margin::symmetric(14, 6)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if self.busy {
                        ui.spinner();
                    }
                    ui.label(RichText::new(&self.status).size(12.0).color(MUTED));
                });
            });
    }

    fn dashboard(&mut self, ui: &mut egui::Ui, t: f32) {
        egui::Panel::left("models")
            .exact_size(330.0)
            .frame(egui::Frame::new().fill(BG).inner_margin(Margin::same(16)))
            .show(ui, |ui| {
                heading(ui, "Choose keyboard model");
                ui.add_space(6.0);
                let mut changed = false;
                for (i, m) in data::MODELS.iter().enumerate() {
                    let sel = self.saved.model == i;
                    let frame = egui::Frame::new()
                        .fill(if sel { PANEL_HI } else { PANEL })
                        .stroke(Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { ACCENT } else { EDGE }))
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(Margin::same(12));
                    let resp = frame
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(Vec2::new(46.0, 30.0), Sense::hover());
                                ui.painter().rect_filled(r, CornerRadius::same(4), Color32::from_rgb(0x10, 0x11, 0x12));
                                for row in 0..4 {
                                    for col in 0..7 {
                                        let p = r.min + Vec2::new(4.0 + col as f32 * 5.6, 4.0 + row as f32 * 5.8);
                                        let c = hsv(col as f32 / 7.0 + t * 0.05, 0.9, if m.verified { 1.0 } else { 0.5 });
                                        ui.painter().rect_filled(
                                            Rect::from_min_size(p, Vec2::splat(4.2)),
                                            CornerRadius::same(1),
                                            Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8),
                                        );
                                    }
                                }
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(m.name).strong().size(14.0));
                                    ui.label(RichText::new(m.subtitle).size(11.0).color(MUTED));
                                    if m.verified {
                                        ui.label(RichText::new(format!("USB {}", data::ids_text(m.ids))).size(11.0).color(MUTED));
                                    }
                                });
                            });
                        })
                        .response
                        .interact(Sense::click());
                    if resp.clicked() && !sel {
                        self.saved.model = i;
                        changed = true;
                    }
                    ui.add_space(4.0);
                }
                if !self.model().verified {
                    card(ui, |ui| {
                        ui.label(RichText::new("USB ids of your keyboard (from lsusb)").size(12.0).color(MUTED));
                        ui.horizontal(|ui| {
                            ui.label("Vendor");
                            changed |= ui.add(egui::TextEdit::singleline(&mut self.saved.custom_vid).desired_width(60.0)).changed();
                            ui.label("Product");
                            changed |= ui.add(egui::TextEdit::singleline(&mut self.saved.custom_pid).desired_width(60.0)).changed();
                        });
                        ui.label(
                            RichText::new("Only the K708-RGB-PRO has been checked against its own software. Other models may use different lighting numbers.")
                                .size(11.0)
                                .color(MUTED),
                        );
                    });
                }
                if changed {
                    self.save();
                    self.rescan();
                }
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(Margin::same(16)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.dashboard_main(ui, t));
            });
    }

    fn dashboard_main(&mut self, ui: &mut egui::Ui, t: f32) {
        {
            {
                card(ui, |ui| {
                    heading(ui, &format!("{} preview", self.model().name));
                    let light = self.profile().light;
                    let painted = matches!(light.mode, 21 | 22).then(|| self.profile().key_colors.clone());
                    keyboard_view(ui, &light, painted.as_deref(), None, None, t, 190.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Effect: {}", data::mode_by_value(light.mode).name)).color(MUTED));
                        ui.label(RichText::new(format!("Brightness {}/5  ·  Speed {}/5", light.brightness, light.speed)).color(MUTED));
                    });
                });
                ui.add_space(8.0);
                card(ui, |ui| {
                    heading(ui, "Connect");
                    let ids = self.ids();
                    if self.nodes.is_empty() {
                        ui.label(RichText::new(format!("No keyboard with USB id {} is plugged in.", data::ids_text(&ids))).color(MUTED));
                        ui.label(RichText::new("Plug in the USB cable or the 2.4G receiver. The list refreshes by itself.").size(12.0).color(MUTED));
                    } else {
                        for (i, n) in self.nodes.iter().enumerate() {
                            let role = if n.command_channel { "settings channel" } else { "typing only" };
                            let access = if n.can_open { "" } else { "  (needs permission)" };
                            let label = format!("{}   {}   {:04x}:{:04x}   {role}{access}", n.path.display(), n.name.trim(), n.vid, n.pid);
                            ui.radio_value(&mut self.node, Some(i), RichText::new(label).color(if n.command_channel { TEXT } else { MUTED }));
                        }
                    }
                    let mut use_other = None;
                    for (i, n) in self.others.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(format!("Found {} ({:04x}:{:04x}) at {}", n.name.trim(), n.vid, n.pid, n.path.display()))
                                    .color(ACCENT),
                            );
                            if ui.button("Use this keyboard").clicked() {
                                use_other = Some(i);
                            }
                        });
                    }
                    if let Some(i) = use_other {
                        self.use_other(i);
                    }
                    let needs_permission = self
                        .node
                        .and_then(|i| self.nodes.get(i))
                        .is_some_and(|n| !n.can_open);
                    ui.horizontal(|ui| {
                        if ui.button("Rescan").clicked() {
                            self.rescan();
                            self.status = format!("Found {} matching device node(s).", self.nodes.len());
                        }
                        if self.connected {
                            if ui.button("Disconnect").clicked() {
                                self.disconnect();
                            }
                            if accent_button(ui, "Open controls").clicked() {
                                self.page = Page::Device;
                            }
                        } else {
                            if accent_button(ui, "Connect").clicked() {
                                self.connect();
                            }
                            if ui.button("Open controls offline").clicked() {
                                self.page = Page::Device;
                            }
                        }
                    });
                    ui.add_space(4.0);
                    let open = needs_permission || self.show_udev;
                    let header = if open {
                        RichText::new("Permission needed: run this once in a terminal").color(ACCENT)
                    } else {
                        RichText::new("Permission help (udev rule)").color(MUTED)
                    };
                    egui::CollapsingHeader::new(header)
                        .id_salt("udev")
                        .open(open.then_some(true))
                        .show(ui, |ui| {
                            let ids = self.rule_ids();
                            let cmd = device::udev_install_command(&ids);
                            ui.label(
                                RichText::new("Linux only lets root open the keyboard's settings channel. This adds a rule so your user can, then unplug and replug the keyboard.")
                                    .size(12.0)
                                    .color(MUTED),
                            );
                            ui.add(
                                egui::TextEdit::multiline(&mut cmd.as_str())
                                    .desired_rows(2)
                                    .desired_width(f32::INFINITY)
                                    .font(egui::TextStyle::Monospace),
                            );
                            if ui.button("Copy command").clicked() {
                                ui.ctx().copy_text(cmd);
                            }
                        });
                });
            }
        }
    }

    fn device_page(&mut self, ui: &mut egui::Ui, t: f32) {
        egui::Panel::left("profiles")
            .exact_size(230.0)
            .frame(egui::Frame::new().fill(Color32::from_rgb(0x17, 0x18, 0x1a)).inner_margin(Margin::same(14)))
            .show(ui, |ui| {
                heading(ui, "Profiles");
                ui.add_space(4.0);
                let mut switch = None;
                for i in 0..self.saved.profiles.len() {
                    let sel = self.saved.active == i;
                    let name = self.saved.profiles[i].name.clone();
                    let b = egui::Button::new(RichText::new(name).color(if sel { Color32::WHITE } else { TEXT }))
                        .fill(if sel { ACCENT } else { PANEL_HI })
                        .min_size(Vec2::new(ui.available_width(), 32.0));
                    if ui.add(b).clicked() && !sel {
                        switch = Some(i);
                    }
                }
                if let Some(i) = switch {
                    self.saved.active = i;
                    self.send_config("Switching profile");
                }
                ui.add_space(6.0);
                ui.label(RichText::new("Name").size(11.0).color(MUTED));
                let i = self.saved.active;
                if ui.text_edit_singleline(&mut self.saved.profiles[i].name).lost_focus() {
                    self.save();
                }
                ui.add_space(8.0);
                if ui.add(egui::Button::new("Copy to next profile").min_size(Vec2::new(ui.available_width(), 26.0))).clicked() {
                    let n = (i + 1) % self.saved.profiles.len();
                    let mut p = self.saved.profiles[i].clone();
                    p.name = self.saved.profiles[n].name.clone();
                    self.saved.profiles[n] = p;
                    self.save();
                    self.status = format!("Copied to {}", self.saved.profiles[n].name);
                }
                if ui.add(egui::Button::new("Reset this profile").min_size(Vec2::new(ui.available_width(), 26.0))).clicked() {
                    let name = self.saved.profiles[i].name.clone();
                    self.saved.profiles[i] = Profile::new(i);
                    self.saved.profiles[i].name = name;
                    self.send_everything();
                }
                ui.add_space(8.0);
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Export").clicked() {
                        let p = config_path().with_file_name("export.json");
                        if let Ok(t) = serde_json::to_string_pretty(&self.saved) {
                            self.status = match std::fs::write(&p, t) {
                                Ok(_) => format!("Exported to {}", p.display()),
                                Err(e) => format!("Export failed: {e}"),
                            };
                        }
                    }
                    if ui.button("Import").clicked() {
                        let p = config_path().with_file_name("export.json");
                        match std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str::<Saved>(&t).ok()) {
                            Some(s) => {
                                self.saved = s;
                                self.save();
                                self.status = format!("Imported {}", p.display());
                            }
                            None => self.status = format!("Nothing to import at {}", p.display()),
                        }
                    }
                });
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    if accent_button(ui, "Send all to keyboard").clicked() {
                        self.send_everything();
                    }
                    ui.label(RichText::new(self.model().name).size(12.0).color(MUTED));
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(Margin::same(14)))
            .show(ui, |ui| {
                let keys = (self.tab == Tab::Keys).then(|| self.profile().keys.clone());
                let light = self.profile().light;
                let pc_driven = matches!(light.mode, 21 | 22);
                let painted = pc_driven.then(|| self.profile().key_colors.clone());
                let clicked = keyboard_view(ui, &light, painted.as_deref(), keys.as_deref(), self.selected_key, t, 200.0);
                if let Some(k) = clicked {
                    if self.tab == Tab::Lighting && pc_driven && self.painting {
                        let c = self.profile().light.color;
                        self.profile_mut().key_colors[k] = c;
                        self.send_colours();
                    } else {
                        self.selected_key = Some(k);
                        if self.tab != Tab::Keys {
                            self.tab = Tab::Keys;
                        }
                        let a = self.profile().keys[k];
                        if a >> 16 == data::TYPE_KEY {
                            self.remap_mods = ((a >> 8) & 0xff) as u8;
                            self.remap_usage = (a & 0xff) as u8;
                        }
                    }
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    for (tab, name) in [
                        (Tab::Lighting, "Light Setting"),
                        (Tab::Keys, "Button Settings"),
                        (Tab::Macros, "Macro Management"),
                        (Tab::Settings, "Other Settings"),
                        (Tab::Screen, "Screen clock"),
                        (Tab::Log, "USB log"),
                    ] {
                        let sel = self.tab == tab;
                        let b = egui::Button::new(RichText::new(name).color(if sel { Color32::WHITE } else { MUTED }))
                            .fill(if sel { ACCENT } else { PANEL })
                            .corner_radius(CornerRadius::same(4))
                            .min_size(Vec2::new(0.0, 28.0));
                        if ui.add(b).clicked() {
                            self.tab = tab;
                        }
                    }
                });
                ui.add_space(4.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.tab {
                    Tab::Lighting => self.lighting_tab(ui),
                    Tab::Keys => self.keys_tab(ui),
                    Tab::Macros => self.macros_tab(ui),
                    Tab::Settings => self.settings_tab(ui),
                    Tab::Screen => self.screen_tab(ui),
                    Tab::Log => self.log_tab(ui),
                });
            });
    }

    fn lighting_tab(&mut self, ui: &mut egui::Ui) {
        let before = self.profile().light;
        card(ui, |ui| {
            heading(ui, "Lighting Pattern");
            let l = &mut self.saved.profiles[self.saved.active].light;
            egui::Grid::new("modes").spacing(Vec2::new(6.0, 6.0)).show(ui, |ui| {
                for (i, m) in data::LIGHT_MODES.iter().enumerate() {
                    let sel = l.mode == m.value;
                    let b = egui::Button::new(RichText::new(m.name).size(12.0).color(if sel { Color32::WHITE } else { TEXT }))
                        .fill(if sel { ACCENT } else { PANEL_HI })
                        .min_size(Vec2::new(112.0, 26.0));
                    if ui.add(b).clicked() {
                        l.mode = m.value;
                    }
                    if i % 6 == 5 {
                        ui.end_row();
                    }
                }
            });
        });
        ui.add_space(6.0);
        let mode = data::mode_by_value(self.profile().light.mode);
        ui.columns(2, |cols| {
            card(&mut cols[0], |ui| {
                heading(ui, "Brightness and speed");
                let l = &mut self.saved.profiles[self.saved.active].light;
                ui.add(egui::Slider::new(&mut l.brightness, 0..=protocol::BRIGHTNESS_MAX).text("Brightness"));
                ui.add(egui::Slider::new(&mut l.speed, 0..=protocol::SPEED_MAX).text("Speed"));
                ui.add_enabled_ui(mode.uses_direction, |ui| {
                    egui::ComboBox::from_label("Direction")
                        .selected_text(data::DIRECTIONS[l.direction as usize % data::DIRECTIONS.len()])
                        .show_ui(ui, |ui| {
                            for (i, d) in data::DIRECTIONS.iter().enumerate() {
                                ui.selectable_value(&mut l.direction, i as u8, *d);
                            }
                        });
                });
                if mode.value == 17 {
                    ui.add(egui::Slider::new(&mut l.surmount_mode, 0..=3).text("Surmount style"));
                }
                if mode.value == 19 {
                    ui.add(egui::Slider::new(&mut l.custom_group, 0..=9).text("Custom group"));
                }
            });
            card(&mut cols[1], |ui| {
                heading(ui, "Color");
                let l = &mut self.saved.profiles[self.saved.active].light;
                ui.add_enabled_ui(mode.uses_color, |ui| {
                    ui.checkbox(&mut l.multicolor, "Colorful (rainbow)");
                    ui.add_enabled_ui(!l.multicolor, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for c in data::PALETTE.iter() {
                                let (r, resp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                                ui.painter().rect_filled(r, CornerRadius::same(3), Color32::from_rgb(c[0], c[1], c[2]));
                                if l.color == *c {
                                    ui.painter().rect_stroke(r, CornerRadius::same(3), Stroke::new(2.0, Color32::WHITE), StrokeKind::Outside);
                                }
                                if resp.clicked() {
                                    l.color = *c;
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("Custom");
                            ui.color_edit_button_srgb(&mut l.color);
                            ui.label(RichText::new(format!("R {}  G {}  B {}", l.color[0], l.color[1], l.color[2])).color(MUTED));
                        });
                    });
                });
            });
        });
        if self.profile().light != before {
            self.send_config("Sending lighting");
        }
        ui.add_space(6.0);
        card(ui, |ui| {
            heading(ui, "Per-key colours");
            if !matches!(mode.value, 21 | 22) {
                ui.label(
                    RichText::new("Pick \"Audio wave\" or \"Light and shadow\": in those effects the keyboard shows colours sent from this computer, one per key.")
                        .size(12.0)
                        .color(MUTED),
                );
                if ui.button("Switch to per-key colours").clicked() {
                    self.profile_mut().light.mode = 22;
                    self.painting = true;
                    self.send_config("Sending lighting");
                }
                return;
            }
            ui.checkbox(&mut self.painting, "Paint: clicking a key above gives it the colour chosen on the right");
            let mut send = false;
            ui.horizontal(|ui| {
                if ui.button("Fill all keys").clicked() {
                    let c = self.profile().light.color;
                    self.profile_mut().key_colors.iter_mut().for_each(|k| *k = c);
                    send = true;
                }
                if ui.button("Rainbow").clicked() {
                    for k in data::LAYOUT.iter() {
                        let p = (k.x - data::LAYOUT_X0) / (data::LAYOUT_X1 - data::LAYOUT_X0);
                        let c = hsv(p, 1.0, 1.0);
                        self.profile_mut().key_colors[k.index] = [(c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8];
                    }
                    send = true;
                }
                if ui.button("All off").clicked() {
                    self.profile_mut().key_colors.iter_mut().for_each(|k| *k = [0, 0, 0]);
                    send = true;
                }
                if ui.button("Send again").clicked() {
                    send = true;
                }
            });
            if self.tables.is_none() {
                ui.label(RichText::new("Connect the keyboard to send colours (the app reads its LED numbers first).").size(11.0).color(MUTED));
            }
            if send {
                self.send_colours();
            }
        });
    }

    fn keys_tab(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            heading(ui, "Button Settings");
            let Some(k) = self.selected_key else {
                ui.label(RichText::new("Click a key on the keyboard above to change what it does.").color(MUTED));
                return;
            };
            let cap = data::LAYOUT.iter().find(|c| c.index == k).map(|c| c.label).unwrap_or("?");
            let current = self.profile().keys[k];
            ui.label(format!("Key: {cap}    now: {}", data::describe(current)));
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label("Combine with:");
                for (bit, name) in data::MOD_NAMES.iter().take(4) {
                    let mut on = self.remap_mods & bit != 0;
                    if ui.checkbox(&mut on, *name).changed() {
                        self.remap_mods ^= bit;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("Key:");
                egui::ComboBox::from_id_salt("usage")
                    .selected_text(data::usage_name(self.remap_usage))
                    .height(320.0)
                    .show_ui(ui, |ui| {
                        for (u, n) in data::USAGES {
                            ui.selectable_value(&mut self.remap_usage, *u, *n);
                        }
                    });
            });
            ui.add_space(4.0);
            let mut new = None;
            ui.horizontal_wrapped(|ui| {
                if accent_button(ui, "Apply to key").clicked() {
                    new = Some(data::assign_key(self.remap_mods, self.remap_usage));
                }
                if ui.button("Key function off").clicked() {
                    new = Some(data::DISABLED);
                }
                if ui.button("Default function").clicked() {
                    new = Some(data::DEFAULT_ASSIGN[k]);
                }
                if !self.saved.macros.is_empty() {
                    egui::ComboBox::from_id_salt("asg_macro").selected_text("Assign macro…").show_ui(ui, |ui| {
                        for (i, m) in self.saved.macros.iter().enumerate() {
                            if ui.selectable_label(false, &m.name).clicked() {
                                new = Some(data::assign_macro(i as u8));
                            }
                        }
                    });
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Shortcuts:").color(MUTED));
                for (name, m, u) in [("Copy", 1u8, 0x06u8), ("Paste", 1, 0x19), ("Cut", 1, 0x1B), ("Undo", 1, 0x1D), ("Select All", 1, 0x04), ("Find", 1, 0x09), ("Save", 1, 0x16)] {
                    if ui.small_button(name).clicked() {
                        new = Some(data::assign_key(m, u));
                    }
                }
            });
            if let Some(v) = new {
                self.profile_mut().keys[k] = v;
                self.send_keys();
            }
        });
        ui.add_space(6.0);
        card(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Reset all keys to default").clicked() {
                    self.profile_mut().keys = data::DEFAULT_ASSIGN.to_vec();
                    self.send_keys();
                }
                ui.label(RichText::new("Changed keys are shown in red on the keyboard.").size(12.0).color(MUTED));
            });
        });
    }

    fn macros_tab(&mut self, ui: &mut egui::Ui) {
        let t = ui.input(|i| i.time);
        if self.recording {
            let events = ui.input(|i| i.events.clone());
            for e in events {
                if let egui::Event::Key { key, pressed, repeat: false, .. } = e {
                    if let Some(u) = hid_usage(key) {
                        let delay = self.fixed_delay.unwrap_or(((t - self.last_event_time) * 1000.0).clamp(10.0, 65535.0) as u16);
                        self.last_event_time = t;
                        if let Some(m) = self.saved.macros.get_mut(self.macro_sel) {
                            if m.events.len() < protocol::MACRO_MAX_ACTIONS {
                                m.events.push(MacroEvent { usage: u, down: pressed, delay_ms: delay });
                            }
                        }
                    }
                }
            }
        }
        ui.columns(2, |cols| {
            card(&mut cols[0], |ui| {
                heading(ui, "Macros");
                for i in 0..self.saved.macros.len() {
                    let sel = self.macro_sel == i;
                    if ui.selectable_label(sel, &self.saved.macros[i].name).clicked() {
                        self.macro_sel = i;
                        self.recording = false;
                    }
                }
                ui.horizontal(|ui| {
                    if ui.button("Add").clicked() {
                        let n = self.saved.macros.len() + 1;
                        self.saved.macros.push(Macro { name: format!("Macro {n}"), events: vec![] });
                        self.macro_sel = n - 1;
                        self.save();
                    }
                    if ui.button("Delete").clicked() && self.macro_sel < self.saved.macros.len() {
                        self.saved.macros.remove(self.macro_sel);
                        self.macro_sel = self.macro_sel.saturating_sub(1);
                        self.save();
                    }
                });
            });
            card(&mut cols[1], |ui| {
                let Some(m) = self.saved.macros.get_mut(self.macro_sel) else {
                    ui.label("Add a macro first.");
                    return;
                };
                ui.horizontal(|ui| {
                    ui.label("Macroname:");
                    ui.text_edit_singleline(&mut m.name);
                });
                ui.horizontal(|ui| {
                    let label = if self.recording { "Stop Recording" } else { "Boot Recording" };
                    if ui.add(egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(if self.recording { ACCENT } else { ACCENT_DIM })).clicked() {
                        self.recording = !self.recording;
                        self.last_event_time = t;
                    }
                    if ui.button("Clear").clicked() {
                        m.events.clear();
                    }
                    let mut fixed = self.fixed_delay.is_some();
                    if ui.checkbox(&mut fixed, "Fixed time (ms)").changed() {
                        self.fixed_delay = fixed.then_some(50);
                    }
                    if let Some(d) = self.fixed_delay.as_mut() {
                        ui.add(egui::DragValue::new(d).range(protocol::MACRO_MIN_INTERVAL_MS..=10000));
                    }
                });
                if self.recording {
                    ui.label(RichText::new("Recording: press keys now (this window must have focus).").color(ACCENT));
                }
                egui::Grid::new("events").striped(true).num_columns(4).show(ui, |ui| {
                    ui.label(RichText::new("Keystroke").color(MUTED));
                    ui.label(RichText::new("Action").color(MUTED));
                    ui.label(RichText::new("Delay(ms)").color(MUTED));
                    ui.label("");
                    ui.end_row();
                    let mut remove = None;
                    for (i, e) in m.events.iter_mut().enumerate() {
                        ui.label(data::usage_name(e.usage));
                        ui.label(if e.down { "Key Down" } else { "Key Up" });
                        ui.add(egui::DragValue::new(&mut e.delay_ms).range(protocol::MACRO_MIN_INTERVAL_MS..=65535));
                        if ui.small_button("Remove").clicked() {
                            remove = Some(i);
                        }
                        ui.end_row();
                    }
                    if let Some(i) = remove {
                        m.events.remove(i);
                    }
                });
                ui.label(RichText::new(format!("{} / {} actions", m.events.len(), protocol::MACRO_MAX_ACTIONS)).size(11.0).color(MUTED));
                ui.add_space(4.0);
                if accent_button(ui, "Storage (send macros)").clicked() {
                    self.recording = false;
                    self.send_macros();
                }
            });
        });
    }

    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        let before = self.profile().system;
        ui.columns(2, |cols| {
            card(&mut cols[0], |ui| {
                heading(ui, "USB report rate");
                let s = &mut self.saved.profiles[self.saved.active].system;
                ui.horizontal(|ui| {
                    for (i, hz) in data::POLLING_HZ.iter().enumerate() {
                        ui.selectable_value(&mut s.polling, i as u8, format!("{hz} Hz"));
                    }
                });
                ui.add_space(6.0);
                heading(ui, "Key Delay");
                ui.add(egui::Slider::new(&mut s.debounce, 0..=protocol::DEBOUNCE_MAX).text("debounce level"));
            });
            card(&mut cols[1], |ui| {
                heading(ui, "Keys");
                let s = &mut self.saved.profiles[self.saved.active].system;
                ui.checkbox(&mut s.win_lock, "Windows Lock (disable the Win key)");
                ui.checkbox(&mut s.w_swap, "Exchange key (W flag)");
                let mut nkey = !s.six_key;
                if ui.checkbox(&mut nkey, "N key rollover (off = 6 keys)").changed() {
                    s.six_key = !nkey;
                }
            });
        });
        if self.profile().system != before {
            self.send_config("Sending settings");
        }
        ui.add_space(6.0);
        card(ui, |ui| {
            heading(ui, "Reset");
            ui.horizontal(|ui| {
                if ui.button("Reset to default settings").clicked() {
                    let i = self.saved.active;
                    let name = self.saved.profiles[i].name.clone();
                    self.saved.profiles[i] = Profile::new(i);
                    self.saved.profiles[i].name = name;
                    self.send_everything();
                }
                ui.label(RichText::new("Restores lighting, keys and settings of this profile.").size(12.0).color(MUTED));
            });
        });
    }

    fn screen_tab(&mut self, ui: &mut egui::Ui) {
        const WEEKDAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
        const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        let before = self.saved.screen;
        let now = self.screen_time();
        ui.columns(2, |cols| {
            card(&mut cols[0], |ui| {
                heading(ui, "On the keyboard screen");
                // Drawn preview of the LCD clock.
                let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().min(300.0), 120.0), Sense::hover());
                let p = ui.painter();
                p.rect_filled(r, CornerRadius::same(8), Color32::from_rgb(0x06, 0x22, 0x3a));
                p.rect_stroke(r, CornerRadius::same(8), Stroke::new(1.5, EDGE), StrokeKind::Inside);
                let (h, suffix) = if self.saved.screen.hour12 {
                    let h12 = now.as_12_hour().hour;
                    (h12, if now.hour < 12 { " AM" } else { " PM" })
                } else {
                    (now.hour, "")
                };
                let time = if self.saved.screen.hour12 {
                    format!("{h}:{:02}:{:02}{suffix}", now.min, now.sec)
                } else {
                    format!("{h:02}:{:02}:{:02}", now.min, now.sec)
                };
                p.text(r.center() - Vec2::new(0.0, 14.0), Align2::CENTER_CENTER, time, FontId::monospace(30.0), Color32::from_rgb(0x9f, 0xe8, 0xff));
                let date = format!(
                    "{}  {} {} {}",
                    WEEKDAYS[now.weekday as usize % 7],
                    now.day,
                    MONTHS[(now.month as usize + 11) % 12],
                    now.year
                );
                p.text(r.center() + Vec2::new(0.0, 26.0), Align2::CENTER_CENTER, date, FontId::proportional(15.0), MUTED);
                ui.add_space(6.0);
                if self.clock_offset == 0 {
                    ui.label(RichText::new("Following this computer's time and date.").size(12.0).color(OK));
                } else {
                    ui.label(RichText::new("Using your own time and date (still running).").size(12.0).color(ACCENT));
                }
                ui.add_space(4.0);
                match self.keyboard_clock {
                    Some((c, at)) => {
                        let ago = (self.frames_time - at).max(0.0) as u64;
                        ui.label(
                            RichText::new(format!(
                                "Keyboard says: {:02}:{:02}:{:02}  {} {} {} {}  (read {ago} s ago)",
                                c.hour,
                                c.min,
                                c.sec,
                                WEEKDAYS[c.weekday as usize % 7],
                                c.day,
                                MONTHS[(c.month as usize + 11) % 12],
                                c.year
                            ))
                            .size(12.0)
                            .color(TEXT),
                        );
                    }
                    None => {
                        ui.label(RichText::new("Keyboard's stored time: not read yet.").size(12.0).color(MUTED));
                    }
                }
                if ui.button("Read keyboard clock (test)").clicked() {
                    self.read_keyboard_clock();
                }
            });
            card(&mut cols[1], |ui| {
                heading(ui, "Time format");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.saved.screen.hour12, false, "24 hour");
                    ui.selectable_value(&mut self.saved.screen.hour12, true, "12 hour");
                });
                if self.saved.screen.hour12 {
                    ui.label(
                        RichText::new("Experimental: Redragon's app always sends 24-hour time. In 12-hour mode this app sends the hour as 1–12 and re-syncs every minute. If the screen shows a wrong time, switch back to 24 hour.")
                            .size(11.0)
                            .color(MUTED),
                    );
                }
                ui.add_space(6.0);
                heading(ui, "Sync");
                ui.checkbox(&mut self.saved.screen.auto_sync, "Keep in sync (on connect and every minute)");
                ui.label(
                    RichText::new("Redragon's app only sets the clock when a setting is sent; this option also sets it on connect and every minute.")
                        .size(11.0)
                        .color(MUTED),
                );
                if accent_button(ui, "Sync with system time and date").clicked() {
                    self.clock_offset = 0;
                    self.sync_clock("Syncing clock with system time");
                }
            });
        });
        ui.add_space(6.0);
        card(ui, |ui| {
            heading(ui, "Set your own time and date");
            let t = &mut self.custom_time;
            ui.horizontal_wrapped(|ui| {
                ui.label("Date");
                ui.add(egui::DragValue::new(&mut t[0]).range(2000..=2099).prefix("year "));
                ui.add(egui::DragValue::new(&mut t[1]).range(1..=12).prefix("month "));
                let max_day = protocol::days_in_month(t[0], t[1]);
                ui.add(egui::DragValue::new(&mut t[2]).range(1..=max_day).prefix("day "));
                ui.add_space(12.0);
                ui.label("Time");
                ui.add(egui::DragValue::new(&mut t[3]).range(0..=23).custom_formatter(|v, _| format!("{v:02}")));
                ui.label(":");
                ui.add(egui::DragValue::new(&mut t[4]).range(0..=59).custom_formatter(|v, _| format!("{v:02}")));
                ui.label(":");
                ui.add(egui::DragValue::new(&mut t[5]).range(0..=59).custom_formatter(|v, _| format!("{v:02}")));
                ui.label(RichText::new("(hours 0–23)").size(11.0).color(MUTED));
            });
            ui.horizontal(|ui| {
                if accent_button(ui, "Send this time").clicked() {
                    self.clock_offset = Self::offset_to(self.custom_time);
                    self.sync_clock("Setting screen clock");
                }
                if ui.button("Fill in current time").clicked() {
                    let c = Self::local_clock(0);
                    self.custom_time = [c.year, c.month, c.day, c.hour, c.min, c.sec];
                }
            });
            ui.label(
                RichText::new("The time and date travel with every settings change, so a chosen time keeps running until you sync with the system again.")
                    .size(11.0)
                    .color(MUTED),
            );
        });
        if self.saved.screen != before {
            self.save();
            self.sync_clock("Setting screen clock");
        }
    }

    fn log_tab(&mut self, ui: &mut egui::Ui) {
        card(ui, |ui| {
            heading(ui, "USB log (first 16 bytes of each report)");
            if ui.button("Clear").clicked() {
                self.log.clear();
            }
            for l in self.log.iter().rev().take(200) {
                ui.label(RichText::new(l).monospace().size(11.0));
            }
        });
    }
}

impl eframe::App for ControlApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        while let Ok(r) = self.rx.try_recv() {
            self.busy = false;
            self.log.extend(r.log);
            if r.tables.is_some() {
                self.tables = r.tables;
            }
            if let Some(c) = r.keyboard_clock {
                self.keyboard_clock = Some((c, self.frames_time));
            }
            self.status = match r.result {
                Ok(msg) => format!("{}: {msg}", r.label),
                Err(e) => format!("{} failed: {e}", r.label),
            };
        }
        let now = ctx.input(|i| i.time);
        if self.frames == 0 {
            self.started = now;
        }
        let t = (now - self.started) as f32 + 1.3;
        self.frames_time = now;
        if self.show_requests.as_ref().is_some_and(|rx| rx.try_recv().is_ok()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        // Keep the screen clock right: once the keyboard's tables are read, then every minute.
        if self.connected
            && self.saved.screen.auto_sync
            && !self.busy
            && self.tables.is_some()
            && now - self.last_clock_sync > CLOCK_RESYNC_SECONDS
        {
            self.sync_clock("Keeping the screen clock in sync");
        }
        // Refresh the device list every 2 s while waiting on the dashboard (plug-in, new permissions).
        if self.page == Page::Dashboard && !self.connected && now - self.last_scan > 2.0 {
            self.last_scan = now;
            self.rescan();
        }

        self.top_bar(ui);
        self.status_bar(ui);
        match self.page {
            Page::Dashboard => self.dashboard(ui, t),
            Page::Device => self.device_page(ui, t),
        }
        ctx.request_repaint();

        self.frames += 1;
        if let Some(path) = self.shot.clone() {
            if self.frames >= 120 && !self.shot_requested {
                self.shot_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            let shot = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(img) = shot {
                if let Err(e) = crate::save_png(&path, &img) {
                    eprintln!("screenshot failed: {e}");
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}
