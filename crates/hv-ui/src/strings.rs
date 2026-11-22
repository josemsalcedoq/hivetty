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
        "search_sessions" => "Search sessions",
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
/// Window buttons of the custom title bar.
pub const MINIMIZE: &str = "Minimize";
/// Window buttons of the custom title bar.
pub const MAXIMIZE: &str = "Maximize";
/// Window buttons of the custom title bar.
pub const RESTORE: &str = "Restore";
/// Window buttons of the custom title bar.
pub const CLOSE_WINDOW: &str = "Close window";
/// What an agent is doing (hv-agents `Activity`), worded for the sidebar.
pub const THINKING: &str = "Thinking…";
/// Activity wording.
pub fn running(command: &str) -> String {
    format!("Running {command}")
}
/// Activity wording.
pub fn editing(file: &str) -> String {
    format!("Editing {file}")
}
/// Activity wording.
pub fn writing(file: &str) -> String {
    format!("Writing {file}")
}
/// Activity wording.
pub fn reading(file: &str) -> String {
    format!("Reading {file}")
}
/// Activity wording.
pub fn searching(pattern: &str) -> String {
    format!("Searching {pattern}")
}
/// Activity wording.
pub fn fetching(url: &str) -> String {
    format!("Fetching {url}")
}
/// Activity wording.
pub fn searching_web(query: &str) -> String {
    format!("Searching the web for {query}")
}
/// Activity wording.
pub fn sub_agent(description: &str) -> String {
    format!("Agent: {description}")
}
/// Activity wording.
pub fn using(tool: &str) -> String {
    format!("Using {tool}")
}

/// Scrollback search bar: `Find: <pattern>▏<status>`.
pub fn find_bar(pattern: &str, status: &str) -> String {
    format!("Find: {pattern}\u{258f}{status}")
}
/// Search bar status.
pub const FIND_NO_MATCHES: &str = "  (no matches)";
/// Search bar status.
pub const FIND_INVALID: &str = "  (invalid pattern)";
/// The pill shown when the view is scrolled up.
pub const JUMP_TO_BOTTOM: &str = "\u{2193} Jump to bottom";

/// Body of the long-command notification.
pub fn command_finished(name: &str, exit_code: Option<i32>, took: &str) -> String {
    match exit_code {
        Some(code) if code != 0 => format!("{name}: exit code {code} after {took}"),
        _ => format!("{name}: done after {took}"),
    }
}
/// Session context menu (S-13).
pub const MENU_FOCUS: &str = "Focus";
/// Session context menu.
pub const MENU_PEEK: &str = "Peek";
/// Session context menu.
pub const MENU_ARCHIVE: &str = "Archive";
/// Session context menu.
pub const MENU_COPY_ID: &str = "Copy session id";
/// Session context menu.
pub const MENU_SHOW_FOLDER: &str = "Show folder";
/// Session context menu.
pub const MENU_CLOSE: &str = "Close session…";
/// Archived section (S-12).
pub const ARCHIVED: &str = "Archived";
/// Archived row buttons.
pub const RESTORE_SESSION: &str = "Restore";
/// Archived row buttons.
pub const DELETE: &str = "Delete";
/// Confirmation before closing a session.
pub const CONFIRM_CLOSE: &str = "Close this session? Its programs will be stopped.";
/// Confirmation button.
pub const CLOSE: &str = "Close";
/// New session: worktree option (A-14).
pub const FIELD_WORKTREE: &str = "Worktree";
/// New session: worktree option.
pub const CREATE_WORKTREE: &str = "create a git worktree";
/// New session: branch hint.
pub const BRANCH_HINT: &str = "branch (default agent/<prompt>)";
/// Notification when the worktree could not be created.
pub const WORKTREE_FAILED: &str = "Cannot create the worktree";
/// Estimated cost with the user's `pricing.toml` (U-60).
pub fn api_equivalent(usd: f64) -> String {
    if usd >= 100.0 {
        format!("≈ ${usd:.0} API equivalent")
    } else {
        format!("≈ ${usd:.2} API equivalent")
    }
}
/// When only some models have a price.
pub const PARTIAL: &str = "some models unpriced";
/// Hover card of a session (S-15).
pub const CARD_AGENT: &str = "Agent";
/// Hover card.
pub const CARD_MODEL: &str = "Model";
/// Hover card.
pub const CARD_BRANCH: &str = "Branch";
/// Hover card.
pub const CARD_FOLDER: &str = "Folder";
/// Hover card.
pub const CARD_TOKENS: &str = "Tokens";
/// Hover card.
pub const CARD_COST: &str = "Cost";
/// Hover card.
pub const CARD_STARTED: &str = "Started";
/// Hover card.
pub const CARD_LAST_TOOL: &str = "Last tool";
/// Hover card: token totals of the session.
pub fn card_tokens(input: &str, output: &str) -> String {
    format!("{input} in · {output} out")
}
/// Hover card: how long ago the session started.
pub fn ago(elapsed: &str) -> String {
    format!("{elapsed} ago")
}
/// Agent time card: the time hivetty measured itself (F4-23).
pub fn observed(duration: &str) -> String {
    format!("{duration} seen here")
}
/// Hover card: context window used.
pub const CARD_CONTEXT: &str = "Context";
/// Plan limits in the usage dashboard (U-65): `Codex · 5-hour limit 24% · resets in 2h 10m`.
pub fn plan_limit(agent: &str, window: &str, percent: f64, resets_in: &str) -> String {
    format!("{agent} · {window} {percent:.0}% · resets in {resets_in}")
}
/// A plan window of another length: `8-hour limit`.
pub fn hours_limit(hours: u64) -> String {
    format!("{hours}-hour limit")
}
/// Plan window names.
pub const FIVE_HOUR_LIMIT: &str = "5-hour limit";
/// Plan window names.
pub const WEEKLY_LIMIT: &str = "Weekly limit";
/// Session context menu.
pub const MENU_RENAME: &str = "Rename…";
/// Rename dialog.
pub const RENAME_SESSION: &str = "Rename session (empty = automatic title)";
/// Rename dialog button.
pub const SAVE: &str = "Save";
/// Project context menu (S-14).
pub const MENU_NEW_SESSION_HERE: &str = "New session here";
/// Project context menu.
pub const MENU_COLLAPSE: &str = "Collapse";
/// Project context menu.
pub const MENU_EXPAND: &str = "Expand";
/// Project context menu.
pub const MENU_ARCHIVE_FINISHED: &str = "Archive finished sessions";
/// Plan limit pace: the limit runs out before it resets (U-65).
pub fn runs_out_in(when: &str) -> String {
    format!("at this pace, runs out in {when}")
}
/// First-run wizard (F6-03).
pub const WELCOME_TITLE: &str = "Welcome to hivetty";
/// First-run wizard.
pub const WELCOME_INTRO: &str =
    "A few choices to start. You can change all of them later in config.toml.";
/// First-run wizard.
pub const WELCOME_THEME: &str = "Theme";
/// First-run wizard: theme options.
pub const THEME_SYSTEM: &str = "Follow the system";
/// First-run wizard: theme options.
pub const THEME_DARK: &str = "Dark";
/// First-run wizard: theme options.
pub const THEME_LIGHT: &str = "Light";
/// First-run wizard.
pub const WELCOME_SHELL: &str = "Shell integration";
/// First-run wizard.
pub const WELCOME_SHELL_DETAIL: &str = "Prompt marks, the current folder and project chips. hivetty loads a small script when your shell starts; your own files are not changed.";
/// First-run wizard.
pub const WELCOME_HOOKS: &str = "Live state of Claude Code sessions (hooks)";
/// First-run wizard.
pub const WELCOME_STATUSLINE: &str = "Context use and plan limits (Claude Code statusline)";
/// First-run wizard.
pub const WELCOME_ALREADY: &str = "already set up";
/// First-run wizard: the file the agent changes go to.
pub fn welcome_changes(path: &str) -> String {
    format!("Changes to {path} (a backup is kept next to it):")
}
/// First-run wizard.
pub const WELCOME_START: &str = "Start";
/// First-run wizard.
pub const WELCOME_SKIP: &str = "Skip";
/// The notice shown when config.toml has a problem (F6-02).
pub fn config_problem(detail: &str) -> String {
    format!("config.toml: {detail}")
}
/// Session menu: only for sessions hivetty opened in a worktree (F4-22).
pub const MENU_REMOVE_WORKTREE: &str = "Remove worktree…";
/// Confirmation before removing a worktree.
pub fn confirm_remove_worktree(dir: &str) -> String {
    format!(
        "Close this session and remove the worktree {dir}? git refuses if it has changes that are not committed."
    )
}
/// Confirmation button.
pub const REMOVE: &str = "Remove";
/// Notification when git refused.
pub const WORKTREE_NOT_REMOVED: &str = "The worktree was not removed";
/// Notification when part of the first-run setup failed.
pub const SETUP_PROBLEM: &str = "Part of the setup was not applied";
/// Output card: the reasoning part (F5-18).
pub const REASONING: &str = "reasoning";
/// Marks a state guessed from the agent's output (A-19).
pub const ESTIMATED: &str = "≈";
/// Hover card: why the state is marked as a guess.
pub const CARD_STATE: &str = "State";
/// Hover card.
pub const ESTIMATED_DETAIL: &str = "estimated from its output (install its hooks for exact states)";
/// Session context menu: start the session again in the same place (S-13).
pub const MENU_RESTART: &str = "Restart";
/// Session context menu: the folder of the agent's transcript.
pub const MENU_SHOW_TRANSCRIPT: &str = "Show transcript";
/// Session context menu.
pub const MENU_OPEN_IN_EDITOR: &str = "Open in editor";
/// Notification when the editor cannot start.
pub const EDITOR_FAILED: &str = "Cannot start the editor (set [editor] command in config.toml)";
/// New session: menu of the folders used before (F4-13).
pub const RECENT: &str = "Recent";
/// The button of a notification that brings its session forward (Linux shows it).
pub const NOTIFY_FOCUS: &str = "Focus";
