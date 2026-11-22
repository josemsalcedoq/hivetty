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
