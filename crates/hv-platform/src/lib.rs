//! OS-specific integrations. The only crate allowed to use unsafe code.

// Platform APIs (Win32, Cocoa, D-Bus) need FFI; unsafe is confined to this crate.
#![allow(unsafe_code)]

/// Makes `println!` reach the console the app was started from.
///
/// Release builds on Windows use the GUI subsystem (no console window of their own), so
/// command-line subcommands like `--doctor` or `usage` would print nowhere. Attaching to the
/// parent process's console fixes that. It does nothing on other platforms, or when there is
/// no parent console (started from Explorer).
pub fn attach_parent_console() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
        // SAFETY: AttachConsole has no preconditions; failure (no parent console, or already
        // attached as in debug builds) is reported through the return value and ignored.
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

/// Shows a desktop notification without blocking the caller (it is sent from a short-lived
/// thread). Failures are logged and otherwise ignored.
///
/// - Windows and Linux: native toasts / freedesktop notifications (`notify-rust`). On Windows
///   they appear under PowerShell's name until the installer registers hivetty's AppUserModelID
///   (F6-09).
/// - macOS: `osascript` for now; native notifications need the signed `.app` bundle (F6-07).
pub fn notify(title: &str, body: &str) {
    let (title, body) = (title.to_owned(), body.to_owned());
    let _ = std::thread::Builder::new()
        .name("notify".into())
        .spawn(move || {
            if let Err(err) = send_notification(&title, &body) {
                log::warn!("notification failed: {err}");
            }
        });
}

#[cfg(not(target_os = "macos"))]
fn send_notification(title: &str, body: &str) -> Result<(), String> {
    notify_rust::Notification::new()
        .summary(title)
        .body(body)
        .appname("hivetty")
        .show()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn send_notification(title: &str, body: &str) -> Result<(), String> {
    // AppleScript string literals: escape backslashes and quotes; the text is data, never code.
    let quote = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    let script = format!(
        "display notification {} with title {}",
        quote(body),
        quote(title)
    );
    std::process::Command::new("osascript")
        .args(["-e", &script])
        .status()
        .map_err(|e| e.to_string())
        .and_then(|s| {
            s.success()
                .then_some(())
                .ok_or_else(|| format!("osascript: {s}"))
        })
}

/// The effective user id of this process.
#[cfg(unix)]
pub fn effective_uid() -> u32 {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() }
}

#[cfg(test)]
mod tests {
    #[test]
    fn attaching_twice_is_harmless() {
        super::attach_parent_console();
        super::attach_parent_console();
    }

    #[cfg(unix)]
    #[test]
    fn effective_uid_owns_new_files() {
        use std::os::unix::fs::MetadataExt;
        let file = std::env::temp_dir().join(format!("hv-uid-{}", std::process::id()));
        std::fs::write(&file, b"").unwrap();
        let owner = std::fs::metadata(&file).unwrap().uid();
        std::fs::remove_file(&file).unwrap();
        assert_eq!(owner, super::effective_uid());
    }
}
