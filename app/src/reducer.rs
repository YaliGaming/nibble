use serde::Serialize;
use serde_json::Value;

const HOUR: i64 = 3_600_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Idle,
    Running,
    NeedsInput,
    Ready,
    Blocked,
}

impl Status {
    pub fn active(self) -> bool {
        matches!(self, Status::Running | Status::NeedsInput)
    }

    pub fn key(self) -> &'static str {
        match self {
            Status::Idle => "idle",
            Status::Running => "running",
            Status::NeedsInput => "needs_input",
            Status::Ready => "ready",
            Status::Blocked => "blocked",
        }
    }
}

const PRIORITY: [Status; 4] = [Status::NeedsInput, Status::Blocked, Status::Ready, Status::Running];

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct Activity {
    pub tool: String,
    pub detail: String,
}

#[derive(Clone, Debug)]
pub struct Pending {
    pub key: Option<String>,
    pub agent: String,
}

#[derive(Clone, Debug)]
pub struct Session {
    pub id: String,
    pub cwd: String,
    pub name: String,
    pub reg_name: String,
    pub first_prompt: String,
    pub last_prompt: String,
    pub status: Status,
    pub status_since: i64,
    pub activity: Option<Activity>,
    pub message: String,
    pub result: String,
    pub unread: bool,
    pub ended: bool,
    pub dismissed: bool,
    pub origin: String,
    pub surface: String,
    pub kind: String,
    pub pid: i64,
    pub hooked: bool,
    pub last_hook_t: i64,
    pub in_registry: bool,
    pub reg_status: String,
    pub pending: Option<Pending>,
    pub started_at: i64,
    pub updated_at: i64,
}

pub fn blank(id: &str) -> Session {
    Session {
        id: id.to_string(),
        cwd: String::new(),
        name: String::new(),
        reg_name: String::new(),
        first_prompt: String::new(),
        last_prompt: String::new(),
        status: Status::Idle,
        status_since: 0,
        activity: None,
        message: String::new(),
        result: String::new(),
        unread: false,
        ended: false,
        dismissed: false,
        origin: "claude".into(),
        surface: String::new(),
        kind: String::new(),
        pid: 0,
        hooked: false,
        last_hook_t: 0,
        in_registry: false,
        reg_status: String::new(),
        pending: None,
        started_at: 0,
        updated_at: 0,
    }
}

fn set_status(s: &mut Session, status: Status, t: i64) {
    if s.status != status {
        s.status = status;
        s.status_since = t;
    }
}

fn text<'a>(e: &'a Value, key: &str) -> &'a str {
    e.get(key).and_then(Value::as_str).unwrap_or("")
}

pub fn event_time(e: &Value) -> i64 {
    e.get("t").and_then(Value::as_f64).map(|t| t as i64).unwrap_or(0)
}

fn truthy(e: &Value, key: &str) -> bool {
    match e.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Null) | None => false,
        Some(_) => true,
    }
}

fn tool_key(e: &Value) -> String {
    format!("{}|{}", text(e, "tool"), text(e, "detail"))
}

fn is_hook_event(ev: &str) -> bool {
    matches!(
        ev,
        "SessionStart"
            | "UserPromptSubmit"
            | "PreToolUse"
            | "PostToolUse"
            | "PostToolUseFailure"
            | "PermissionRequest"
            | "Notification"
            | "ElicitationResult"
            | "Stop"
            | "StopFailure"
            | "SessionEnd"
    )
}

fn needs_input_notification(kind: &str) -> bool {
    matches!(kind, "permission_prompt" | "elicitation_dialog" | "elicitation_url_dialog" | "agent_needs_input")
}

fn activity_of(e: &Value) -> Option<Activity> {
    Some(Activity { tool: text(e, "tool").to_string(), detail: text(e, "detail").to_string() })
}

pub fn apply(s: &mut Session, e: &Value) {
    let t = event_time(e);
    if s.started_at == 0 {
        s.started_at = t;
    }
    if !text(e, "cwd").is_empty() {
        s.cwd = text(e, "cwd").to_string();
    }
    if !text(e, "ep").is_empty() {
        s.surface = text(e, "ep").to_string();
    }
    if !text(e, "name").is_empty() {
        s.name = text(e, "name").to_string();
    }
    let ev = text(e, "ev");
    if is_hook_event(ev) {
        s.hooked = true;
        s.last_hook_t = t;
        s.updated_at = t;
    }
    let agent = text(e, "agent").to_string();

    match ev {
        "SessionStart" => {
            s.ended = false;
            let source = text(e, "source");
            if (source == "startup" || source == "clear") && s.status.active() {
                set_status(s, Status::Idle, t);
            }
        }
        "UserPromptSubmit" => {
            set_status(s, Status::Running, t);
            let prompt = text(e, "prompt");
            if !prompt.is_empty() {
                s.last_prompt = prompt.to_string();
                if s.first_prompt.is_empty() {
                    s.first_prompt = prompt.to_string();
                }
            }
            s.activity = None;
            s.message.clear();
            s.result.clear();
            s.pending = None;
            s.unread = false;
            s.ended = false;
            s.dismissed = false;
        }
        "PreToolUse" => {
            let tool = text(e, "tool");
            if tool == "AskUserQuestion" || tool == "ExitPlanMode" {
                s.pending = Some(Pending { key: Some(tool_key(e)), agent });
                set_status(s, Status::NeedsInput, t);
                s.message = text(e, "detail").to_string();
                s.activity = activity_of(e);
            } else {
                if let Some(p) = &s.pending {
                    if p.agent == agent && p.key.as_deref() != Some(tool_key(e).as_str()) {
                        s.pending = None;
                    }
                }
                if s.pending.is_none() {
                    set_status(s, Status::Running, t);
                    s.activity = activity_of(e);
                }
            }
        }
        "PermissionRequest" => {
            s.pending = Some(Pending { key: Some(tool_key(e)), agent });
            set_status(s, Status::NeedsInput, t);
            s.activity = activity_of(e);
            s.message.clear();
        }
        "Notification" => {
            if needs_input_notification(text(e, "ntype")) {
                if s.pending.is_none() {
                    s.pending = Some(Pending { key: None, agent: String::new() });
                }
                set_status(s, Status::NeedsInput, t);
                if !text(e, "msg").is_empty() && s.message.is_empty() {
                    s.message = text(e, "msg").to_string();
                }
            }
        }
        "ElicitationResult" | "Answered" => {
            s.pending = None;
            if s.status == Status::NeedsInput {
                set_status(s, Status::Running, t);
            }
        }
        "PostToolUse" | "PostToolUseFailure" => {
            if let Some(p) = &s.pending {
                if p.key.is_none() || p.key.as_deref() == Some(tool_key(e).as_str()) {
                    s.pending = None;
                }
            }
            if truthy(e, "interrupt") {
                s.pending = None;
                s.activity = None;
                set_status(s, Status::Idle, t);
            } else if s.pending.is_none() {
                if s.status == Status::NeedsInput {
                    s.message.clear();
                }
                set_status(s, Status::Running, t);
            }
        }
        "Stop" => {
            s.pending = None;
            s.activity = None;
            set_status(s, Status::Ready, t);
            s.status_since = t;
            s.unread = true;
            s.message = text(e, "msg").to_string();
        }
        "StopFailure" => {
            s.pending = None;
            s.activity = None;
            set_status(s, Status::Blocked, t);
            s.status_since = t;
            s.unread = true;
            s.message = if !text(e, "msg").is_empty() { text(e, "msg") } else { text(e, "error") }.to_string();
        }
        "SessionEnd" => {
            s.ended = true;
            s.pending = None;
            s.activity = None;
            if s.status.active() {
                set_status(s, Status::Idle, t);
            }
        }
        "QuickChat" => {
            s.origin = if truthy(e, "hatch") { "hatch" } else { "quick" }.to_string();
            set_status(s, Status::Running, t);
            let prompt = text(e, "prompt");
            if !prompt.is_empty() {
                s.last_prompt = prompt.to_string();
                if s.first_prompt.is_empty() {
                    s.first_prompt = prompt.to_string();
                }
            }
            s.activity = None;
            s.message.clear();
            s.result.clear();
            s.unread = false;
            s.ended = false;
            s.dismissed = false;
            s.updated_at = t;
        }
        "QuickChatDone" => {
            s.ended = true;
            s.pending = None;
            s.activity = None;
            if !text(e, "error").is_empty() {
                set_status(s, Status::Blocked, t);
                s.message = text(e, "error").to_string();
            } else {
                set_status(s, Status::Ready, t);
                if !text(e, "result").is_empty() {
                    s.result = text(e, "result").to_string();
                    s.message = text(e, "result").to_string();
                }
            }
            s.status_since = t;
            s.unread = true;
            s.updated_at = t;
        }
        "Seen" => {
            s.unread = false;
            if s.status == Status::Ready || s.status == Status::Blocked {
                set_status(s, Status::Idle, t);
            }
        }
        "Dismiss" => {
            s.dismissed = true;
            s.unread = false;
        }
        "Reg" => {
            s.pid = e.get("pid").and_then(Value::as_f64).map(|p| p as i64).unwrap_or(0);
            if !text(e, "kind").is_empty() {
                s.kind = text(e, "kind").to_string();
            }
            if !text(e, "rname").is_empty() {
                s.reg_name = text(e, "rname").to_string();
            }
            s.in_registry = true;
            s.ended = false;
        }
        "RegStatus" => {
            let before = std::mem::replace(&mut s.reg_status, text(e, "status").to_string());
            match text(e, "status") {
                "busy" => {
                    if !s.hooked {
                        set_status(s, Status::Running, t);
                        s.pending = None;
                        s.message.clear();
                        s.unread = false;
                        s.dismissed = false;
                        s.updated_at = t;
                    } else if s.status == Status::NeedsInput && before == "waiting" {
                        s.pending = None;
                        s.message.clear();
                        set_status(s, Status::Running, t);
                    }
                }
                "waiting" => {
                    if !s.hooked || s.status == Status::Running {
                        if s.pending.is_none() {
                            s.pending = Some(Pending { key: None, agent: String::new() });
                        }
                        set_status(s, Status::NeedsInput, t);
                        if !text(e, "waitingFor").is_empty() && s.message.is_empty() {
                            s.message = text(e, "waitingFor").to_string();
                        }
                        if !s.hooked {
                            s.dismissed = false;
                            s.updated_at = t;
                        }
                    }
                }
                "idle" => {
                    if !s.hooked {
                        if s.status.active() {
                            set_status(s, Status::Ready, t);
                            s.pending = None;
                            s.message.clear();
                            s.unread = true;
                            s.updated_at = t;
                        }
                    } else if s.status == Status::Running && t > s.last_hook_t + 1500 {
                        set_status(s, Status::Idle, t);
                        s.activity = None;
                    }
                }
                _ => {}
            }
        }
        "Gone" => {
            s.in_registry = false;
            s.ended = true;
            s.pending = None;
            s.activity = None;
            if s.status.active() {
                set_status(s, Status::Idle, t);
            }
        }
        _ => {}
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub id: String,
    pub title: String,
    pub cwd: String,
    pub folder: String,
    pub no_folder: bool,
    pub background: bool,
    pub status: Status,
    pub unread: bool,
    pub hidden: bool,
    pub live: bool,
    pub activity: Option<Activity>,
    pub message: String,
    pub result: String,
    pub last_prompt: String,
    pub origin: String,
    pub surface: String,
    pub ended: bool,
    pub hooked: bool,
    pub updated_at: i64,
    pub status_since: i64,
    pub can_reply: bool,
}

pub struct ViewOptions {
    pub ready_ttl_min: i64,
    pub show_background: bool,
}

fn clip(text: &str, n: usize) -> String {
    if text.chars().count() > n {
        let cut: String = text.chars().take(n).collect();
        format!("{}…", cut.trim_end())
    } else {
        text.to_string()
    }
}

fn base_name(path: &str) -> String {
    path.split(['/', '\\']).filter(|p| !p.is_empty()).next_back().unwrap_or("").to_string()
}

fn first_non_empty<'a>(options: &[&'a str]) -> &'a str {
    options.iter().copied().find(|s| !s.is_empty()).unwrap_or("")
}

pub fn view(s: &Session, now: i64, opts: &ViewOptions) -> View {
    let ready_ttl = opts.ready_ttl_min * 60_000;
    let mut status = s.status;
    let mut unread = s.unread;
    if status == Status::Ready && now - s.status_since > ready_ttl {
        status = Status::Idle;
        unread = false;
    }
    if status == Status::Blocked && now - s.status_since > 4 * HOUR {
        status = Status::Idle;
        unread = false;
    }
    if status.active() && !s.in_registry && now - s.updated_at > 6 * HOUR {
        status = Status::Idle;
    }

    let live = s.in_registry && !s.ended;
    let quiet = now - s.updated_at;
    let keep = if live {
        12 * HOUR
    } else if s.origin == "quick" {
        HOUR
    } else {
        HOUR / 4
    };
    let background = s.origin == "claude"
        && (s.surface.to_lowercase().starts_with("sdk") || (!s.kind.is_empty() && s.kind != "interactive"));
    let hidden = s.dismissed
        || s.origin == "hatch"
        || (background && !opts.show_background)
        || (status == Status::Idle && quiet > keep);

    let first_line = clip(
        s.first_prompt.split('\n').find(|l| !l.trim().is_empty()).unwrap_or("").trim(),
        70,
    );
    let folder = base_name(&s.cwd);
    let placeholder_name = !s.reg_name.is_empty()
        && !folder.is_empty()
        && s.reg_name.chars().count() <= folder.chars().count() + 5
        && s.reg_name.to_lowercase().starts_with(&format!("{}-", folder.to_lowercase()));
    let title = if s.origin == "quick" {
        first_line.clone()
    } else {
        let reg = if placeholder_name { "" } else { s.reg_name.as_str() };
        first_non_empty(&[reg, &s.name, &first_line, &s.reg_name]).to_string()
    };
    let normalised = s.cwd.replace('\\', "/").to_lowercase();
    let no_folder = normalised.contains("/scratch-workspaces/") && folder.to_lowercase().starts_with("scratch-");
    let shown_folder = if no_folder { String::new() } else { folder.clone() };
    View {
        id: s.id.clone(),
        title: first_non_empty(&[&title, &shown_folder]).to_string(),
        cwd: s.cwd.clone(),
        folder: shown_folder,
        no_folder,
        background,
        status,
        unread,
        hidden,
        live,
        activity: s.activity.clone(),
        message: s.message.clone(),
        result: s.result.clone(),
        last_prompt: s.last_prompt.clone(),
        origin: s.origin.clone(),
        surface: s.surface.clone(),
        ended: s.ended,
        hooked: s.hooked,
        updated_at: s.updated_at,
        status_since: s.status_since,
        can_reply: s.origin == "quick" && !status.active(),
    }
}

pub fn sort_views(mut views: Vec<View>) -> Vec<View> {
    let rank = |v: &View| PRIORITY.iter().position(|p| *p == v.status).unwrap_or(PRIORITY.len());
    views.sort_by(|a, b| rank(a).cmp(&rank(b)).then(b.updated_at.cmp(&a.updated_at)));
    views
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Counts {
    pub needs_input: u32,
    pub blocked: u32,
    pub ready: u32,
    pub running: u32,
    pub idle: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overall {
    pub status: Status,
    pub count: usize,
    pub top: Option<View>,
    pub counts: Counts,
    pub last_activity: i64,
}

pub fn overall(views: &[View]) -> Overall {
    let visible: Vec<&View> = views.iter().filter(|v| !v.hidden).collect();
    let mut counts = Counts::default();
    for v in &visible {
        match v.status {
            Status::NeedsInput => counts.needs_input += 1,
            Status::Blocked => counts.blocked += 1,
            Status::Ready => counts.ready += 1,
            Status::Running => counts.running += 1,
            Status::Idle => counts.idle += 1,
        }
    }
    let last_activity = visible.iter().map(|v| v.updated_at).max().unwrap_or(0).max(0);
    for status in PRIORITY {
        let group: Vec<&&View> = visible.iter().filter(|v| v.status == status).collect();
        if !group.is_empty() {
            let mut top = group[0];
            for v in &group[1..] {
                if v.updated_at > top.updated_at {
                    top = v;
                }
            }
            return Overall { status, count: group.len(), top: Some((**top).clone()), counts, last_activity };
        }
    }
    Overall { status: Status::Idle, count: 0, top: None, counts, last_activity }
}
