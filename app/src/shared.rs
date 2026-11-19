use crate::connect::{self, Connection};
use crate::locales::{self, Tables};
use crate::pets::{self, Pet};
use crate::quick::{self, PrintJob};
use crate::reducer::Status;
use crate::store::Store;
use crate::win;
use nibble_core::config::{Anchor, Config};
use nibble_core::paths::Paths;
use nibble_core::{now_ms, sanitize};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;

pub const PANEL_WIDTH: f64 = 380.0;
pub const BUBBLE_ZONE: f64 = 62.0;
pub const CONTROLS_ZONE: f64 = 50.0;
const SLEEP_AFTER: i64 = 10 * 60_000;

pub fn panel_height(view: &str) -> Option<f64> {
    match view {
        "chat" => Some(196.0),
        "home" | "thread" => Some(520.0),
        "settings" => Some(600.0),
        _ => None,
    }
}

pub struct Ask {
    pub id: u64,
    pub tool: String,
    pub detail: String,
    pub answer: Option<oneshot::Sender<Option<String>>>,
}

pub struct Drag {
    pub dx: f64,
    pub dy: f64,
    pub generation: u64,
}

pub struct Runtime {
    pub connection: Connection,
    pub hatching: bool,
    pub last_hatch: Option<Value>,
    pub shortcut_ok: bool,
    pub panel_view: String,
    pub keep_panel_open: bool,
    pub panel_after_menu: Option<String>,
    pub menu_open: bool,
    pub drag: Option<Drag>,
    pub pointer_inside: bool,
    pub pet_ignoring: bool,
    pub broadcast_pending: bool,
    pub asks: HashMap<String, Ask>,
    pub ask_counter: u64,
    pub alerted: HashMap<String, Status>,
    pub alerts_armed: bool,
    pub tray_key: String,
    pub drag_generation: u64,
}

pub struct Shared {
    pub app: AppHandle,
    pub paths: Paths,
    pub config: Mutex<Config>,
    pub pets: Mutex<Vec<Pet>>,
    pub tables: Tables,
    pub store: Arc<Store>,
    pub rt: Mutex<Runtime>,
}

pub type Ctx = Arc<Shared>;

pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Layout {
    pub mini: bool,
    pub pet_id: Option<String>,
    pub px: u32,
    pub width: f64,
    pub height: f64,
}

impl Shared {
    pub fn config(&self) -> Config {
        lock(&self.config).clone()
    }

    pub fn save_config(&self) {
        let config = lock(&self.config).clone();
        config.save(&self.paths.config_file);
    }

    pub fn language(&self) -> String {
        let preferred: Vec<String> = sys_locale::get_locales().collect();
        self.tables.pick(&lock(&self.config).lang, &preferred)
    }

    pub fn strings(&self) -> Map<String, Value> {
        self.tables.strings(&self.language())
    }

    pub fn t(&self, key: &str) -> String {
        locales::translate(&self.strings(), key, &[])
    }

    pub fn current_pet(&self) -> Option<Pet> {
        let id = lock(&self.config).pet.clone();
        if id == "mini" {
            return None;
        }
        let pets = lock(&self.pets);
        pets.iter()
            .find(|p| p.id == id)
            .or_else(|| pets.iter().find(|p| p.id == "ember"))
            .or_else(|| pets.first())
            .cloned()
    }

    pub fn layout(&self) -> Layout {
        let size = lock(&self.config).size;
        match self.current_pet() {
            None => Layout { mini: true, pet_id: None, px: 0, width: 250.0, height: BUBBLE_ZONE + CONTROLS_ZONE + 6.0 },
            Some(pet) => {
                let m = pets::metrics(&pet);
                let px = ((96.0 * size) / m.w.max(m.h) as f64).round().max(1.0) as u32;
                Layout {
                    mini: false,
                    pet_id: Some(pet.id.clone()),
                    px,
                    width: ((m.lw as f64) * px as f64 + 40.0).max(250.0),
                    height: BUBBLE_ZONE + (m.lh as f64 - pets::TUCK as f64) * px as f64 + CONTROLS_ZONE,
                }
            }
        }
    }

    pub fn public_state(&self) -> Value {
        let snap = self.store.snapshot();
        let now = now_ms();
        let sleeping = snap.overall.status == Status::Idle && now - snap.overall.last_activity > SLEEP_AFTER;
        let mut overall = serde_json::to_value(&snap.overall).unwrap_or(Value::Null);
        overall["sleeping"] = json!(sleeping);
        let sessions: Vec<_> = snap.sessions.into_iter().take(40).collect();
        let config = self.config();
        let strings = self.strings();
        let rt = lock(&self.rt);
        let asks: Map<String, Value> = rt
            .asks
            .iter()
            .map(|(sid, a)| (sid.clone(), json!({ "tool": a.tool, "detail": a.detail })))
            .collect();
        let mut connection = serde_json::to_value(&rt.connection).unwrap_or(Value::Null);
        connection["settingsFile"] = json!(tilde_path(&rt.connection.settings_file));
        let shortcut_ok = rt.shortcut_ok;
        let hatching = rt.hatching;
        let last_hatch = rt.last_hatch.clone();
        drop(rt);
        let pets = lock(&self.pets).clone();
        json!({
            "now": now,
            "sessions": sessions,
            "overall": overall,
            "config": {
                "pet": config.pet,
                "size": config.size,
                "lang": self.language(),
                "langSetting": config.lang,
                "shortcut": config.shortcut,
                "shortcutOk": shortcut_ok,
                "quickMode": config.quick_mode,
                "quickCwd": config.quick_cwd,
                "welcomed": config.welcomed,
                "showBackground": config.show_background,
                "storeText": config.store_text,
                "sound": config.sound,
                "notify": config.notify,
                "approvals": config.approvals,
            },
            "brand": nibble_core::NAME,
            "dir": if locales::is_rtl(&strings) { "rtl" } else { "ltr" },
            "strings": strings,
            "languages": self.tables.list(),
            "asks": asks,
            "layout": self.layout(),
            "pets": pets,
            "connection": connection,
            "hatching": hatching,
            "lastHatch": last_hatch,
            "claudeFound": quick::find_claude().is_some(),
            "canOpenClaudeApp": protocol_registered("claude"),
            "platform": "win32",
        })
    }

    pub fn broadcast(self: &Arc<Self>) {
        {
            let mut rt = lock(&self.rt);
            if rt.broadcast_pending {
                return;
            }
            rt.broadcast_pending = true;
        }
        let ctx = self.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(60)).await;
            lock(&ctx.rt).broadcast_pending = false;
            let c = ctx.clone();
            let state = tauri::async_runtime::spawn_blocking(move || c.public_state()).await.unwrap_or(Value::Null);
            for label in ["pet", "panel"] {
                let _ = ctx.app.emit_to(label, "state", &state);
            }
            win::update_tray(&ctx, &state);
        });
    }

    pub fn set_config(self: &Arc<Self>, patch: &Value) {
        let Some(patch) = patch.as_object() else { return };
        let before = self.config();
        {
            let mut config = lock(&self.config);
            if let Some(pet) = patch.get("pet").and_then(Value::as_str) {
                if pet == "mini" || lock(&self.pets).iter().any(|p| p.id == pet) {
                    config.pet = pet.to_string();
                }
            }
            if let Some(size) = patch.get("size").and_then(Value::as_f64).filter(|s| s.is_finite()) {
                config.size = ((size * 20.0).round() / 20.0).clamp(0.6, 2.0);
            }
            if let Some(mode) = patch.get("quickMode").and_then(Value::as_str) {
                if mode == "background" || mode == "terminal" {
                    config.quick_mode = mode.to_string();
                }
            }
            if let Some(cwd) = patch.get("quickCwd").and_then(Value::as_str) {
                config.quick_cwd = cwd.to_string();
            }
            if let Some(shortcut) = patch.get("shortcut").and_then(Value::as_str) {
                config.shortcut = shortcut.chars().take(60).collect();
            }
            if let Some(b) = patch.get("welcomed").and_then(Value::as_bool) {
                config.welcomed = b;
            }
            let flag = |key: &str| patch.get(key).and_then(Value::as_bool);
            if let Some(b) = flag("showBackground") {
                config.show_background = b;
            }
            if let Some(b) = flag("storeText") {
                config.store_text = b;
            }
            if let Some(b) = flag("sound") {
                config.sound = b;
            }
            if let Some(b) = flag("notify") {
                config.notify = b;
            }
            if let Some(b) = flag("approvals") {
                config.approvals = b;
            }
            if let Some(lang) = patch.get("lang").and_then(Value::as_str) {
                if lang == "auto" || self.tables.has(lang) {
                    config.lang = lang.to_string();
                }
            }
        }
        self.save_config();
        let now = self.config();
        self.store.set_show_background(now.show_background);
        if now.pet != before.pet || now.size != before.size {
            win::resize_pet(self);
        }
        if now.shortcut != before.shortcut {
            win::register_shortcut(self);
        }
        if now.lang != before.lang {
            self.write_app_info();
        }
        if !now.approvals {
            let sids: Vec<String> = lock(&self.rt).asks.keys().cloned().collect();
            for sid in sids {
                self.settle_ask(&sid, None);
            }
        }
        self.broadcast();
    }

    pub fn write_app_info(&self) {
        let mut texts = Map::new();
        for key in ["on", "off", "started", "chat", "notInstalled", "failed"] {
            texts.insert(key.into(), json!(self.t(&format!("cli.{key}"))));
        }
        let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        let info = json!({ "exe": exe, "args": [], "lang": self.language(), "texts": texts });
        let _ = std::fs::create_dir_all(&self.paths.pet_home);
        if let Ok(text) = serde_json::to_string_pretty(&info) {
            let _ = std::fs::write(&self.paths.app_info_file, text);
        }
    }

    pub fn turn_off(self: &Arc<Self>) {
        lock(&self.config).visible = false;
        self.save_config();
        let app = self.app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(80)).await;
            app.exit(0);
        });
    }

    pub fn settle_ask(self: &Arc<Self>, sid: &str, decision: Option<&str>) {
        let ask = lock(&self.rt).asks.remove(sid);
        let Some(mut ask) = ask else { return };
        if let Some(tx) = ask.answer.take() {
            let _ = tx.send(decision.map(str::to_string));
        }
        self.broadcast();
    }

    pub fn drop_answered_asks(self: &Arc<Self>) {
        let sids: Vec<String> = lock(&self.rt).asks.keys().cloned().collect();
        for sid in sids {
            let waiting = self.store.get(&sid).is_some_and(|s| s.status == Status::NeedsInput);
            if !waiting {
                self.settle_ask(&sid, None);
            }
        }
    }

    pub fn register_ask(self: &Arc<Self>, sid: &str, tool: String, detail: String) -> (u64, oneshot::Receiver<Option<String>>) {
        let (tx, rx) = oneshot::channel();
        let id = {
            let mut rt = lock(&self.rt);
            rt.ask_counter += 1;
            let id = rt.ask_counter;
            rt.asks.insert(sid.to_string(), Ask { id, tool, detail, answer: Some(tx) });
            id
        };
        self.broadcast();
        (id, rx)
    }

    pub fn forget_ask(self: &Arc<Self>, sid: &str, id: u64) {
        let removed = {
            let mut rt = lock(&self.rt);
            if rt.asks.get(sid).is_some_and(|a| a.id == id) {
                rt.asks.remove(sid);
                true
            } else {
                false
            }
        };
        if removed {
            self.broadcast();
        }
    }

    pub fn announce_changes(self: &Arc<Self>) {
        let snap = self.store.snapshot();
        let mut loudest: Option<crate::reducer::View> = None;
        {
            let mut rt = lock(&self.rt);
            for s in snap.sessions {
                let was = rt.alerted.insert(s.id.clone(), s.status);
                let worth = matches!(s.status, Status::NeedsInput | Status::Ready | Status::Blocked);
                if !rt.alerts_armed || was == Some(s.status) || !worth {
                    continue;
                }
                if loudest.is_none() || s.status == Status::NeedsInput {
                    loudest = Some(s);
                }
            }
        }
        let Some(loudest) = loudest else { return };
        let config = self.config();
        if config.sound {
            let _ = self.app.emit_to("pet", "pet:chime", loudest.status.key());
        }
        if config.notify {
            let body = if !loudest.title.is_empty() {
                loudest.title.clone()
            } else if !loudest.no_folder && !loudest.folder.is_empty() {
                loudest.folder.clone()
            } else {
                nibble_core::NAME.to_string()
            };
            let title = self.t(&format!("alert.{}", loudest.status.key()));
            win::notify(self, &title, &body, &loudest.id);
        }
    }

    pub fn start_quick_chat(self: &Arc<Self>, payload: &Value) -> Value {
        let prompt = payload.get("prompt").and_then(Value::as_str).unwrap_or("").trim().to_string();
        let cwd = payload.get("cwd").and_then(Value::as_str).unwrap_or("").to_string();
        if prompt.is_empty() {
            return json!({ "ok": false, "error": "empty" });
        }
        if quick::find_claude().is_none() {
            return json!({ "ok": false, "error": "claude-not-found" });
        }
        let dir = if cwd.is_empty() { self.paths.quick_dir.to_string_lossy().into_owned() } else { cwd };
        if self.config().quick_mode == "terminal" {
            let launcher = connect::hook_binary(&self.paths);
            let res = quick::chat_in_terminal(&self.paths, &dir, &prompt, launcher);
            if res.get("pasted").and_then(Value::as_bool) == Some(true) {
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    let _ = clipboard.set_text(prompt);
                }
            }
            return res;
        }
        let sid = uuid::Uuid::new_v4().to_string();
        self.store.record(&sid, json!({ "ev": "QuickChat", "prompt": prompt.chars().take(2000).collect::<String>(), "cwd": dir }));
        self.run_and_record(&sid, prompt, dir, false);
        json!({ "ok": true, "sid": sid })
    }

    pub fn reply_to_chat(self: &Arc<Self>, payload: &Value) -> Value {
        let sid = payload.get("sid").and_then(Value::as_str).unwrap_or("").to_string();
        let prompt = payload.get("prompt").and_then(Value::as_str).unwrap_or("").trim().to_string();
        let session = self.store.get(&sid);
        let Some(session) = session.filter(|s| !prompt.is_empty() && s.origin == "quick") else {
            return json!({ "ok": false, "error": "empty" });
        };
        self.store.record(&sid, json!({ "ev": "QuickChat", "prompt": prompt.chars().take(2000).collect::<String>() }));
        self.run_and_record(&sid, prompt, session.cwd, true);
        json!({ "ok": true, "sid": sid })
    }

    fn run_and_record(self: &Arc<Self>, sid: &str, prompt: String, cwd: String, resume: bool) {
        let ctx = self.clone();
        let sid = sid.to_string();
        std::thread::spawn(move || {
            let res = quick::run_print(
                &ctx.paths,
                PrintJob { prompt: &prompt, cwd: &cwd, session_id: &sid, resume, concise: true, timeout: Duration::from_secs(20 * 60) },
            );
            let event = match res {
                Ok(text) => json!({ "ev": "QuickChatDone", "result": text.chars().take(30000).collect::<String>() }),
                Err(e) => json!({ "ev": "QuickChatDone", "error": e.chars().take(600).collect::<String>() }),
            };
            ctx.store.record(&sid, event);
        });
    }

    pub fn hatch_pet(self: &Arc<Self>, description: &str) -> Value {
        let description: String = description.trim().chars().take(400).collect();
        {
            let rt = lock(&self.rt);
            if description.is_empty() || rt.hatching {
                return json!({ "ok": false, "error": "busy" });
            }
        }
        if quick::find_claude().is_none() {
            return json!({ "ok": false, "error": "claude-not-found" });
        }
        {
            let mut rt = lock(&self.rt);
            rt.hatching = true;
            rt.last_hatch = None;
        }
        self.broadcast();
        let sid = uuid::Uuid::new_v4().to_string();
        let quick_dir = self.paths.quick_dir.to_string_lossy().into_owned();
        self.store.record(&sid, json!({ "ev": "QuickChat", "hatch": true, "prompt": description, "cwd": quick_dir }));
        let res = quick::run_print(
            &self.paths,
            PrintJob {
                prompt: &pets::hatch_prompt(&description),
                cwd: &quick_dir,
                session_id: &sid,
                resume: false,
                concise: false,
                timeout: Duration::from_secs(5 * 60),
            },
        );
        let done = match &res {
            Ok(_) => json!({ "ev": "QuickChatDone", "result": "hatched" }),
            Err(e) => json!({ "ev": "QuickChatDone", "error": e.chars().take(300).collect::<String>() }),
        };
        self.store.record(&sid, done);
        let outcome = res.and_then(|text| {
            let value = pets::extract_json(&text)?;
            pets::save_custom(&self.paths, &value)
        });
        match outcome {
            Ok(id) => {
                *lock(&self.pets) = pets::load_all(&self.paths);
                let last = json!({ "ok": true, "id": id });
                {
                    let mut rt = lock(&self.rt);
                    rt.last_hatch = Some(last.clone());
                    rt.hatching = false;
                }
                self.set_config(&json!({ "pet": id }));
                last
            }
            Err(e) => {
                let last = json!({ "ok": false, "error": e.chars().take(300).collect::<String>() });
                {
                    let mut rt = lock(&self.rt);
                    rt.last_hatch = Some(last.clone());
                    rt.hatching = false;
                }
                self.broadcast();
                last
            }
        }
    }

    pub fn reload_pets(&self) {
        *lock(&self.pets) = pets::load_all(&self.paths);
        lock(&self.rt).tray_key.clear();
    }

    pub fn session_id(&self, value: Option<&Value>) -> String {
        sanitize(value.and_then(Value::as_str).unwrap_or(""))
    }

    pub fn set_anchor(&self, anchor: Option<Anchor>) {
        lock(&self.config).anchor = anchor;
    }
}

pub fn tilde_path(file: &str) -> String {
    let Some(home) = dirs_home() else { return file.to_string() };
    if file.starts_with(&home) {
        format!("~{}", file[home.len()..].replace('\\', "/"))
    } else {
        file.to_string()
    }
}

fn dirs_home() -> Option<String> {
    std::env::var(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).ok()
}

#[cfg(windows)]
pub fn protocol_registered(scheme: &str) -> bool {
    use winreg::enums::HKEY_CLASSES_ROOT;
    use winreg::RegKey;
    RegKey::predef(HKEY_CLASSES_ROOT)
        .open_subkey(scheme)
        .and_then(|k| k.get_value::<String, _>("URL Protocol"))
        .is_ok()
}

#[cfg(not(windows))]
pub fn protocol_registered(_scheme: &str) -> bool {
    false
}
