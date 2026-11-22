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
/// Placeholder of the command palette's filter box.
pub const PALETTE_HINT: &str = "Type a command…";
/// Shown when no command matches.
pub const PALETTE_EMPTY: &str = "No matching commands";

/// Label of an action in the command palette, by its config name (docs/keybindings.md).
pub fn action_label(name: &str) -> Option<&'static str> {
    Some(match name {
        "new_tab" => "New tab",
        "close_pane" => "Close pane",
        "split_right" => "Split right",
        "split_down" => "Split down",
        "focus_left" => "Focus pane on the left",
        "focus_right" => "Focus pane on the right",
        "focus_up" => "Focus pane above",
        "focus_down" => "Focus pane below",
        "resize_left" => "Move divider left",
        "resize_right" => "Move divider right",
        "resize_up" => "Move divider up",
        "resize_down" => "Move divider down",
        "toggle_zoom" => "Maximize or restore pane",
        "next_tab" => "Next tab",
        "previous_tab" => "Previous tab",
        "toggle_sidebar" => "Show or hide sidebar",
        "find" => "Find in scrollback",
        "copy" => "Copy",
        "paste" => "Paste",
        "font_bigger" => "Increase font size",
        "font_smaller" => "Decrease font size",
        "font_reset" => "Reset font size",
        "command_palette" => "Command palette",
        _ => return None,
    })
}
