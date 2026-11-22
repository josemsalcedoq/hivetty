//! Codex CLI hooks. Codex reads them from `[hooks]` in `~/.codex/config.toml` and sends the
//! same JSON as Claude Code (`hook_event_name`, `tool_name: "Bash"`, `prompt`…), so the Claude
//! Code interpreter reads them; Codex adds `PermissionRequest`. Real payloads from version
//! 0.171.0 are in `tests/fixtures/hooks/codex/0.171.0/`.
//!
//! On Windows Codex runs hooks with Windows PowerShell, so each entry has a `commandWindows`
//! next to the POSIX `command`. Codex asks the user to trust new hooks before running them.

use std::path::Path;

use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};

use crate::install::{HookTarget, Plan};
use crate::{AgentState, Update};

/// Events hivetty listens to in Codex.
pub const EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "Stop",
];

/// Codex as an install target (timeouts in seconds).
pub const TARGET: HookTarget = HookTarget {
    agent: "codex",
    events: EVENTS,
    tool_events: &["PreToolUse", "PermissionRequest", "PostToolUse"],
    timeout: 5,
    powershell_on_windows: false,
};

/// Interprets one hook call.
pub fn from_hook(event: &str, payload: &str) -> Update {
    let json: serde_json::Value = serde_json::from_str(payload).unwrap_or_default();
    let name = json
        .get("hook_event_name")
        .and_then(|v| v.as_str())
        .unwrap_or(event);
    if name == "PermissionRequest" {
        let mut update = crate::claude::from_hook("PreToolUse", payload);
        update.state = Some(AgentState::Blocked {
            kind: Some("permission".to_owned()),
        });
        return update;
    }
    crate::claude::from_hook(event, payload)
}

/// `~/.codex/config.toml` (or `$CODEX_HOME/config.toml`).
pub fn config_path(home: &Path) -> std::path::PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"))
        .join("config.toml")
}

fn marker() -> String {
    format!(" hook --agent {} --event ", TARGET.agent)
}

/// The `command` of an entry, if it is one hivetty wrote.
fn our_command(entry: &Table) -> Option<String> {
    entry.get("hooks")?.as_array()?.iter().find_map(|h| {
        let command = h.as_inline_table()?.get("command")?.as_str()?;
        command.contains(&marker()).then(|| command.to_owned())
    })
}

fn new_entry(exe: &Path, event: &str) -> Table {
    let mut hook = InlineTable::new();
    hook.insert("type", "command".into());
    hook.insert("command", TARGET.command_for(exe, event, false).into());
    hook.insert(
        "commandWindows",
        crate::install::powershell_command(&TARGET, exe, event).into(),
    );
    hook.insert("timeout", i64::try_from(TARGET.timeout).unwrap_or(5).into());
    let mut hooks = Array::new();
    hooks.push(Value::InlineTable(hook));
    let mut entry = Table::new();
    if TARGET.tool_events.contains(&event) {
        entry.insert("matcher", toml_edit::value("*"));
    }
    entry.insert("hooks", toml_edit::value(hooks));
    entry
}

fn parse(text: &str) -> Result<DocumentMut, String> {
    text.parse::<DocumentMut>()
        .map_err(|e| format!("config.toml is not valid TOML: {e}"))
}

/// Adds hivetty's hooks to Codex's `config.toml`, keeping everything else (and its comments)
/// as it is. Entries written before with another binary path are updated.
pub fn plan_install(text: &str, exe: &Path) -> Result<Plan, String> {
    crate::install::check_exe(exe)?;
    let mut doc = parse(text)?;
    let hooks = doc
        .entry("hooks")
        .or_insert_with(|| {
            let mut t = Table::new();
            t.set_implicit(true);
            Item::Table(t)
        })
        .as_table_mut()
        .ok_or("\"hooks\" in config.toml is not a table")?;
    let mut changes = Vec::new();
    for event in EVENTS {
        let wanted = new_entry(exe, event);
        let wanted_command = TARGET.command_for(exe, event, false);
        let list = hooks
            .entry(event)
            .or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()))
            .as_array_of_tables_mut()
            .ok_or_else(|| format!("hooks.{event} in config.toml is not a list of tables"))?;
        let existing = list.iter().position(|entry| our_command(entry).is_some());
        match existing {
            Some(i) => {
                let entry = list.get_mut(i).expect("index from position");
                if our_command(entry).as_deref() != Some(wanted_command.as_str()) {
                    *entry = wanted;
                    changes.push(format!("~ {event}: {wanted_command}"));
                }
            }
            None => {
                list.push(wanted);
                changes.push(format!("+ {event}: {wanted_command}"));
            }
        }
    }
    Ok(Plan {
        new_text: doc.to_string(),
        changes,
    })
}

/// Removes hivetty's hooks from Codex's `config.toml` (and lists left empty).
pub fn plan_uninstall(text: &str) -> Result<Plan, String> {
    let mut doc = parse(text)?;
    let mut changes = Vec::new();
    if let Some(hooks) = doc.get_mut("hooks").and_then(Item::as_table_mut) {
        let events: Vec<String> = hooks.iter().map(|(k, _)| k.to_owned()).collect();
        for event in events {
            let Some(list) = hooks.get_mut(&event).and_then(Item::as_array_of_tables_mut) else {
                continue;
            };
            let before = list.len();
            list.retain(|entry| our_command(entry).is_none());
            if list.len() != before {
                changes.push(format!("- {event}"));
            }
            if list.is_empty() {
                hooks.remove(&event);
            }
        }
        if hooks.is_empty() {
            doc.remove("hooks");
        }
    }
    Ok(Plan {
        new_text: doc.to_string(),
        changes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Activity, Subtitle};

    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/../../tests/fixtures/hooks/codex/0.171.0/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    #[test]
    fn real_payloads() {
        assert_eq!(
            from_hook("SessionStart", &fixture("SessionStart")).state,
            Some(AgentState::Starting)
        );
        let prompt = from_hook("UserPromptSubmit", &fixture("UserPromptSubmit"));
        assert_eq!(prompt.state, Some(AgentState::Working { progress: None }));
        let tool = from_hook("PreToolUse", &fixture("PreToolUse"));
        assert_eq!(
            tool.subtitle,
            Some(Subtitle::Activity(Activity::Running("echo hi".into())))
        );
        assert!(tool.agent_session.is_some());
        assert_eq!(
            from_hook("Stop", &fixture("Stop")).state,
            Some(AgentState::Done)
        );
        let permission = fixture("PreToolUse").replace("\"PreToolUse\"", "\"PermissionRequest\"");
        assert_eq!(
            from_hook("PermissionRequest", &permission).state,
            Some(AgentState::Blocked {
                kind: Some("permission".into())
            })
        );
    }

    #[test]
    fn config_toml_keeps_the_users_settings_and_comments() {
        let exe = Path::new("/opt/hivetty/hivetty");
        let user = "# my settings\nmodel = \"gpt-6.2-sol\"\n\n[[hooks.Stop]]\nhooks = [{ type = \"command\", command = \"notify-send done\" }]\n";
        let plan = plan_install(user, exe).unwrap();
        assert_eq!(plan.changes.len(), EVENTS.len());
        assert!(
            plan.new_text
                .starts_with("# my settings\nmodel = \"gpt-6.2-sol\"")
        );
        assert!(plan.new_text.contains("notify-send done"));
        let parsed: toml::Table = toml::from_str(&plan.new_text).unwrap();
        let stop = parsed["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2, "the user's own Stop hook stays");
        let pre = &parsed["hooks"]["PreToolUse"].as_array().unwrap()[0];
        assert_eq!(pre["matcher"].as_str(), Some("*"));
        let hook = &pre["hooks"].as_array().unwrap()[0];
        assert_eq!(
            hook["command"].as_str(),
            Some("'/opt/hivetty/hivetty' hook --agent codex --event PreToolUse")
        );
        assert!(hook["commandWindows"].as_str().unwrap().starts_with("& '"));
        // Again: nothing to do. Then removing keeps the user's hook.
        assert!(
            plan_install(&plan.new_text, exe)
                .unwrap()
                .changes
                .is_empty()
        );
        let removed = plan_uninstall(&plan.new_text).unwrap();
        assert_eq!(removed.changes.len(), EVENTS.len());
        assert!(removed.new_text.contains("notify-send done"));
        assert!(!removed.new_text.contains("hivetty"));
        assert!(plan_install("not = [toml", exe).is_err());
    }
}
