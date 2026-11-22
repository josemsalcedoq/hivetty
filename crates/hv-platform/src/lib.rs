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

/// Shows how many sessions need the user on the app's icon (F4-19); 0 clears it. Call it on
/// the main thread.
///
/// - macOS: the dock badge.
/// - Windows and Linux: nothing yet (the taskbar overlay icon needs COM and an icon per count;
///   Linux has no common API). The taskbar still flashes through winit's attention request.
pub fn set_badge(count: usize) {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSApplication;
        use objc2_foundation::NSString;
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let label = (count > 0).then(|| NSString::from_str(&count.to_string()));
        NSApplication::sharedApplication(mtm)
            .dockTile()
            .setBadgeLabel(label.as_deref());
    }
    #[cfg(not(target_os = "macos"))]
    let _ = count;
}

/// Shows a desktop notification without blocking the caller (it is sent from a short-lived
/// thread). Failures are logged and otherwise ignored.
///
/// - Windows and Linux: native toasts / freedesktop notifications (`notify-rust`). On Windows
///   they appear under PowerShell's name until the installer registers hivetty's AppUserModelID
///   (F6-09).
/// - macOS: `osascript` for now; native notifications need the signed `.app` bundle (F6-07).
pub fn notify(title: &str, body: &str) {
    notify_with(title, body, NotifyOptions::default());
}

/// What a notification does besides showing its text (F4-18).
#[derive(Default)]
pub struct NotifyOptions {
    /// Play the system's notification sound.
    pub sound: bool,
    /// Called (on the notification's thread) when the user clicks the notification, with the
    /// label of its button where one is shown (Linux). Windows and Linux only.
    pub on_click: Option<(String, Box<dyn FnOnce() + Send>)>,
}

impl std::fmt::Debug for NotifyOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotifyOptions")
            .field("sound", &self.sound)
            .field("on_click", &self.on_click.as_ref().map(|(label, _)| label))
            .finish()
    }
}

/// Notifications whose thread may wait for a click at the same time. A toast that went to the
/// Windows action center can wait forever; beyond this many, new ones are not waited on.
#[cfg(not(target_os = "macos"))]
const MAX_WAITING: usize = 8;
#[cfg(not(target_os = "macos"))]
static WAITING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// [`notify`] with a sound and a click action.
pub fn notify_with(title: &str, body: &str, options: NotifyOptions) {
    let (title, body) = (title.to_owned(), body.to_owned());
    let _ = std::thread::Builder::new()
        .name("notify".into())
        .spawn(move || {
            if let Err(err) = send_notification(&title, &body, options) {
                log::warn!("notification failed: {err}");
            }
        });
}

/// Runs `wait` unless too many notifications are already waiting for a click.
#[cfg(not(target_os = "macos"))]
fn wait_bounded(wait: impl FnOnce()) {
    use std::sync::atomic::Ordering;
    if WAITING.fetch_add(1, Ordering::AcqRel) < MAX_WAITING {
        wait();
    }
    WAITING.fetch_sub(1, Ordering::AcqRel);
}

/// hivetty's AppUserModelID, as the installer registers it (F6-09).
#[cfg(windows)]
const APP_USER_MODEL_ID: &str = "dev.hivetty.Hivetty";

/// The AppUserModelID to send toasts as, when the installer registered it. Windows shows no
/// toast at all for an unregistered ID, so a build that was not installed keeps the default
/// (PowerShell's).
#[cfg(windows)]
fn registered_app_id() -> Option<&'static str> {
    use std::sync::OnceLock;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CLASSES_ROOT, KEY_READ, RegCloseKey, RegOpenKeyExW,
    };
    static REGISTERED: OnceLock<bool> = OnceLock::new();
    let registered = *REGISTERED.get_or_init(|| {
        let path: Vec<u16> = format!("AppUserModelId\\{APP_USER_MODEL_ID}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: `path` is a NUL-terminated UTF-16 string that outlives the call, `key` is a
        // valid out pointer, and a key that was opened is closed right away.
        unsafe {
            let opened =
                RegOpenKeyExW(HKEY_CLASSES_ROOT, path.as_ptr(), 0, KEY_READ, &mut key) == 0;
            if opened {
                RegCloseKey(key);
            }
            opened
        }
    });
    registered.then_some(APP_USER_MODEL_ID)
}

#[cfg(not(target_os = "macos"))]
fn send_notification(title: &str, body: &str, options: NotifyOptions) -> Result<(), String> {
    let mut notification = notify_rust::Notification::new();
    notification.summary(title).body(body).appname("hivetty");
    #[cfg(windows)]
    if let Some(app_id) = registered_app_id() {
        notification.app_id(app_id);
    }
    if options.sound {
        // Windows' default toast sound; the freedesktop sound for a new message on Linux.
        notification.sound_name(if cfg!(windows) {
            "Default"
        } else {
            "message-new-instant"
        });
    }
    #[cfg(not(windows))]
    if let Some((label, _)) = &options.on_click {
        // The freedesktop "default" action is a click on the notification itself.
        notification.action("default", label);
    }
    let handle = notification.show().map_err(|e| e.to_string())?;
    if let Some((_, on_click)) = options.on_click {
        wait_bounded(|| {
            // A click on the toast's body is the "default" response.
            #[cfg(windows)]
            let _ = handle.wait_for_response(|response: &notify_rust::NotificationResponse| {
                if matches!(response, notify_rust::NotificationResponse::Default) {
                    on_click();
                }
            });
            #[cfg(not(windows))]
            handle.wait_for_action(|action| {
                if action == "default" {
                    on_click();
                }
            });
        });
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn send_notification(title: &str, body: &str, options: NotifyOptions) -> Result<(), String> {
    // AppleScript string literals: escape backslashes and quotes; the text is data, never code.
    let quote = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
    let sound = if options.sound {
        " sound name \"Glass\""
    } else {
        ""
    };
    let script = format!(
        "display notification {} with title {}{sound}",
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
