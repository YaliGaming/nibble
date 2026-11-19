use std::path::Path;
use std::process::{Command, Stdio};

#[cfg(windows)]
pub const DETACHED: u32 = 0x0000_0008 | 0x0800_0000;

pub fn detached_self(args: &[&str]) {
    let Ok(exe) = std::env::current_exe() else { return };
    let mut command = Command::new(exe);
    command.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(DETACHED);
    }
    let _ = command.spawn();
}

#[cfg(windows)]
pub fn shell_open(exe: &Path, args: &[String]) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide = |s: &std::ffi::OsStr| -> Vec<u16> { s.encode_wide().chain(std::iter::once(0)).collect() };
    let exe_w = wide(exe.as_os_str());
    let params = args.iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(" ");
    let params_w = wide(std::ffi::OsStr::new(&params));
    let dir_w = exe.parent().map(|d| wide(d.as_os_str()));
    let verb = wide(std::ffi::OsStr::new("open"));
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            exe_w.as_ptr(),
            if params.is_empty() { std::ptr::null() } else { params_w.as_ptr() },
            dir_w.as_ref().map_or(std::ptr::null(), |d| d.as_ptr()),
            SW_SHOWNORMAL,
        )
    };
    (result as isize) > 32
}

#[cfg(not(windows))]
pub fn shell_open(exe: &Path, args: &[String]) -> bool {
    Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
}
