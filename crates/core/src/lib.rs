pub mod config;
pub mod ipc;
pub mod paths;

pub const NAME: &str = "Nibble";
pub const APP_ID: &str = "io.github.yaligaming.nibble";

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn sanitize(sid: &str) -> String {
    sid.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect()
}
