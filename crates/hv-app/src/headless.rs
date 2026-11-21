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
    /// Draw the egui chrome (sidebar) too.
    pub(crate) ui: bool,
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
            ui: false,
        }
    }
}

/// Width reserved for the sidebar in `--ui` captures (matches hv-ui at 1x scale).
const SIDEBAR_PX: u32 = 260;

/// egui input for an offscreen frame of `width` x `height` pixels at 1x scale.
fn raw_input(width: u32, height: u32) -> egui::RawInput {
    egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(width as f32, height as f32),
        )),
        ..Default::default()
    }
}

/// Runs `spec`, waits for it to exit (or for `opts.wait`), and writes the screen to `out`.
pub(crate) fn screenshot(spec: &SpawnSpec, opts: &Capture, out: &Path) -> Result<String> {
    let mut renderer = Renderer::new_headless(1, 1, 1.0, opts.font_size)?;
    let (grid_w, height) = renderer.size_for_grid(opts.cols, opts.rows);
    let sidebar = if opts.ui { SIDEBAR_PX } else { 0 };
    let width = grid_w + sidebar;
    renderer.resize(width, height);
    let mut chrome = opts.ui.then(hv_ui::Chrome::new);
    let mut info = crate::model::SessionInfo::new();
    // Texture updates (the font atlas) accumulate until the first frame is painted.
    let mut pending_textures = egui::TexturesDelta::default();
    if let Some(chrome) = chrome.as_mut() {
        let model = hv_ui::ChromeModel {
            sidebar_visible: true,
            ..Default::default()
        };
        let out = chrome.run(raw_input(width, height), &model);
        pending_textures.append(out.full.textures_delta);
        renderer.set_grid_area(Some(out.terminal_area));
    }
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
        for event in &batch {
            if let SessionEvent::Title(title) = event {
                info.title = Some(title.clone());
            }
        }
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
    let mut frame = chrome.as_mut().map(|chrome| {
        info.exited = exited;
        let model = hv_ui::ChromeModel {
            sidebar_visible: true,
            sessions: vec![crate::model::session_row(&info, session.cwd().as_deref())],
        };
        let out = chrome.run(raw_input(width, height), &model);
        let mut textures = std::mem::take(&mut pending_textures);
        textures.append(out.full.textures_delta);
        hv_render::UiFrame {
            primitives: chrome.tessellate(out.full.shapes, out.full.pixels_per_point),
            textures,
            pixels_per_point: out.full.pixels_per_point,
        }
    });
    let (w, h, rgba) = renderer.render_to_rgba_with_ui(&snapshot, frame.as_mut())?;
    pending_textures.clear();
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
