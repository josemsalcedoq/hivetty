//! Gemini CLI hooks (F4: live state for a second agent). They mirror Claude Code's: JSON on
//! stdin with `session_id`, `cwd` and `hook_event_name`, configured in
//! `~/.gemini/settings.json`. Real payloads from version 0.70.0 are in
//! `tests/fixtures/hooks/gemini-cli/0.70.0/`; `Notification` follows the documentation (it only
//! fires on an interactive permission prompt).

use serde_json::Value;

use crate::claude::{short_path, shorten};
use crate::{Activity, AgentState, Subtitle, Update};

/// Interprets one hook call, like [`crate::claude::from_hook`].
pub fn from_hook(event: &str, payload: &str) -> Update {
    let json: Value = serde_json::from_str(payload).unwrap_or(Value::Null);
    let event = json
        .get("hook_event_name")
        .and_then(Value::as_str)
        .unwrap_or(event);
    let text = |key: &str| json.get(key).and_then(Value::as_str);
    let agent_session = text("session_id").map(str::to_owned);
    let working = Some(AgentState::Working { progress: None });
    let (state, subtitle) = match event {
        "SessionStart" => (Some(AgentState::Starting), None),
        "BeforeAgent" => (working, Some(Subtitle::Activity(Activity::Thinking))),
        "BeforeTool" => (
            working,
            Some(tool_action(
                text("tool_name").unwrap_or("tool"),
                json.get("tool_input"),
            )),
        ),
        "AfterTool" => (working, None),
        "Notification" => {
            let kind = match text("notification_type") {
                Some("ToolPermission") => Some("permission".to_owned()),
                _ => None,
            };
            (
                Some(AgentState::Blocked { kind }),
                text("message").map(|m| Subtitle::Text(m.to_owned())),
            )
        }
        "AfterAgent" => (Some(AgentState::Done), None),
        "SessionEnd" => (Some(AgentState::Exited), None),
        _ => (None, None),
    };
    Update {
        state,
        subtitle: subtitle.map(shorten),
        agent_session,
    }
}

/// What a Gemini CLI tool call is doing.
fn tool_action(tool: &str, input: Option<&Value>) -> Subtitle {
    let field = |key: &str| input.and_then(|i| i.get(key)).and_then(Value::as_str);
    let file = field("file_path")
        .or_else(|| field("absolute_path"))
        .or_else(|| field("dir_path"))
        .or_else(|| field("path"));
    let activity = match tool {
        "run_shell_command" => field("description").map(return_text).or_else(|| {
            field("command").map(|c| Subtitle::Activity(Activity::Running(c.to_owned())))
        }),
        "replace" | "edit" => file.map(|f| Subtitle::Activity(Activity::Editing(short_path(f)))),
        "write_file" => file.map(|f| Subtitle::Activity(Activity::Writing(short_path(f)))),
        "read_file" | "read_many_files" | "list_directory" => {
            file.map(|f| Subtitle::Activity(Activity::Reading(short_path(f))))
        }
        "glob" | "search_file_content" | "grep_search" => {
            field("pattern").map(|p| Subtitle::Activity(Activity::Searching(p.to_owned())))
        }
        "web_fetch" => field("url")
            .or_else(|| field("prompt"))
            .map(|u| Subtitle::Activity(Activity::Fetching(u.to_owned()))),
        "google_web_search" => {
            field("query").map(|q| Subtitle::Activity(Activity::SearchingWeb(q.to_owned())))
        }
        _ => None,
    };
    activity.unwrap_or_else(|| Subtitle::Activity(Activity::Using(tool.to_owned())))
}

/// A command's own description is the agent's words.
fn return_text(description: &str) -> Subtitle {
    Subtitle::Text(description.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/../../tests/fixtures/hooks/gemini-cli/0.70.0/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    #[test]
    fn a_real_turn_goes_working_then_done() {
        let start = from_hook("SessionStart", &fixture("SessionStart"));
        assert_eq!(start.state, Some(AgentState::Starting));
        assert!(start.agent_session.is_some());
        let prompt = from_hook("BeforeAgent", &fixture("BeforeAgent"));
        assert_eq!(prompt.state, Some(AgentState::Working { progress: None }));
        assert_eq!(
            prompt.subtitle,
            Some(Subtitle::Activity(Activity::Thinking))
        );
        let tool = from_hook("BeforeTool", &fixture("BeforeTool"));
        assert_eq!(
            tool.subtitle,
            Some(Subtitle::Activity(Activity::Reading("me/proj".into())))
        );
        assert_eq!(
            from_hook("AfterTool", &fixture("AfterTool")).state,
            Some(AgentState::Working { progress: None })
        );
        assert_eq!(
            from_hook("AfterAgent", &fixture("AfterAgent")).state,
            Some(AgentState::Done)
        );
        assert_eq!(
            from_hook("SessionEnd", &fixture("SessionEnd")).state,
            Some(AgentState::Exited)
        );
    }

    #[test]
    fn permission_notifications_and_garbage() {
        let n = from_hook(
            "Notification",
            r#"{"hook_event_name":"Notification","notification_type":"ToolPermission","message":"Allow run_shell_command?"}"#,
        );
        assert_eq!(
            n.state,
            Some(AgentState::Blocked {
                kind: Some("permission".into())
            })
        );
        assert_eq!(from_hook("Weird", "not json").state, None);
    }
}
