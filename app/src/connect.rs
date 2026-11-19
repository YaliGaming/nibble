use include_dir::{include_dir, Dir};
use nibble_core::paths::{forward, Paths};
use nibble_core::now_ms;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::fs;
use std::path::PathBuf;

static SKILLS: Dir = include_dir!("$CARGO_MANIFEST_DIR/../skill");

const SKILL_NAMES: [&str; 2] = ["hatch-pet", "pet"];
const LEGACY_MARKS: [&str; 2] = [".nibble-pet/hook.js", ".nibble-pet/pet.js"];

struct EventSpec {
    name: &'static str,
    is_async: bool,
    timeout: Option<u32>,
    slash_command: Option<&'static str>,
}

const fn spec(name: &'static str, is_async: bool, timeout: Option<u32>) -> EventSpec {
    EventSpec { name, is_async, timeout, slash_command: None }
}

const EVENTS: [EventSpec; 12] = [
    spec("SessionStart", false, None),
    spec("UserPromptSubmit", false, None),
    spec("PreToolUse", true, None),
    spec("PostToolUse", true, None),
    spec("PostToolUseFailure", true, None),
    spec("PermissionRequest", false, Some(600)),
    spec("Notification", true, None),
    spec("ElicitationResult", true, None),
    spec("Stop", false, None),
    spec("StopFailure", false, None),
    spec("SessionEnd", false, None),
    EventSpec { name: "UserPromptExpansion", is_async: false, timeout: Some(30), slash_command: Some("pet") },
];

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub connected: bool,
    pub partial: bool,
    pub outdated: bool,
    pub node: bool,
    pub error: Option<String>,
    pub settings_file: String,
}

fn hook_entry(paths: &Paths, spec: &EventSpec) -> Value {
    let args: Vec<&str> = if spec.slash_command.is_some() { vec!["pet", "toggle", "--hook"] } else { vec!["hook"] };
    let mut hook = Map::new();
    hook.insert("type".into(), json!("command"));
    hook.insert("command".into(), json!(forward(&paths.hook_file)));
    hook.insert("args".into(), json!(args));
    hook.insert("timeout".into(), json!(spec.timeout.unwrap_or(5)));
    if spec.is_async {
        hook.insert("async".into(), json!(true));
    }
    Value::Object(hook)
}

fn hook_group(paths: &Paths, spec: &EventSpec) -> Value {
    match spec.slash_command {
        Some(matcher) => json!({ "matcher": matcher, "hooks": [hook_entry(paths, spec)] }),
        None => json!({ "hooks": [hook_entry(paths, spec)] }),
    }
}

fn mentions(paths: &Paths, text: &str) -> bool {
    let text = text.replace('\\', "/");
    let own = forward(&paths.hook_file);
    LEGACY_MARKS.iter().any(|m| text.contains(m)) || text.contains(&own) || text.contains(".nibble-pet/nibble-hook")
}

fn is_ours(paths: &Paths, hook: &Value) -> bool {
    if !hook.is_object() {
        return false;
    }
    let command = hook.get("command").and_then(Value::as_str).is_some_and(|c| mentions(paths, c));
    let args = hook
        .get("args")
        .and_then(Value::as_array)
        .is_some_and(|a| a.iter().any(|v| v.as_str().is_some_and(|s| mentions(paths, s))));
    command || args
}

fn read_settings(paths: &Paths) -> Result<(Map<String, Value>, bool), String> {
    let text = match fs::read_to_string(&paths.settings_file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok((Map::new(), false)),
        Err(e) => return Err(e.to_string()),
    };
    if text.trim().is_empty() {
        return Ok((Map::new(), true));
    }
    match serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) {
        Ok(Value::Object(map)) => Ok((map, true)),
        Ok(_) => Err("settings.json is not a JSON object".into()),
        Err(e) => Err(e.to_string()),
    }
}

fn iso_stamp(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let millis = ms.rem_euclid(1000);
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}-{:02}-{:02}-{millis:03}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

fn write_settings(paths: &Paths, settings: &Map<String, Value>, existed: bool) -> Result<(), String> {
    if let Some(dir) = paths.settings_file.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    if existed {
        fs::create_dir_all(&paths.backup_dir).map_err(|e| e.to_string())?;
        let name = format!("settings-{}.json", iso_stamp(now_ms()));
        fs::copy(&paths.settings_file, paths.backup_dir.join(name)).map_err(|e| e.to_string())?;
    }
    let mut tmp = paths.settings_file.as_os_str().to_owned();
    tmp.push(".nibble.tmp");
    let tmp = PathBuf::from(tmp);
    let text = serde_json::to_string_pretty(&Value::Object(settings.clone())).map_err(|e| e.to_string())?;
    fs::write(&tmp, text + "\n").map_err(|e| e.to_string())?;
    fs::rename(&tmp, &paths.settings_file).map_err(|e| e.to_string())
}

fn our_entries<'a>(paths: &Paths, groups: Option<&'a Value>) -> Vec<&'a Value> {
    groups
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter(|g| {
                    g.get("hooks")
                        .and_then(Value::as_array)
                        .is_some_and(|hooks| hooks.iter().any(|h| is_ours(paths, h)))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn without_ours(paths: &Paths, groups: Option<&Value>) -> Vec<Value> {
    let Some(list) = groups.and_then(Value::as_array) else { return Vec::new() };
    list.iter()
        .filter_map(|g| match g.get("hooks").and_then(Value::as_array) {
            Some(hooks) => {
                let kept: Vec<Value> = hooks.iter().filter(|h| !is_ours(paths, h)).cloned().collect();
                if kept.is_empty() {
                    None
                } else {
                    let mut group = g.clone();
                    group["hooks"] = Value::Array(kept);
                    Some(group)
                }
            }
            None => Some(g.clone()),
        })
        .collect()
}

pub fn status(paths: &Paths) -> Connection {
    let mut out = Connection {
        connected: false,
        partial: false,
        outdated: false,
        node: true,
        error: None,
        settings_file: paths.settings_file.to_string_lossy().into_owned(),
    };
    match read_settings(paths) {
        Ok((settings, _)) => {
            let hooks = settings.get("hooks").and_then(Value::as_object);
            let mut have = 0;
            let mut current = 0;
            for spec in &EVENTS {
                let ours = our_entries(paths, hooks.and_then(|h| h.get(spec.name)));
                if ours.is_empty() {
                    continue;
                }
                have += 1;
                if ours.len() == 1 && *ours[0] == hook_group(paths, spec) {
                    current += 1;
                }
            }
            out.connected = have == EVENTS.len() && paths.hook_file.exists();
            out.partial = !out.connected && have > 0;
            out.outdated = have > 0 && current < EVENTS.len();
        }
        Err(e) => out.error = Some(e),
    }
    out
}

fn bundled_hook() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let file = exe.parent()?.join(nibble_core::paths::HOOK_BINARY);
    file.exists().then_some(file)
}

pub fn hook_binary(paths: &Paths) -> Option<PathBuf> {
    if paths.hook_file.exists() { Some(paths.hook_file.clone()) } else { bundled_hook() }
}

pub fn install_hook_file(paths: &Paths) -> Result<(), String> {
    fs::create_dir_all(&paths.pet_home).map_err(|e| e.to_string())?;
    let Some(source) = bundled_hook() else {
        return if paths.hook_file.exists() { Ok(()) } else { Err("The Nibble hook program was not found next to the app.".into()) };
    };
    let dest = &paths.hook_file;
    if let (Ok(a), Ok(b)) = (fs::read(&source), fs::read(dest)) {
        if a == b {
            return Ok(());
        }
    }
    if dest.exists() {
        let old = dest.with_extension("old");
        let _ = fs::remove_file(&old);
        let _ = fs::rename(dest, &old);
    }
    fs::copy(&source, dest).map(|_| ()).map_err(|e| e.to_string())?;
    let _ = fs::remove_file(dest.with_extension("old"));
    Ok(())
}

pub fn install_skills(paths: &Paths) {
    for name in SKILL_NAMES {
        let Some(file) = SKILLS.get_file(format!("{name}/SKILL.md")) else { continue };
        let dir = paths.skill_dir(name);
        if fs::create_dir_all(&dir).is_ok() {
            let _ = fs::write(dir.join("SKILL.md"), file.contents());
        }
    }
}

fn remove_skills(paths: &Paths) {
    for name in SKILL_NAMES {
        let dir = paths.skill_dir(name);
        let _ = fs::remove_file(dir.join("SKILL.md"));
        let _ = fs::remove_dir(&dir);
    }
}

pub fn connect(paths: &Paths) -> Result<Connection, String> {
    let (mut settings, existed) = read_settings(paths)?;
    install_hook_file(paths)?;
    if !settings.get("hooks").is_some_and(Value::is_object) {
        settings.insert("hooks".into(), json!({}));
    }
    if let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) {
        for spec in &EVENTS {
            let mut groups = without_ours(paths, hooks.get(spec.name));
            groups.push(hook_group(paths, spec));
            hooks.insert(spec.name.into(), Value::Array(groups));
        }
    }
    write_settings(paths, &settings, existed)?;
    install_skills(paths);
    Ok(status(paths))
}

pub fn disconnect(paths: &Paths) -> Result<Connection, String> {
    let (mut settings, existed) = read_settings(paths)?;
    let mut touched = false;
    if let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) {
        touched = true;
        let names: Vec<String> = hooks.keys().cloned().collect();
        for name in names {
            if !hooks.get(&name).is_some_and(Value::is_array) {
                continue;
            }
            let kept = without_ours(paths, hooks.get(&name));
            if kept.is_empty() {
                hooks.shift_remove(&name);
            } else {
                hooks.insert(name, Value::Array(kept));
            }
        }
        if hooks.is_empty() {
            settings.shift_remove("hooks");
        }
    }
    if touched && existed {
        write_settings(paths, &settings, existed)?;
    }
    remove_skills(paths);
    Ok(status(paths))
}
