//! SP-1b: libghostty-vt vs alacritty_terminal 0.26, same workload, same binary.
//!
//! Run: `cargo run --release` (needs `zig` 0.17 on PATH for the vendored build).
//! Optional arg: megabytes to feed (default 64, like `cargo xtask bench`).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Instant;

use alacritty_terminal::Term;
use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::Config;
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
use hv_grid::{Cursor, CursorKind, Rgb8, SnapCell, Snapshot};
use libghostty_vt::Terminal;
use libghostty_vt::effect::UnknownSequence;
use libghostty_vt::render::{CellIterator, RenderState, RowIterator};
use libghostty_vt::screen::{CellContentTag, CellWide, cell};
use libghostty_vt::style::Underline;

// hv-term's own snapshot code and OSC interceptor, compiled in unchanged.
#[allow(dead_code, unreachable_pub)]
#[path = "../../../crates/hv-term/src/osc.rs"]
mod osc;
#[allow(dead_code, unreachable_pub)]
#[path = "../../../crates/hv-term/src/snapshot.rs"]
mod snapshot;

const COLS: u16 = 200;
const ROWS: u16 = 60;

#[derive(Clone)]
struct Proxy(mpsc::Sender<Event>);
impl EventListener for Proxy {
    fn send_event(&self, event: Event) {
        let _ = self.0.send(event);
    }
}

struct Dims;
impl Dimensions for Dims {
    fn total_lines(&self) -> usize {
        ROWS.into()
    }
    fn screen_lines(&self) -> usize {
        ROWS.into()
    }
    fn columns(&self) -> usize {
        COLS.into()
    }
}

/// Same generator as `hv-term`'s `bench_throughput_and_snapshot`.
fn workload() -> Vec<u8> {
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
    chunk
}

/// A heavier mix: 256/truecolor SGR, cursor movement, erase, scroll regions, wide chars.
fn workload_heavy() -> Vec<u8> {
    let mut chunk = Vec::new();
    for i in 0..2000u32 {
        let r = (i * 7 % 256) as u8;
        let g = (i * 13 % 256) as u8;
        let line = format!(
            "\x1b[38;5;{}m{:>6}\x1b[48;2;{r};{g};40m bg \x1b[0m\x1b[3;4m italic-ul \x1b[0m\x1b[7mrev\x1b[27m 表示テスト🙂 \x1b[K{}\r\n",
            i % 256,
            i,
            "-".repeat((i % 80) as usize)
        );
        chunk.extend_from_slice(line.as_bytes());
        if i % 100 == 0 {
            // Cursor addressing, partial erase, scroll region dance (like a TUI redraw).
            chunk.extend_from_slice(b"\x1b[s\x1b[5;20r\x1b[10;1H\x1b[2K\x1b[1Mstatus\x1b[r\x1b[u");
            chunk.extend_from_slice(b"\x1b]8;;https://example.com\x07link\x1b]8;;\x07\r\n");
        }
    }
    chunk
}

fn ghostty_terminal<'a>(events: Rc<RefCell<usize>>) -> Terminal<'a, 'a> {
    let mut t = Terminal::new(COLS, ROWS).expect("ghostty terminal");
    t.set_scrollback_max_lines(Some(10_000))
        .expect("scrollback");
    // The callbacks hv-term would need (replacing our interceptor).
    let e1 = Rc::clone(&events);
    let e2 = Rc::clone(&events);
    let e3 = Rc::clone(&events);
    t.on_pwd_changed(move |_t| *e1.borrow_mut() += 1)
        .expect("pwd cb")
        .on_semantic_prompt(move |_t, _p| *e2.borrow_mut() += 1)
        .expect("133 cb")
        .on_unknown_sequence(move |_t, seq| {
            if let UnknownSequence::Osc { content, .. } = seq
                && content.starts_with(b"7501;")
            {
                *e3.borrow_mut() += 1;
            }
        })
        .expect("unknown cb")
        .set_unknown_max_bytes(4096)
        .expect("unknown max");
    t
}

fn rgb(c: libghostty_vt::color::Rgb) -> Rgb8 {
    Rgb8 {
        r: c.r,
        g: c.g,
        b: c.b,
    }
}

/// Copies the visible screen of a ghostty terminal into the same `hv_grid::Snapshot`.
fn ghostty_snapshot<'a>(
    term: &Terminal<'a, '_>,
    state: &mut RenderState<'a>,
    rows_it: &mut RowIterator<'a>,
    cells_it: &mut CellIterator<'a>,
    fast: bool,
) -> Snapshot {
    let snap = state.update(term).expect("render update");
    let colors = snap.colors().expect("colors");
    let foreground = rgb(colors.foreground);
    let background = rgb(colors.background);
    let cols = usize::from(snap.cols().expect("cols"));
    let rows = usize::from(snap.rows().expect("rows"));
    let blank = SnapCell {
        ch: ' ',
        fg: foreground,
        bg: background,
        bold: false,
        italic: false,
        underline: false,
        wide_spacer: false,
        selected: false,
        link: 0,
    };
    let mut cells = vec![blank; rows * cols];
    let mut row_iter = rows_it.update(&snap).expect("rows");
    let mut y = 0usize;
    let mut buf = ['\0'; 16];
    while let Some(row) = row_iter.next() {
        let sel = row.selection().expect("selection");
        let mut cell_iter = cells_it.update(row).expect("cells");
        let mut x = 0usize;
        while let Some(cell) = cell_iter.next() {
            let raw = cell.raw_cell().expect("raw");
            let (cp, wide, styled, tag) = raw
                .get_multi((
                    cell::Codepoint,
                    cell::Wide,
                    cell::HasStyling,
                    cell::ContentTag,
                ))
                .expect("raw fields");
            if fast
                && !styled
                && matches!(
                    tag,
                    CellContentTag::Codepoint | CellContentTag::CodepointGrapheme
                )
            {
                // Plain cell: the raw u64 already has everything (one FFI call instead of ~6).
                if let Some(slot) = cells.get_mut(y * cols + x) {
                    slot.ch = char::from_u32(cp).filter(|&c| c != '\0').unwrap_or(' ');
                    slot.wide_spacer = wide == CellWide::SpacerTail;
                    slot.selected =
                        sel.is_some_and(|s| (s.start_x..=s.end_x).contains(&(x as u16)));
                }
                x += 1;
                continue;
            }
            let n = cell.graphemes_len().expect("glen") as usize;
            let ch = if n == 0 {
                ' '
            } else if n <= buf.len() {
                cell.graphemes_buf(&mut buf[..n]).expect("gbuf");
                buf[0]
            } else {
                cell.graphemes().expect("g")[0]
            };
            let (bold, italic, underline, inverse, faint, invisible) = if styled {
                let s = cell.style().expect("style");
                (
                    s.bold,
                    s.italic,
                    s.underline != Underline::None,
                    s.inverse,
                    s.faint,
                    s.invisible,
                )
            } else {
                (false, false, false, false, false, false)
            };
            let mut fg = cell.fg_color().expect("fg").map_or(foreground, rgb);
            let mut bg = cell.bg_color().expect("bg").map_or(background, rgb);
            if faint {
                fg = fg.dim();
            }
            if inverse {
                std::mem::swap(&mut fg, &mut bg);
            }
            if invisible {
                fg = bg;
            }
            let selected = sel.is_some_and(|s| {
                let x = x as u16;
                x >= s.start_x && x <= s.end_x
            });
            if let Some(slot) = cells.get_mut(y * cols + x) {
                *slot = SnapCell {
                    ch,
                    fg,
                    bg,
                    bold,
                    italic,
                    underline,
                    wide_spacer: wide == CellWide::SpacerTail,
                    selected,
                    link: 0, // hyperlinks: GridRef::hyperlink_uri, per cell flagged by raw.has_hyperlink()
                };
            }
            x += 1;
        }
        y += 1;
    }
    let cursor = snap.cursor_viewport().ok().flatten().map(|c| Cursor {
        row: usize::from(c.y),
        col: usize::from(c.x),
        shape: CursorKind::Block,
    });
    Snapshot {
        cols,
        rows,
        cells,
        cursor,
        background,
        foreground,
        display_offset: 0,
        hyperlinks: Vec::new(),
    }
}

fn text(s: &Snapshot) -> String {
    let mut out = String::new();
    for r in 0..s.rows {
        let line: String = s.cells[r * s.cols..(r + 1) * s.cols]
            .iter()
            .filter(|c| !c.wide_spacer)
            .map(|c| c.ch)
            .collect();
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

fn run(name: &str, chunk: &[u8], mb_target: usize) {
    let repeats = (mb_target * 1024 * 1024 / chunk.len()).max(1);
    let mb = (repeats * chunk.len()) as f64 / (1024.0 * 1024.0);
    println!("\n== workload `{name}`: {mb:.0} MB, 200x60, 10k scrollback, 64 KiB pieces ==");

    // --- alacritty_terminal, bare
    let (tx, _rx) = mpsc::channel();
    let cfg = || Config {
        scrolling_history: 10_000,
        ..Config::default()
    };
    let mut aterm = Term::new(cfg(), &Dims, Proxy(tx));
    let mut proc_: Processor<StdSyncHandler> = Processor::new();
    let t = Instant::now();
    for _ in 0..repeats {
        for piece in chunk.chunks(64 * 1024) {
            proc_.advance(&mut aterm, piece);
        }
    }
    let a_bare = mb / t.elapsed().as_secs_f64();

    // --- alacritty_terminal + hv-term interceptor (what hivetty does today)
    let (tx, _rx2) = mpsc::channel();
    let mut aterm2 = Term::new(cfg(), &Dims, Proxy(tx));
    let mut proc2: Processor<StdSyncHandler> = Processor::new();
    let mut icpt = osc::Interceptor::new();
    let mut a_events = 0;
    let t = Instant::now();
    for _ in 0..repeats {
        for piece in chunk.chunks(64 * 1024) {
            a_events += icpt.feed(piece).len();
            proc2.advance(&mut aterm2, piece);
        }
    }
    let a_full = mb / t.elapsed().as_secs_f64();

    // --- ghostty, no callbacks
    let mut g = Terminal::new(COLS, ROWS).expect("ghostty");
    g.set_scrollback_max_lines(Some(10_000)).expect("sb");
    let t = Instant::now();
    for _ in 0..repeats {
        for piece in chunk.chunks(64 * 1024) {
            g.vt_write(piece);
        }
    }
    let g_bare = mb / t.elapsed().as_secs_f64();
    drop(g);

    // --- ghostty with the callbacks that would replace the interceptor
    let events = Rc::new(RefCell::new(0usize));
    let mut g2 = ghostty_terminal(Rc::clone(&events));
    let t = Instant::now();
    for _ in 0..repeats {
        for piece in chunk.chunks(64 * 1024) {
            g2.vt_write(piece);
        }
    }
    let g_full = mb / t.elapsed().as_secs_f64();

    println!("throughput  alacritty bare           {a_bare:7.0} MB/s");
    println!("throughput  alacritty + interceptor  {a_full:7.0} MB/s  ({a_events} OSC events)");
    println!("throughput  ghostty bare             {g_bare:7.0} MB/s");
    println!(
        "throughput  ghostty + callbacks      {g_full:7.0} MB/s  ({} callback events)",
        events.borrow()
    );

    // --- snapshot cost (full-screen copy into hv_grid::Snapshot)
    let n = 200;
    let t = Instant::now();
    for _ in 0..n {
        std::hint::black_box(snapshot::take(&aterm2));
    }
    let a_snap = t.elapsed().as_secs_f64() * 1000.0 / f64::from(n);

    let mut state = RenderState::new().expect("rs");
    let mut rows_it = RowIterator::new().expect("ri");
    let mut cells_it = CellIterator::new().expect("ci");
    // Clean update (nothing changed since the last one): the steady-state per-frame cost.
    let first = ghostty_snapshot(&g2, &mut state, &mut rows_it, &mut cells_it, false);
    let t = Instant::now();
    for _ in 0..n {
        std::hint::black_box(ghostty_snapshot(
            &g2,
            &mut state,
            &mut rows_it,
            &mut cells_it,
            false,
        ));
    }
    let g_snap = t.elapsed().as_secs_f64() * 1000.0 / f64::from(n);
    let t = Instant::now();
    for _ in 0..n {
        std::hint::black_box(ghostty_snapshot(
            &g2,
            &mut state,
            &mut rows_it,
            &mut cells_it,
            true,
        ));
    }
    let g_snap_fast = t.elapsed().as_secs_f64() * 1000.0 / f64::from(n);
    let fast_same =
        first.cells == ghostty_snapshot(&g2, &mut state, &mut rows_it, &mut cells_it, true).cells;

    // Dirty update: one new screen of output between frames (update work included).
    let screen: Vec<u8> = chunk.iter().copied().take(16 * 1024).collect();
    let mut a_dirty = 0.0;
    let mut g_dirty = 0.0;
    let mut g_dirty_fast = 0.0;
    let mut ap: Processor<StdSyncHandler> = Processor::new();
    for i in 0..n {
        ap.advance(&mut aterm2, &screen);
        g2.vt_write(&screen);
        let t = Instant::now();
        std::hint::black_box(snapshot::take(&aterm2));
        a_dirty += t.elapsed().as_secs_f64();
        let fast = i % 2 == 1;
        let t = Instant::now();
        std::hint::black_box(ghostty_snapshot(
            &g2,
            &mut state,
            &mut rows_it,
            &mut cells_it,
            fast,
        ));
        if fast {
            g_dirty_fast += t.elapsed().as_secs_f64();
        } else {
            g_dirty += t.elapsed().as_secs_f64();
        }
    }
    let ms = |s: f64| s * 1000.0 / f64::from(n);
    println!("snapshot    alacritty (clean)        {a_snap:7.3} ms");
    println!("snapshot    ghostty   (clean)        {g_snap:7.3} ms");
    println!(
        "snapshot    ghostty   (clean, fast)  {g_snap_fast:7.3} ms  (same cells: {fast_same})"
    );
    println!(
        "snapshot    alacritty (after output) {:7.3} ms",
        ms(a_dirty)
    );
    println!(
        "snapshot    ghostty   (after output) {:7.3} ms",
        ms(g_dirty) * 2.0
    );
    println!(
        "snapshot    ghostty   (after, fast)  {:7.3} ms",
        ms(g_dirty_fast) * 2.0
    );

    // Sanity: both engines end on the same visible text.
    let a_text = text(&snapshot::take(&aterm2));
    let g_text = text(&ghostty_snapshot(
        &g2,
        &mut state,
        &mut rows_it,
        &mut cells_it,
        false,
    ));
    let same = a_text
        .lines()
        .zip(g_text.lines())
        .filter(|(a, b)| a == b)
        .count();
    println!(
        "check       same visible lines: {same}/{} (first snapshot had {} cells)",
        ROWS,
        first.cells.len()
    );
    if same < usize::from(ROWS) {
        for (a, b) in a_text
            .lines()
            .zip(g_text.lines())
            .filter(|(a, b)| a != b)
            .take(3)
        {
            println!("  alacritty: {a:?}\n  ghostty:   {b:?}");
        }
    }
}

fn main() {
    let mb: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(64);
    println!(
        "libghostty-vt vt-1.0.1, SIMD: {:?}",
        libghostty_vt::build_info::supports_simd().ok()
    );
    run("hv-term bench (logs + OSC)", &workload(), mb);
    run(
        "heavy (SGR 256/truecolor, CUP, DECSTBM, wide, OSC 8)",
        &workload_heavy(),
        mb,
    );
}
