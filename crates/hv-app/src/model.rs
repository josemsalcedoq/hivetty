//! Builds what the chrome shows from the session state (shared by the window and `--screenshot`).

use std::path::Path;
use std::time::Instant;

use hv_agents::{AgentState, Source, Tracker};
use hv_term::osc::TermEvent;
use hv_ui::{RowState, SessionRow, format_elapsed, strings};

/// What the app tracks about the session in one pane.
#[derive(Debug, Clone)]
pub(crate) struct SessionInfo {
    /// Last title the program set (OSC 0/2), if any.
    pub(crate) title: Option<String>,
    /// When the session started.
    pub(crate) started: Instant,
    /// Whether the program has exited.
    pub(crate) exited: bool,
    /// Agent state from OSC 7501, hooks and notifications.
    pub(crate) agent: Tracker,
    /// Something finished or needs the user here while another pane had the focus.
    pub(crate) unseen: bool,
    /// The bell rang here while another pane had the focus.
    pub(crate) bell: bool,
    /// OSC 9;4 progress: `(state, percent)`; state 1 = normal, 2 = error, 3 = indeterminate,
    /// 4 = paused.
    pub(crate) progress: Option<(u8, u8)>,
}

impl SessionInfo {
    pub(crate) fn new() -> Self {
        Self {
            title: None,
            started: Instant::now(),
            exited: false,
            agent: Tracker::default(),
            unseen: false,
            bell: false,
            progress: None,
        }
    }

    /// Feeds a terminal event. Returns true if the sidebar changed.
    pub(crate) fn on_term_event(&mut self, event: &TermEvent) -> bool {
        let now = Instant::now();
        match event {
            TermEvent::ProgramStatus(status) => hv_agents::from_program_status(
                &status.state,
                status.kind.as_deref(),
                status.progress,
                status.message.as_deref(),
            )
            .is_some_and(|update| self.agent.apply(Source::Osc7501, update, now)),
            TermEvent::Notify { body, .. } => self.agent.apply(
                Source::Notify,
                hv_agents::Update {
                    state: Some(AgentState::Blocked { kind: None }),
                    subtitle: Some(body.clone()).filter(|b| !b.trim().is_empty()),
                    agent_session: None,
                },
                now,
            ),
            TermEvent::Progress { state, value } => {
                let new = (*state != 0).then_some((*state, *value));
                let changed = new != self.progress;
                self.progress = new;
                changed
            }
            _ => false,
        }
    }

    /// Whether the current state is one the user should come back for.
    pub(crate) fn needs_attention(&self) -> bool {
        matches!(
            self.agent.state(),
            Some(AgentState::Done | AgentState::Blocked { .. } | AgentState::Error)
        )
    }

    /// Feeds a hook forwarded by `hivetty hook` (already routed to this pane by its
    /// `HIVETTY_SESSION_ID`). Returns true if the sidebar changed.
    pub(crate) fn on_hook(&mut self, hook: &hv_ipc::HookEvent) -> bool {
        let update = match hook.agent.as_str() {
            "claude" => hv_agents::claude::from_hook(&hook.event, &hook.payload),
            _ => return false,
        };
        self.agent.apply(Source::Hook, update, Instant::now())
    }
}

/// When the formatted elapsed time of a session started at `started` next changes.
pub(crate) fn next_elapsed_change(started: Instant, now: Instant) -> Instant {
    let secs = now.saturating_duration_since(started).as_secs();
    let step = match secs {
        0..60 => 1,
        60..3600 => 60,
        3600..86_400 => 3600,
        _ => 86_400,
    };
    started + std::time::Duration::from_secs((secs / step + 1) * step)
}

/// The sidebar row for a session.
pub(crate) fn session_row(info: &SessionInfo, cwd: Option<&Path>) -> SessionRow {
    let project = cwd
        .and_then(|p| p.file_name())
        .map_or_else(|| "~".to_owned(), |n| n.to_string_lossy().into_owned());
    let title = info
        .title
        .clone()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| strings::DEFAULT_TITLE.to_owned());
    let (state, label) = if info.exited {
        (RowState::Done, strings::STATE_EXITED)
    } else {
        match info.agent.state() {
            None => (RowState::Working, strings::STATE_RUNNING),
            Some(AgentState::Starting) => (RowState::Working, strings::STATE_STARTING),
            Some(AgentState::Working { .. }) => (RowState::Working, strings::STATE_WORKING),
            Some(AgentState::Blocked { kind }) => (
                RowState::Waiting,
                match kind.as_deref() {
                    Some("permission") => strings::STATE_NEEDS_PERMISSION,
                    Some("auth") => strings::STATE_NEEDS_LOGIN,
                    _ => strings::STATE_WAITING,
                },
            ),
            Some(AgentState::Idle) => (RowState::Idle, strings::STATE_IDLE),
            Some(AgentState::Done) => (RowState::Done, strings::STATE_DONE),
            Some(AgentState::Error) => (RowState::Error, strings::STATE_ERROR),
            Some(AgentState::Exited) => (RowState::Done, strings::STATE_EXITED),
        }
    };
    let subtitle = match (info.exited, info.agent.subtitle()) {
        (false, Some(detail)) if state == RowState::Waiting => format!("{label}: {detail}"),
        (false, Some(detail)) => detail.to_owned(),
        _ => label.to_owned(),
    };
    let started = info
        .agent
        .since()
        .filter(|_| !info.exited)
        .unwrap_or(info.started);
    SessionRow {
        title,
        subtitle,
        project,
        elapsed: format_elapsed(started.elapsed().as_secs()),
        state,
        focused: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn elapsed_ticks_follow_the_display_granularity() {
        let t0 = Instant::now();
        let at = |s: u64| next_elapsed_change(t0, t0 + Duration::from_millis(s * 1000 + 300));
        assert_eq!(at(5), t0 + Duration::from_secs(6));
        assert_eq!(at(59), t0 + Duration::from_secs(60));
        assert_eq!(at(61), t0 + Duration::from_secs(120));
        assert_eq!(at(3601), t0 + Duration::from_secs(7200));
    }

    fn hook(event: &str, payload: &str) -> hv_ipc::HookEvent {
        hv_ipc::HookEvent {
            session: Some("1".into()),
            agent: "claude".into(),
            event: event.into(),
            payload: payload.into(),
        }
    }

    #[test]
    fn progress_and_attention_follow_terminal_events() {
        let mut info = SessionInfo::new();
        assert!(info.on_term_event(&TermEvent::Progress {
            state: 1,
            value: 40
        }));
        assert_eq!(info.progress, Some((1, 40)));
        assert!(!info.on_term_event(&TermEvent::Progress {
            state: 1,
            value: 40
        }));
        assert!(info.on_term_event(&TermEvent::Progress { state: 0, value: 0 }));
        assert_eq!(info.progress, None);
        assert!(!info.needs_attention());
        info.on_term_event(&TermEvent::Notify {
            title: None,
            body: "Claude needs your input".into(),
        });
        assert!(info.needs_attention());
    }

    #[test]
    fn hooks_drive_the_sidebar_row() {
        let mut info = SessionInfo::new();
        assert!(info.on_hook(&hook(
            "PreToolUse",
            r#"{"tool_name":"Bash","tool_input":{"description":"Running tests"}}"#
        )));
        let row = session_row(&info, None);
        assert_eq!(
            (row.state, row.subtitle.as_str()),
            (RowState::Working, "Running tests")
        );

        assert!(info.on_hook(&hook(
            "Notification",
            r#"{"message":"Bash(cargo test)","notification_type":"permission_prompt"}"#
        )));
        let row = session_row(&info, None);
        assert_eq!(row.state, RowState::Waiting);
        assert_eq!(row.subtitle, "Needs permission: Bash(cargo test)");

        let other_agent = hv_ipc::HookEvent {
            agent: "unknown".into(),
            ..hook("Stop", "{}")
        };
        assert!(!info.on_hook(&other_agent));
    }
}
