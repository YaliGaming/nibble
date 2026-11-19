use serde_json::{json, Map, Value};

pub fn clip(text: &str, n: usize) -> String {
    if text.chars().count() > n {
        let mut out: String = text.chars().take(n).collect();
        out.push('…');
        out
    } else {
        text.to_string()
    }
}

fn str_of<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn base_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or("").to_string()
}

fn hostname(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    let host = if host.starts_with('[') {
        host.split(']').next().map(|h| format!("{h}]"))?
    } else {
        host.split(':').next()?.to_string()
    };
    if host.is_empty() { None } else { Some(host.to_lowercase()) }
}

pub fn tool_detail(name: &str, input: Option<&Value>) -> Option<String> {
    let input = input.filter(|v| v.is_object())?;
    match name {
        "Bash" | "PowerShell" => {
            let text = str_of(input, "description").filter(|s| !s.is_empty()).or_else(|| str_of(input, "command"))?;
            Some(clip(text, 100))
        }
        "Edit" | "Write" | "Read" | "NotebookEdit" => {
            let path = str_of(input, "file_path").filter(|s| !s.is_empty()).or_else(|| str_of(input, "notebook_path"))?;
            Some(clip(&base_name(path), 80))
        }
        "Grep" | "Glob" => str_of(input, "pattern").map(|s| clip(s, 80)),
        "WebSearch" => str_of(input, "query").map(|s| clip(s, 80)),
        "WebFetch" => str_of(input, "url").and_then(hostname),
        "Agent" | "Task" => str_of(input, "description").map(|s| clip(s, 80)),
        "AskUserQuestion" => {
            let first = input.get("questions")?.as_array()?.first()?;
            str_of(first, "question").map(|s| clip(s, 200))
        }
        _ => None,
    }
}

pub fn compact(e: &Value, t: i64, keep_text: bool) -> String {
    let text = |value: Option<String>| if keep_text { value } else { None };
    let mut o = Map::new();
    o.insert("t".into(), json!(t));
    let ev = str_of(e, "hook_event_name").unwrap_or("");
    o.insert("ev".into(), json!(ev));
    o.insert("sid".into(), e.get("session_id").cloned().unwrap_or(Value::Null));
    if let Some(cwd) = e.get("cwd") {
        o.insert("cwd".into(), cwd.clone());
    }
    if let Some(agent) = e.get("agent_id").filter(|v| !v.is_null()) {
        let id = match agent {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        if !id.is_empty() {
            o.insert("agent".into(), json!(id));
        }
    }
    if let Ok(ep) = std::env::var("CLAUDE_CODE_ENTRYPOINT") {
        if !ep.is_empty() {
            o.insert("ep".into(), json!(ep));
        }
    }
    let mut put = |key: &str, value: Option<String>| {
        if let Some(v) = value {
            o.insert(key.into(), json!(v));
        }
    };
    let clipped = |key: &str, n: usize| str_of(e, key).map(|s| clip(s, n));
    match ev {
        "SessionStart" => {
            if let Some(source) = str_of(e, "source") {
                put("source", Some(source.to_string()));
            }
            put("name", text(clipped("session_title", 120)));
        }
        "UserPromptSubmit" => {
            put("prompt", text(clipped("prompt", 400)));
            put("name", text(clipped("session_title", 120)));
        }
        "PreToolUse" | "PostToolUse" | "PostToolUseFailure" | "PermissionRequest" => {
            let tool = str_of(e, "tool_name").unwrap_or("");
            put("tool", str_of(e, "tool_name").map(str::to_string));
            put("detail", text(tool_detail(tool, e.get("tool_input"))));
            if e.get("is_interrupt").is_some_and(|v| v.as_bool() == Some(true) || (v.is_number() && v.as_f64() != Some(0.0))) {
                o.insert("interrupt".into(), json!(1));
            }
        }
        "Notification" => {
            put("ntype", str_of(e, "notification_type").map(str::to_string));
            put("msg", text(clipped("message", 300)));
        }
        "Stop" => put("msg", text(clipped("last_assistant_message", 4000))),
        "StopFailure" => {
            put("error", str_of(e, "error").map(str::to_string));
            let message = str_of(e, "last_assistant_message").filter(|s| !s.is_empty()).or_else(|| str_of(e, "error_details"));
            put("msg", text(message.map(|s| clip(s, 600))));
        }
        "SessionEnd" => put("reason", str_of(e, "reason").map(str::to_string)),
        _ => {}
    }
    Value::Object(o).to_string()
}
