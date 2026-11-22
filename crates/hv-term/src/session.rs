//! A terminal session: a PTY, its reader thread and the VT emulator state.

use std::fmt;
use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use alacritty_terminal::Term;
use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Boundary, Column, Direction, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::search::{Match, RegexSearch};
use alacritty_terminal::term::{Config, TermMode};
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
use anyhow::Result;
use hv_pty::{GridSize, Pty, SpawnSpec};

use crate::input::{KeyModes, MouseModes};
use crate::marks::Marks;
use crate::osc::{Interceptor, TermEvent};
use crate::snapshot::{self, Snapshot};

/// How a mouse selection grows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectKind {
    /// Character by character (single click and drag).
    Simple,
    /// Whole words (double click).
    Word,
    /// Whole lines (triple click).
    Line,
    /// Rectangular block (Alt + drag).
    Block,
}

/// Something the UI should react to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    /// The program set the window/tab title (OSC 0/2).
    Title(String),
    /// The program reset the title.
    ResetTitle,
    /// BEL.
    Bell,
    /// OSC 52: the program wants to put text on the clipboard (reading is never allowed).
    ClipboardStore(String),
    /// A sequence caught by the interceptor (cwd, prompt marks, notifications, OSC 7501…).
    Term(TermEvent),
    /// The child process exited (PTY closed).
    Exited,
}

/// Forwards emulator events to the reader thread.
#[derive(Clone)]
struct EventProxy(Sender<Event>);

impl EventListener for EventProxy {
    fn send_event(&self, event: Event) {
        let _ = self.0.send(event);
    }
}

struct Dims {
    cols: usize,
    rows: usize,
}

impl Dimensions for Dims {
    fn total_lines(&self) -> usize {
        self.rows
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

struct TermState {
    term: Term<EventProxy>,
    search: Option<SearchState>,
    processor: Processor<StdSyncHandler>,
    interceptor: Interceptor,
    term_events: Receiver<Event>,
    /// Shell prompt marks (OSC 133) by scrollback line.
    marks: Marks,
    /// When the program last wrote something (agent activity heuristic, F4-09).
    last_output: Option<std::time::Instant>,
}

/// A running terminal session.
pub struct Session {
    state: Arc<Mutex<TermState>>,
    recorder: Arc<Mutex<Option<crate::record::Recorder>>>,
    cwd: Arc<Mutex<Option<std::path::PathBuf>>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pty: Mutex<Pty>,
    events: Receiver<SessionEvent>,
    /// When hivetty last sent the program input or resized it (its output right after that is
    /// an echo or a redraw, not work).
    last_input: Mutex<Option<std::time::Instant>>,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session").finish_non_exhaustive()
    }
}

/// Largest OSC 52 clipboard write accepted.
const MAX_OSC52_BYTES: usize = 1024 * 1024;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Session {
    /// Spawns `spec` and starts reading its output. `wake` is called (from the reader thread)
    /// whenever the screen changed or an event is ready, so the UI can schedule a redraw.
    pub fn spawn(
        spec: &SpawnSpec,
        size: GridSize,
        scrollback_lines: usize,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self> {
        let (mut pty, io) = Pty::spawn(spec, size)?;
        let waiter = pty.take_waiter();
        let (term_tx, term_rx) = mpsc::channel();
        let dims = Dims {
            cols: usize::from(size.cols.max(1)),
            rows: usize::from(size.rows.max(1)),
        };
        let config = Config {
            scrolling_history: scrollback_lines,
            // Track the kitty keyboard protocol stack (CSI > u / < u / ? u); encoding is ours.
            kitty_keyboard: true,
            ..Config::default()
        };
        let term = Term::new(config, &dims, EventProxy(term_tx));
        let state = Arc::new(Mutex::new(TermState {
            term,
            processor: Processor::new(),
            search: None,
            interceptor: Interceptor::new(),
            marks: Marks::default(),
            term_events: term_rx,
            last_output: None,
        }));
        let writer = Arc::new(Mutex::new(io.writer));
        let (events_tx, events_rx) = mpsc::channel();

        let cwd = Arc::new(Mutex::new(spec.cwd.clone()));
        let thread_cwd = Arc::clone(&cwd);
        // Exactly one `Exited` event, from whichever notices first: EOF on the PTY output
        // (Unix) or the child process exiting (needed on Windows, where ConPTY keeps the pipe open).
        let exited = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let wake = Arc::new(wake);
        if let Some(waiter) = waiter {
            let events = events_tx.clone();
            let exited = Arc::clone(&exited);
            let wake = Arc::clone(&wake);
            std::thread::Builder::new()
                .name("hivetty-child-wait".into())
                .spawn(move || {
                    waiter.wait();
                    // Give the reader a moment to deliver the last output first.
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    if !exited.swap(true, std::sync::atomic::Ordering::AcqRel) {
                        let _ = events.send(SessionEvent::Exited);
                        wake();
                    }
                })?;
        }
        let recorder = Arc::new(Mutex::new(None));
        let thread_recorder = Arc::clone(&recorder);
        let thread_exited = Arc::clone(&exited);
        let thread_state = Arc::clone(&state);
        let thread_writer = Arc::clone(&writer);
        std::thread::Builder::new()
            .name("hivetty-pty-reader".into())
            .spawn(move || {
                reader_loop(
                    io.reader,
                    &thread_state,
                    &thread_writer,
                    &thread_cwd,
                    &events_tx,
                    &thread_exited,
                    &thread_recorder,
                    &*wake,
                )
            })?;

        Ok(Self {
            recorder,
            state,
            cwd,
            writer,
            pty: Mutex::new(pty),
            events: events_rx,
            last_input: Mutex::new(None),
        })
    }

    /// Sends bytes to the program (keystrokes, paste).
    pub fn write(&self, bytes: &[u8]) {
        *lock(&self.last_input) = Some(std::time::Instant::now());
        let mut writer = lock(&self.writer);
        let _ = writer.write_all(bytes).and_then(|()| writer.flush());
    }

    /// Starts a selection at a visible cell. `right_half` is true when the pointer is over the
    /// right half of the cell (so the selection starts after that character).
    pub fn select_start(&self, row: usize, col: usize, right_half: bool, kind: SelectKind) {
        let mut state = lock(&self.state);
        let point = visible_point(&state.term, row, col);
        let ty = match kind {
            SelectKind::Simple => SelectionType::Simple,
            SelectKind::Word => SelectionType::Semantic,
            SelectKind::Line => SelectionType::Lines,
            SelectKind::Block => SelectionType::Block,
        };
        state.term.selection = Some(Selection::new(ty, point, side(right_half)));
    }

    /// Extends the current selection to a visible cell.
    pub fn select_update(&self, row: usize, col: usize, right_half: bool) {
        let mut state = lock(&self.state);
        let point = visible_point(&state.term, row, col);
        if let Some(selection) = state.term.selection.as_mut() {
            selection.update(point, side(right_half));
        }
    }

    /// Clears the selection.
    pub fn select_clear(&self) {
        lock(&self.state).term.selection = None;
    }

    /// The selected text, if any.
    pub fn selection_text(&self) -> Option<String> {
        let state = lock(&self.state);
        state.term.selection_to_string().filter(|s| !s.is_empty())
    }

    /// Last working directory reported by the shell (OSC 7), or the one it started in.
    pub fn cwd(&self) -> Option<std::path::PathBuf> {
        lock(&self.cwd).clone()
    }

    /// Starts (or changes) a scrollback search. The nearest match above the bottom of the
    /// screen is selected and scrolled into view. Returns `Ok(false)` when nothing matches.
    pub fn search(&self, pattern: &str) -> Result<bool, String> {
        let mut state = lock(&self.state);
        if pattern.is_empty() {
            state.search = None;
            state.term.selection = None;
            return Ok(true);
        }
        let regex = RegexSearch::new(pattern).map_err(|e| e.to_string())?;
        let origin = state.term.grid().bottommost_line();
        let origin = Point::new(origin, state.term.last_column());
        let mut search = SearchState {
            regex,
            current: None,
        };
        let found = state.term.search_next(
            &mut search.regex,
            origin,
            Direction::Left,
            Side::Right,
            None,
        );
        state.search = Some(search);
        Ok(show_match(&mut state, found))
    }

    /// Moves to the next match: older (`up = true`) or newer. Returns false when nothing matches.
    pub fn search_step(&self, up: bool) -> bool {
        let mut state = lock(&self.state);
        let TermState { term, search, .. } = &mut *state;
        let Some(search) = search.as_mut() else {
            return false;
        };
        let (origin, direction) = match &search.current {
            Some(m) if up => (m.start().sub(term, Boundary::None, 1), Direction::Left),
            Some(m) => (m.end().add(term, Boundary::None, 1), Direction::Right),
            None => (
                Point::new(term.grid().bottommost_line(), term.last_column()),
                Direction::Left,
            ),
        };
        let side = if up { Side::Right } else { Side::Left };
        let found = term.search_next(&mut search.regex, origin, direction, side, None);
        show_match(&mut state, found)
    }

    /// Ends the search and removes its highlight.
    pub fn search_clear(&self) {
        let mut state = lock(&self.state);
        if state.search.take().is_some() {
            state.term.selection = None;
        }
    }

    /// Mouse reporting modes the program enabled.
    pub fn mouse_modes(&self) -> MouseModes {
        let state = lock(&self.state);
        let mode = state.term.mode();
        MouseModes {
            click: mode.contains(TermMode::MOUSE_REPORT_CLICK),
            drag: mode.contains(TermMode::MOUSE_DRAG),
            motion: mode.contains(TermMode::MOUSE_MOTION),
            sgr: mode.contains(TermMode::SGR_MOUSE),
        }
    }

    /// Whether the program asked for mouse events (then clicks go to it, not to selection).
    pub fn mouse_mode(&self) -> bool {
        lock(&self.state)
            .term
            .mode()
            .intersects(TermMode::MOUSE_MODE)
    }

    /// Starts recording all output of this session to an asciicast v2 file.
    pub fn start_recording(&self, path: &std::path::Path, size: GridSize) -> Result<()> {
        let rec = crate::record::Recorder::create(path, size.cols, size.rows)?;
        *lock(&self.recorder) = Some(rec);
        Ok(())
    }

    /// Pastes text. Escape characters are removed so pasted text can never end bracketed-paste
    /// mode early and run commands (docs/security.md 2.2).
    pub fn paste(&self, text: &str) {
        let clean: String = text.chars().filter(|&c| c != '\x1b').collect();
        let bracketed = lock(&self.state)
            .term
            .mode()
            .contains(TermMode::BRACKETED_PASTE);
        if bracketed {
            self.write(format!("\x1b[200~{clean}\x1b[201~").as_bytes());
        } else {
            // Without bracketed paste, line breaks are sent as CR, like typing Enter.
            self.write(clean.replace("\r\n", "\r").replace('\n', "\r").as_bytes());
        }
        self.scroll_to_bottom();
    }

    /// Resizes the emulator and the PTY.
    pub fn resize(&self, size: GridSize) {
        // A resize makes full-screen programs redraw: not activity of their own.
        *lock(&self.last_input) = Some(std::time::Instant::now());
        {
            let mut state = lock(&self.state);
            let dims = Dims {
                cols: usize::from(size.cols.max(1)),
                rows: usize::from(size.rows.max(1)),
            };
            state.term.resize(dims);
        }
        if let Some(rec) = lock(&self.recorder).as_mut() {
            rec.resize(size.cols, size.rows);
        }
        if let Err(err) = lock(&self.pty).resize(size) {
            log::warn!("PTY resize failed: {err:#}");
        }
    }

    /// Copies the visible screen.
    pub fn snapshot(&self) -> Snapshot {
        let state = lock(&self.state);
        let mut snap = snapshot::take(&state.term);
        if !state.term.mode().contains(TermMode::ALT_SCREEN) {
            let top = state
                .term
                .grid()
                .history_size()
                .saturating_sub(snap.display_offset);
            snap.marks = state.marks.visible(top, snap.rows);
        }
        snap
    }

    /// Process id of the program started in the PTY (the shell), if known.
    pub fn pid(&self) -> Option<u32> {
        lock(&self.pty).pid()
    }

    /// Scrollback line of the newest shell prompt (OSC 133;A), if the shell reports them.
    pub fn last_prompt_line(&self) -> Option<usize> {
        lock(&self.state).marks.last_prompt()
    }

    /// Scrolls so the previous (`up`) or next shell prompt is at the top of the view.
    /// Returns false when there is none in that direction.
    pub fn jump_to_prompt(&self, up: bool) -> bool {
        let mut state = lock(&self.state);
        let history = state.term.grid().history_size();
        let offset = state.term.grid().display_offset();
        let top = history.saturating_sub(offset);
        let Some(target) = state.marks.prompt_near(top, up) else {
            if !up {
                state.term.scroll_display(Scroll::Bottom);
            }
            return false;
        };
        let new_offset = history.saturating_sub(target).min(history);
        let delta = i32::try_from(new_offset).unwrap_or(i32::MAX)
            - i32::try_from(offset).unwrap_or(i32::MAX);
        state.term.scroll_display(Scroll::Delta(delta));
        true
    }

    /// Text printed by the last command (between its OSC 133 C and D marks), if the shell
    /// reports them.
    pub fn last_command_output(&self) -> Option<String> {
        let state = lock(&self.state);
        let grid = state.term.grid();
        let history = grid.history_size();
        let end = history + grid.screen_lines();
        let (start, stop) = state.marks.last_output(end)?;
        let stop = stop.min(end);
        if stop <= start {
            return None;
        }
        let to_line = |abs: usize| {
            Line(i32::try_from(abs).unwrap_or(i32::MAX) - i32::try_from(history).unwrap_or(0))
        };
        let first = Point::new(to_line(start), Column(0));
        let last = Point::new(to_line(stop - 1), state.term.last_column());
        let text = state.term.bounds_to_string(first, last);
        let text = text.trim_end().to_owned();
        (!text.is_empty()).then_some(text)
    }

    /// When the program last wrote output, and when hivetty last sent it input or resized it.
    pub fn activity(&self) -> (Option<std::time::Instant>, Option<std::time::Instant>) {
        (lock(&self.state).last_output, *lock(&self.last_input))
    }

    /// Modes that affect key encoding.
    pub fn key_modes(&self) -> KeyModes {
        let state = lock(&self.state);
        let mode = state.term.mode();
        KeyModes {
            app_cursor: mode.contains(TermMode::APP_CURSOR),
            kitty_disambiguate: mode.contains(TermMode::DISAMBIGUATE_ESC_CODES),
            kitty_all_keys: mode.contains(TermMode::REPORT_ALL_KEYS_AS_ESC),
        }
    }

    /// Whether the program enabled the alternate screen (full-screen TUIs).
    pub fn alt_screen(&self) -> bool {
        lock(&self.state).term.mode().contains(TermMode::ALT_SCREEN)
    }

    /// Scrolls the view by `lines` (positive = up into the scrollback).
    pub fn scroll(&self, lines: i32) {
        lock(&self.state).term.scroll_display(Scroll::Delta(lines));
    }

    /// Jumps back to the bottom of the scrollback.
    pub fn scroll_to_bottom(&self) {
        lock(&self.state).term.scroll_display(Scroll::Bottom);
    }

    /// Deadline of a pending synchronized update (DECSET 2026), if any.
    pub fn sync_deadline(&self) -> Option<Instant> {
        lock(&self.state).processor.sync_timeout().sync_timeout()
    }

    /// Ends a synchronized update whose deadline has passed. Returns true if it did.
    pub fn flush_expired_sync(&self) -> bool {
        let mut state = lock(&self.state);
        let TermState {
            term, processor, ..
        } = &mut *state;
        match processor.sync_timeout().sync_timeout() {
            Some(deadline) if deadline <= Instant::now() => {
                processor.stop_sync(term);
                true
            }
            _ => false,
        }
    }

    /// Takes the events that arrived since the last call.
    pub fn drain_events(&self) -> Vec<SessionEvent> {
        self.events.try_iter().collect()
    }

    /// Kills the program.
    pub fn kill(&self) {
        let _ = lock(&self.pty).kill();
    }
}

struct SearchState {
    regex: RegexSearch,
    current: Option<Match>,
}

/// Highlights a search match (as the selection, so it can be copied) and scrolls to it.
fn show_match(state: &mut TermState, found: Option<Match>) -> bool {
    let Some(m) = found else { return false };
    let mut selection = Selection::new(SelectionType::Simple, *m.start(), Side::Left);
    selection.update(*m.end(), Side::Right);
    state.term.selection = Some(selection);
    state.term.scroll_to_point(*m.start());
    if let Some(search) = state.search.as_mut() {
        search.current = Some(m);
    }
    true
}

/// Grid point of a visible cell, taking the scrollback offset into account.
fn visible_point(term: &Term<EventProxy>, row: usize, col: usize) -> Point {
    let offset = term.grid().display_offset();
    let line = i32::try_from(row).unwrap_or(i32::MAX) - i32::try_from(offset).unwrap_or(0);
    let col = col.min(term.columns().saturating_sub(1));
    Point::new(Line(line), Column(col))
}

fn side(right_half: bool) -> Side {
    if right_half { Side::Right } else { Side::Left }
}

/// Feeds one chunk of PTY output to the emulator, recording prompt marks on the line the
/// cursor is on when each mark arrives, and collecting the intercepted events.
fn feed(
    term: &mut Term<EventProxy>,
    processor: &mut Processor<StdSyncHandler>,
    interceptor: &mut crate::osc::Interceptor,
    marks: &mut crate::marks::Marks,
    bytes: &[u8],
    out: &mut Vec<SessionEvent>,
) {
    let history_before = term.grid().history_size();
    let alt_before = term.mode().contains(TermMode::ALT_SCREEN);
    // Feed the emulator up to each intercepted sequence, so a prompt mark is recorded
    // on the line the cursor is on when it arrives.
    let mut fed = 0;
    for (end, event) in interceptor.feed_split(bytes) {
        processor.advance(term, &bytes[fed..end]);
        fed = end;
        if let TermEvent::PromptMark { kind, exit_code } = &event
            && !term.mode().contains(TermMode::ALT_SCREEN)
        {
            let row = usize::try_from(term.grid().cursor.point.line.0).unwrap_or(0);
            marks.record(*kind, *exit_code, term.grid().history_size() + row);
        }
        out.push(SessionEvent::Term(event));
    }
    processor.advance(term, &bytes[fed..]);
    // The scrollback was cleared (`clear`, CSI 3 J): the marks point nowhere now. The
    // alternate screen has no scrollback of its own, so entering or leaving it (less,
    // vim…) is not a clear.
    let alt_after = term.mode().contains(TermMode::ALT_SCREEN);
    if !alt_before && !alt_after && term.grid().history_size() < history_before {
        marks.clear();
    }
}

// Each argument is one piece of state shared with the UI thread; bundling them would only move
// the list into a struct used in this one place.
#[allow(clippy::too_many_arguments)]
fn reader_loop(
    mut reader: Box<dyn Read + Send>,
    state: &Mutex<TermState>,
    writer: &Mutex<Box<dyn Write + Send>>,
    cwd: &Mutex<Option<std::path::PathBuf>>,
    events: &Sender<SessionEvent>,
    exited: &std::sync::atomic::AtomicBool,
    recorder: &Mutex<Option<crate::record::Recorder>>,
    wake: &(impl Fn() + Send),
) {
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let bytes = &buf[..n];
        if let Some(rec) = lock(recorder).as_mut() {
            rec.output(bytes);
        }
        let mut out = Vec::new();
        let sync_pending = {
            let mut guard = lock(state);
            let TermState {
                term,
                processor,
                interceptor,
                term_events,
                marks,
                last_output,
                ..
            } = &mut *guard;
            feed(term, processor, interceptor, marks, bytes, &mut out);
            *last_output = Some(std::time::Instant::now());
            for event in term_events.try_iter() {
                match event {
                    // Answers to queries (DA, DSR/CSI 6n, colors…). ConPTY blocks until it gets
                    // the cursor position report, so this must go out right away.
                    Event::PtyWrite(text) => {
                        let mut w = lock(writer);
                        let _ = w.write_all(text.as_bytes()).and_then(|()| w.flush());
                    }
                    Event::Title(title) => out.push(SessionEvent::Title(title)),
                    Event::ResetTitle => out.push(SessionEvent::ResetTitle),
                    Event::Bell => out.push(SessionEvent::Bell),
                    // OSC 52 write, capped so a program cannot flood the clipboard
                    // (docs/security.md 2.1). `ClipboardLoad` (reading) is ignored on purpose.
                    Event::ClipboardStore(_, text) if text.len() <= MAX_OSC52_BYTES => {
                        out.push(SessionEvent::ClipboardStore(text));
                    }
                    _ => {}
                }
            }
            processor.sync_timeout().sync_timeout().is_some()
        };
        for event in &out {
            if let SessionEvent::Term(TermEvent::Cwd(dir)) = event {
                *lock(cwd) = Some(std::path::PathBuf::from(dir));
            }
        }
        for event in out {
            let _ = events.send(event);
        }
        // While a synchronized update is open the screen is mid-frame: don't redraw yet.
        // The UI flushes it when the deadline passes (see `flush_expired_sync`).
        if !sync_pending {
            wake();
        }
    }
    if !exited.swap(true, std::sync::atomic::Ordering::AcqRel) {
        let _ = events.send(SessionEvent::Exited);
        wake();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Feeds bytes straight into an emulator (no PTY) and returns the snapshot.
    fn render(bytes: &[u8], cols: usize, rows: usize) -> Snapshot {
        let (tx, _rx) = mpsc::channel();
        let mut term = Term::new(Config::default(), &Dims { cols, rows }, EventProxy(tx));
        let mut processor: Processor<StdSyncHandler> = Processor::new();
        processor.advance(&mut term, bytes);
        snapshot::take(&term)
    }

    #[test]
    fn full_screen_programs_keep_prompt_marks_but_clear_drops_them() {
        let (tx, _rx) = mpsc::channel();
        let mut term = Term::new(
            Config::default(),
            &Dims { cols: 20, rows: 4 },
            EventProxy(tx),
        );
        let mut processor: Processor<StdSyncHandler> = Processor::new();
        let mut interceptor = crate::osc::Interceptor::default();
        let mut marks = crate::marks::Marks::default();
        let mut out = Vec::new();
        let mut run = |bytes: &[u8], marks: &mut crate::marks::Marks| {
            feed(
                &mut term,
                &mut processor,
                &mut interceptor,
                marks,
                bytes,
                &mut out,
            );
        };
        // Enough lines to have scrollback, then a prompt.
        run(
            b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n\x1b]133;A\x07$ ",
            &mut marks,
        );
        assert!(marks.last_prompt().is_some());
        // less / vim: enter and leave the alternate screen.
        run(b"\x1b[?1049hfull screen\x1b[?1049l", &mut marks);
        assert!(
            marks.last_prompt().is_some(),
            "the alternate screen is not a clear"
        );
        // `clear` with CSI 3 J empties the scrollback.
        run(b"\x1b[H\x1b[2J\x1b[3J", &mut marks);
        assert_eq!(marks.last_prompt(), None);
    }

    #[test]
    fn plain_text_and_newlines() {
        let snap = render(b"hello\r\nworld", 20, 4);
        assert_eq!(snap.row_text(0), "hello");
        assert_eq!(snap.row_text(1), "world");
        assert_eq!(snap.cursor.map(|c| (c.row, c.col)), Some((1, 5)));
    }

    #[test]
    fn sgr_colors_are_resolved() {
        let snap = render(b"\x1b[31mR\x1b[38;2;1;2;3mT\x1b[7mI", 10, 2);
        assert_eq!(snap.cell(0, 0).fg, crate::snapshot::palette::indexed(1));
        assert_eq!(snap.cell(0, 1).fg, Rgb8Ext::rgb(1, 2, 3));
        // Inverse swaps fg and bg.
        assert_eq!(snap.cell(0, 2).bg, Rgb8Ext::rgb(1, 2, 3));
    }

    #[test]
    fn selection_returns_text_and_marks_cells() {
        let (tx, _rx) = mpsc::channel();
        let mut term = Term::new(
            Config::default(),
            &Dims { cols: 20, rows: 2 },
            EventProxy(tx),
        );
        let mut processor: Processor<StdSyncHandler> = Processor::new();
        processor.advance(&mut term, b"hello world");
        let start = visible_point(&term, 0, 6);
        let mut selection = Selection::new(SelectionType::Simple, start, Side::Left);
        selection.update(visible_point(&term, 0, 10), Side::Right);
        term.selection = Some(selection);
        assert_eq!(term.selection_to_string().as_deref(), Some("world"));
        let snap = snapshot::take(&term);
        assert!(snap.cell(0, 6).selected && snap.cell(0, 10).selected);
        assert!(!snap.cell(0, 5).selected);
    }

    #[test]
    fn osc52_write_reaches_the_event_channel() {
        let (tx, rx) = mpsc::channel();
        let mut term = Term::new(
            Config::default(),
            &Dims { cols: 10, rows: 2 },
            EventProxy(tx),
        );
        let mut processor: Processor<StdSyncHandler> = Processor::new();
        processor.advance(&mut term, b"\x1b]52;c;aGVsbG8=\x07");
        let stored = rx.try_iter().find_map(|e| match e {
            Event::ClipboardStore(_, text) => Some(text),
            _ => None,
        });
        assert_eq!(stored.as_deref(), Some("hello"));
    }

    #[test]
    fn search_finds_matches_newest_first_and_steps() {
        let size = GridSize {
            cols: 20,
            rows: 3,
            cell_width: 8,
            cell_height: 16,
        };
        let (program, args) = if cfg!(windows) {
            (
                "cmd.exe".to_owned(),
                vec![
                    "/C".to_owned(),
                    "echo alpha& echo beta& echo alpha2& echo gamma& echo delta".to_owned(),
                ],
            )
        } else {
            (
                "/bin/sh".to_owned(),
                vec![
                    "-c".to_owned(),
                    "printf 'alpha\nbeta\nalpha2\ngamma\ndelta\n'".to_owned(),
                ],
            )
        };
        let spec = SpawnSpec {
            program: Some(program),
            args,
            ..SpawnSpec::default()
        };
        let session = Session::spawn(&spec, size, 1000, || {}).expect("spawn");
        let deadline = Instant::now() + Duration::from_secs(20);
        while !session.drain_events().contains(&SessionEvent::Exited) {
            assert!(Instant::now() < deadline, "command did not finish");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(session.search("alpha"), Ok(true));
        assert_eq!(
            session.selection_text().as_deref(),
            Some("alpha"),
            "newest match first"
        );
        assert!(session.search_step(true));
        assert!(session.selection_text().is_some());
        assert_eq!(session.search("zzz-not-there"), Ok(false));
        assert!(session.search("(").is_err(), "invalid regex is reported");
        session.search_clear();
        assert!(session.selection_text().is_none());
    }

    /// Throughput benchmark (F1-28). Run with `cargo xtask bench`.
    #[test]
    #[ignore = "benchmark; run with cargo xtask bench"]
    fn bench_throughput_and_snapshot() {
        // ~64 MB of realistic output: colored log lines, unicode and some OSC sequences.
        let mut chunk = Vec::new();
        for i in 0..2000 {
            let line = format!(
                "\x1b[32m[ok]\x1b[0m line {i:05} compiling crate \x1b[1;34mhv-term\x1b[0m ñ 日本 ✓ {}\r\n",
                "x".repeat(i % 60)
            );
            chunk.extend_from_slice(line.as_bytes());
            if i % 500 == 0 {
                chunk.extend_from_slice(b"\x1b]133;A\x07\x1b]7;file://h/tmp\x07");
            }
        }
        let repeats = (64 * 1024 * 1024) / chunk.len();
        let (tx, _rx) = mpsc::channel();
        let config = Config {
            scrolling_history: 10_000,
            ..Config::default()
        };
        let mut term = Term::new(
            config,
            &Dims {
                cols: 200,
                rows: 60,
            },
            EventProxy(tx),
        );
        let mut processor: Processor<StdSyncHandler> = Processor::new();
        let mut interceptor = Interceptor::new();

        let start = Instant::now();
        let mut events = 0;
        for _ in 0..repeats {
            for piece in chunk.chunks(64 * 1024) {
                events += interceptor.feed(piece).len();
                processor.advance(&mut term, piece);
            }
        }
        let secs = start.elapsed().as_secs_f64();
        let mb = (repeats * chunk.len()) as f64 / (1024.0 * 1024.0);
        println!(
            "BENCH throughput: {:.0} MB/s ({mb:.0} MB in {secs:.2} s, {events} OSC events)",
            mb / secs
        );

        // Baseline: the emulator alone (what Alacritty itself does), to see the interceptor cost.
        let (tx, _rx) = mpsc::channel();
        let mut bare = Term::new(
            Config::default(),
            &Dims {
                cols: 200,
                rows: 60,
            },
            EventProxy(tx),
        );
        let mut bare_processor: Processor<StdSyncHandler> = Processor::new();
        let start = Instant::now();
        for _ in 0..repeats {
            for piece in chunk.chunks(64 * 1024) {
                bare_processor.advance(&mut bare, piece);
            }
        }
        let bare_secs = start.elapsed().as_secs_f64();
        println!(
            "BENCH emulator only: {:.0} MB/s -> hivetty is {:.0}% of the bare emulator",
            mb / bare_secs,
            bare_secs / secs * 100.0
        );

        let start = Instant::now();
        let n = 200;
        for _ in 0..n {
            std::hint::black_box(snapshot::take(&term));
        }
        let per = start.elapsed().as_secs_f64() * 1000.0 / f64::from(n);
        println!("BENCH snapshot 200x60: {per:.3} ms");
        assert!(events > 0);
    }

    #[test]
    fn wide_characters_take_two_cells() {
        let snap = render("日本".as_bytes(), 10, 1);
        assert!(snap.cell(0, 1).wide_spacer);
        assert_eq!(snap.row_text(0), "日本");
    }

    #[test]
    fn synchronized_output_defers_rendering() {
        let (tx, _rx) = mpsc::channel();
        let mut term = Term::new(
            Config::default(),
            &Dims { cols: 10, rows: 2 },
            EventProxy(tx),
        );
        let mut processor: Processor<StdSyncHandler> = Processor::new();
        processor.advance(&mut term, b"\x1b[?2026hframe");
        assert!(processor.sync_timeout().sync_timeout().is_some());
        assert_eq!(
            snapshot::take(&term).row_text(0),
            "",
            "content must not show mid-frame"
        );
        processor.advance(&mut term, b"\x1b[?2026l");
        assert!(processor.sync_timeout().sync_timeout().is_none());
        assert_eq!(snapshot::take(&term).row_text(0), "frame");
    }

    #[test]
    fn session_runs_a_command_end_to_end() {
        let (program, args) = if cfg!(windows) {
            (
                "cmd.exe".to_owned(),
                vec!["/C".to_owned(), "echo hivetty-session-ok".to_owned()],
            )
        } else {
            (
                "/bin/sh".to_owned(),
                vec!["-c".to_owned(), "echo hivetty-session-ok".to_owned()],
            )
        };
        let spec = SpawnSpec {
            program: Some(program),
            args,
            ..SpawnSpec::default()
        };
        let size = GridSize {
            cols: 80,
            rows: 10,
            cell_width: 8,
            cell_height: 16,
        };
        let session = Session::spawn(&spec, size, 1000, || {}).expect("spawn");

        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let snap = session.snapshot();
            if (0..snap.rows).any(|r| snap.row_text(r).contains("hivetty-session-ok")) {
                break;
            }
            assert!(Instant::now() < deadline, "output never appeared on screen");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Small helper so tests can build colors tersely.
    struct Rgb8Ext;
    impl Rgb8Ext {
        fn rgb(r: u8, g: u8, b: u8) -> crate::snapshot::Rgb8 {
            crate::snapshot::Rgb8 { r, g, b }
        }
    }
}
