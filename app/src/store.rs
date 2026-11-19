use crate::presence::pid_alive;
use crate::reducer::{self, Overall, Session, View, ViewOptions};
use nibble_core::{now_ms, sanitize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const DAY: i64 = 86_400_000;
const MAX_EVENTS: usize = 2000;

struct Record {
    base: Session,
    events: Vec<Value>,
    state: Session,
    last_t: i64,
}

#[derive(Default)]
struct Inner {
    records: HashMap<String, Record>,
    offsets: HashMap<String, u64>,
    dirty: bool,
}

pub struct Store {
    events_dir: PathBuf,
    registry_dir: PathBuf,
    ready_ttl_min: i64,
    show_background: AtomicBool,
    stopped: AtomicBool,
    inner: Mutex<Inner>,
    on_change: OnceLock<Box<dyn Fn() + Send + Sync>>,
}

pub struct Snapshot {
    pub sessions: Vec<View>,
    pub overall: Overall,
}

fn lock(inner: &Mutex<Inner>) -> MutexGuard<'_, Inner> {
    inner.lock().unwrap_or_else(|e| e.into_inner())
}

impl Store {
    pub fn new(events_dir: PathBuf, registry_dir: PathBuf) -> Arc<Store> {
        Arc::new(Store {
            events_dir,
            registry_dir,
            ready_ttl_min: 30,
            show_background: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            inner: Mutex::new(Inner::default()),
            on_change: OnceLock::new(),
        })
    }

    pub fn set_on_change(&self, f: impl Fn() + Send + Sync + 'static) {
        let _ = self.on_change.set(Box::new(f));
    }

    pub fn set_show_background(&self, on: bool) {
        self.show_background.store(on, Ordering::SeqCst);
    }

    fn emit(&self) {
        if let Some(f) = self.on_change.get() {
            f();
        }
    }

    pub fn flush(&self) {
        let dirty = {
            let mut inner = lock(&self.inner);
            std::mem::replace(&mut inner.dirty, false)
        };
        if dirty {
            self.emit();
        }
    }

    pub fn start(self: &Arc<Self>) {
        let _ = fs::create_dir_all(&self.events_dir);
        self.prune();
        self.scan();
        self.poll_registry(true);
        self.flush();
        let store = self.clone();
        thread::Builder::new()
            .name("nibble-store".into())
            .spawn(move || store.run())
            .ok();
    }

    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }

    fn run(self: Arc<Self>) {
        use notify::{RecursiveMode, Watcher};
        let (tx, rx) = mpsc::channel::<String>();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                for path in event.paths {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.ends_with(".jsonl") {
                            let _ = tx.send(name.to_string());
                        }
                    }
                }
            }
        })
        .ok()
        .and_then(|mut w| w.watch(&self.events_dir, RecursiveMode::NonRecursive).ok().map(|_| w));

        let mut next_registry = Instant::now() + Duration::from_secs(1);
        let mut next_scan = Instant::now() + Duration::from_millis(2500);
        let mut next_tick = Instant::now() + Duration::from_secs(30);
        while !self.stopped.load(Ordering::SeqCst) {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(name) => {
                    self.read_log(&name);
                    while let Ok(more) = rx.try_recv() {
                        self.read_log(&more);
                    }
                    self.flush();
                }
                Err(mpsc::RecvTimeoutError::Timeout) | Err(mpsc::RecvTimeoutError::Disconnected) => {}
            }
            let now = Instant::now();
            if now >= next_registry {
                next_registry = now + Duration::from_secs(1);
                self.poll_registry(false);
                self.flush();
            }
            if now >= next_scan {
                next_scan = now + Duration::from_millis(2500);
                self.scan();
                self.flush();
            }
            if now >= next_tick {
                next_tick = now + Duration::from_secs(30);
                self.emit();
            }
        }
        drop(watcher);
    }

    fn prune(&self) {
        let Ok(entries) = fs::read_dir(&self.events_dir) else { return };
        let cutoff = std::time::SystemTime::now() - Duration::from_millis((7 * DAY) as u64);
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            if let Ok(modified) = entry.metadata().and_then(|m| m.modified()) {
                if modified < cutoff {
                    let _ = fs::remove_file(&path);
                }
            }
        }
    }

    fn scan(&self) {
        let Ok(entries) = fs::read_dir(&self.events_dir) else { return };
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".jsonl") {
                    self.read_log(name);
                }
            }
        }
    }

    pub fn read_log(&self, name: &str) {
        let mut inner = lock(&self.inner);
        read_log(&mut inner, &self.events_dir, name);
    }

    pub fn append(&self, sid: &str, event: Value) {
        let mut inner = lock(&self.inner);
        append(&mut inner, &self.events_dir, sid, event);
    }

    pub fn record(&self, sid: &str, event: Value) {
        self.append(sid, event);
        self.flush();
    }

    fn poll_registry(&self, first: bool) {
        let mut inner = lock(&self.inner);
        let mut seen: Vec<String> = Vec::new();
        let now = now_ms();
        if let Ok(entries) = fs::read_dir(&self.registry_dir) {
            for entry in entries.flatten() {
                let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
                let Some(stem) = name.strip_suffix(".json") else { continue };
                if stem.is_empty() || !stem.chars().all(|c| c.is_ascii_digit()) {
                    continue;
                }
                let Some(j) = fs::read_to_string(entry.path())
                    .ok()
                    .and_then(|t| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok())
                else {
                    continue;
                };
                let sid = sanitize(j.get("sessionId").and_then(Value::as_str).unwrap_or(""));
                let pid = j.get("pid").and_then(Value::as_f64).map(|p| p as i64).unwrap_or(0);
                if sid.is_empty() || !pid_alive(pid) {
                    continue;
                }
                seen.push(sid.clone());
                let (needs_reg, reg_status_differs) = {
                    let state = inner.records.get(&sid).map(|r| &r.state);
                    let reg_name: String = j
                        .get("name")
                        .and_then(Value::as_str)
                        .map(|n| n.chars().take(120).collect())
                        .unwrap_or_default();
                    let needs = match state {
                        None => true,
                        Some(s) => !s.in_registry || s.pid != pid || (!reg_name.is_empty() && s.reg_name != reg_name),
                    };
                    let status = j.get("status").and_then(Value::as_str).unwrap_or("");
                    let differs = !status.is_empty() && state.map(|s| s.reg_status.as_str()) != Some(status);
                    (needs, differs)
                };
                if needs_reg {
                    let reg_name: String = j
                        .get("name")
                        .and_then(Value::as_str)
                        .map(|n| n.chars().take(120).collect())
                        .unwrap_or_default();
                    let mut event = json!({ "ev": "Reg", "pid": pid });
                    if !reg_name.is_empty() {
                        event["rname"] = json!(reg_name);
                    }
                    for (src, dst) in [("cwd", "cwd"), ("entrypoint", "ep"), ("kind", "kind")] {
                        if let Some(v) = j.get(src).filter(|v| !v.is_null()) {
                            event[dst] = v.clone();
                        }
                    }
                    append(&mut inner, &self.events_dir, &sid, event);
                }
                if reg_status_differs {
                    let status = j.get("status").and_then(Value::as_str).unwrap_or("");
                    let mut event = json!({ "ev": "RegStatus", "status": status });
                    if status == "waiting" {
                        if let Some(w) = j.get("waitingFor").and_then(Value::as_str) {
                            event["waitingFor"] = json!(w.chars().take(120).collect::<String>());
                        }
                    }
                    let stamp = j.get("statusUpdatedAt").and_then(Value::as_f64).filter(|t| *t != 0.0);
                    event["t"] = json!(stamp.map(|t| t as i64).unwrap_or(now));
                    append(&mut inner, &self.events_dir, &sid, event);
                }
            }
        }
        let gone: Vec<String> = inner
            .records
            .iter()
            .filter(|(sid, rec)| rec.state.in_registry && !seen.contains(sid))
            .map(|(sid, _)| sid.clone())
            .collect();
        for sid in gone {
            append(&mut inner, &self.events_dir, &sid, json!({ "ev": "Gone" }));
        }
        if first {
            inner.dirty = true;
        }
    }

    pub fn clear(&self) {
        {
            let mut inner = lock(&self.inner);
            if let Ok(entries) = fs::read_dir(&self.events_dir) {
                for entry in entries.flatten() {
                    if entry.path().extension().and_then(|e| e.to_str()) == Some("jsonl") {
                        let _ = fs::remove_file(entry.path());
                    }
                }
            }
            inner.records.clear();
            inner.offsets.clear();
        }
        self.poll_registry(true);
        self.flush();
    }

    pub fn snapshot(&self) -> Snapshot {
        let now = now_ms();
        let opts = ViewOptions { ready_ttl_min: self.ready_ttl_min, show_background: self.show_background.load(Ordering::SeqCst) };
        let inner = lock(&self.inner);
        let views: Vec<View> = inner.records.values().map(|r| reducer::view(&r.state, now, &opts)).collect();
        drop(inner);
        let overall = reducer::overall(&views);
        let sessions = reducer::sort_views(views.into_iter().filter(|v| !v.hidden).collect());
        Snapshot { sessions, overall }
    }

    pub fn get(&self, sid: &str) -> Option<Session> {
        lock(&self.inner).records.get(&sanitize(sid)).map(|r| r.state.clone())
    }

    pub fn live_pids(&self) -> Vec<i64> {
        lock(&self.inner)
            .records
            .values()
            .filter(|r| r.state.in_registry && r.state.pid != 0)
            .map(|r| r.state.pid)
            .collect()
    }
}

fn read_log(inner: &mut Inner, dir: &Path, name: &str) {
    let Some(sid) = name.strip_suffix(".jsonl") else { return };
    let file = dir.join(name);
    let Ok(size) = fs::metadata(&file).map(|m| m.len()) else { return };
    let mut offset = inner.offsets.get(name).copied().unwrap_or(0);
    if size < offset {
        offset = 0;
        inner.records.remove(sid);
    }
    if size == offset {
        return;
    }
    let mut buf = vec![0u8; (size - offset) as usize];
    let read = fs::File::open(&file).and_then(|mut f| {
        f.seek(SeekFrom::Start(offset))?;
        f.read_exact(&mut buf)
    });
    if read.is_err() {
        return;
    }
    let Some(end) = buf.iter().rposition(|b| *b == b'\n') else { return };
    inner.offsets.insert(name.to_string(), offset + end as u64 + 1);
    let mut events = Vec::new();
    for line in String::from_utf8_lossy(&buf[..end]).split('\n') {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(e) = serde_json::from_str::<Value>(line) {
            if e.get("ev").is_some_and(|v| v.as_str().is_some_and(|s| !s.is_empty())) {
                events.push(e);
            }
        }
    }
    if !events.is_empty() {
        ingest(inner, sid, events);
    }
}

fn ingest(inner: &mut Inner, sid: &str, events: Vec<Value>) {
    let rec = inner.records.entry(sid.to_string()).or_insert_with(|| Record {
        base: reducer::blank(sid),
        events: Vec::new(),
        state: reducer::blank(sid),
        last_t: 0,
    });
    let mut out_of_order = false;
    for e in &events {
        let t = reducer::event_time(e);
        if t < rec.last_t {
            out_of_order = true;
        } else {
            rec.last_t = t;
        }
    }
    rec.events.extend(events.iter().cloned());
    if out_of_order {
        rec.events.sort_by_key(reducer::event_time);
        rec.state = rec.base.clone();
        for e in &rec.events {
            reducer::apply(&mut rec.state, e);
        }
    } else {
        for e in &events {
            reducer::apply(&mut rec.state, e);
        }
    }
    if rec.events.len() > MAX_EVENTS {
        let drained: Vec<Value> = rec.events.drain(..MAX_EVENTS / 2).collect();
        for e in &drained {
            reducer::apply(&mut rec.base, e);
        }
    }
    inner.dirty = true;
}

fn append(inner: &mut Inner, dir: &Path, sid: &str, event: Value) {
    let sid = sanitize(sid);
    if sid.is_empty() {
        return;
    }
    let mut e = match event {
        Value::Object(map) => map,
        _ => return,
    };
    e.entry("t").or_insert_with(|| json!(now_ms()));
    e.insert("sid".into(), json!(sid));
    let _ = fs::create_dir_all(dir);
    let line = format!("{}\n", Value::Object(e));
    let written = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(format!("{sid}.jsonl")))
        .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
    if written.is_err() {
        return;
    }
    read_log(inner, dir, &format!("{sid}.jsonl"));
}
