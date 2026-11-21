//! The window: winit event loop wired to one terminal session and the GPU renderer.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use hv_render::Renderer;
use hv_term::input::{Key as TermKey, Mods, encode_key};
use std::time::{Duration, Instant};

use hv_term::{GridSize, SelectKind, Session, SessionEvent, SpawnSpec};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{
    ElementState, KeyEvent, MouseButton, MouseScrollDelta, StartCause, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

const DEFAULT_FONT_SIZE: f32 = 14.0;
const APP_TITLE: &str = "hivetty";

/// Events sent to the UI thread from other threads.
#[derive(Debug, Clone, Copy)]
pub(crate) enum AppEvent {
    /// The PTY reader produced output or an event.
    Wakeup,
}

/// Runs the terminal window until the shell exits or the window is closed.
pub(crate) fn run(spec: SpawnSpec) -> Result<()> {
    let event_loop = EventLoop::<AppEvent>::with_user_event()
        .build()
        .context("cannot start event loop")?;
    let mut app = App::new(spec, event_loop.create_proxy());
    event_loop.run_app(&mut app).context("event loop failed")?;
    if let Some(err) = app.fatal.take() {
        return Err(err);
    }
    Ok(())
}

struct Ui {
    window: Arc<Window>,
    renderer: Renderer,
    session: Session,
}

struct App {
    spec: SpawnSpec,
    proxy: EventLoopProxy<AppEvent>,
    wake_pending: Arc<AtomicBool>,
    ui: Option<Ui>,
    modifiers: ModifiersState,
    scroll_remainder: f32,
    fatal: Option<anyhow::Error>,
    mouse: MouseState,
}

/// Pointer position and click tracking for selection.
#[derive(Default)]
struct MouseState {
    x: f32,
    y: f32,
    selecting: bool,
    last_click: Option<(Instant, usize, usize)>,
    click_count: u8,
}

/// Max time between clicks to count as a double / triple click.
const MULTI_CLICK: Duration = Duration::from_millis(400);

impl App {
    fn new(spec: SpawnSpec, proxy: EventLoopProxy<AppEvent>) -> Self {
        Self {
            spec,
            proxy,
            wake_pending: Arc::new(AtomicBool::new(false)),
            ui: None,
            modifiers: ModifiersState::empty(),
            scroll_remainder: 0.0,
            fatal: None,
            mouse: MouseState::default(),
        }
    }

    fn create_ui(&mut self, event_loop: &ActiveEventLoop) -> Result<Ui> {
        let attrs = Window::default_attributes()
            .with_title(APP_TITLE)
            .with_inner_size(LogicalSize::new(1000.0, 640.0));
        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .context("cannot create window")?,
        );
        let size = window.inner_size();
        let scale = window.scale_factor() as f32;
        let renderer = Renderer::new(
            Arc::clone(&window),
            size.width,
            size.height,
            scale,
            DEFAULT_FONT_SIZE,
        )?;
        log::info!("renderer: {}", renderer.adapter_description());

        let grid = grid_size(&renderer);
        let proxy = self.proxy.clone();
        let pending = Arc::clone(&self.wake_pending);
        let wake = move || {
            // Coalesce: one pending wakeup is enough, the UI drains everything at once.
            if !pending.swap(true, Ordering::AcqRel) {
                let _ = proxy.send_event(AppEvent::Wakeup);
            }
        };
        let session = Session::spawn(&self.spec, grid, wake)?;
        Ok(Ui {
            window,
            renderer,
            session,
        })
    }

    fn on_key(&mut self, event: &KeyEvent) {
        let Some(ui) = &mut self.ui else { return };
        if event.state != ElementState::Pressed {
            return;
        }
        let m = self.modifiers;
        // The app's own shortcut modifier: Cmd on macOS, Ctrl+Shift elsewhere (docs/keybindings.md).
        let primary = if cfg!(target_os = "macos") {
            m.super_key()
        } else {
            m.control_key() && m.shift_key()
        };

        if primary && let Key::Character(c) = &event.logical_key {
            match c.to_lowercase().as_str() {
                "c" => {
                    copy_selection(&ui.session);
                    return;
                }
                "v" => {
                    paste_clipboard(&ui.session);
                    return;
                }
                "=" | "+" => {
                    zoom(ui, 1.0);
                    return;
                }
                "-" => {
                    zoom(ui, -1.0);
                    return;
                }
                "0" => {
                    zoom(ui, DEFAULT_FONT_SIZE - ui.renderer.font_size());
                    return;
                }
                _ => {}
            }
        }
        // Windows convention: Ctrl+C copies when there is a selection, otherwise it is ^C.
        if cfg!(windows)
            && m.control_key()
            && !m.shift_key()
            && !m.alt_key()
            && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("c"))
            && ui.session.selection_text().is_some()
        {
            copy_selection(&ui.session);
            ui.session.select_clear();
            ui.window.request_redraw();
            return;
        }
        if cfg!(target_os = "macos") && m.super_key() {
            return; // never send Cmd+<key> to the program
        }

        let mods = Mods {
            shift: m.shift_key(),
            ctrl: m.control_key(),
            alt: m.alt_key(),
        };
        let Some(key) = translate_key(event, mods) else {
            return;
        };
        // On Windows AltGr arrives as Ctrl+Alt: if it produced printable text, send that text as is.
        let mods = if cfg!(windows)
            && mods.ctrl
            && mods.alt
            && matches!(&key, TermKey::Text(t) if !t.chars().any(char::is_control))
        {
            Mods::default()
        } else {
            mods
        };
        if let Some(bytes) = encode_key(&key, mods, ui.session.key_modes()) {
            ui.session.select_clear();
            ui.session.scroll_to_bottom();
            ui.session.write(&bytes);
        }
    }

    fn on_mouse_button(&mut self, state: ElementState, button: MouseButton) {
        let Some(ui) = &mut self.ui else { return };
        if button != MouseButton::Left {
            return;
        }
        if state == ElementState::Pressed
            && ui.renderer.hits_jump_to_bottom(self.mouse.x, self.mouse.y)
        {
            ui.session.scroll_to_bottom();
            ui.window.request_redraw();
            return;
        }
        // Programs that asked for mouse reporting get clicks (F1-16); Shift forces selection.
        if ui.session.mouse_mode() && !self.modifiers.shift_key() {
            return;
        }
        match state {
            ElementState::Pressed => {
                let (row, col, right) = ui.renderer.cell_at(self.mouse.x, self.mouse.y);
                let now = Instant::now();
                self.mouse.click_count = match self.mouse.last_click {
                    Some((t, r, c)) if now - t < MULTI_CLICK && r == row && c == col => {
                        self.mouse.click_count % 3 + 1
                    }
                    _ => 1,
                };
                self.mouse.last_click = Some((now, row, col));
                let kind = match self.mouse.click_count {
                    2 => SelectKind::Word,
                    3 => SelectKind::Line,
                    _ if self.modifiers.alt_key() => SelectKind::Block,
                    _ => SelectKind::Simple,
                };
                ui.session.select_start(row, col, right, kind);
                if kind != SelectKind::Simple && kind != SelectKind::Block {
                    ui.session.select_update(row, col, right);
                }
                self.mouse.selecting = true;
            }
            ElementState::Released => {
                self.mouse.selecting = false;
                // A plain click without dragging leaves no selection behind.
                if self.mouse.click_count == 1 && ui.session.selection_text().is_none() {
                    ui.session.select_clear();
                }
            }
        }
        ui.window.request_redraw();
    }

    fn on_cursor_moved(&mut self, x: f32, y: f32) {
        self.mouse.x = x;
        self.mouse.y = y;
        if let (true, Some(ui)) = (self.mouse.selecting, &self.ui) {
            let (row, col, right) = ui.renderer.cell_at(x, y);
            ui.session.select_update(row, col, right);
            ui.window.request_redraw();
        }
    }

    fn on_wheel(&mut self, delta: MouseScrollDelta) {
        let Some(ui) = &mut self.ui else { return };
        let (_, cell_h) = ui.renderer.cell_size();
        let lines = match delta {
            MouseScrollDelta::LineDelta(_, y) => y * 3.0,
            MouseScrollDelta::PixelDelta(p) => p.y as f32 / cell_h,
        } + self.scroll_remainder;
        let whole = lines.trunc();
        self.scroll_remainder = lines - whole;
        let whole = whole as i32;
        if whole == 0 {
            return;
        }
        if ui.session.alt_screen() {
            // Full-screen programs (vim, less, agent TUIs) get arrow keys, like xterm's alternate scroll.
            let key = if whole > 0 {
                TermKey::Up
            } else {
                TermKey::Down
            };
            if let Some(bytes) = encode_key(&key, Mods::default(), ui.session.key_modes()) {
                ui.session
                    .write(&bytes.repeat(whole.unsigned_abs() as usize));
            }
        } else {
            ui.session.scroll(whole);
        }
        ui.window.request_redraw();
    }

    fn handle_session_events(&mut self, event_loop: &ActiveEventLoop) {
        let Some(ui) = &mut self.ui else { return };
        for event in ui.session.drain_events() {
            match event {
                SessionEvent::Title(title) => ui.window.set_title(&title),
                SessionEvent::ResetTitle => ui.window.set_title(APP_TITLE),
                SessionEvent::Exited => event_loop.exit(),
                SessionEvent::Bell => ui
                    .window
                    .request_user_attention(Some(winit::window::UserAttentionType::Informational)),
                SessionEvent::Term(term_event) => log::debug!("terminal event: {term_event:?}"),
            }
        }
    }
}

impl ApplicationHandler<AppEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.ui.is_some() {
            return;
        }
        match self.create_ui(event_loop) {
            Ok(ui) => {
                ui.window.request_redraw();
                self.ui = Some(ui);
            }
            Err(err) => {
                self.fatal = Some(err);
                event_loop.exit();
            }
        }
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if let (StartCause::ResumeTimeReached { .. }, Some(ui)) = (cause, &self.ui) {
            ui.window.request_redraw();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::Wakeup => {
                self.wake_pending.store(false, Ordering::Release);
                self.handle_session_events(event_loop);
                if let Some(ui) = &self.ui {
                    ui.window.request_redraw();
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                if let Some(ui) = &self.ui {
                    ui.session.kill();
                }
                event_loop.exit();
            }
            WindowEvent::Resized(PhysicalSize { width, height }) => {
                if let Some(ui) = &mut self.ui {
                    ui.renderer.resize(width, height);
                    ui.session.resize(grid_size(&ui.renderer));
                    ui.window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(ui) = &mut self.ui {
                    ui.renderer.set_scale(scale_factor as f32);
                    ui.session.resize(grid_size(&ui.renderer));
                }
            }
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::KeyboardInput { event, .. } => self.on_key(&event),
            WindowEvent::MouseWheel { delta, .. } => self.on_wheel(delta),
            WindowEvent::MouseInput { state, button, .. } => self.on_mouse_button(state, button),
            WindowEvent::CursorMoved { position, .. } => {
                self.on_cursor_moved(position.x as f32, position.y as f32);
            }
            WindowEvent::RedrawRequested => {
                if let Some(ui) = &mut self.ui {
                    ui.session.flush_expired_sync();
                    let snapshot = ui.session.snapshot();
                    if let Err(err) = ui.renderer.render(&snapshot) {
                        log::error!("render failed: {err:#}");
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // A synchronized update (DECSET 2026) must be flushed when its deadline passes even if
        // no more output arrives.
        let deadline = self.ui.as_ref().and_then(|ui| ui.session.sync_deadline());
        event_loop.set_control_flow(deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
}

fn grid_size(renderer: &Renderer) -> GridSize {
    let (cols, rows) = renderer.grid_size();
    let (cw, ch) = renderer.cell_size();
    GridSize {
        cols,
        rows,
        cell_width: cw as u16,
        cell_height: ch as u16,
    }
}

fn zoom(ui: &mut Ui, delta: f32) {
    ui.renderer.set_font_size(ui.renderer.font_size() + delta);
    ui.session.resize(grid_size(&ui.renderer));
    ui.window.request_redraw();
}

fn copy_selection(session: &Session) {
    let Some(text) = session.selection_text() else {
        return;
    };
    if let Err(err) = arboard::Clipboard::new().and_then(|mut c| c.set_text(text)) {
        log::warn!("clipboard write failed: {err}");
    }
}

fn paste_clipboard(session: &Session) {
    match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
        Ok(text) => session.paste(&text),
        Err(err) => log::warn!("clipboard read failed: {err}"),
    }
}

/// Maps a winit key event to the terminal's key type.
fn translate_key(event: &KeyEvent, mods: Mods) -> Option<TermKey> {
    let key = match &event.logical_key {
        Key::Named(named) => match named {
            NamedKey::Enter => TermKey::Enter,
            NamedKey::Backspace => TermKey::Backspace,
            NamedKey::Tab => TermKey::Tab,
            NamedKey::Escape => TermKey::Escape,
            NamedKey::ArrowUp => TermKey::Up,
            NamedKey::ArrowDown => TermKey::Down,
            NamedKey::ArrowLeft => TermKey::Left,
            NamedKey::ArrowRight => TermKey::Right,
            NamedKey::Home => TermKey::Home,
            NamedKey::End => TermKey::End,
            NamedKey::PageUp => TermKey::PageUp,
            NamedKey::PageDown => TermKey::PageDown,
            NamedKey::Insert => TermKey::Insert,
            NamedKey::Delete => TermKey::Delete,
            NamedKey::Space => TermKey::Text(" ".into()),
            NamedKey::F1 => TermKey::F(1),
            NamedKey::F2 => TermKey::F(2),
            NamedKey::F3 => TermKey::F(3),
            NamedKey::F4 => TermKey::F(4),
            NamedKey::F5 => TermKey::F(5),
            NamedKey::F6 => TermKey::F(6),
            NamedKey::F7 => TermKey::F(7),
            NamedKey::F8 => TermKey::F(8),
            NamedKey::F9 => TermKey::F(9),
            NamedKey::F10 => TermKey::F(10),
            NamedKey::F11 => TermKey::F(11),
            NamedKey::F12 => TermKey::F(12),
            _ => return None,
        },
        // With Ctrl or Alt use the logical character (event.text may already be a control code).
        Key::Character(c) if mods.ctrl || mods.alt => TermKey::Text(c.to_string()),
        _ => TermKey::Text(event.text.as_ref()?.to_string()),
    };
    Some(key)
}
