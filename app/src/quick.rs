use nibble_core::paths::Paths;
use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const SYSTEM_PROMPT: &str =
    "This request comes from a small floating desktop companion window. Keep the answer short and direct unless detail is asked for.";

#[cfg(windows)]
const NO_WINDOW: u32 = 0x0800_0000;
#[cfg(windows)]
const DETACHED: u32 = 0x0000_0008;

static CLAUDE: Mutex<Option<(Option<PathBuf>, Instant)>> = Mutex::new(None);

fn which_all(name: &str) -> Vec<PathBuf> {
    which::which_all(name).map(|i| i.collect()).unwrap_or_default()
}

pub fn find_claude() -> Option<PathBuf> {
    let mut cache = CLAUDE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((found, at)) = cache.as_ref() {
        if found.is_some() || at.elapsed() < Duration::from_secs(10) {
            return found.clone();
        }
    }
    let found = which_all("claude");
    let mut bin = found
        .iter()
        .find(|f| f.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")))
        .or_else(|| found.first())
        .cloned();
    if bin.is_none() {
        if let Some(home) = dirs_home() {
            let native = home.join(".local").join("bin").join(if cfg!(windows) { "claude.exe" } else { "claude" });
            if native.exists() {
                bin = Some(native);
            }
        }
    }
    *cache = Some((bin.clone(), Instant::now()));
    bin
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

fn is_shim(bin: &Path) -> bool {
    cfg!(windows) && bin.extension().is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
}

fn clean_env(command: &mut Command) {
    const MARKERS: [&str; 5] = [
        "CLAUDECODE",
        "CLAUDE_CODE_ENTRYPOINT",
        "CLAUDE_CODE_SESSION_ID",
        "CLAUDE_CODE_HOST_SESSION_ID",
        "CLAUDE_CODE_CHILD_SESSION",
    ];
    if !MARKERS.iter().any(|k| std::env::var_os(k).is_some_and(|v| !v.is_empty())) {
        return;
    }
    for (key, _) in std::env::vars_os() {
        let Some(key) = key.to_str() else { continue };
        let upper = key.to_string();
        if upper == "CLAUDECODE"
            || upper.starts_with("CLAUDE_CODE_")
            || upper.starts_with("CLAUDE_AGENT_SDK_")
            || upper == "CLAUDE_PID"
            || upper == "CLAUDE_EFFORT"
            || upper.starts_with("CLAUDE_PREVIEW_")
        {
            command.env_remove(key);
        }
    }
    command.env_remove("ANTHROPIC_BASE_URL");
}

fn existing_dir(dir: &str) -> Option<PathBuf> {
    let p = PathBuf::from(dir);
    if !dir.is_empty() && p.is_dir() { Some(p) } else { None }
}

pub struct PrintJob<'a> {
    pub prompt: &'a str,
    pub cwd: &'a str,
    pub session_id: &'a str,
    pub resume: bool,
    pub concise: bool,
    pub timeout: Duration,
}

pub fn run_print(paths: &Paths, job: PrintJob) -> Result<String, String> {
    let bin = find_claude().ok_or("claude-not-found")?;
    let mut args: Vec<String> = vec!["-p".into(), "--output-format".into(), "json".into()];
    if !job.session_id.is_empty() {
        args.push(if job.resume { "--resume" } else { "--session-id" }.into());
        args.push(job.session_id.into());
    }
    if job.concise && !is_shim(&bin) {
        args.push("--append-system-prompt".into());
        args.push(SYSTEM_PROMPT.into());
    }
    let dir = existing_dir(job.cwd).unwrap_or_else(|| paths.quick_dir.clone());
    let _ = fs::create_dir_all(&dir);

    let mut command = Command::new(&bin);
    command
        .args(&args)
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    clean_env(&mut command);
    command.env("NIBBLE_CHAT", "1");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(NO_WINDOW);
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;

    let mut stdin = child.stdin.take();
    let prompt = job.prompt.to_string();
    std::thread::spawn(move || {
        if let Some(mut s) = stdin.take() {
            let _ = s.write_all(prompt.as_bytes());
        }
    });
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(s) = stdout.as_mut() {
            let _ = s.read_to_end(&mut buf);
        }
        String::from_utf8_lossy(&buf).into_owned()
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(s) = stderr.as_mut() {
            let _ = s.read_to_end(&mut buf);
        }
        String::from_utf8_lossy(&buf).into_owned()
    });

    let deadline = Instant::now() + job.timeout;
    let code = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    let out = out_reader.join().unwrap_or_default();
    let err = err_reader.join().unwrap_or_default();
    interpret(&out, &err, code)
}

fn clip(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn interpret(out: &str, err: &str, code: Option<i32>) -> Result<String, String> {
    let mut parsed: Option<Value> = serde_json::from_str(out).ok();
    if let Some(Value::Array(list)) = &parsed {
        parsed = list.iter().rev().find(|m| m.get("type").and_then(Value::as_str) == Some("result")).cloned();
    }
    if let Some(p) = parsed.filter(|p| p.get("type").and_then(Value::as_str) == Some("result")) {
        let text = p.get("result").and_then(Value::as_str).unwrap_or("").to_string();
        if p.get("is_error").and_then(Value::as_bool) == Some(true) {
            let fallback = p.get("subtype").and_then(Value::as_str).unwrap_or("error");
            return Err(if text.is_empty() { fallback.to_string() } else { text });
        }
        return Ok(text);
    }
    if code == Some(0) && !out.trim().is_empty() {
        return Ok(out.trim().to_string());
    }
    if code == Some(0) {
        let e = clip(err.trim(), 600);
        return Err(if e.is_empty() { "Claude Code finished without an answer.".into() } else { e });
    }
    let detail = if !err.trim().is_empty() {
        err.trim().to_string()
    } else if !out.trim().is_empty() {
        out.trim().to_string()
    } else {
        format!("claude exited with code {}", code.map_or("unknown".to_string(), |c| c.to_string()))
    };
    Err(clip(&detail, 600))
}

pub fn open_terminal(cwd: &str, command: &Path, args: &[String]) -> bool {
    let dir = existing_dir(cwd)
        .or_else(dirs_home)
        .unwrap_or_else(|| PathBuf::from("."));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let escape = |a: &str| a.replace(';', "\\;");
        let mut cmd = if !which_all("wt.exe").is_empty() {
            let mut c = Command::new("wt.exe");
            c.arg("-d").arg(escape(&dir.to_string_lossy())).arg(escape(&command.to_string_lossy()));
            for a in args {
                c.arg(escape(a));
            }
            c
        } else {
            let mut c = Command::new("cmd.exe");
            c.args(["/c", "start", "\"\"", "/D"]).arg(&dir).arg(command).args(args);
            c
        };
        clean_env(&mut cmd);
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        cmd.creation_flags(DETACHED);
        cmd.spawn().is_ok()
    }
    #[cfg(not(windows))]
    {
        let _ = (dir, command, args);
        false
    }
}

pub fn resume_in_terminal(cwd: &str, session_id: &str) -> bool {
    let Some(bin) = find_claude() else { return false };
    open_terminal(cwd, &bin, &["--resume".to_string(), session_id.to_string()])
}

pub fn chat_in_terminal(paths: &Paths, cwd: &str, prompt: &str, launcher: Option<PathBuf>) -> Value {
    let Some(bin) = find_claude() else { return json!({ "ok": false, "error": "claude-not-found" }) };
    let dir = existing_dir(cwd).unwrap_or_else(|| paths.quick_dir.clone());
    let _ = fs::create_dir_all(&dir);
    let dir_text = dir.to_string_lossy().into_owned();
    let Some(launcher) = launcher.filter(|_| !is_shim(&bin)) else {
        return json!({ "ok": open_terminal(&dir_text, &bin, &[]), "pasted": true });
    };
    let _ = fs::create_dir_all(&paths.tmp_dir);
    let file = paths.tmp_dir.join(format!("{}.json", uuid::Uuid::new_v4()));
    if fs::write(&file, json!({ "bin": bin, "prompt": prompt }).to_string()).is_err() {
        return json!({ "ok": open_terminal(&dir_text, &bin, &[]), "pasted": true });
    }
    json!({ "ok": open_terminal(&dir_text, &launcher, &["launch-chat".to_string(), file.to_string_lossy().into_owned()]) })
}
