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
/// Notification title when a long shell command finishes.
pub const NOTIFY_COMMAND_DONE: &str = "Command finished";
/// Notification title when a long shell command fails.
pub const NOTIFY_COMMAND_FAILED: &str = "Command failed";
/// Merged notification when several sessions changed at once.
pub fn sessions_need_you(count: u32) -> String {
    format!("{count} sessions need you")
}
/// Body of the merged notification.
pub const NOTIFY_OPEN_HIVETTY: &str = "Open hivetty to see them";
/// Title used when the program set none.
pub const DEFAULT_TITLE: &str = "Shell";
/// Tooltip and accessible name of the `+` button in the tab bar.
pub const NEW_TAB: &str = "New tab";
/// Accessible name of a tab's close button.
pub const CLOSE_TAB: &str = "Close tab";
/// macOS menu bar titles.
pub const MENU_SHELL: &str = "Shell";
/// macOS menu bar titles.
pub const MENU_EDIT: &str = "Edit";
/// macOS menu bar titles.
pub const MENU_VIEW: &str = "View";
/// macOS menu bar titles.
pub const MENU_WINDOW: &str = "Window";
/// Tooltip and accessible name of the ☰ menu button.
pub const MENU: &str = "Menu";
/// Placeholder of the command palette's filter box.
pub const PALETTE_HINT: &str = "Type a command…";
/// Shown when no command matches.
pub const PALETTE_EMPTY: &str = "No matching commands";

/// Label of an action in the command palette, by its config name (docs/keybindings.md).
pub fn action_label(name: &str) -> Option<&'static str> {
    Some(match name {
        "new_tab" => "New tab",
        "new_agent_session" => "New agent session…",
        "new_window" => "New window",
        "move_tab_to_new_window" => "Move tab to new window",
        "close_window" => "Close window",
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
        "toggle_usage" => "Agent usage",
        "find" => "Find in scrollback",
        "previous_prompt" => "Jump to previous prompt",
        "next_prompt" => "Jump to next prompt",
        "copy_last_output" => "Copy last command output",
        "copy" => "Copy",
        "paste" => "Paste",
        "font_bigger" => "Increase font size",
        "font_smaller" => "Decrease font size",
        "font_reset" => "Reset font size",
        "command_palette" => "Command palette",
        "next_needs_you" => "Go to next session that needs you",
        _ => return None,
    })
}
/// Placeholder of the sidebar search box.
pub const SEARCH_SESSIONS: &str = "Search sessions";
/// Sidebar filters.
pub const FILTER_ALL: &str = "All";
/// Sidebar filters.
pub const FILTER_NEEDS_YOU: &str = "Needs you";
/// Sidebar filters.
pub const FILTER_WORKING: &str = "Working";
/// Sidebar filters.
pub const FILTER_DONE: &str = "Done";
/// Shown when the search or filter hides every session.
pub const NO_MATCHES: &str = "No matching sessions";
/// New session button and dialog.
pub const NEW_SESSION: &str = "New session";
/// Dialog field labels.
pub const FIELD_AGENT: &str = "Agent";
/// Dialog field labels.
pub const FIELD_FOLDER: &str = "Folder";
/// Dialog field labels.
pub const FIELD_PROMPT: &str = "Prompt";
/// Dialog field labels.
pub const FIELD_ARGUMENTS: &str = "Arguments";
/// Dialog field labels.
pub const FIELD_OPEN_IN: &str = "Open in";
/// Hint in optional fields.
pub const OPTIONAL: &str = "optional";
/// "Open in" choices.
pub const OPEN_NEW_TAB: &str = "New tab";
/// "Open in" choices.
pub const OPEN_SPLIT_RIGHT: &str = "Split right";
/// Dialog buttons.
pub const CREATE: &str = "Create";
/// Dialog buttons.
pub const CANCEL: &str = "Cancel";
/// Shown when the folder does not exist.
pub const FOLDER_MISSING: &str = "That folder does not exist";
/// Usage dashboard (docs/ui-spec.md §6).
pub const USAGE_TITLE: &str = "Agent usage";
/// Range selector.
pub const RANGE_7: &str = "7 days";
/// Range selector.
pub const RANGE_30: &str = "30 days";
/// Range selector.
pub const RANGE_90: &str = "90 days";
/// Range selector.
pub const RANGE_ALL: &str = "All";
/// End of the date range and of the X axis.
pub const TODAY: &str = "today";
/// Close button of the dashboard.
pub const ESC: &str = "esc";
/// While the logs are read.
pub const USAGE_LOADING: &str = "Reading usage…";
/// Empty state.
pub const USAGE_EMPTY: &str =
    "No usage data yet. hivetty reads usage from: Claude Code (~/.claude/projects)";
/// KPI card labels.
pub const KPI_INPUT: &str = "INPUT TOKENS";
/// KPI card labels.
pub const KPI_OUTPUT: &str = "OUTPUT TOKENS";
/// KPI card labels.
pub const KPI_TASKS: &str = "TASKS";
/// KPI card labels.
pub const KPI_AGENT_TIME: &str = "AGENT TIME";
/// KPI subtitles.
pub const FROM_CACHE: &str = "from cache";
/// KPI subtitles.
pub const PER_TURN: &str = "per turn";
/// KPI subtitles.
pub const TURNS: &str = "turns";
/// "up to 3 at once".
pub const UP_TO: &str = "up to";
/// "up to 3 at once".
pub const AT_ONCE: &str = "at once";
/// Chart titles.
pub const TOKENS_BY_DAY: &str = "TOKENS BY DAY";
/// Chart titles.
pub const WHEN_THEY_WORK: &str = "WHEN THEY WORK";
/// Chart titles.
pub const MODELS: &str = "MODELS";
/// Chart titles.
pub const AGENTS: &str = "AGENTS";
/// Axis and summary words.
pub const INPUT: &str = "input";
/// Axis and summary words.
pub const OUTPUT: &str = "output";
/// "29 of 30 days with agents".
pub const DAYS_WITH_AGENTS: &str = "days with agents";
/// "402M/day".
pub const DAY: &str = "day";
/// Heatmap rows, Monday first.
pub const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
/// Heatmap legend.
pub const LESS: &str = "less";
/// Heatmap legend.
pub const MORE: &str = "more";
/// "most active Sat 10 h".
pub const MOST_ACTIVE: &str = "most active";
/// Hint next to the agents table.
pub const CLICK_TO_FILTER: &str = "click one to see only its usage";
/// Tooltip of the filter chip.
pub const SHOW_ALL_AGENTS: &str = "Show all agents";
/// "10 more" rows.
pub const MORE_ROWS: &str = "more";
/// Peek (S-20).
pub const PEEK: &str = "Peek";
/// Peek reply box placeholder.
pub const PEEK_REPLY_HINT: &str = "Reply to this session (Enter sends)";
/// Peek buttons.
pub const PEEK_SEND: &str = "Send";
/// Peek buttons.
pub const PEEK_GO_TO: &str = "Go to session";
