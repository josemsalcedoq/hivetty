//! `--screenshot`: run a command in a real PTY and render the result to a PNG without opening
//! a window. Used to check rendering on machines and CI runners without touching the desktop.

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use hv_render::{PaneView, Renderer};
use hv_term::{GridSize, Session, SessionEvent, SpawnSpec};

use crate::model::SessionInfo;
use crate::workspace::{Axis, PaneNode};

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
    /// Draw the egui chrome (sidebar and tab bar) too.
    pub(crate) ui: bool,
    /// Draw this as IME composition text at the cursor.
    pub(crate) preedit: Option<String>,
    /// Run the program in two panes side by side (the second one focused).
    pub(crate) split: bool,
    /// Show the command palette (with `ui`).
    pub(crate) palette: bool,
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
            preedit: None,
            split: false,
            palette: false,
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

/// One captured pane.
struct Captured {
    id: u64,
    session: Session,
    info: SessionInfo,
    exited: bool,
}

/// Runs `spec`, waits for it to exit (or for `opts.wait`), and writes the screen to `out`.
pub(crate) fn screenshot(spec: &SpawnSpec, opts: &Capture, out: &Path) -> Result<String> {
    let mut renderer = Renderer::new_headless(1, 1, 1.0, opts.font_size)?;
    let (grid_w, grid_h) = renderer.size_for_grid(opts.cols, opts.rows);
    let panes_w = if opts.split { grid_w * 2 + 1 } else { grid_w };
    let (sidebar, tab_bar) = if opts.ui {
        (SIDEBAR_PX, hv_ui::TAB_BAR_HEIGHT as u32)
    } else {
        (0, 0)
    };
    let (width, height) = (panes_w + sidebar, grid_h + tab_bar);
    renderer.resize(width, height);
    let mut chrome = opts.ui.then(hv_ui::Chrome::new);
    // Texture updates (the font atlas) accumulate until the first frame is painted.
    let mut pending_textures = egui::TexturesDelta::default();
    let mut area = [0.0, 0.0, width as f32, height as f32];
    if let Some(chrome) = chrome.as_mut() {
        let model = hv_ui::ChromeModel {
            sidebar_visible: true,
            tabs: vec![hv_ui::TabItem::new("", hv_ui::RowState::Idle, true)],
            ..Default::default()
        };
        let out = chrome.run(raw_input(width, height), &model);
        pending_textures.append(out.full.textures_delta);
        area = out.terminal_area;
        renderer.set_grid_area(Some(area));
    }
    let mut root = PaneNode::Leaf(1);
    if opts.split {
        root.split(1, 2, Axis::Horizontal);
    }
    let layout = root.layout(area, 1.0);
    let (cw, ch) = renderer.cell_size();
    let mut panes = Vec::new();
    for (id, pane_area) in &layout.panes {
        let (cols, rows) = renderer.grid_size_in(*pane_area);
        let grid = GridSize {
            cols,
            rows,
            cell_width: cw as u16,
            cell_height: ch as u16,
        };
        panes.push(Captured {
            id: *id,
            session: Session::spawn(spec, grid, 10_000, || {})?,
            info: SessionInfo::new(),
            exited: false,
        });
    }

    let deadline = Instant::now() + opts.wait;
    let mut events = Vec::new();
    while panes.iter().any(|p| !p.exited) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        for pane in &mut panes {
            let batch = pane.session.drain_events();
            pane.exited |= batch.contains(&SessionEvent::Exited);
            for event in &batch {
                match event {
                    SessionEvent::Title(title) => pane.info.title = Some(title.clone()),
                    SessionEvent::Term(term_event) => {
                        pane.info.on_term_event(term_event);
                    }
                    _ => {}
                }
            }
            events.extend(batch);
        }
    }
    if opts.print_events {
        for event in &events {
            println!("event: {event:?}");
        }
    }
    // Let the last output settle and close any open synchronized update.
    std::thread::sleep(Duration::from_millis(150));
    let focused = panes.last().map_or(1, |p| p.id);
    for pane in &mut panes {
        pane.session.flush_expired_sync();
        if !pane.exited {
            pane.session.kill();
        }
        pane.info.exited = pane.exited;
    }
    let main = panes.last().expect("at least one pane");
    if opts.scroll_up > 0 {
        main.session.scroll(opts.scroll_up);
    }
    renderer.set_preedit(opts.preedit.as_deref());
    if let Some(pattern) = &opts.find {
        let status = match main.session.search(pattern) {
            Ok(true) => "",
            Ok(false) => "  (no matches)",
            Err(_) => "  (invalid pattern)",
        };
        renderer.set_search_bar(Some(&format!("Find: {pattern}▏{status}")));
    }
    let snapshots: Vec<_> = panes.iter().map(|p| p.session.snapshot()).collect();
    let mut frame = chrome.as_mut().map(|chrome| {
        let sessions = panes
            .iter()
            .map(|p| {
                let mut row = crate::model::session_row(&p.info, p.session.cwd().as_deref());
                row.focused = p.id == focused;
                row
            })
            .collect();
        let title = panes
            .last()
            .and_then(|p| p.info.title.clone())
            .unwrap_or_else(|| hv_ui::strings::DEFAULT_TITLE.to_owned());
        let model = hv_ui::ChromeModel {
            sidebar_visible: true,
            sessions,
            tabs: vec![{
                let main = panes.last().expect("at least one pane");
                let state = crate::model::session_row(&main.info, None).state;
                let mut tab = hv_ui::TabItem::new(title, state, true);
                if let Some((kind, value)) = main.info.progress {
                    tab.progress = Some(f32::from(value) / 100.0);
                    tab.progress_error = kind == 2;
                }
                tab
            }],
        };
        if opts.palette {
            let items = crate::keymap::PALETTE_ACTIONS
                .iter()
                .enumerate()
                .filter_map(|(id, name)| {
                    let action = crate::keymap::Action::parse(name)?;
                    Some(hv_ui::PaletteItem {
                        id,
                        label: hv_ui::strings::action_label(name)?.to_owned(),
                        hint: crate::keymap::Keymap::defaults()
                            .chord_for(&action)
                            .map(crate::keymap::Chord::pretty)
                            .unwrap_or_default(),
                    })
                })
                .collect();
            chrome.open_palette(items);
            // One frame to lay the palette out and focus it.
            let warm = chrome.run(raw_input(width, height), &model);
            pending_textures.append(warm.full.textures_delta);
        }
        let out = chrome.run(raw_input(width, height), &model);
        let mut textures = std::mem::take(&mut pending_textures);
        textures.append(out.full.textures_delta);
        hv_render::UiFrame {
            primitives: chrome.tessellate(out.full.shapes, out.full.pixels_per_point),
            textures,
            pixels_per_point: out.full.pixels_per_point,
        }
    });
    let views: Vec<PaneView<'_>> = panes
        .iter()
        .zip(&snapshots)
        .map(|(pane, snap)| PaneView {
            id: pane.id,
            snap,
            area: layout.area(pane.id).unwrap_or(area),
            focused: pane.id == focused,
        })
        .collect();
    let dividers: Vec<[f32; 4]> = layout.dividers.iter().map(|d| d.rect).collect();
    let (w, h, rgba) = renderer.render_panes_to_rgba(&views, &dividers, frame.as_mut())?;
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
