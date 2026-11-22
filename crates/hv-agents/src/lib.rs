//! Agent profiles, detection, state machine, hook installer, resume and worktrees.
//!
//! For now: the live state model (docs/agent-integration.md 2.1), the source priority rule
//! (2.2) and the interpreters for Claude Code hooks and OSC 7501.

pub mod claude;
pub mod codex;
pub mod gemini;
pub mod install;
pub mod profile;
pub mod statusline;
pub mod watch;

use std::time::{Duration, Instant};

/// Live state of an agent session (docs/agent-integration.md 2.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentState {
    /// Session started, nothing done yet.
    Starting,
    /// Working on a task. `progress` is 0-100 when the agent reports it.
    Working {
        /// Percent done, if known.
        progress: Option<u8>,
    },
    /// Needs the user. `kind` is `permission`, `question` or `auth` when known.
    Blocked {
        /// Why it is blocked.
        kind: Option<String>,
    },
    /// Waiting for the next prompt.
    Idle,
    /// Finished a task (stays "unread" until the user looks at it).
    Done,
    /// Failed.
    Error,
    /// The agent process ended.
    Exited,
}

/// Where a state update came from, from most to least reliable (docs/agent-integration.md 2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    /// OSC 7501 program status.
    Osc7501,
    /// The agent's hooks through `hivetty hook`.
    Hook,
    /// OSC 9/99/777 notifications and OSC 9;4 progress.
    Notify,
    /// Window title changes.
    Title,
    /// BEL.
    Bell,
    /// Output and process heuristics.
    Heuristic,
}

const SOURCES: usize = 6;

/// What a session is doing, as data: the UI words it (hv-ui `strings.rs`), this crate never
/// writes UI text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activity {
    /// The agent got a prompt and is thinking.
    Thinking,
    /// Running a shell command (the command).
    Running(String),
    /// Editing a file (short path).
    Editing(String),
    /// Writing a new file (short path).
    Writing(String),
    /// Reading a file (short path).
    Reading(String),
    /// Searching files for a pattern.
    Searching(String),
    /// Fetching a URL.
    Fetching(String),
    /// Searching the web for a query.
    SearchingWeb(String),
    /// Running a sub-agent (its description).
    SubAgent(String),
    /// Using some other tool (its name).
    Using(String),
}

/// The second line of a session row: the agent's own words, or an activity to word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Subtitle {
    /// Text that came from the agent (a command description, a notification message).
    Text(String),
    /// Something it is doing.
    Activity(Activity),
}

/// One interpreted event.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Update {
    /// New state, if the event implies one.
    pub state: Option<AgentState>,
    /// New subtitle (current action or reason), if any.
    pub subtitle: Option<Subtitle>,
    /// The agent's own session id (Claude Code `session_id`).
    pub agent_session: Option<String>,
}

/// How long a higher-priority source keeps lower ones from changing the state.
pub const SOURCE_HOLD: Duration = Duration::from_secs(30);

/// Combines updates from several sources into one state (docs/agent-integration.md 2.2):
/// a source wins while no higher-priority source has spoken in the last [`SOURCE_HOLD`].
#[derive(Debug, Clone, Default)]
pub struct Tracker {
    state: Option<AgentState>,
    subtitle: Option<Subtitle>,
    agent_session: Option<String>,
    last_seen: [Option<Instant>; SOURCES],
    /// When the current state was set.
    since: Option<Instant>,
    /// The source that set the current state.
    source: Option<Source>,
}

impl Tracker {
    /// Applies an update. Returns true if what the UI shows changed.
    pub fn apply(&mut self, source: Source, update: Update, now: Instant) -> bool {
        let rank = source as usize;
        self.last_seen[rank] = Some(now);
        let outranked = self.last_seen[..rank]
            .iter()
            .flatten()
            .any(|t| now.saturating_duration_since(*t) < SOURCE_HOLD);
        if update.agent_session.is_some() {
            self.agent_session = update.agent_session;
        }
        let mut changed = false;
        // Subtitles are extra detail: any source may fill them unless it is outranked.
        if let Some(subtitle) = update.subtitle
            && !outranked
            && self.subtitle.as_ref() != Some(&subtitle)
        {
            self.subtitle = Some(subtitle);
            changed = true;
        }
        if let Some(state) = update.state
            && !outranked
        {
            if self.state.as_ref() != Some(&state) {
                // A new state without its own subtitle drops the old one, except the current
                // action while still working.
                if !matches!(state, AgentState::Working { .. }) && !changed {
                    self.subtitle = None;
                }
                self.state = Some(state);
                self.since = Some(now);
                changed = true;
            }
            // The same state confirmed by another source: a guess may have become a fact.
            if self.source != Some(source) {
                let was_estimated = self.estimated();
                self.source = Some(source);
                changed |= was_estimated != self.estimated();
            }
        }
        changed
    }

    /// Current state, if any source reported one.
    pub fn state(&self) -> Option<&AgentState> {
        self.state.as_ref()
    }

    /// Current subtitle.
    pub fn subtitle(&self) -> Option<&Subtitle> {
        self.subtitle.as_ref()
    }

    /// When the current state started.
    pub fn since(&self) -> Option<Instant> {
        self.since
    }

    /// Whether the current state is a guess (from output activity, the title or a bell) rather
    /// than something the agent reported (A-19: shown with "≈").
    pub fn estimated(&self) -> bool {
        matches!(
            self.source,
            Some(Source::Heuristic | Source::Title | Source::Bell)
        )
    }

    /// Whether the agent ever reported its state itself (OSC 7501 or hooks). Then guesses are
    /// not needed for this session.
    pub fn reliable(&self) -> bool {
        self.last_seen[Source::Osc7501 as usize].is_some()
            || self.last_seen[Source::Hook as usize].is_some()
    }

    /// The agent's own session id, once known.
    pub fn agent_session(&self) -> Option<&str> {
        self.agent_session.as_deref()
    }
}

/// Interprets an OSC 7501 status (`state`, `kind`, `progress`, decoded `msg`).
/// `None` for the `?` query, which the terminal answers instead.
pub fn from_program_status(
    state: &str,
    kind: Option<&str>,
    progress: Option<u8>,
    message: Option<&str>,
) -> Option<Update> {
    let state = match state {
        "idle" => AgentState::Idle,
        "working" => AgentState::Working { progress },
        "blocked" => AgentState::Blocked {
            kind: kind.map(str::to_owned),
        },
        "done" => AgentState::Done,
        "error" => AgentState::Error,
        "clear" => AgentState::Idle,
        _ => return None,
    };
    Some(Update {
        state: Some(state),
        subtitle: message
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .map(|m| Subtitle::Text(m.to_owned())),
        agent_session: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guesses_are_marked_and_reliable_sources_are_remembered() {
        let t0 = Instant::now();
        let mut tracker = Tracker::default();
        assert!(tracker.apply(Source::Heuristic, working(), t0));
        assert!(tracker.estimated());
        assert!(!tracker.reliable());
        // The hook confirms the same state: no longer a guess.
        assert!(tracker.apply(Source::Hook, working(), t0));
        assert!(!tracker.estimated());
        assert!(tracker.reliable());
    }

    fn working() -> Update {
        Update {
            state: Some(AgentState::Working { progress: None }),
            ..Default::default()
        }
    }

    fn done() -> Update {
        Update {
            state: Some(AgentState::Done),
            ..Default::default()
        }
    }

    #[test]
    fn higher_priority_sources_hold_the_state_for_30_seconds() {
        let t0 = Instant::now();
        let mut tracker = Tracker::default();
        assert!(tracker.apply(Source::Osc7501, working(), t0));
        // A heuristic guess cannot override OSC 7501 right away…
        assert!(!tracker.apply(Source::Heuristic, done(), t0 + Duration::from_secs(5)));
        assert_eq!(
            tracker.state(),
            Some(&AgentState::Working { progress: None })
        );
        // …but can once OSC 7501 has been silent for longer than the hold.
        assert!(tracker.apply(Source::Heuristic, done(), t0 + Duration::from_secs(31)));
        assert_eq!(tracker.state(), Some(&AgentState::Done));
        // Lower sources never block higher ones.
        assert!(tracker.apply(Source::Hook, working(), t0 + Duration::from_secs(32)));
    }

    #[test]
    fn repeated_updates_report_no_change() {
        let t0 = Instant::now();
        let mut tracker = Tracker::default();
        assert!(tracker.apply(Source::Hook, working(), t0));
        assert!(!tracker.apply(Source::Hook, working(), t0));
    }

    #[test]
    fn program_status_maps_every_state() {
        let blocked =
            from_program_status("blocked", Some("permission"), None, Some(" Bash ")).unwrap();
        assert_eq!(
            blocked.state,
            Some(AgentState::Blocked {
                kind: Some("permission".into())
            })
        );
        assert_eq!(blocked.subtitle, Some(Subtitle::Text("Bash".into())));
        assert_eq!(
            from_program_status("working", None, Some(40), None)
                .unwrap()
                .state,
            Some(AgentState::Working { progress: Some(40) })
        );
        assert!(from_program_status("?", None, None, None).is_none());
        assert!(from_program_status("bogus", None, None, None).is_none());
    }
}
