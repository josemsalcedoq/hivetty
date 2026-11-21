//! Builds what the chrome shows from the session state (shared by the window and `--screenshot`).

use std::path::Path;
use std::time::Instant;

use hv_ui::{RowState, SessionRow, format_elapsed, strings};

/// What the app tracks about the (single, for now) session.
#[derive(Debug, Clone)]
pub(crate) struct SessionInfo {
    /// Last title the program set (OSC 0/2), if any.
    pub(crate) title: Option<String>,
    /// When the session started.
    pub(crate) started: Instant,
    /// Whether the program has exited.
    pub(crate) exited: bool,
}

impl SessionInfo {
    pub(crate) fn new() -> Self {
        Self {
            title: None,
            started: Instant::now(),
            exited: false,
        }
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
    let (state, subtitle) = if info.exited {
        (RowState::Done, strings::STATE_EXITED)
    } else {
        (RowState::Working, strings::STATE_RUNNING)
    };
    SessionRow {
        title,
        subtitle: subtitle.to_owned(),
        project,
        elapsed: format_elapsed(info.started.elapsed().as_secs()),
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
}
