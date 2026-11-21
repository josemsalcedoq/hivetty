//! The window: winit event loop wired to one terminal session and the GPU renderer.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use hv_render::Renderer;
use hv_term::input::{
    Key as TermKey, Mods, MouseAction, MouseButton as TermButton, encode_key, encode_mouse,
};
use std::time::{Duration, Instant};

use hv_term::{GridSize, SelectKind, Session, SessionEvent, SpawnSpec};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::dpi::{PhysicalPosition, PhysicalSize as PhysSize};
use winit::event::{
    ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, StartCause, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

use hv_term::links::{self, LinkTarget};

const DEFAULT_FONT_SIZE: f32 = 14.0;
const APP_TITLE: &str = "hivetty";

/// Events sent to the UI thread from other threads.
#[derive(Debug, Clone, Copy)]
pub(crate) enum AppEvent {
    /// The PTY reader produced output or an event.
    Wakeup,
    /// The config file changed on disk.
    ConfigChanged,
}

/// Runs the terminal window until the shell exits or the window is closed.
pub(crate) fn run(spec: SpawnSpec, record: Option<std::path::PathBuf>) -> Result<()> {
    let event_loop = EventLoop::<AppEvent>::with_user_event()
        .build()
        .context("cannot start event loop")?;
    let config_path = hv_config::default_path();
    let (config, config_error) = config_path
        .as_deref()
        .map_or_else(|| (hv_config::Config::default(), None), hv_config::load);
    if let Some(err) = config_error {
        log::warn!("{err}");
    }
    // An explicit `-e PROGRAM` wins over the configured shell.
    let mut spec = spec;
    if spec.program.is_none() && config.shell.program.is_some() {
        spec.program = config.shell.program.clone();
        spec.args = config.shell.args.clone();
    }
    let proxy = event_loop.create_proxy();
    let _watcher = config_path.as_deref().and_then(|path| {
        let proxy = proxy.clone();
        hv_config::watch(path, move || {
            let _ = proxy.send_event(AppEvent::ConfigChanged);
        })
    });
    let mut app = App::new(spec, proxy);
    app.config = config;
    app.config_path = config_path;
    app.record = record;
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
    /// Sidebar and other egui chrome (SP-3, ADR 0003).
    chrome: hv_ui::Chrome,
    egui: egui_winit::State,
    /// What the sidebar shows about the session.
    info: crate::model::SessionInfo,
    /// Grid area egui left last frame, in physical pixels.
    terminal_area: Option<[f32; 4]>,
}

impl Ui {
    /// Whether a point (physical pixels) is over the chrome rather than the terminal.
    fn over_chrome(&self, x: f32, y: f32) -> bool {
        self.terminal_area
            .is_some_and(|[ax, ay, aw, ah]| x < ax || y < ay || x >= ax + aw || y >= ay + ah)
    }

    /// Runs the chrome for this frame, applies a changed grid area and returns what to paint.
    fn run_chrome(
        &mut self,
        sidebar_visible: bool,
        cursor: egui::CursorIcon,
    ) -> hv_render::UiFrame {
        let input = self.egui.take_egui_input(&self.window);
        let model = hv_ui::ChromeModel {
            sidebar_visible,
            sessions: vec![crate::model::session_row(
                &self.info,
                self.session.cwd().as_deref(),
            )],
        };
        let mut out = self.chrome.run(input, &model);
        if self.terminal_area != Some(out.terminal_area) {
            self.terminal_area = Some(out.terminal_area);
            self.renderer.set_grid_area(Some(out.terminal_area));
            self.session.resize(grid_size(&self.renderer));
        }
        let ppp = out.full.pixels_per_point;
        let pointer_over_terminal = self
            .chrome
            .context()
            .pointer_latest_pos()
            .is_some_and(|p| !self.over_chrome(p.x * ppp, p.y * ppp));
        if pointer_over_terminal {
            out.full.platform_output.cursor_icon = cursor;
        }
        let platform = std::mem::take(&mut out.full.platform_output);
        self.egui.handle_platform_output(&self.window, platform);
        if out
            .full
            .viewport_output
            .values()
            .any(|v| v.repaint_delay.is_zero())
        {
            self.window.request_redraw();
        }
        hv_render::UiFrame {
            primitives: self.chrome.tessellate(out.full.shapes, ppp),
            textures: out.full.textures_delta,
            pixels_per_point: ppp,
        }
    }
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
    /// An input method is composing text; raw key presses belong to it, not to the program.
    ime_composing: bool,
    config: hv_config::Config,
    config_path: Option<std::path::PathBuf>,
    /// Scrollback search in progress (F1-21).
    search: Option<SearchUi>,
    /// `--record` target.
    record: Option<std::path::PathBuf>,
    /// Visual bell: flash the window until this instant (F1-23).
    bell_until: Option<Instant>,
    /// Sidebar shown (toggled with Ctrl+Shift+B / Cmd+B).
    sidebar_visible: bool,
}

#[derive(Debug, Default)]
struct SearchUi {
    query: String,
    /// `None` = not searched yet, `Some(Ok(found))`, or `Some(Err(regex error))`.
    result: Option<Result<bool, String>>,
}

impl SearchUi {
    /// The text shown in the search bar.
    fn label(&self) -> String {
        let status = match &self.result {
            Some(Ok(false)) => "  (no matches)",
            Some(Err(_)) => "  (invalid pattern)",
            _ => "",
        };
        format!("Find: {}▏{status}", self.query)
    }
}

/// Pointer position and click tracking for selection.
#[derive(Default)]
struct MouseState {
    x: f32,
    y: f32,
    selecting: bool,
    last_click: Option<(Instant, usize, usize)>,
    click_count: u8,
    /// Link under the pointer while the link modifier is held: `(row, start, end, target)`.
    hover_link: Option<(usize, usize, usize, LinkTarget)>,
    /// Button held while reporting to the program, and the last cell reported.
    reported_button: Option<TermButton>,
    last_reported_cell: Option<(usize, usize)>,
}

/// How long the visual bell flashes.
const BELL_FLASH: Duration = Duration::from_millis(120);

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
            ime_composing: false,
            config: hv_config::Config::default(),
            config_path: None,
            search: None,
            record: None,
            bell_until: None,
            sidebar_visible: true,
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
        // Needed for CJK input methods and, on macOS, for dead-key composition (´ + a = á).
        window.set_ime_allowed(true);
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
        let session = Session::spawn(
            &self.spec,
            grid,
            self.config.terminal.scrollback_lines,
            wake,
        )?;
        if let Some(path) = &self.record {
            session.start_recording(path, grid)?;
        }
        let chrome = hv_ui::Chrome::new();
        let egui = egui_winit::State::new(
            chrome.context().clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(scale),
            None,
            None,
        );
        let mut ui = Ui {
            window,
            renderer,
            session,
            chrome,
            egui,
            info: crate::model::SessionInfo::new(),
            terminal_area: None,
        };
        apply_font(&mut ui, &self.config.font);
        Ok(ui)
    }

    fn on_key(&mut self, event: &KeyEvent) {
        let Some(ui) = &mut self.ui else { return };
        if event.state != ElementState::Pressed || self.ime_composing {
            return;
        }
        if self.search.is_some() {
            self.on_search_key(event);
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
                "b" => {
                    self.sidebar_visible = !self.sidebar_visible;
                    ui.window.request_redraw();
                    return;
                }
                "f" => {
                    self.search = Some(SearchUi::default());
                    self.refresh_search_bar();
                    return;
                }
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

    fn on_ime(&mut self, ime: Ime) {
        match ime {
            Ime::Preedit(text, _) => self.ime_composing = !text.is_empty(),
            Ime::Commit(text) => {
                self.ime_composing = false;
                if let Some(ui) = &self.ui {
                    ui.session.select_clear();
                    ui.session.scroll_to_bottom();
                    ui.session.write(text.as_bytes());
                }
            }
            Ime::Enabled | Ime::Disabled => self.ime_composing = false,
        }
    }

    fn current_mods(&self) -> Mods {
        Mods {
            shift: self.modifiers.shift_key(),
            ctrl: self.modifiers.control_key(),
            alt: self.modifiers.alt_key(),
        }
    }

    /// Sends a mouse event to the program if it asked for them (and Shift is not held).
    /// Returns true if the event was consumed by reporting.
    fn report_mouse(&mut self, button: TermButton, action: MouseAction) -> bool {
        let Some(ui) = &self.ui else { return false };
        let modes = ui.session.mouse_modes();
        if !modes.any() || self.modifiers.shift_key() {
            return false;
        }
        let (row, col, _) = ui.renderer.cell_at(self.mouse.x, self.mouse.y);
        if action == MouseAction::Motion && self.mouse.last_reported_cell == Some((row, col)) {
            return true;
        }
        self.mouse.last_reported_cell = Some((row, col));
        if let Some(bytes) = encode_mouse(button, action, row, col, self.current_mods(), modes) {
            ui.session.write(&bytes);
        }
        true
    }

    /// Ctrl (Cmd on macOS) turns text under the pointer into a clickable link.
    fn link_modifier(&self) -> bool {
        if cfg!(target_os = "macos") {
            self.modifiers.super_key()
        } else {
            self.modifiers.control_key() && !self.modifiers.shift_key()
        }
    }

    /// Recomputes which link (if any) is under the pointer and updates the mouse cursor.
    fn update_hover_link(&mut self) {
        let hover = match (&self.ui, self.link_modifier()) {
            (Some(ui), true) => {
                let (row, col, _) = ui.renderer.cell_at(self.mouse.x, self.mouse.y);
                let snap = ui.session.snapshot();
                link_at(&snap, row, col)
            }
            _ => None,
        };
        if hover != self.mouse.hover_link {
            if let Some(ui) = &self.ui {
                ui.window.request_redraw(); // the frame updates the underline and the cursor
            }
            self.mouse.hover_link = hover;
        }
    }

    /// Keys while the search bar is open: type the pattern, Enter = older match,
    /// Shift+Enter = newer match, Esc = close.
    fn on_search_key(&mut self, event: &KeyEvent) {
        let (Some(ui), Some(search)) = (&self.ui, &mut self.search) else {
            return;
        };
        let mut changed = false;
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => {
                ui.session.search_clear();
                self.search = None;
                self.refresh_search_bar();
                return;
            }
            Key::Named(NamedKey::Enter) => {
                let found = ui.session.search_step(!self.modifiers.shift_key());
                search.result = Some(Ok(found));
            }
            Key::Named(NamedKey::Backspace) => {
                search.query.pop();
                changed = true;
            }
            _ => {
                if let Some(text) = &event.text
                    && !text.chars().any(char::is_control)
                {
                    search.query.push_str(text);
                    changed = true;
                }
            }
        }
        if changed {
            search.result = (!search.query.is_empty()).then(|| ui.session.search(&search.query));
        }
        self.refresh_search_bar();
    }

    fn refresh_search_bar(&mut self) {
        if let Some(ui) = &mut self.ui {
            let label = self.search.as_ref().map(SearchUi::label);
            ui.renderer.set_search_bar(label.as_deref());
            ui.window.request_redraw();
        }
    }

    /// A file dropped on the window: insert its path, quoted for the running shell.
    fn on_drop(&mut self, path: &std::path::Path) {
        let Some(ui) = &self.ui else { return };
        let program = self
            .spec
            .program
            .clone()
            .unwrap_or_else(|| hv_pty::default_shell().0);
        let quoting = hv_term::input::Quoting::for_program(&program);
        let text = hv_term::input::quote_path(&path.to_string_lossy(), quoting);
        ui.session.paste(&format!("{text} "));
        ui.window.focus_window();
    }

    fn on_mouse_button(&mut self, state: ElementState, button: MouseButton) {
        // Presses on the sidebar belong to egui; releases still end a selection drag.
        if state == ElementState::Pressed
            && self
                .ui
                .as_ref()
                .is_some_and(|ui| ui.over_chrome(self.mouse.x, self.mouse.y))
        {
            return;
        }
        if button == MouseButton::Left && state == ElementState::Pressed && self.link_modifier() {
            self.update_hover_link();
            if let (Some((.., target)), Some(ui)) = (self.mouse.hover_link.clone(), &self.ui) {
                open_link(&target, ui.session.cwd().as_deref());
                return;
            }
        }
        let term_button = match button {
            MouseButton::Left => TermButton::Left,
            MouseButton::Middle => TermButton::Middle,
            MouseButton::Right => TermButton::Right,
            _ => return,
        };
        let action = if state == ElementState::Pressed {
            MouseAction::Press
        } else {
            MouseAction::Release
        };
        if self.report_mouse(term_button, action) {
            self.mouse.reported_button = (action == MouseAction::Press).then_some(term_button);
            return;
        }
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
        if let Some(ui) = &self.ui {
            ui.window.request_redraw(); // egui hover state
        }
        self.update_hover_link();
        let held = self.mouse.reported_button.unwrap_or(TermButton::None);
        if self.report_mouse(held, MouseAction::Motion) {
            return;
        }
        if let (true, Some(ui)) = (self.mouse.selecting, &self.ui) {
            let (row, col, right) = ui.renderer.cell_at(x, y);
            ui.session.select_update(row, col, right);
            ui.window.request_redraw();
        }
    }

    fn on_wheel(&mut self, delta: MouseScrollDelta) {
        let Some(ui) = &mut self.ui else { return };
        if ui.over_chrome(self.mouse.x, self.mouse.y) {
            return;
        }
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
        if ui.session.mouse_modes().any() && !self.modifiers.shift_key() {
            let button = if whole > 0 {
                TermButton::WheelUp
            } else {
                TermButton::WheelDown
            };
            for _ in 0..whole.unsigned_abs() {
                self.report_mouse(button, MouseAction::Press);
                self.mouse.last_reported_cell = None;
            }
            return;
        }
        let Some(ui) = &mut self.ui else { return };
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

    /// Applies a changed config file. Font changes apply live; shell and scrollback apply to
    /// new sessions.
    fn reload_config(&mut self) {
        let Some(path) = &self.config_path else {
            return;
        };
        let (config, error) = hv_config::load(path);
        if let Some(err) = error {
            log::warn!("{err}");
            return; // keep the last good config
        }
        if let Some(ui) = &mut self.ui {
            apply_font(ui, &config.font);
        }
        self.config = config;
    }

    fn handle_session_events(&mut self, event_loop: &ActiveEventLoop) {
        let Some(ui) = &mut self.ui else { return };
        for event in ui.session.drain_events() {
            match event {
                SessionEvent::Title(title) => {
                    ui.window.set_title(&title);
                    ui.info.title = Some(title);
                }
                SessionEvent::ResetTitle => {
                    ui.window.set_title(APP_TITLE);
                    ui.info.title = None;
                }
                SessionEvent::Exited => event_loop.exit(),
                SessionEvent::Bell => {
                    ui.window.request_user_attention(Some(
                        winit::window::UserAttentionType::Informational,
                    ));
                    self.bell_until = Some(Instant::now() + BELL_FLASH);
                    ui.renderer.set_flash(true);
                    ui.window.request_redraw();
                }
                SessionEvent::ClipboardStore(text) => {
                    if let Err(err) = arboard::Clipboard::new().and_then(|mut c| c.set_text(text)) {
                        log::warn!("OSC 52 clipboard write failed: {err}");
                    }
                }
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
            AppEvent::ConfigChanged => self.reload_config(),
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
        // egui sees pointer and window events; the keyboard belongs to the terminal.
        if let Some(ui) = &mut self.ui
            && !matches!(
                event,
                WindowEvent::KeyboardInput { .. }
                    | WindowEvent::Ime(_)
                    | WindowEvent::RedrawRequested
            )
        {
            let _ = ui.egui.on_window_event(&ui.window, &event);
        }
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
                    // The next frame lays out the chrome and resizes the grid to what is left.
                    ui.terminal_area = None;
                    ui.window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(ui) = &mut self.ui {
                    ui.renderer.set_scale(scale_factor as f32);
                    ui.terminal_area = None;
                    ui.window.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = m.state();
                self.update_hover_link();
            }
            WindowEvent::KeyboardInput { event, .. } => self.on_key(&event),
            WindowEvent::Ime(ime) => self.on_ime(ime),
            WindowEvent::MouseWheel { delta, .. } => self.on_wheel(delta),
            WindowEvent::MouseInput { state, button, .. } => self.on_mouse_button(state, button),
            WindowEvent::DroppedFile(path) => self.on_drop(&path),
            WindowEvent::CursorMoved { position, .. } => {
                self.on_cursor_moved(position.x as f32, position.y as f32);
            }
            WindowEvent::RedrawRequested => {
                if let Some(ui) = &mut self.ui {
                    let cursor = if self.mouse.hover_link.is_some() {
                        egui::CursorIcon::PointingHand
                    } else {
                        egui::CursorIcon::Text
                    };
                    let mut frame = ui.run_chrome(self.sidebar_visible, cursor);
                    ui.session.flush_expired_sync();
                    let mut snapshot = ui.session.snapshot();
                    // Underline the link under the pointer while Ctrl/Cmd is held.
                    if let Some((row, start, end, _)) = &self.mouse.hover_link
                        && *row < snapshot.rows
                    {
                        let cols = snapshot.cols;
                        for cell in
                            &mut snapshot.cells[row * cols + start..row * cols + (*end).min(cols)]
                        {
                            cell.underline = true;
                        }
                    }
                    // Keep the IME candidate window next to the text cursor.
                    if let Some(cursor) = snapshot.cursor {
                        let (x, y, w, h) = ui.renderer.cell_rect(cursor.row, cursor.col);
                        ui.window
                            .set_ime_cursor_area(PhysicalPosition::new(x, y), PhysSize::new(w, h));
                    }
                    if self.bell_until.is_some_and(|t| Instant::now() >= t) {
                        self.bell_until = None;
                        ui.renderer.set_flash(false);
                    }
                    if let Err(err) = ui.renderer.render_with_ui(&snapshot, Some(&mut frame)) {
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
        let sync = self.ui.as_ref().and_then(|ui| ui.session.sync_deadline());
        // The sidebar's elapsed time changes every second for a minute, then every minute.
        let tick = self
            .ui
            .as_ref()
            .filter(|_| self.sidebar_visible)
            .map(|ui| crate::model::next_elapsed_change(ui.info.started, Instant::now()));
        let deadline = [sync, self.bell_until, tick].into_iter().flatten().min();
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

fn apply_font(ui: &mut Ui, font: &hv_config::Font) {
    if !ui.renderer.set_font_family(font.family.as_deref()) {
        log::warn!(
            "font {:?} is not installed; keeping the current font",
            font.family
        );
    }
    ui.renderer.set_font_size(font.size);
    ui.session.resize(grid_size(&ui.renderer));
    ui.window.request_redraw();
}

fn zoom(ui: &mut Ui, delta: f32) {
    ui.renderer.set_font_size(ui.renderer.font_size() + delta);
    ui.session.resize(grid_size(&ui.renderer));
    ui.window.request_redraw();
}

/// The link covering a cell: an OSC 8 hyperlink, or a URL / file reference in the text.
fn link_at(
    snap: &hv_term::Snapshot,
    row: usize,
    col: usize,
) -> Option<(usize, usize, usize, LinkTarget)> {
    if row >= snap.rows || col >= snap.cols {
        return None;
    }
    if let Some(uri) = snap.hyperlink(row, col) {
        let same = |c: usize| snap.cell(row, c).link == snap.cell(row, col).link;
        let start = (0..=col)
            .rev()
            .take_while(|c| same(*c))
            .last()
            .unwrap_or(col);
        let end = (col..snap.cols)
            .take_while(|c| same(*c))
            .last()
            .map_or(col + 1, |c| c + 1);
        return Some((row, start, end, LinkTarget::Url(uri.to_owned())));
    }
    let link = links::link_at(&snap.row_chars(row), col)?;
    Some((row, link.start, link.end, link.target))
}

/// Opens a link with the system handler, or a file reference in the editor at its line.
fn open_link(target: &LinkTarget, cwd: Option<&std::path::Path>) {
    let result = match target {
        LinkTarget::Url(url) if links::is_allowed_url(url) => {
            opener::open(url).map_err(|e| e.to_string())
        }
        LinkTarget::Url(url) => Err(format!(
            "refusing to open URL with an unsupported scheme: {url}"
        )),
        LinkTarget::File { path, line, column } => {
            let full = links::resolve_path(path, cwd);
            if !full.exists() {
                Err(format!("file not found: {}", full.display()))
            } else if let (Some(line), Some(code)) = (line, find_vscode()) {
                let location = format!("{}:{line}:{}", full.display(), column.unwrap_or(1));
                std::process::Command::new(code)
                    .args(["-g", &location])
                    .spawn()
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            } else {
                opener::open(&full).map_err(|e| e.to_string())
            }
        }
    };
    if let Err(err) = result {
        log::warn!("cannot open link: {err}");
    }
}

/// VS Code's command line launcher, if installed (`code -g file:line:col`).
fn find_vscode() -> Option<std::path::PathBuf> {
    let path = hv_pty::resolve_program("code");
    path.is_absolute().then_some(path)
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
