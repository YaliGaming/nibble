use crate::spawn;
use nibble_core::config::Config;
use nibble_core::ipc;
use nibble_core::paths::Paths;
use serde_json::{json, Value};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

fn default_text(key: &str) -> &'static str {
    match key {
        "on" => "Nibble is on.",
        "off" => "Nibble is off. Type /pet again to bring it back.",
        "started" => "Nibble is on. It will open and close together with Claude from now on.",
        "chat" => "Quick Chat is open next to the pet.",
        "notInstalled" => "Nibble has not been started on this computer yet. Open Nibble once from the Start menu.",
        _ => "Nibble could not be reached.",
    }
}

fn read_json(file: &std::path::Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(file).ok()?).ok()
}

fn say(app: &Value, key: &str, as_hook: bool) {
    let line = app
        .get("texts")
        .and_then(|t| t.get(key))
        .and_then(Value::as_str)
        .unwrap_or_else(|| default_text(key))
        .to_string();
    let out = if as_hook {
        json!({
            "decision": "block",
            "reason": line,
            "hookSpecificOutput": { "hookEventName": "UserPromptExpansion", "suppressOriginalPrompt": true }
        })
        .to_string()
    } else {
        line
    };
    let mut stdout = std::io::stdout();
    let _ = writeln!(stdout, "{out}");
    let _ = stdout.flush();
}

fn remember(paths: &Paths, on: bool) {
    let mut config = Config::load(&paths.config_file);
    if config.visible != on {
        config.visible = on;
        config.save(&paths.config_file);
    }
}

fn start(paths: &Paths, app: &Value) -> bool {
    let Some(exe) = app.get("exe").and_then(Value::as_str).map(PathBuf::from).filter(|p| p.exists()) else {
        return false;
    };
    let args: Vec<String> = app
        .get("args")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    remember(paths, true);
    spawn::shell_open(&exe, &args)
}

fn ask(paths: &Paths, word: &str) -> bool {
    ipc::request(&paths.pet_home, word, Duration::from_millis(1500))
        .and_then(|r| r.get("ok").and_then(Value::as_bool))
        .unwrap_or(false)
}

pub fn run(paths: &Paths, args: &[String]) {
    let command = match args.first().map(String::as_str) {
        Some("on" | "show" | "start") => "on",
        Some("off" | "hide") => "off",
        Some("chat") => "chat",
        _ => "toggle",
    };
    let as_hook = args.iter().any(|a| a == "--hook");
    let app = read_json(&paths.app_info_file).unwrap_or(Value::Null);
    let outcome = match command {
        "off" => {
            ask(paths, "off");
            remember(paths, false);
            "off"
        }
        "toggle" => {
            if ask(paths, "off") {
                remember(paths, false);
                "off"
            } else if start(paths, &app) {
                "started"
            } else {
                "notInstalled"
            }
        }
        other => {
            if ask(paths, if other == "chat" { "chat" } else { "ping" }) {
                if other == "chat" { "chat" } else { "on" }
            } else if start(paths, &app) {
                "started"
            } else {
                "notInstalled"
            }
        }
    };
    say(&app, outcome, as_hook);
}

pub fn launch_chat(file: Option<&String>) {
    let Some(file) = file else { std::process::exit(1) };
    let job = read_json(std::path::Path::new(file));
    let _ = std::fs::remove_file(file);
    let (Some(bin), Some(prompt)) = (
        job.as_ref().and_then(|j| j.get("bin")).and_then(Value::as_str),
        job.as_ref().and_then(|j| j.get("prompt")).and_then(Value::as_str),
    ) else {
        eprintln!("Nibble: could not read the chat request.");
        std::process::exit(1);
    };
    let status = Command::new(bin).arg(prompt).status();
    std::process::exit(status.ok().and_then(|s| s.code()).unwrap_or(0));
}
