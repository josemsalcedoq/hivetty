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
