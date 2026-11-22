//! Claude Code hooks (docs/agent-integration.md 2.3). Fixtures from real runs live in
//! `tests/fixtures/hooks/claude/<version>/`.

use serde_json::Value;

use crate::{Activity, AgentState, Subtitle, Update};

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
            Some(Subtitle::Activity(Activity::Thinking)),
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
            (
                Some(AgentState::Blocked { kind }),
                message.map(Subtitle::Text),
            )
        }
        "Stop" => (Some(AgentState::Done), None),
        "SessionEnd" => (Some(AgentState::Exited), None),
        _ => (None, None),
    };
    Update {
        state,
        subtitle: subtitle.map(shorten),
        agent_session,
    }
}

/// What a tool call is doing, from the tool and its input. A command's own description is
/// the agent's text; everything else is an [`Activity`] the UI words.
fn tool_action(tool: &str, input: Option<&Value>) -> Option<Subtitle> {
    let field = |key: &str| input.and_then(|i| i.get(key)).and_then(Value::as_str);
    let file = field("file_path").or_else(|| field("notebook_path"));
    let activity = |a: Activity| Some(Subtitle::Activity(a));
    let subtitle = match tool {
        "Bash" | "PowerShell" => field("description")
            .map(|d| Subtitle::Text(d.to_owned()))
            .or_else(|| field("command").and_then(|c| activity(Activity::Running(c.to_owned())))),
        "Edit" | "MultiEdit" | "NotebookEdit" => {
            file.and_then(|f| activity(Activity::Editing(short_path(f))))
        }
        "Write" => file.and_then(|f| activity(Activity::Writing(short_path(f)))),
        "Read" => file.and_then(|f| activity(Activity::Reading(short_path(f)))),
        "Grep" | "Glob" => {
            field("pattern").and_then(|p| activity(Activity::Searching(p.to_owned())))
        }
        "WebFetch" => field("url").and_then(|u| activity(Activity::Fetching(u.to_owned()))),
        "WebSearch" => field("query").and_then(|q| activity(Activity::SearchingWeb(q.to_owned()))),
        "Task" | "Agent" => {
            field("description").and_then(|d| activity(Activity::SubAgent(d.to_owned())))
        }
        _ => None,
    };
    Some(subtitle.unwrap_or(Subtitle::Activity(Activity::Using(tool.to_owned()))))
}

/// Keeps the free text of a subtitle to one short line.
pub(crate) fn shorten(subtitle: Subtitle) -> Subtitle {
    match subtitle {
        Subtitle::Text(t) => Subtitle::Text(truncate(&t)),
        Subtitle::Activity(a) => Subtitle::Activity(match a {
            Activity::Running(t) => Activity::Running(truncate(&t)),
            Activity::Searching(t) => Activity::Searching(truncate(&t)),
            Activity::Fetching(t) => Activity::Fetching(truncate(&t)),
            Activity::SearchingWeb(t) => Activity::SearchingWeb(truncate(&t)),
            Activity::SubAgent(t) => Activity::SubAgent(truncate(&t)),
            other => other,
        }),
    }
}

/// The last two components of a path, which is what fits in a sidebar row.
pub(crate) fn short_path(path: &str) -> String {
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
        assert_eq!(
            tool.subtitle,
            Some(Subtitle::Text("Echo hook test string".into()))
        );
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
            update.subtitle,
            Some(Subtitle::Text(
                "Claude needs your permission to use Bash".into()
            ))
        );
    }

    #[test]
    fn tool_actions_are_short_and_readable() {
        let edit = from_hook(
            "PreToolUse",
            r#"{"tool_name":"Edit","tool_input":{"file_path":"C:\\repo\\crates\\hv-ui\\src\\lib.rs"}}"#,
        );
        assert_eq!(
            edit.subtitle,
            Some(Subtitle::Activity(Activity::Editing("src/lib.rs".into())))
        );
        let other = from_hook("PreToolUse", r#"{"tool_name":"mcp__x__y","tool_input":{}}"#);
        assert_eq!(
            other.subtitle,
            Some(Subtitle::Activity(Activity::Using("mcp__x__y".into())))
        );
    }

    #[test]
    fn garbage_never_panics() {
        assert_eq!(from_hook("Stop", "not json").state, Some(AgentState::Done));
        assert_eq!(from_hook("Whatever", "").state, None);
        let long = format!(
            r#"{{"tool_name":"Bash","tool_input":{{"description":"{}"}}}}"#,
            "a".repeat(500)
        );
        let Some(Subtitle::Text(text)) = from_hook("PreToolUse", &long).subtitle else {
            panic!("a command description stays text");
        };
        assert_eq!(text.chars().count(), MAX_SUBTITLE);
    }
}
