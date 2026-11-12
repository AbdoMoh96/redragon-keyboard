//! One running copy at a time, like the original (sub_480a30): it creates the mutex
//! "Redragon K708-RGB-PRO"; when that already exists it broadcasts the registered message of
//! the same name so the running copy comes to the front, and exits.
//!
//! On Linux the same rule uses a Unix socket in the runtime folder: the first copy listens,
//! a second copy connects, asks the first to show itself, and exits. Two copies would otherwise
//! talk to the keyboard at the same time and mix up each other's replies.

use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};

/// Name of the original's mutex and window message, reused for the socket.
pub const INSTANCE_NAME: &str = "Redragon K708-RGB-PRO";
const SHOW_REQUEST: &[u8] = b"show\n";

fn socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let user = unsafe { libc::getuid() };
    dir.join(format!("{}-{user}.sock", INSTANCE_NAME.replace(' ', "-")))
}

pub enum Instance {
    /// This is the only copy; `show` gets a message whenever another copy starts.
    First { show: Receiver<()> },
    /// Another copy is running and was asked to come to the front.
    AlreadyRunning,
}

pub fn claim() -> Instance {
    let path = socket_path();
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(_) => {
            if let Ok(mut s) = UnixStream::connect(&path) {
                let _ = s.write_all(SHOW_REQUEST);
                return Instance::AlreadyRunning;
            }
            // Left over from a copy that didn't exit cleanly: nobody answers, take it over.
            let _ = std::fs::remove_file(&path);
            match UnixListener::bind(&path) {
                Ok(l) => l,
                Err(_) => {
                    let (_tx, rx) = channel();
                    return Instance::First { show: rx };
                }
            }
        }
    };
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut buf = [0u8; 16];
            if stream.read(&mut buf).is_ok() && buf.starts_with(SHOW_REQUEST) && tx.send(()).is_err() {
                break;
            }
        }
    });
    Instance::First { show: rx }
}

/// Remove the socket when the app closes normally.
pub fn release() {
    let _ = std::fs::remove_file(socket_path());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_copy_asks_first_to_show() {
        let dir = std::env::temp_dir().join(format!("k708-instance-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("XDG_RUNTIME_DIR", &dir);
        let Instance::First { show } = claim() else { panic!("first claim should win") };
        assert!(matches!(claim(), Instance::AlreadyRunning));
        assert!(show.recv_timeout(std::time::Duration::from_secs(2)).is_ok());
        release();
        // After release the next copy is first again.
        assert!(matches!(claim(), Instance::First { .. }));
        release();
        let _ = std::fs::remove_dir(&dir);
    }
}
