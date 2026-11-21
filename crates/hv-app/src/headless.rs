//! `--screenshot`: run a command in a real PTY and render the result to a PNG without opening
//! a window. Used to check rendering on machines and CI runners without touching the desktop.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use hv_render::Renderer;
use hv_term::{GridSize, Session, SessionEvent, SpawnSpec};

/// Options for a headless capture.
#[derive(Debug, Clone)]
pub(crate) struct Capture {
    pub(crate) cols: u16,
    pub(crate) rows: u16,
    /// Maximum time to wait for the program to exit.
    pub(crate) wait: Duration,
    pub(crate) font_size: f32,
    /// Print the terminal events (titles, OSC 7/9/133/777/7501…) seen during the run.
    pub(crate) print_events: bool,
    /// Lines to scroll up into the scrollback before capturing.
    pub(crate) scroll_up: i32,
    /// Run a scrollback search and show the search bar before capturing.
    pub(crate) find: Option<String>,
}

impl Default for Capture {
    fn default() -> Self {
        Self {
            cols: 100,
            rows: 30,
            wait: Duration::from_secs(5),
            font_size: 14.0,
            print_events: false,
            scroll_up: 0,
            find: None,
        }
    }
}

/// Runs `spec`, waits for it to exit (or for `opts.wait`), and writes the screen to `out`.
pub(crate) fn screenshot(spec: &SpawnSpec, opts: &Capture, out: &Path) -> Result<String> {
    let mut renderer = Renderer::new_headless(1, 1, 1.0, opts.font_size)?;
    let (width, height) = renderer.size_for_grid(opts.cols, opts.rows);
    renderer.resize(width, height);
    let (cw, ch) = renderer.cell_size();
    let grid = GridSize {
        cols: opts.cols,
        rows: opts.rows,
        cell_width: cw as u16,
        cell_height: ch as u16,
    };

    let session = Session::spawn(spec, grid, 10_000, || {})?;
    let deadline = Instant::now() + opts.wait;
    let mut exited = false;
    let mut events = Vec::new();
    while !exited && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        let batch = session.drain_events();
        exited = batch.contains(&SessionEvent::Exited);
        events.extend(batch);
    }
    if opts.print_events {
        for event in &events {
            println!("event: {event:?}");
        }
    }
    // Let the last output settle and close any open synchronized update.
    std::thread::sleep(Duration::from_millis(150));
    session.flush_expired_sync();
    if !exited {
        session.kill();
    }

    if opts.scroll_up > 0 {
        session.scroll(opts.scroll_up);
    }
    if let Some(pattern) = &opts.find {
        let status = match session.search(pattern) {
            Ok(true) => "",
            Ok(false) => "  (no matches)",
            Err(_) => "  (invalid pattern)",
        };
        renderer.set_search_bar(Some(&format!("Find: {pattern}▏{status}")));
    }
    let snapshot = session.snapshot();
    let (w, h, rgba) = renderer.render_to_rgba(&snapshot)?;
    let file = File::create(out).with_context(|| format!("cannot create {}", out.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(&rgba)?;
    Ok(format!(
        "{} ({w}x{h}, GPU: {})",
        out.display(),
        renderer.adapter_description()
    ))
}
