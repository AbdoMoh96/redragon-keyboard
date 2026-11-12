mod app;
mod data;
mod device;
mod instance;
mod protocol;

use eframe::egui;
use std::path::{Path, PathBuf};

pub fn save_png(path: &Path, img: &egui::ColorImage) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
    }
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), img.size[0] as u32, img.size[1] as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(|e| e.to_string())?;
    let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
    w.write_image_data(&bytes).map_err(|e| e.to_string())
}

/// `--test-clock`: without a window, open the keyboard, read its tables, set the screen
/// clock to the system time with the current lighting kept, and read the clock back.
fn test_clock() -> i32 {
    let ids = data::MODELS[0].ids;
    let Some(node) = device::scan().into_iter().find(|n| n.command_channel && ids.contains(&(n.vid, n.pid))) else {
        eprintln!("no K708 settings channel found");
        return 1;
    };
    println!("using {} ({:04x}:{:04x})", node.path.display(), node.vid, node.pid);
    let mut kb = match device::Keyboard::open(&node) {
        Ok(k) => k,
        Err(e) => {
            eprintln!("open failed: {e}");
            return 1;
        }
    };
    let mut log = Vec::new();
    let result = (|| -> Result<(), String> {
        let t = kb.read_tables(&mut log)?;
        println!("keys {}, macro space {} bytes", t.keys(), t.macro_space());
        let block = kb.read_config(0, &mut log)?;
        println!("clock before: {:?}", protocol::block_clock(&block));
        // Keep the keyboard's own settings; only the clock bytes change.
        let mut b = block.clone();
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        let now = unsafe { libc::time(std::ptr::null_mut()) };
        unsafe { libc::localtime_r(&now, &mut tm) };
        let bcd = |v: i32| (((v / 10) % 10) << 4 | (v % 10)) as u8;
        b[0x23] = bcd(tm.tm_sec);
        b[0x24] = bcd(tm.tm_min);
        b[0x25] = bcd(tm.tm_hour);
        b[0x26] = bcd(tm.tm_wday);
        b[0x27] = bcd(tm.tm_mday);
        b[0x28] = bcd(tm.tm_mon + 1);
        b[0x29] = bcd(tm.tm_year % 100);
        b[0x2f] = 1;
        let mut packets = vec![protocol::simple(protocol::CMD_BEGIN)];
        packets.extend(protocol::chunked(protocol::CMD_WRITE_CONFIG, 0, &b, kb.chunk));
        packets.push(protocol::simple(protocol::CMD_END));
        kb.send_all(&packets, &mut log)?;
        let after = kb.read_config(0, &mut log)?;
        println!("clock after:  {:?}", protocol::block_clock(&after));
        Ok(())
    })();
    for l in &log {
        println!("{l}");
    }
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("failed: {e}");
            1
        }
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--test-clock") {
        std::process::exit(test_clock());
    }
    let shot = args
        .iter()
        .position(|a| a == "--screenshot")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);
    // Wayland stops sending frames to windows it treats as hidden (idle screen), which would
    // stall the 120-frame screenshot run; X11 keeps drawing, so prefer it for screenshots.
    if shot.is_some() && std::env::var_os("DISPLAY").is_some() {
        std::env::remove_var("WAYLAND_DISPLAY");
    }
    // --page lighting|keys|macros|settings|screen|log opens the control pages directly.
    let start = args.iter().position(|a| a == "--page").and_then(|i| args.get(i + 1)).cloned();
    // One running copy at a time (sub_480a30); screenshot runs are exempt so checks can run
    // next to an open window.
    let show_requests = if shot.is_none() {
        match instance::claim() {
            instance::Instance::First { show } => Some(show),
            instance::Instance::AlreadyRunning => {
                println!("Keyboard Control is already running; brought it to the front.");
                return Ok(());
            }
        }
    } else {
        None
    };
    let owns_instance = show_requests.is_some();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Keyboard Control")
            .with_inner_size([1000.0, 640.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "k708-control",
        options,
        Box::new(move |cc| Ok(Box::new(app::ControlApp::new(cc, shot, start, show_requests)))),
    );
    if owns_instance {
        instance::release();
    }
    result
}
