//! Builds what the chrome shows from the session state (shared by the window and `--screenshot`).

use std::path::Path;
use std::time::{Duration, Instant};

use hv_agents::{AgentState, Source, Tracker};
use hv_term::osc::TermEvent;
use hv_ui::{RowState, SessionRow, format_elapsed, strings};

/// What the app tracks about the session in one pane.
#[derive(Debug, Clone)]
pub(crate) struct SessionInfo {
    /// Last title the program set (OSC 0/2), if any.
    pub(crate) title: Option<String>,
    /// A title the user gave the session; wins over the program's (S-13).
    pub(crate) custom_title: Option<String>,
    /// The repository and the git worktree hivetty created for this session (F4-22).
    pub(crate) worktree: Option<(std::path::PathBuf, std::path::PathBuf)>,
    /// Remove that worktree when the pane closes (the user asked to).
    pub(crate) remove_worktree: bool,
    /// The agent's transcript, from its hooks (`transcript_path`).
    pub(crate) transcript: Option<std::path::PathBuf>,
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
    /// When the running shell command started (OSC 133;C).
    pub(crate) command_started: Option<Instant>,
    /// Agent running in the pane (profile id), from the process tree or its hooks.
    pub(crate) agent_id: Option<String>,
    /// Its display name.
    pub(crate) agent_name: Option<String>,
    /// Context window used, from the agent's statusline (F4-32).
    pub(crate) context_percent: Option<f64>,
    /// Since when the agent has been working, as seen here (F4-23): Unix seconds for the record,
    /// and the monotonic clock for its length (which skips a suspended machine on macOS and
    /// Linux).
    pub(crate) working_since: Option<(i64, Instant)>,
    /// The last output that looked like work, and when the current stretch of it began (F4-09).
    pub(crate) last_activity: Option<Instant>,
    pub(crate) work_started: Option<Instant>,
    /// The last tool the agent used, worded for the hover card (S-15).
    pub(crate) last_tool: Option<String>,
    /// The attention state last notified ("waiting", "done", "error"), so a state is announced
    /// once.
    pub(crate) notified: Option<&'static str>,
}

impl SessionInfo {
    pub(crate) fn new() -> Self {
        Self {
            title: None,
            custom_title: None,
            worktree: None,
            remove_worktree: false,
            transcript: None,
            started: Instant::now(),
            exited: false,
            agent: Tracker::default(),
            unseen: false,
            bell: false,
            progress: None,
            command_started: None,
            agent_id: None,
            agent_name: None,
            last_tool: None,
            last_activity: None,
            work_started: None,
            working_since: None,
            context_percent: None,
            notified: None,
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
                    subtitle: Some(body.clone())
                        .filter(|b| !b.trim().is_empty())
                        .map(hv_agents::Subtitle::Text),
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

    /// Tracks shell commands from OSC 133 marks. On `D`, returns how long the command ran and
    /// its exit code.
    pub(crate) fn command_finished(
        &mut self,
        event: &TermEvent,
        now: Instant,
    ) -> Option<(std::time::Duration, Option<i32>)> {
        match event {
            TermEvent::PromptMark { kind: 'C', .. } => {
                self.command_started = Some(now);
                None
            }
            TermEvent::PromptMark {
                kind: 'D',
                exit_code,
            } => self
                .command_started
                .take()
                .map(|start| (now.saturating_duration_since(start), *exit_code)),
            _ => None,
        }
    }

    /// The notification for the current state, once per state (docs/ui-spec.md §7):
    /// `(kind, title, body)` with title `<Agent> · <project>` and body `<title>: <reason>`.
    pub(crate) fn take_notification(
        &mut self,
        project: &str,
    ) -> Option<(&'static str, String, String)> {
        let kind = match self.agent.state() {
            Some(AgentState::Blocked { .. }) => "waiting",
            Some(AgentState::Done) => "done",
            Some(AgentState::Error) => "error",
            _ => {
                self.notified = None;
                return None;
            }
        };
        if self.notified == Some(kind) {
            return None;
        }
        self.notified = Some(kind);
        let agent = self.agent_name.as_deref().unwrap_or("hivetty");
        let row = session_row(self, None);
        let title = format!("{agent} · {project}");
        let body = format!("{}: {}", row.title, row.subtitle);
        Some((kind, title, body))
    }

    /// Guesses the state of an agent that does not report it (F4-09, A-19) from when the
    /// program last wrote output and when hivetty last sent it input. Output right after input
    /// is an echo or a redraw. Recent output means working; `quiet` without output after a
    /// real stretch of work means done (a short burst goes back to idle, without a
    /// notification). Returns true if the sidebar changed.
    pub(crate) fn estimate(
        &mut self,
        output: Option<Instant>,
        input: Option<Instant>,
        now: Instant,
        quiet: Duration,
    ) -> bool {
        if self.exited || self.agent_id.is_none() || self.agent.reliable() {
            return false;
        }
        if let Some(out) = output
            && self.last_activity.is_none_or(|last| out > last)
            && input.is_none_or(|i| out < i || out.duration_since(i) > ECHO)
        {
            self.last_activity = Some(out);
        }
        let Some(last) = self.last_activity else {
            return false;
        };
        let working = matches!(self.agent.state(), Some(AgentState::Working { .. }));
        let quiet_for = now.saturating_duration_since(last);
        let state = if quiet_for < ACTIVE {
            if !working {
                self.work_started = Some(last);
            }
            AgentState::Working { progress: None }
        } else if quiet_for >= quiet && working && self.agent.estimated() {
            let worked = last.saturating_duration_since(self.work_started.unwrap_or(last));
            if worked >= MIN_WORK {
                AgentState::Done
            } else {
                AgentState::Idle
            }
        } else {
            return false;
        };
        let update = hv_agents::Update {
            state: Some(state),
            ..Default::default()
        };
        self.agent.apply(Source::Heuristic, update, now)
    }

    /// When [`Self::estimate`] next needs to run although nothing new arrived: the end of the
    /// quiet period of a guessed working state.
    pub(crate) fn estimate_deadline(&self, quiet: Duration) -> Option<Instant> {
        let guessing = self.agent.estimated()
            && matches!(self.agent.state(), Some(AgentState::Working { .. }));
        guessing
            .then(|| self.last_activity.map(|t| t + quiet))
            .flatten()
    }

    /// Whether the current state is one the user should come back for.
    pub(crate) fn needs_attention(&self) -> bool {
        matches!(
            self.agent.state(),
            Some(AgentState::Done | AgentState::Blocked { .. } | AgentState::Error)
        )
    }

    /// Opens or closes the observed working stretch after the state may have changed; `closing`
    /// when the pane goes away. Returns a stretch that just ended (F4-23).
    pub(crate) fn working_interval(
        &mut self,
        closing: bool,
    ) -> Option<hv_usage::observed::Interval> {
        let working = !closing
            && !self.exited
            && matches!(self.agent.state(), Some(AgentState::Working { .. }));
        match (working, self.working_since) {
            (true, None) => {
                self.working_since = Some((jiff::Timestamp::now().as_second(), Instant::now()));
                None
            }
            (false, Some((start, began))) => {
                self.working_since = None;
                let length = i64::try_from(began.elapsed().as_secs()).unwrap_or(0);
                Some(hv_usage::observed::Interval {
                    agent: self.agent_id.clone()?,
                    start,
                    end: start + length,
                })
                .filter(|i| i.end > i.start)
            }
            _ => None,
        }
    }

    /// Feeds a hook forwarded by `hivetty hook` (already routed to this pane by its
    /// `HIVETTY_SESSION_ID`). Returns true if the sidebar changed.
    pub(crate) fn on_hook(&mut self, hook: &hv_ipc::HookEvent) -> bool {
        // The statusline is not a state change; it only reports numbers.
        if hook.event == "StatusLine" {
            if let Some(status) = hv_agents::statusline::parse(&hook.payload) {
                self.context_percent = status.context_percent.or(self.context_percent);
            }
            return false;
        }
        if let Some(path) = transcript_path(&hook.payload) {
            self.transcript = Some(path);
        }
        let (update, id, name) = match hook.agent.as_str() {
            "claude" | "claude-code" => (
                hv_agents::claude::from_hook(&hook.event, &hook.payload),
                "claude-code",
                "Claude Code",
            ),
            "codex" => (
                hv_agents::codex::from_hook(&hook.event, &hook.payload),
                "codex",
                "Codex",
            ),
            "gemini" | "gemini-cli" => (
                hv_agents::gemini::from_hook(&hook.event, &hook.payload),
                "gemini-cli",
                "Gemini CLI",
            ),
            _ => return false,
        };
        if self.agent_id.is_none() {
            self.agent_id = Some(id.to_owned());
            self.agent_name = Some(name.to_owned());
        }
        let changed = self.agent.apply(Source::Hook, update, Instant::now());
        if let Some(hv_agents::Subtitle::Activity(activity)) = self.agent.subtitle()
            && *activity != hv_agents::Activity::Thinking
        {
            self.last_tool = self.agent.subtitle().map(subtitle_text);
        }
        changed
    }
}

/// The `transcript_path` of a hook payload: an absolute path to a `.jsonl` or `.json` file
/// (it is only ever shown in the file manager, never opened or run).
fn transcript_path(payload: &str) -> Option<std::path::PathBuf> {
    let json: serde_json::Value = serde_json::from_str(payload).ok()?;
    let path = std::path::PathBuf::from(json.get("transcript_path")?.as_str()?);
    let log = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("jsonl") || e.eq_ignore_ascii_case("json"));
    (path.is_absolute() && log).then_some(path)
}

/// Output this soon after input is its echo or a redraw, not work (F4-09).
const ECHO: Duration = Duration::from_millis(500);
/// Output this recent means the agent is working.
const ACTIVE: Duration = Duration::from_secs(2);
/// A stretch of work at least this long ends as done; a shorter burst goes back to idle.
const MIN_WORK: Duration = Duration::from_secs(3);

/// The sidebar wording of a subtitle: the agent's own text as is, activities from
/// `strings.rs`.
pub(crate) fn subtitle_text(subtitle: &hv_agents::Subtitle) -> String {
    use hv_agents::{Activity, Subtitle};
    match subtitle {
        Subtitle::Text(text) => text.clone(),
        Subtitle::Activity(activity) => match activity {
            Activity::Thinking => strings::THINKING.to_owned(),
            Activity::Running(c) => strings::running(c),
            Activity::Editing(f) => strings::editing(f),
            Activity::Writing(f) => strings::writing(f),
            Activity::Reading(f) => strings::reading(f),
            Activity::Searching(p) => strings::searching(p),
            Activity::Fetching(u) => strings::fetching(u),
            Activity::SearchingWeb(q) => strings::searching_web(q),
            Activity::SubAgent(d) => strings::sub_agent(d),
            Activity::Using(t) => strings::using(t),
        },
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
        .custom_title
        .clone()
        .or_else(|| info.title.clone())
        .filter(|t| !t.trim().is_empty())
        .or_else(|| info.agent_name.clone())
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
    let detail = info.agent.subtitle().map(subtitle_text);
    let subtitle = match (info.exited, detail) {
        (false, Some(detail)) if state == RowState::Waiting => format!("{label}: {detail}"),
        (false, Some(detail)) => detail,
        _ => label.to_owned(),
    };
    // A guessed state says so (A-19).
    let subtitle = if info.agent.estimated() && !info.exited {
        format!("{} {subtitle}", strings::ESTIMATED)
    } else {
        subtitle
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
        agent: info.agent_name.clone().unwrap_or_default(),
        unseen: info.unseen,
        worktree: info.worktree.is_some(),
        transcript: info.transcript.is_some(),
        card: Vec::new(),
    }
}

/// The chips for a directory, in display order: technology, branch, versions.
pub(crate) fn chips(info: &hv_shell::detect::ProjectInfo) -> Vec<hv_render::Chip> {
    use hv_render::Chip;
    use hv_shell::detect::Tech;
    use hv_term::Rgb8;
    let mut out = Vec::new();
    if let Some(tech) = info.tech {
        // Language colors as commonly used for these languages (no logos, docs/licensing.md).
        let color = match tech {
            Tech::Rust => 0xDEA584,
            Tech::Python => 0x3572A5,
            Tech::Bun => 0xFBF0DF,
            Tech::TypeScript => 0x3178C6,
            Tech::Node | Tech::JavaScript => 0xF1E05A,
            Tech::Go => 0x00ADD8,
            Tech::Flutter | Tech::Dart => 0x00B4AB,
            Tech::Deno => 0x70FFAF,
            Tech::Java => 0xB07219,
            Tech::Kotlin => 0xA97BFF,
            Tech::DotNet => 0x512BD4,
            Tech::Ruby => 0x701516,
            Tech::Php => 0x4F5D95,
            Tech::Elixir => 0x6E4A7E,
            Tech::Swift => 0xF05138,
            Tech::Zig => 0xEC915C,
            Tech::Cpp => 0xF34B7D,
        };
        out.push(Chip {
            label: tech.name().to_owned(),
            color: Rgb8::hex(color),
        });
    }
    if let Some(branch) = &info.branch {
        out.push(Chip {
            label: branch.clone(),
            color: Rgb8::hex(0xBC8CFF),
        });
    }
    for (tool, version) in &info.versions {
        out.push(Chip {
            label: format!("{tool} {version}"),
            color: Rgb8::hex(0x6B6B73),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_paths_are_absolute_logs_only() {
        let abs = if cfg!(windows) {
            "C:/Users/me/.claude/s.jsonl"
        } else {
            "/home/me/.claude/s.jsonl"
        };
        let payload = |p: &str| format!(r#"{{"transcript_path":"{p}"}}"#);
        assert_eq!(
            transcript_path(&payload(abs)),
            Some(std::path::PathBuf::from(abs))
        );
        assert_eq!(transcript_path(&payload("relative/s.jsonl")), None);
        let exe = if cfg!(windows) {
            "C:/evil/run.exe"
        } else {
            "/tmp/run.sh"
        };
        assert_eq!(transcript_path(&payload(exe)), None);
        assert_eq!(transcript_path("not json"), None);
    }
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
    fn gemini_hooks_name_the_agent_and_set_its_state() {
        let mut info = SessionInfo::new();
        let gemini = |event: &str, payload: &str| hv_ipc::HookEvent {
            session: Some("1".into()),
            agent: "gemini-cli".into(),
            event: event.into(),
            payload: payload.into(),
        };
        assert!(info.on_hook(&gemini(
            "BeforeTool",
            r#"{"hook_event_name":"BeforeTool","session_id":"s","tool_name":"run_shell_command","tool_input":{"command":"cargo test"}}"#,
        )));
        assert_eq!(info.agent_id.as_deref(), Some("gemini-cli"));
        assert_eq!(info.agent_name.as_deref(), Some("Gemini CLI"));
        assert_eq!(
            info.agent.state(),
            Some(&AgentState::Working { progress: None })
        );
        assert_eq!(info.last_tool.as_deref(), Some("Running cargo test"));
    }

    #[test]
    fn the_statusline_reports_context_without_changing_state() {
        let mut info = SessionInfo::new();
        info.on_hook(&hook("UserPromptSubmit", "{}"));
        let state = info.agent.state().cloned();
        let changed = info.on_hook(&hook(
            "StatusLine",
            r#"{"context_window":{"used_percentage":42}}"#,
        ));
        assert!(!changed);
        assert_eq!(info.agent.state().cloned(), state);
        assert_eq!(info.context_percent, Some(42.0));
    }

    #[test]
    fn states_are_guessed_from_output_without_hooks() {
        let quiet = Duration::from_secs(5);
        let t0 = Instant::now();
        let at = |s: u64| t0 + Duration::from_millis(s);
        let mut info = SessionInfo::new();
        // No agent: no guesses.
        assert!(!info.estimate(Some(at(0)), None, at(10), quiet));
        info.agent_id = Some("aider".into());
        // Output 100 ms after a key press is its echo.
        assert!(!info.estimate(Some(at(100)), Some(at(0)), at(200), quiet));
        assert_eq!(info.agent.state(), None);
        // Output well after the input: working, marked as a guess.
        assert!(info.estimate(Some(at(1_000)), Some(at(0)), at(1_100), quiet));
        assert_eq!(
            info.agent.state(),
            Some(&AgentState::Working { progress: None })
        );
        assert!(
            session_row(&info, None)
                .subtitle
                .starts_with(strings::ESTIMATED)
        );
        // Keeps working for 4 s, then 5 s of quiet: done.
        assert!(!info.estimate(Some(at(5_000)), Some(at(0)), at(5_100), quiet));
        assert_eq!(info.estimate_deadline(quiet), Some(at(10_000)));
        assert!(!info.estimate(Some(at(5_000)), Some(at(0)), at(8_000), quiet));
        assert!(info.estimate(Some(at(5_000)), Some(at(0)), at(10_000), quiet));
        assert_eq!(info.agent.state(), Some(&AgentState::Done));
        assert!(info.needs_attention());
        // A short burst afterwards (a redraw) ends as idle, not done.
        assert!(info.estimate(Some(at(20_000)), Some(at(0)), at(20_100), quiet));
        assert!(info.estimate(Some(at(20_500)), Some(at(0)), at(26_000), quiet));
        assert_eq!(info.agent.state(), Some(&AgentState::Idle));
        // Once hooks report the state, no more guesses.
        info.on_hook(&hook("UserPromptSubmit", "{}"));
        assert!(!info.estimate(Some(at(40_000)), None, at(40_100), quiet));
        assert!(!info.agent.estimated());
    }

    #[test]
    fn working_stretches_are_measured() {
        let mut info = SessionInfo::new();
        info.on_hook(&hook("UserPromptSubmit", "{}"));
        assert_eq!(info.working_interval(false), None, "a stretch opens");
        let (start, began) = info.working_since.unwrap();
        let start = start - 30;
        info.working_since = Some((start, began - std::time::Duration::from_secs(30)));
        assert_eq!(info.working_interval(false), None, "still working");
        info.on_hook(&hook("Stop", "{}"));
        let done = info.working_interval(false).unwrap();
        assert_eq!((done.agent.as_str(), done.start), ("claude-code", start));
        assert!(done.end - done.start >= 30);
        assert_eq!(info.working_interval(true), None, "nothing open");
    }

    #[test]
    fn each_attention_state_is_notified_once() {
        let mut info = SessionInfo::new();
        info.agent_name = Some("Claude Code".into());
        let hook = |event: &str, payload: &str| hv_ipc::HookEvent {
            session: Some("1".into()),
            agent: "claude-code".into(),
            event: event.into(),
            payload: payload.into(),
        };
        info.on_hook(&hook("UserPromptSubmit", "{}"));
        assert_eq!(info.take_notification("hivetty"), None);
        info.on_hook(&hook(
            "Notification",
            r#"{"message":"Bash(cargo test)","notification_type":"permission_prompt"}"#,
        ));
        let (kind, title, body) = info.take_notification("hivetty").unwrap();
        assert_eq!(kind, "waiting");
        assert_eq!(title, "Claude Code · hivetty");
        assert_eq!(body, "Claude Code: Needs permission: Bash(cargo test)");
        assert_eq!(info.take_notification("hivetty"), None, "only once");
        info.on_hook(&hook("Stop", "{}"));
        assert_eq!(info.take_notification("hivetty").unwrap().0, "done");
    }

    #[test]
    fn commands_are_timed_from_c_to_d() {
        let mut info = SessionInfo::new();
        let t0 = Instant::now();
        let mark = |kind, exit_code| TermEvent::PromptMark { kind, exit_code };
        assert_eq!(info.command_finished(&mark('C', None), t0), None);
        let done = info.command_finished(&mark('D', Some(2)), t0 + Duration::from_secs(40));
        assert_eq!(done, Some((Duration::from_secs(40), Some(2))));
        // A D without a C (an empty line) is not a command.
        assert_eq!(info.command_finished(&mark('D', Some(0)), t0), None);
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
