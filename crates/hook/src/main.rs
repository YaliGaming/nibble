#![cfg_attr(windows, windows_subsystem = "windows")]

mod compact;
mod pet;
mod spawn;

use nibble_core::config::Config;
use nibble_core::paths::Paths;
use nibble_core::{ipc, now_ms, sanitize};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let paths = Paths::detect();
    match args.first().map(String::as_str) {
        Some("pet") => pet::run(&paths, &args[1..]),
        Some("launch-chat") => pet::launch_chat(args.get(1)),
        _ => run_hook(&paths),
    }
}

fn read_stdin() -> Option<String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut raw = Vec::new();
        let _ = std::io::stdin().read_to_end(&mut raw);
        let _ = tx.send(String::from_utf8_lossy(&raw).into_owned());
    });
    rx.recv_timeout(Duration::from_secs(4)).ok()
}

fn run_hook(paths: &Paths) {
    let started = now_ms();
    let Some(raw) = read_stdin() else { return };
    let Ok(event) = serde_json::from_str::<Value>(raw.trim_start_matches('\u{feff}')) else { return };
    let config = Config::load(&paths.config_file);
    let sid = sanitize(event.get("session_id").and_then(Value::as_str).unwrap_or(""));
    let name = event.get("hook_event_name").and_then(Value::as_str).unwrap_or("");
    if sid.is_empty() || name.is_empty() {
        return;
    }
    let line = compact::compact(&event, started, config.store_text);
    if std::fs::create_dir_all(&paths.events_dir).is_ok() {
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(paths.events_dir.join(format!("{sid}.jsonl")))
        {
            let _ = file.write_all(format!("{line}\n").as_bytes());
        }
    }
    match name {
        "SessionStart" | "UserPromptSubmit" => wake_pet(paths, &config),
        "PermissionRequest" if pet_may_answer(&config) => ask_pet(paths, &event),
        _ => {}
    }
}

fn wake_pet(paths: &Paths, config: &Config) {
    if !config.visible {
        return;
    }
    if !ipc::is_running(&paths.pet_home, Duration::from_millis(500)) {
        spawn::detached_self(&["pet", "on"]);
    }
}

fn pet_may_answer(config: &Config) -> bool {
    if !config.approvals {
        return false;
    }
    if std::env::var("NIBBLE_CHAT").as_deref() == Ok("1") {
        return true;
    }
    if std::env::var("CLAUDE_CODE_ENTRYPOINT").as_deref() == Ok("claude-desktop") {
        return true;
    }
    config.extra_bool("approvalsEverywhere") == Some(true)
}

fn ask_pet(paths: &Paths, event: &Value) {
    let ask = json!({
        "sid": event.get("session_id").and_then(Value::as_str).unwrap_or(""),
        "tool": event.get("tool_name"),
        "detail": compact::tool_detail(event.get("tool_name").and_then(Value::as_str).unwrap_or(""), event.get("tool_input")),
    });
    let reply = ipc::request(&paths.pet_home, &format!("ask {ask}"), Duration::from_secs(9 * 60));
    let decision = reply
        .as_ref()
        .and_then(|r| r.get("decision"))
        .and_then(Value::as_str)
        .filter(|d| *d == "allow" || *d == "deny");
    let Some(decision) = decision else { return };
    let mut answer = json!({ "behavior": decision });
    if decision == "deny" {
        answer["message"] = json!("Denied from Nibble.");
    }
    let out = json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": answer } });
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(out.to_string().as_bytes());
    let _ = stdout.flush();
}
