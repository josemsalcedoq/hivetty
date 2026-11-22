//! Claude Code hooks (docs/agent-integration.md 2.3). Fixtures from real runs live in
//! `tests/fixtures/hooks/claude/<version>/`.

use serde_json::Value;

use crate::{AgentState, Update};

/// Longest subtitle kept, in characters.
const MAX_SUBTITLE: usize = 120;

/// Interprets one hook call. `event` is the name given on the command line; the payload's own
/// `hook_event_name` wins when present. Unknown events and malformed payloads give an empty
/// update instead of an error: a hook must never break the agent.
pub fn from_hook(event: &str, payload: &str) -> Update {
    let json: Value = serde_json::from_str(payload).unwrap_or(Value::Null);
    let event = json
        .get("hook_event_name")
        .and_then(Value::as_str)
        .unwrap_or(event);
    let text = |key: &str| json.get(key).and_then(Value::as_str);
    let agent_session = text("session_id").map(str::to_owned);
    let (state, subtitle) = match event {
        "SessionStart" => (Some(AgentState::Starting), None),
        "UserPromptSubmit" => (
            Some(AgentState::Working { progress: None }),
            Some("Thinking…".to_owned()),
        ),
        "PreToolUse" => (
            Some(AgentState::Working { progress: None }),
            tool_action(text("tool_name").unwrap_or("tool"), json.get("tool_input")),
        ),
        "PostToolUse" | "PostToolUseFailure" => {
            (Some(AgentState::Working { progress: None }), None)
        }
        "Notification" => {
            let message = text("message").map(str::to_owned);
            let kind = match text("notification_type") {
                Some("permission_prompt") => Some("permission".to_owned()),
                Some("idle_prompt") => Some("question".to_owned()),
                Some("auth_success") => None,
                _ => message
                    .as_deref()
                    .filter(|m| m.to_ascii_lowercase().contains("permission"))
                    .map(|_| "permission".to_owned()),
            };
            (Some(AgentState::Blocked { kind }), message)
        }
        "Stop" => (Some(AgentState::Done), None),
        "SessionEnd" => (Some(AgentState::Exited), None),
        _ => (None, None),
    };
    Update {
        state,
        subtitle: subtitle.map(|s| truncate(&s)),
        agent_session,
    }
}

/// "Running tests", "Editing src/main.rs"… from the tool and its input.
fn tool_action(tool: &str, input: Option<&Value>) -> Option<String> {
    let field = |key: &str| input.and_then(|i| i.get(key)).and_then(Value::as_str);
    let file = field("file_path").or_else(|| field("notebook_path"));
    let action = match tool {
        "Bash" | "PowerShell" => field("description")
            .map(str::to_owned)
            .or_else(|| field("command").map(|c| format!("Running {c}"))),
        "Edit" | "MultiEdit" | "NotebookEdit" => file.map(|f| format!("Editing {}", short_path(f))),
        "Write" => file.map(|f| format!("Writing {}", short_path(f))),
        "Read" => file.map(|f| format!("Reading {}", short_path(f))),
        "Grep" | "Glob" => field("pattern").map(|p| format!("Searching {p}")),
        "WebFetch" => field("url").map(|u| format!("Fetching {u}")),
        "WebSearch" => field("query").map(|q| format!("Searching the web for {q}")),
        "Task" | "Agent" => field("description").map(|d| format!("Agent: {d}")),
        _ => None,
    };
    Some(action.unwrap_or_else(|| format!("Using {tool}")))
}

/// The last two components of a path, which is what fits in a sidebar row.
fn short_path(path: &str) -> String {
    let parts: Vec<&str> = path.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    let start = parts.len().saturating_sub(2);
    parts[start..].join("/")
}

fn truncate(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default().trim();
    if line.chars().count() <= MAX_SUBTITLE {
        line.to_owned()
    } else {
        line.chars().take(MAX_SUBTITLE - 1).collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/../../tests/fixtures/hooks/claude/2.1.330/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    #[test]
    fn real_payloads_from_claude_code_2_1_330() {
        let start = from_hook("x", &fixture("SessionStart"));
        assert_eq!(start.state, Some(AgentState::Starting));
        assert_eq!(
            start.agent_session.as_deref(),
            Some("ed27be5b-573d-41de-9db1-c5c2b5e16934")
        );
        let prompt = from_hook("x", &fixture("UserPromptSubmit"));
        assert_eq!(prompt.state, Some(AgentState::Working { progress: None }));
        let tool = from_hook("x", &fixture("PreToolUse"));
        assert_eq!(tool.subtitle.as_deref(), Some("Echo hook test string"));
        assert_eq!(
            from_hook("x", &fixture("PostToolUse")).state,
            Some(AgentState::Working { progress: None })
        );
        assert_eq!(
            from_hook("x", &fixture("Stop")).state,
            Some(AgentState::Done)
        );
        assert_eq!(
            from_hook("x", &fixture("SessionEnd")).state,
            Some(AgentState::Exited)
        );
    }

    #[test]
    fn notification_becomes_blocked_with_a_reason() {
        let update = from_hook(
            "Notification",
            r#"{"message":"Claude needs your permission to use Bash","notification_type":"permission_prompt"}"#,
        );
        assert_eq!(
            update.state,
            Some(AgentState::Blocked {
                kind: Some("permission".into())
            })
        );
        assert_eq!(
            update.subtitle.as_deref(),
            Some("Claude needs your permission to use Bash")
        );
    }

    #[test]
    fn tool_actions_are_short_and_readable() {
        let edit = from_hook(
            "PreToolUse",
            r#"{"tool_name":"Edit","tool_input":{"file_path":"C:\\repo\\crates\\hv-ui\\src\\lib.rs"}}"#,
        );
        assert_eq!(edit.subtitle.as_deref(), Some("Editing src/lib.rs"));
        let other = from_hook("PreToolUse", r#"{"tool_name":"mcp__x__y","tool_input":{}}"#);
        assert_eq!(other.subtitle.as_deref(), Some("Using mcp__x__y"));
    }

    #[test]
    fn garbage_never_panics() {
        assert_eq!(from_hook("Stop", "not json").state, Some(AgentState::Done));
        assert_eq!(from_hook("Whatever", "").state, None);
        let long = format!(
            r#"{{"tool_name":"Bash","tool_input":{{"description":"{}"}}}}"#,
            "a".repeat(500)
        );
        assert_eq!(
            from_hook("PreToolUse", &long)
                .subtitle
                .unwrap()
                .chars()
                .count(),
            MAX_SUBTITLE
        );
    }
}
