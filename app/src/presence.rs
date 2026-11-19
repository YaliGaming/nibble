use std::thread;
use std::time::Duration;

#[cfg(windows)]
pub fn pid_alive(pid: i64) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ACCESS_DENIED};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    if pid <= 0 {
        return false;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid as u32);
        if handle.is_null() {
            return GetLastError() == ERROR_ACCESS_DENIED;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(handle, &mut code);
        CloseHandle(handle);
        ok != 0 && code == 259
    }
}

#[cfg(not(windows))]
pub fn pid_alive(pid: i64) -> bool {
    pid > 0 && std::path::Path::new(&format!("/proc/{pid}")).exists()
}

#[cfg(windows)]
pub fn list_claude_pids() -> Option<Vec<i64>> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
    };
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut pids = Vec::new();
        let mut more = Process32FirstW(snapshot, &mut entry) != 0;
        while more {
            let len = entry.szExeFile.iter().position(|c| *c == 0).unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            if name.eq_ignore_ascii_case("claude.exe") {
                pids.push(entry.th32ProcessID as i64);
            }
            more = Process32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
        Some(pids)
    }
}

#[cfg(not(windows))]
pub fn list_claude_pids() -> Option<Vec<i64>> {
    None
}

pub fn watch_claude(
    session_pids: impl Fn() -> Vec<i64> + Send + 'static,
    on_gone: impl Fn() + Send + 'static,
) {
    thread::spawn(move || {
        let mut known: Vec<i64> = Vec::new();
        let mut seen = false;
        let mut missed = 0;
        loop {
            known.retain(|p| pid_alive(*p));
            let mut present = !known.is_empty() || session_pids().iter().any(|p| pid_alive(*p));
            let mut unknown = false;
            if !present {
                match list_claude_pids() {
                    Some(found) => {
                        known = found.into_iter().filter(|p| pid_alive(*p)).collect();
                        present = !known.is_empty();
                    }
                    None => unknown = true,
                }
            }
            if !unknown {
                if present {
                    seen = true;
                    missed = 0;
                } else if seen {
                    missed += 1;
                    if missed >= 3 {
                        on_gone();
                        return;
                    }
                }
            }
            thread::sleep(Duration::from_secs(5));
        }
    });
}
