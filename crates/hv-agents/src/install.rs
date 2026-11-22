//! Hook installer for Claude Code (F4-07, docs/agent-integration.md 2.4): adds hivetty's
//! entries to `~/.claude/settings.json` without touching the user's own, and removes only its
//! own. The caller shows [`Plan::changes`] and asks before calling [`apply`].

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

/// Events hivetty listens to (docs/agent-integration.md 2.3).
pub const CLAUDE_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "Notification",
    "Stop",
    "SubagentStop",
    "SessionEnd",
];

/// Events hivetty listens to in Gemini CLI (its hooks mirror Claude Code's).
pub const GEMINI_EVENTS: &[&str] = &[
    "SessionStart",
    "BeforeAgent",
    "BeforeTool",
    "AfterTool",
    "Notification",
    "AfterAgent",
    "SessionEnd",
];

/// An agent whose settings file takes hivetty's hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HookTarget {
    /// Profile id, as passed to `hivetty hook --agent`.
    pub agent: &'static str,
    pub events: &'static [&'static str],
    /// Events that take a tool `matcher`.
    pub tool_events: &'static [&'static str],
    /// The `timeout` written with each hook, in the agent's unit (seconds or milliseconds).
    pub timeout: u64,
    /// Runs hook commands with PowerShell on Windows (checked with the real agent), which
    /// needs `& 'program' args` instead of `'program' args`.
    pub powershell_on_windows: bool,
}

/// Claude Code: `~/.claude/settings.json`, timeouts in seconds.
pub const CLAUDE: HookTarget = HookTarget {
    agent: "claude-code",
    events: CLAUDE_EVENTS,
    tool_events: &["PreToolUse", "PostToolUse"],
    timeout: 5,
    // Git Bash on Windows.
    powershell_on_windows: false,
};

/// Gemini CLI: `~/.gemini/settings.json`, timeouts in milliseconds.
pub const GEMINI: HookTarget = HookTarget {
    agent: "gemini-cli",
    events: GEMINI_EVENTS,
    tool_events: &["BeforeTool", "AfterTool"],
    timeout: 5000,
    // Windows PowerShell 5.1 (verified with 0.70.0).
    powershell_on_windows: true,
};

impl HookTarget {
    /// The target for a profile id, if hivetty can install hooks there.
    pub fn for_agent(agent: &str) -> Option<Self> {
        [CLAUDE, GEMINI].into_iter().find(|t| t.agent == agent)
    }

    /// Text that identifies hivetty's hook commands for this agent.
    fn marker(&self) -> String {
        format!(" hook --agent {} --event ", self.agent)
    }

    /// The command the agent runs for `event` on this platform.
    pub fn command(&self, exe: &Path, event: &str) -> String {
        self.command_for(exe, event, cfg!(windows))
    }

    /// The command for `event`, written for Windows or for a POSIX shell.
    pub fn command_for(&self, exe: &Path, event: &str, windows: bool) -> String {
        if windows && self.powershell_on_windows {
            format!("& {}{}{event}", quote_exe_powershell(exe), self.marker())
        } else {
            format!("{}{}{event}", quote_exe(exe), self.marker())
        }
    }

    fn is_ours(&self, command: &str) -> bool {
        command.contains(&self.marker())
    }
}

/// The hivetty binary in single quotes, with forward slashes. Forward slashes work in every
/// shell the agents use, including bash on Windows; single quotes keep `$`, backticks and `\`
/// literal for the shell.
fn quote_exe(exe: &Path) -> String {
    let exe = exe
        .to_string_lossy()
        .replace('\\', "/")
        .replace('\'', r"'\''");
    format!("'{exe}'")
}

/// The hivetty binary for PowerShell: single quotes, where a quote is doubled.
fn quote_exe_powershell(exe: &Path) -> String {
    let exe = exe.to_string_lossy().replace('\\', "/").replace('\'', "''");
    format!("'{exe}'")
}

/// What an install or uninstall would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The settings file after the change, pretty-printed.
    pub new_text: String,
    /// Human-readable list of changes ("+ Stop: …", "- Stop: …").
    pub changes: Vec<String>,
}

/// The command Claude Code runs for `event`.
pub fn hook_command(exe: &Path, event: &str) -> String {
    CLAUDE.command(exe, event)
}

/// Parses settings text (empty = no file yet). Errors on invalid JSON or a non-object root.
fn parse(text: &str) -> Result<Map<String, Value>, String> {
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err("settings.json is not a JSON object".to_owned()),
        Err(e) => Err(format!("settings.json is not valid JSON: {e}")),
    }
}

/// Adds the missing Claude Code hooks (see [`plan_install_for`]).
pub fn plan_install(text: &str, exe: &Path) -> Result<Plan, String> {
    plan_install_for(CLAUDE, text, exe)
}

/// Removes every hivetty hook from Claude Code's settings (see [`plan_uninstall_for`]).
pub fn plan_uninstall(text: &str) -> Result<Plan, String> {
    plan_uninstall_for(CLAUDE, text)
}

/// Adds the missing hivetty hooks for `target`. Entries hivetty added before with another
/// binary path are updated; everything else is left as it is.
pub fn plan_install_for(target: HookTarget, text: &str, exe: &Path) -> Result<Plan, String> {
    let mut root = parse(text)?;
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(hooks) = hooks else {
        return Err("\"hooks\" in settings.json is not an object".to_owned());
    };
    let mut changes = Vec::new();
    for event in target.events {
        let wanted = target.command(exe, event);
        let list = hooks
            .entry(*event)
            .or_insert_with(|| Value::Array(Vec::new()));
        let Value::Array(list) = list else {
            return Err(format!("\"hooks.{event}\" in settings.json is not a list"));
        };
        let mut found = false;
        for group in list.iter_mut() {
            let Some(inner) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                continue;
            };
            for hook in inner.iter_mut() {
                let Some(command) = hook.get("command").and_then(Value::as_str) else {
                    continue;
                };
                if target.is_ours(command) {
                    found = true;
                    if command != wanted {
                        changes.push(format!("~ {event}: {wanted}"));
                        hook["command"] = Value::String(wanted.clone());
                    }
                }
            }
        }
        if !found {
            let mut group = json!({
                "hooks": [{ "type": "command", "command": wanted, "timeout": target.timeout }]
            });
            if target.tool_events.contains(event) {
                group["matcher"] = Value::String("*".to_owned());
            }
            list.push(group);
            changes.push(format!("+ {event}: {wanted}"));
        }
    }
    Ok(Plan {
        new_text: to_text(&root)?,
        changes,
    })
}

/// Removes every hivetty hook for `target` (and any group or event list left empty by that).
pub fn plan_uninstall_for(target: HookTarget, text: &str) -> Result<Plan, String> {
    let mut root = parse(text)?;
    let mut changes = Vec::new();
    if let Some(Value::Object(hooks)) = root.get_mut("hooks") {
        for (event, list) in hooks.iter_mut() {
            let Value::Array(list) = list else { continue };
            for group in list.iter_mut() {
                if let Some(inner) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                    inner.retain(|hook| {
                        let ours = hook
                            .get("command")
                            .and_then(Value::as_str)
                            .is_some_and(|c| target.is_ours(c));
                        if ours {
                            changes.push(format!("- {event}"));
                        }
                        !ours
                    });
                }
            }
            list.retain(|group| {
                group
                    .get("hooks")
                    .and_then(Value::as_array)
                    .is_none_or(|inner| !inner.is_empty())
            });
        }
        hooks.retain(|_, list| list.as_array().is_none_or(|l| !l.is_empty()));
        if hooks.is_empty() {
            root.remove("hooks");
        }
    }
    Ok(Plan {
        new_text: to_text(&root)?,
        changes,
    })
}

/// The `statusLine` command hivetty installs (F4-32).
pub fn statusline_command(exe: &Path) -> String {
    format!("{}{STATUSLINE_MARKER}", quote_exe(exe))
}

/// What identifies hivetty's statusline command.
const STATUSLINE_MARKER: &str = " statusline";

/// A statusline command hivetty wrote: `'<…/hivetty[.exe]>' statusline`, for any path (the
/// binary may have moved). Another tool's `'…/ccusage' statusline` is not ours.
fn is_our_statusline(value: &Value) -> bool {
    value
        .get("command")
        .and_then(Value::as_str)
        .and_then(|c| c.strip_suffix(&format!("'{STATUSLINE_MARKER}")))
        .and_then(|quoted| quoted.strip_prefix('\''))
        .is_some_and(|exe| {
            let name = exe.rsplit('/').next().unwrap_or(exe);
            name.eq_ignore_ascii_case("hivetty") || name.eq_ignore_ascii_case("hivetty.exe")
        })
}

/// Sets hivetty's statusline. A statusline the user already has is never replaced.
pub fn plan_statusline_install(text: &str, exe: &Path) -> Result<Plan, String> {
    let mut root = parse(text)?;
    let wanted = statusline_command(exe);
    let mut changes = Vec::new();
    match root.get("statusLine") {
        Some(current) if !is_our_statusline(current) => {
            return Err(
                "settings.json already has a statusLine; hivetty leaves it as it is (remove it first to use hivetty's)"
                    .to_owned(),
            );
        }
        Some(current) if current.get("command").and_then(Value::as_str) == Some(&wanted) => {}
        current => {
            let sign = if current.is_some() { "~" } else { "+" };
            changes.push(format!("{sign} statusLine: {wanted}"));
            root.insert(
                "statusLine".to_owned(),
                json!({ "type": "command", "command": wanted }),
            );
        }
    }
    Ok(Plan {
        new_text: to_text(&root)?,
        changes,
    })
}

/// Removes hivetty's statusline (only if it is hivetty's).
pub fn plan_statusline_uninstall(text: &str) -> Result<Plan, String> {
    let mut root = parse(text)?;
    let mut changes = Vec::new();
    if root.get("statusLine").is_some_and(is_our_statusline) {
        root.remove("statusLine");
        changes.push("- statusLine".to_owned());
    }
    Ok(Plan {
        new_text: to_text(&root)?,
        changes,
    })
}

fn to_text(root: &Map<String, Value>) -> Result<String, String> {
    serde_json::to_string_pretty(root)
        .map(|t| t + "\n")
        .map_err(|e| e.to_string())
}

/// `~/.gemini/settings.json`.
pub fn gemini_settings_path(home: &Path) -> PathBuf {
    home.join(".gemini").join("settings.json")
}

/// `~/.claude/settings.json`.
pub fn claude_settings_path(home: &Path) -> PathBuf {
    home.join(".claude").join("settings.json")
}

/// Writes `plan` to `path`: a copy of the old file goes to `settings.json.hivetty-bak`, and the
/// new one is written to a temporary file and renamed over the old (never half-written).
pub fn apply(path: &Path, plan: &Plan) -> std::io::Result<()> {
    // Write through a symlink (dotfiles repos link settings.json) instead of replacing it.
    let target = if path.is_symlink() {
        std::fs::canonicalize(path)?
    } else {
        path.to_path_buf()
    };
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let permissions = std::fs::metadata(&target).ok().map(|m| m.permissions());
    if target.exists() {
        std::fs::copy(&target, target.with_extension("json.hivetty-bak"))?;
    }
    let tmp = target.with_extension("json.hivetty-tmp");
    std::fs::write(&tmp, &plan.new_text)?;
    // Keep the original permissions (the file may hold API keys and be private).
    if let Some(permissions) = permissions {
        std::fs::set_permissions(&tmp, permissions)?;
    }
    std::fs::rename(&tmp, &target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gemini_hooks_use_milliseconds_and_their_own_events() {
        let exe = Path::new("/opt/hivetty/hivetty");
        let plan = plan_install_for(GEMINI, "{}", exe).unwrap();
        assert_eq!(plan.changes.len(), GEMINI_EVENTS.len());
        let root: Value = serde_json::from_str(&plan.new_text).unwrap();
        let tool = &root["hooks"]["BeforeTool"][0];
        assert_eq!(tool["matcher"], "*");
        assert_eq!(tool["hooks"][0]["timeout"], 5000);
        assert_eq!(
            tool["hooks"][0]["command"],
            GEMINI.command(exe, "BeforeTool")
        );
        assert_eq!(
            GEMINI.command_for(exe, "BeforeTool", false),
            "'/opt/hivetty/hivetty' hook --agent gemini-cli --event BeforeTool"
        );
        // PowerShell on Windows, with quotes doubled.
        assert_eq!(
            GEMINI.command_for(
                Path::new(r"C:\Program Files\it's\hivetty.exe"),
                "AfterAgent",
                true
            ),
            "& 'C:/Program Files/it''s/hivetty.exe' hook --agent gemini-cli --event AfterAgent"
        );
        // Claude Code uses Git Bash on Windows too.
        assert!(CLAUDE.command_for(exe, "Stop", true).starts_with('\''));
        // Claude Code's entries are not Gemini's, and the other way round.
        assert!(
            plan_uninstall_for(CLAUDE, &plan.new_text)
                .unwrap()
                .changes
                .is_empty()
        );
        let removed = plan_uninstall_for(GEMINI, &plan.new_text).unwrap();
        assert_eq!(removed.changes.len(), GEMINI_EVENTS.len());
        assert_eq!(HookTarget::for_agent("gemini-cli"), Some(GEMINI));
        assert_eq!(HookTarget::for_agent("codex"), None);
    }

    #[test]
    fn statusline_is_added_and_removed_but_never_replaces_the_users() {
        let exe = Path::new("/opt/hivetty/hivetty");
        let plan = plan_statusline_install("{\"model\":\"opus\"}", exe).unwrap();
        assert_eq!(plan.changes.len(), 1);
        let root: Value = serde_json::from_str(&plan.new_text).unwrap();
        assert_eq!(
            root["statusLine"]["command"],
            "'/opt/hivetty/hivetty' statusline"
        );
        assert_eq!(root["model"], "opus");
        let again = plan_statusline_install(&plan.new_text, exe).unwrap();
        assert!(again.changes.is_empty());
        let users = r#"{"statusLine":{"type":"command","command":"~/.claude/mine.sh"}}"#;
        let other_tool =
            r#"{"statusLine":{"type":"command","command":"'/usr/bin/ccusage' statusline"}}"#;
        assert!(plan_statusline_install(other_tool, exe).is_err());
        assert!(
            plan_statusline_uninstall(other_tool)
                .unwrap()
                .changes
                .is_empty()
        );
        assert!(plan_statusline_install(users, exe).is_err());
        assert!(plan_statusline_uninstall(users).unwrap().changes.is_empty());
        let removed = plan_statusline_uninstall(&plan.new_text).unwrap();
        assert_eq!(removed.changes, ["- statusLine"]);
        assert!(!removed.new_text.contains("statusLine"));
    }

    const USER: &str = r#"{
  "model": "opus",
  "hooks": {
    "Stop": [
      { "hooks": [{ "type": "command", "command": "say done" }] }
    ]
  },
  "permissions": { "allow": ["Bash(ls:*)"] }
}"#;

    fn exe() -> PathBuf {
        PathBuf::from(r"C:\Program Files\hivetty\hivetty.exe")
    }

    #[test]
    fn install_adds_every_event_and_keeps_the_users_entries_and_key_order() {
        let plan = plan_install(USER, &exe()).unwrap();
        assert_eq!(plan.changes.len(), CLAUDE_EVENTS.len());
        let value: Value = serde_json::from_str(&plan.new_text).unwrap();
        let stop = value["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2, "the user's Stop hook stays");
        assert_eq!(stop[0]["hooks"][0]["command"], "say done");
        assert_eq!(
            stop[1]["hooks"][0]["command"],
            "'C:/Program Files/hivetty/hivetty.exe' hook --agent claude-code --event Stop"
        );
        assert_eq!(value["hooks"]["PreToolUse"][0]["matcher"], "*");
        // Keys keep their original order.
        let model = plan.new_text.find("\"model\"").unwrap();
        let permissions = plan.new_text.find("\"permissions\"").unwrap();
        assert!(model < permissions);
    }

    #[test]
    fn install_is_idempotent_and_updates_a_moved_binary() {
        let once = plan_install(USER, &exe()).unwrap();
        let twice = plan_install(&once.new_text, &exe()).unwrap();
        assert!(twice.changes.is_empty());
        assert_eq!(twice.new_text, once.new_text);
        let moved = plan_install(&once.new_text, Path::new("/opt/hivetty")).unwrap();
        assert_eq!(moved.changes.len(), CLAUDE_EVENTS.len());
        assert!(moved.changes.iter().all(|c| c.starts_with('~')));
    }

    #[test]
    fn uninstall_removes_only_ours() {
        let installed = plan_install(USER, &exe()).unwrap();
        let removed = plan_uninstall(&installed.new_text).unwrap();
        assert_eq!(removed.changes.len(), CLAUDE_EVENTS.len());
        let before: Value = serde_json::from_str(USER).unwrap();
        let after: Value = serde_json::from_str(&removed.new_text).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn shell_characters_in_the_path_stay_literal() {
        let cmd = hook_command(Path::new("/opt/it's $(id)/hivetty"), "Stop");
        assert_eq!(
            cmd,
            r"'/opt/it'\''s $(id)/hivetty' hook --agent claude-code --event Stop"
        );
    }

    #[test]
    fn empty_or_missing_files_and_bad_json() {
        assert!(plan_install("", &exe()).is_ok());
        assert!(plan_install("[1,2]", &exe()).is_err());
        assert!(plan_install("{ nope", &exe()).is_err());
        assert!(plan_install(r#"{"hooks": 3}"#, &exe()).is_err());
    }

    #[test]
    fn apply_backs_up_and_replaces() {
        let dir = std::env::temp_dir().join(format!("hv-install-{}", std::process::id()));
        let path = dir.join("settings.json");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, USER).unwrap();
        let plan = plan_install(USER, &exe()).unwrap();
        apply(&path, &plan).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), plan.new_text);
        assert_eq!(
            std::fs::read_to_string(path.with_extension("json.hivetty-bak")).unwrap(),
            USER
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
