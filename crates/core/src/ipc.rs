use serde_json::Value;
use sha1::{Digest, Sha1};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

pub fn pipe_path(home: &Path) -> String {
    if cfg!(windows) {
        let lower = home.to_string_lossy().to_lowercase();
        let digest = Sha1::digest(lower.as_bytes());
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        format!("\\\\.\\pipe\\nibble-{}", &hex[..12])
    } else {
        home.join("pet.sock").to_string_lossy().into_owned()
    }
}

#[cfg(windows)]
type Channel = std::fs::File;
#[cfg(not(windows))]
type Channel = std::os::unix::net::UnixStream;

fn open(home: &Path) -> std::io::Result<Channel> {
    let path = pipe_path(home);
    #[cfg(windows)]
    {
        std::fs::OpenOptions::new().read(true).write(true).open(path)
    }
    #[cfg(not(windows))]
    {
        std::os::unix::net::UnixStream::connect(path)
    }
}

pub fn request_blocking(home: &Path, line: &str) -> Option<Value> {
    let mut channel = open(home).ok()?;
    channel.write_all(format!("{line}\n").as_bytes()).ok()?;
    channel.flush().ok()?;
    let mut reader = BufReader::new(channel);
    let mut reply = String::new();
    reader.read_line(&mut reply).ok()?;
    serde_json::from_str(reply.trim()).ok()
}

pub fn request(home: &Path, line: &str, timeout: Duration) -> Option<Value> {
    let (tx, rx) = mpsc::channel();
    let home = home.to_path_buf();
    let line = line.to_string();
    thread::spawn(move || {
        let _ = tx.send(request_blocking(&home, &line));
    });
    rx.recv_timeout(timeout).ok().flatten()
}

pub fn is_running(home: &Path, timeout: Duration) -> bool {
    let (tx, rx) = mpsc::channel();
    let home = home.to_path_buf();
    thread::spawn(move || {
        let _ = tx.send(open(&home).is_ok());
    });
    rx.recv_timeout(timeout).unwrap_or(true)
}
