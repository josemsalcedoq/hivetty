//! Every user-facing UI string, in one place (the app is English-only; docs/ui-spec.md 9).

/// Sidebar header.
pub const SESSIONS: &str = "SESSIONS";
/// Shown when there are no sessions.
pub const NO_SESSIONS: &str = "No sessions yet";
/// Subtitle of a running shell.
pub const STATE_RUNNING: &str = "Running";
/// Subtitle of a finished session.
pub const STATE_EXITED: &str = "Exited";
/// Agent session started.
pub const STATE_STARTING: &str = "Starting";
/// Agent working.
pub const STATE_WORKING: &str = "Working";
/// Agent waiting for the user (no specific reason).
pub const STATE_WAITING: &str = "Waiting";
/// Agent waiting for a permission decision.
pub const STATE_NEEDS_PERMISSION: &str = "Needs permission";
/// Agent waiting for a login.
pub const STATE_NEEDS_LOGIN: &str = "Needs login";
/// Agent idle, waiting for the next prompt.
pub const STATE_IDLE: &str = "Idle";
/// Agent finished its task.
pub const STATE_DONE: &str = "Done";
/// Agent failed.
pub const STATE_ERROR: &str = "Error";
/// Title used when the program set none.
pub const DEFAULT_TITLE: &str = "Shell";
/// Tooltip and accessible name of the `+` button in the tab bar.
pub const NEW_TAB: &str = "New tab";
/// Accessible name of a tab's close button.
pub const CLOSE_TAB: &str = "Close tab";
