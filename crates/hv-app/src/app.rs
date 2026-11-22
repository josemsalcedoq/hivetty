//! The window: winit event loop wired to tabs of split panes (one terminal session each), the
//! egui chrome and the GPU renderer.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use hv_render::{PaneView, Renderer};
use hv_term::input::{
    Key as TermKey, Mods, MouseAction, MouseButton as TermButton, encode_key, encode_mouse,
};
use hv_term::links::{self, LinkTarget};
use hv_term::{GridSize, SelectKind, Session, SessionEvent, SpawnSpec};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{
    ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, StartCause, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
use winit::window::{Window, WindowId};

use crate::keymap::{Action, Chord, Keymap};
use crate::model::SessionInfo;
use crate::workspace::{Axis, Direction, Divider, Layout, PaneId, Tabs};

const DEFAULT_FONT_SIZE: f32 = 14.0;
const APP_TITLE: &str = "hivetty";
/// Share of a split moved by one resize shortcut.
const RESIZE_STEP: f32 = 0.05;
/// Extra pixels on each side of a divider that still grab it (6 px zone, docs/ui-spec.md 5).
const DIVIDER_SLACK: f32 = 3.0;

/// Events sent to the UI thread from other threads.
#[derive(Debug)]
pub(crate) enum AppEvent {
    /// A PTY reader produced output or an event.
    Wakeup,
    /// The config file changed on disk.
    ConfigChanged,
    /// An agent hook arrived over IPC (`hivetty hook`).
    Hook(hv_ipc::HookEvent),
    /// A screen reader asked for the accessibility tree or an action (SP-3, F4-25).
    AccessKit(egui_winit::accesskit_winit::Event),
    /// A menu bar item was chosen (macOS); the id is the action name.
    #[cfg(target_os = "macos")]
    Menu(String),
}

impl From<egui_winit::accesskit_winit::Event> for AppEvent {
    fn from(event: egui_winit::accesskit_winit::Event) -> Self {
        Self::AccessKit(event)
    }
}

/// Runs the terminal window until the last pane exits or the window is closed.
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
    let explicit_program = spec.program.is_some();
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
    // Agent hooks reach the app over local IPC; panes find it through the environment.
    let ipc = {
        let proxy = std::sync::Mutex::new(proxy.clone());
        hv_ipc::Server::start(move |message| {
            let hv_ipc::Message::Hook(hook) = message;
            if let Ok(proxy) = proxy.lock() {
                let _ = proxy.send_event(AppEvent::Hook(hook));
            }
        })
    };
    match &ipc {
        Ok(server) => spec.env.extend(server.endpoint().env()),
        Err(err) => log::warn!("agent hooks disabled: cannot start IPC: {err}"),
    }
    // Restore and save the layout only for a plain launch (not `hivetty -e program`).
    let mut app = App::new(spec, proxy);
    if !explicit_program {
        app.layout_path = crate::saved::layout_path();
        app.restore = app
            .layout_path
            .as_deref()
            .and_then(crate::saved::SavedLayout::load);
    }
    app.keymap = keymap_from(&config);
    app.config = config;
    app.config_path = config_path;
    app.record = record;
    event_loop.run_app(&mut app).context("event loop failed")?;
    if let Some(err) = app.fatal.take() {
        return Err(err);
    }
    Ok(())
}

/// The default shortcuts with the user's `[keybindings]` applied.
fn keymap_from(config: &hv_config::Config) -> Keymap {
    let (keymap, errors) = Keymap::defaults().with_overrides(&config.keybindings);
    for err in errors {
        log::warn!("keybindings: {err}");
    }
    keymap
}

/// One terminal pane.
struct Pane {
    session: Session,
    info: SessionInfo,
    /// Where it was laid out last, to resize the PTY only when it changes.
    area: Option<[f32; 4]>,
}

struct Ui {
    window: Arc<Window>,
    renderer: Renderer,
    /// Sidebar, tab bar and other egui chrome (SP-3, ADR 0003).
    chrome: hv_ui::Chrome,
    egui: egui_winit::State,
    panes: HashMap<PaneId, Pane>,
    tabs: Tabs,
    /// Pane areas and dividers of the active tab, from the last frame.
    layout: Layout,
    /// Space egui left for the terminals last frame, in physical pixels.
    terminal_area: Option<[f32; 4]>,
    /// Sidebar row index → pane, from the last frame.
    row_panes: Vec<PaneId>,
    /// When egui asked to be painted again (animations such as the tab spinner).
    repaint_at: Option<Instant>,
}

impl Ui {
    fn focused_id(&self) -> Option<PaneId> {
        self.tabs.focused()
    }

    fn focused(&self) -> Option<&Pane> {
        self.panes.get(&self.focused_id()?)
    }

    fn pane(&self, id: PaneId) -> Option<&Pane> {
        self.panes.get(&id)
    }

    /// Whether a point (physical pixels) is over the chrome rather than the terminals.
    fn over_chrome(&self, x: f32, y: f32) -> bool {
        self.terminal_area
            .is_some_and(|[ax, ay, aw, ah]| x < ax || y < ay || x >= ax + aw || y >= ay + ah)
    }

    /// Width of the line between panes.
    fn gap(&self) -> f32 {
        (self.window.scale_factor() as f32).round().max(1.0)
    }

    /// Lays out the active tab and resizes the PTYs whose area changed.
    fn relayout(&mut self) {
        let (Some(area), Some(tab)) = (self.terminal_area, self.tabs.active()) else {
            self.layout = Layout::default();
            return;
        };
        self.layout = tab.layout(area, self.gap());
        for (id, area) in &self.layout.panes {
            if let Some(pane) = self.panes.get_mut(id)
                && pane.area != Some(*area)
            {
                pane.area = Some(*area);
                pane.session.resize(grid_size_in(&self.renderer, *area));
            }
        }
    }

    /// Resizes every laid-out pane again (after a font or DPI change).
    fn resize_all(&mut self) {
        for pane in self.panes.values_mut() {
            pane.area = None;
        }
        self.relayout();
    }

    /// The sidebar and tab bar contents.
    fn chrome_model(&mut self, sidebar_visible: bool, keymap: &Keymap) -> hv_ui::ChromeModel {
        let focused = self.focused_id();
        let mut sessions = Vec::new();
        self.row_panes.clear();
        let mut tabs = Vec::new();
        for (index, tab) in self.tabs.list.iter().enumerate() {
            let mut most_urgent = hv_ui::RowState::Idle;
            for id in tab.root.panes() {
                let Some(pane) = self.panes.get(&id) else {
                    continue;
                };
                let mut row = crate::model::session_row(&pane.info, pane.session.cwd().as_deref());
                row.focused = Some(id) == focused;
                most_urgent = more_urgent(most_urgent, row.state);
                sessions.push(row);
                self.row_panes.push(id);
            }
            let title = tab.title_override.clone().unwrap_or_else(|| {
                self.panes
                    .get(&tab.focused)
                    .and_then(|p| p.info.title.clone())
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| hv_ui::strings::DEFAULT_TITLE.to_owned())
            });
            let panes: Vec<&Pane> = tab
                .root
                .panes()
                .iter()
                .filter_map(|id| self.panes.get(id))
                .collect();
            let mut item = hv_ui::TabItem::new(title, most_urgent, index == self.tabs.active);
            item.unseen = panes.iter().any(|p| p.info.unseen);
            item.bell = panes.iter().any(|p| p.info.bell);
            // The focused pane's progress, else any pane's.
            let progress = self
                .panes
                .get(&tab.focused)
                .and_then(|p| p.info.progress)
                .or_else(|| panes.iter().find_map(|p| p.info.progress));
            if let Some((state, value)) = progress {
                item.progress = Some(if state == 3 {
                    1.0
                } else {
                    f32::from(value) / 100.0
                });
                item.progress_error = state == 2;
            }
            tabs.push(item);
        }
        // macOS has the system menu bar; elsewhere the same entries live in the ☰ menu.
        let menu = if cfg!(target_os = "macos") {
            Vec::new()
        } else {
            crate::keymap::MENU_SECTIONS
                .iter()
                .map(|section| {
                    section
                        .iter()
                        .filter_map(|name| crate::keymap::menu_item(keymap, name))
                        .collect()
                })
                .collect()
        };
        hv_ui::ChromeModel {
            sessions,
            sidebar_visible,
            tabs,
            menu,
        }
    }
}

/// Which of two states the tab dot shows: the one that needs the user most.
fn more_urgent(a: hv_ui::RowState, b: hv_ui::RowState) -> hv_ui::RowState {
    use hv_ui::RowState::{Done, Error, Idle, Waiting, Working};
    let rank = |s| match s {
        Waiting => 4,
        Error => 3,
        Done => 2,
        Working => 1,
        Idle => 0,
    };
    if rank(b) > rank(a) { b } else { a }
}

struct App {
    spec: SpawnSpec,
    proxy: EventLoopProxy<AppEvent>,
    wake_pending: Arc<AtomicBool>,
    ui: Option<Ui>,
    next_pane: PaneId,
    modifiers: ModifiersState,
    scroll_remainder: f32,
    fatal: Option<anyhow::Error>,
    mouse: MouseState,
    /// An input method is composing text; raw key presses belong to it, not to the program.
    ime_composing: bool,
    config: hv_config::Config,
    config_path: Option<std::path::PathBuf>,
    keymap: Keymap,
    /// Scrollback search in progress in the focused pane (F1-21).
    search: Option<SearchUi>,
    /// `--record` target (first pane only).
    record: Option<std::path::PathBuf>,
    /// Visual bell: flash the window until this instant (F1-23).
    bell_until: Option<Instant>,
    /// Sidebar shown (toggled with Ctrl+Shift+B / Cmd+B).
    sidebar_visible: bool,
    /// An action chosen in the chrome (palette, tabs) during a frame, run afterwards where the
    /// event loop is available.
    pending_action: Option<Action>,
    /// Layout to reopen at start (F2-08).
    restore: Option<crate::saved::SavedLayout>,
    /// Where the layout is saved on close; `None` for `hivetty -e program`.
    layout_path: Option<std::path::PathBuf>,
    /// The macOS menu bar (kept alive while shown).
    #[cfg(target_os = "macos")]
    menu: Option<muda::Menu>,
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

/// Pointer position and click tracking.
#[derive(Default)]
struct MouseState {
    x: f32,
    y: f32,
    /// Pane that owns the current press (selection drag or reported button).
    pane: Option<PaneId>,
    selecting: bool,
    last_click: Option<(Instant, usize, usize)>,
    click_count: u8,
    /// Link under the pointer while the link modifier is held: `(pane, row, start, end, target)`.
    hover_link: Option<(PaneId, usize, usize, usize, LinkTarget)>,
    /// Button held while reporting to the program, and the last cell reported.
    reported_button: Option<TermButton>,
    last_reported_cell: Option<(usize, usize)>,
    /// Divider being dragged.
    dragging: Option<Divider>,
    /// Divider under the pointer (for the resize cursor).
    over_divider: Option<Axis>,
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
            next_pane: 1,
            modifiers: ModifiersState::empty(),
            scroll_remainder: 0.0,
            fatal: None,
            mouse: MouseState::default(),
            ime_composing: false,
            config: hv_config::Config::default(),
            config_path: None,
            keymap: Keymap::defaults(),
            search: None,
            record: None,
            bell_until: None,
            sidebar_visible: true,
            pending_action: None,
            restore: None,
            layout_path: None,
            #[cfg(target_os = "macos")]
            menu: None,
        }
    }

    fn create_ui(&mut self, event_loop: &ActiveEventLoop) -> Result<Ui> {
        let restore = self
            .restore
            .take()
            .filter(|_| self.config.window.restore_layout);
        let geometry = restore
            .as_ref()
            .and_then(|l| l.window)
            .filter(|_| self.config.window.remember_geometry);
        let mut attrs = Window::default_attributes()
            .with_title(APP_TITLE)
            .with_inner_size(LogicalSize::new(1000.0, 640.0))
            // AccessKit must be attached before the window is first shown.
            .with_visible(false);
        if let Some(g) = geometry {
            attrs = attrs
                .with_inner_size(PhysicalSize::new(g.width.max(320), g.height.max(200)))
                .with_maximized(g.maximized);
            if let (Some(x), Some(y)) = (g.x, g.y)
                && on_some_monitor(event_loop, x, y)
            {
                attrs = attrs.with_position(PhysicalPosition::new(x, y));
            }
        }
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
        let chrome = hv_ui::Chrome::new();
        let mut egui = egui_winit::State::new(
            chrome.context().clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(scale),
            None,
            None,
        );
        egui.init_accesskit(event_loop, &window, self.proxy.clone());
        #[cfg(target_os = "macos")]
        {
            self.menu = crate::menu_mac::install(&self.keymap, self.proxy.clone());
        }
        window.set_visible(true);
        let mut ui = Ui {
            window,
            renderer,
            chrome,
            egui,
            panes: HashMap::new(),
            tabs: Tabs::default(),
            layout: Layout::default(),
            terminal_area: None,
            row_panes: Vec::new(),
            repaint_at: None,
        };
        apply_font(&mut ui, &self.config.font);
        if let Some(layout) = restore {
            let mut spawned = Vec::new();
            let tabs = layout.restore(|cwd| match self.spawn_pane(&ui, cwd.map(Into::into)) {
                Ok((id, pane)) => {
                    spawned.push((id, pane));
                    Some(id)
                }
                Err(err) => {
                    log::warn!("cannot reopen a pane: {err:#}");
                    None
                }
            });
            ui.tabs = tabs;
            ui.panes.extend(spawned);
            if !ui.tabs.list.is_empty() {
                return Ok(ui);
            }
        }
        let first = self.spawn_pane(&ui, None)?;
        if let Some(path) = self.record.take() {
            first
                .1
                .session
                .start_recording(&path, grid_size_in(&ui.renderer, [0.0, 0.0, 1.0, 1.0]))?;
        }
        ui.panes.insert(first.0, first.1);
        ui.tabs.open(first.0);
        Ok(ui)
    }

    /// Starts a new terminal in `cwd` (or the default directory).
    fn spawn_pane(&mut self, ui: &Ui, cwd: Option<std::path::PathBuf>) -> Result<(PaneId, Pane)> {
        let id = self.next_pane;
        self.next_pane += 1;
        let mut spec = self.spec.clone();
        if cwd.is_some() {
            spec.cwd = cwd;
        }
        spec.env
            .push((hv_ipc::ENV_SESSION.to_owned(), id.to_string()));
        let proxy = self.proxy.clone();
        let pending = Arc::clone(&self.wake_pending);
        let wake = move || {
            // Coalesce: one pending wakeup is enough, the UI drains every pane at once.
            if !pending.swap(true, Ordering::AcqRel) {
                let _ = proxy.send_event(AppEvent::Wakeup);
            }
        };
        let area =
            ui.terminal_area
                .unwrap_or([0.0, 0.0, ui.window.inner_size().width as f32, 600.0]);
        let session = Session::spawn(
            &spec,
            grid_size_in(&ui.renderer, area),
            self.config.terminal.scrollback_lines,
            wake,
        )?;
        Ok((
            id,
            Pane {
                session,
                info: SessionInfo::new(),
                area: None,
            },
        ))
    }

    /// The pane under the pointer, or the one that owns the current press.
    fn pointer_pane(&self) -> Option<(PaneId, [f32; 4])> {
        let ui = self.ui.as_ref()?;
        let id = self
            .mouse
            .pane
            .or_else(|| ui.layout.pane_at(self.mouse.x, self.mouse.y))?;
        Some((id, ui.layout.area(id)?))
    }

    fn run_action(&mut self, action: Action, event_loop: &ActiveEventLoop) {
        let Some(ui) = &mut self.ui else { return };
        match action {
            Action::NewTab | Action::SplitRight | Action::SplitDown => {
                let cwd = ui.focused().and_then(|p| p.session.cwd());
                let Some(ui) = self.ui.take() else { return };
                let spawned = self.spawn_pane(&ui, cwd);
                let mut ui = ui;
                match spawned {
                    Ok((id, pane)) => {
                        ui.panes.insert(id, pane);
                        if action == Action::NewTab {
                            ui.tabs.open(id);
                        } else if let Some(tab) = ui.tabs.active_mut() {
                            let axis = if action == Action::SplitRight {
                                Axis::Horizontal
                            } else {
                                Axis::Vertical
                            };
                            tab.root.split(tab.focused, id, axis);
                            tab.focused = id;
                            tab.zoomed = false;
                        }
                    }
                    Err(err) => log::error!("cannot start a new terminal: {err:#}"),
                }
                self.ui = Some(ui);
                self.focus_changed();
            }
            Action::ClosePane => {
                if let Some(id) = ui.focused_id() {
                    self.close_pane(id, event_loop);
                }
            }
            Action::FocusPane(dir) => {
                if let Some(from) = ui.focused_id()
                    && let Some(to) = crate::workspace::neighbor(&ui.layout, from, dir)
                {
                    ui.tabs.focus_pane(to);
                    self.focus_changed();
                }
            }
            Action::ResizePane(dir) => {
                if let Some(tab) = ui.tabs.active_mut() {
                    let (axis, delta) = match dir {
                        Direction::Left => (Axis::Horizontal, -RESIZE_STEP),
                        Direction::Right => (Axis::Horizontal, RESIZE_STEP),
                        Direction::Up => (Axis::Vertical, -RESIZE_STEP),
                        Direction::Down => (Axis::Vertical, RESIZE_STEP),
                    };
                    let focused = tab.focused;
                    tab.root.resize(focused, axis, delta);
                    ui.relayout();
                }
            }
            Action::ToggleZoom => {
                if let Some(tab) = ui.tabs.active_mut() {
                    tab.zoomed = !tab.zoomed;
                    ui.relayout();
                }
            }
            Action::NextTab => {
                ui.tabs.cycle(1);
                self.focus_changed();
            }
            Action::PreviousTab => {
                ui.tabs.cycle(-1);
                self.focus_changed();
            }
            Action::GoToTab(n) => {
                if n < ui.tabs.list.len() {
                    ui.tabs.active = n;
                    self.focus_changed();
                }
            }
            Action::ToggleSidebar => self.sidebar_visible = !self.sidebar_visible,
            Action::Find => {
                self.search = Some(SearchUi::default());
                self.refresh_search_bar();
            }
            Action::Copy => {
                if let Some(pane) = ui.focused() {
                    copy_selection(&pane.session);
                }
            }
            Action::Paste => {
                if let Some(pane) = ui.focused() {
                    paste_clipboard(&pane.session);
                }
            }
            Action::FontBigger => zoom(ui, 1.0),
            Action::FontSmaller => zoom(ui, -1.0),
            Action::FontReset => zoom(ui, DEFAULT_FONT_SIZE - ui.renderer.font_size()),
            Action::CommandPalette => {
                let items = crate::keymap::PALETTE_ACTIONS
                    .iter()
                    .filter(|name| **name != "command_palette")
                    .filter_map(|name| crate::keymap::menu_item(&self.keymap, name))
                    .collect();
                ui.chrome.open_palette(items);
            }
            Action::SendText(text) => {
                if let Some(pane) = ui.focused() {
                    pane.session.scroll_to_bottom();
                    pane.session.write(text.as_bytes());
                }
            }
            Action::None => {}
        }
        if let Some(ui) = &self.ui {
            ui.window.request_redraw();
        }
    }

    /// Writes the current layout for the next launch.
    fn save_layout(&self) {
        let (Some(ui), Some(path)) = (&self.ui, &self.layout_path) else {
            return;
        };
        let size = ui.window.inner_size();
        let position = ui.window.outer_position().ok();
        let window = crate::saved::SavedWindow {
            width: size.width,
            height: size.height,
            x: position.map(|p| p.x),
            y: position.map(|p| p.y),
            maximized: ui.window.is_maximized(),
        };
        let layout = crate::saved::SavedLayout::capture(&ui.tabs, Some(window), |id| {
            ui.panes.get(&id).and_then(|p| p.session.cwd())
        });
        // Off the UI thread; joined because the process is about to exit.
        let path = path.clone();
        let writer = std::thread::spawn(move || layout.save(&path));
        match writer.join() {
            Ok(Err(err)) => log::warn!("cannot save the layout: {err}"),
            Err(_) => log::warn!("cannot save the layout"),
            Ok(Ok(())) => {}
        }
    }

    /// Kills a pane and removes it; exits when it was the last one.
    fn close_pane(&mut self, id: PaneId, event_loop: &ActiveEventLoop) {
        let Some(ui) = &mut self.ui else { return };
        if let Some(pane) = ui.panes.remove(&id) {
            pane.session.kill();
        }
        if ui.tabs.remove_pane(id) {
            event_loop.exit();
            return;
        }
        if self.mouse.pane == Some(id) {
            self.mouse.pane = None;
            self.mouse.selecting = false;
        }
        self.focus_changed();
    }

    /// Something else has the focus now: close the search, retitle the window, relayout.
    fn focus_changed(&mut self) {
        if self.search.is_some() {
            if let Some(pane) = self.ui.as_ref().and_then(Ui::focused) {
                pane.session.search_clear();
            }
            self.search = None;
            self.refresh_search_bar();
        }
        self.mouse.hover_link = None;
        if let Some(ui) = &mut self.ui {
            // Looking at a pane clears its indicators.
            if let Some(id) = ui.focused_id()
                && let Some(pane) = ui.panes.get_mut(&id)
            {
                pane.info.unseen = false;
                pane.info.bell = false;
            }
            let title = ui
                .focused()
                .and_then(|p| p.info.title.clone())
                .unwrap_or_else(|| APP_TITLE.to_owned());
            ui.window.set_title(&title);
            ui.renderer.set_preedit(None);
            ui.relayout();
            ui.window.request_redraw();
        }
    }

    fn on_key(&mut self, event: &KeyEvent, event_loop: &ActiveEventLoop) {
        if event.state != ElementState::Pressed || self.ime_composing || self.ui.is_none() {
            return;
        }
        let ctx = crate::keymap::Context {
            search_open: self.search.is_some(),
            agent: None,
        };
        if let Some(chord) = pressed_chord(event, self.modifiers)
            && let Some(action) = self.keymap.lookup(&chord, &ctx)
        {
            self.run_action(action, event_loop);
            return;
        }
        if self.search.is_some() {
            self.on_search_key(event);
            return;
        }
        let Some(ui) = &mut self.ui else { return };
        let Some(pane) = ui.focused() else { return };
        let session = &pane.session;
        let m = self.modifiers;
        // Windows convention: Ctrl+C copies when there is a selection, otherwise it is ^C.
        if cfg!(windows)
            && m.control_key()
            && !m.shift_key()
            && !m.alt_key()
            && matches!(&event.logical_key, Key::Character(c) if c.eq_ignore_ascii_case("c"))
            && session.selection_text().is_some()
        {
            copy_selection(session);
            session.select_clear();
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
        if let Some(bytes) = encode_key(&key, mods, session.key_modes()) {
            session.select_clear();
            session.scroll_to_bottom();
            session.write(&bytes);
        }
    }

    fn on_ime(&mut self, ime: Ime) {
        let Some(ui) = &mut self.ui else { return };
        match ime {
            Ime::Preedit(text, _) => {
                self.ime_composing = !text.is_empty();
                ui.renderer.set_preedit(Some(&text));
            }
            Ime::Commit(text) => {
                self.ime_composing = false;
                ui.renderer.set_preedit(None);
                if let Some(pane) = ui.focused() {
                    pane.session.select_clear();
                    pane.session.scroll_to_bottom();
                    pane.session.write(text.as_bytes());
                }
            }
            Ime::Enabled | Ime::Disabled => {
                self.ime_composing = false;
                ui.renderer.set_preedit(None);
            }
        }
        ui.window.request_redraw();
    }

    fn current_mods(&self) -> Mods {
        Mods {
            shift: self.modifiers.shift_key(),
            ctrl: self.modifiers.control_key(),
            alt: self.modifiers.alt_key(),
        }
    }

    /// Sends a mouse event to the program under the pointer if it asked for them (and Shift
    /// is not held). Returns true if the event was consumed by reporting.
    fn report_mouse(&mut self, button: TermButton, action: MouseAction) -> bool {
        let Some((id, area)) = self.pointer_pane() else {
            return false;
        };
        let Some(ui) = &self.ui else { return false };
        let Some(pane) = ui.pane(id) else {
            return false;
        };
        let modes = pane.session.mouse_modes();
        if !modes.any() || self.modifiers.shift_key() {
            return false;
        }
        let (row, col, _) = ui.renderer.cell_at_in(area, self.mouse.x, self.mouse.y);
        if action == MouseAction::Motion && self.mouse.last_reported_cell == Some((row, col)) {
            return true;
        }
        self.mouse.last_reported_cell = Some((row, col));
        if let Some(bytes) = encode_mouse(button, action, row, col, self.current_mods(), modes) {
            pane.session.write(&bytes);
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

    /// Recomputes which link (if any) is under the pointer.
    fn update_hover_link(&mut self) {
        let hover = match (self.link_modifier(), self.pointer_pane(), &self.ui) {
            (true, Some((id, area)), Some(ui)) => ui.pane(id).and_then(|pane| {
                let (row, col, _) = ui.renderer.cell_at_in(area, self.mouse.x, self.mouse.y);
                let snap = pane.session.snapshot();
                link_at(&snap, row, col).map(|(r, s, e, t)| (id, r, s, e, t))
            }),
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
        let Some(pane) = ui.focused() else { return };
        let session = &pane.session;
        let mut changed = false;
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => {
                session.search_clear();
                self.search = None;
                self.refresh_search_bar();
                return;
            }
            Key::Named(NamedKey::Enter) => {
                let found = session.search_step(!self.modifiers.shift_key());
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
            search.result = (!search.query.is_empty()).then(|| session.search(&search.query));
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

    /// A file dropped on the window: insert its path, quoted for the shell, into the pane under
    /// the pointer.
    fn on_drop(&mut self, path: &std::path::Path) {
        let target = self.pointer_pane().map(|(id, _)| id);
        let Some(ui) = &self.ui else { return };
        let Some(pane) = target.and_then(|id| ui.pane(id)).or_else(|| ui.focused()) else {
            return;
        };
        let program = self
            .spec
            .program
            .clone()
            .unwrap_or_else(|| hv_pty::default_shell().0);
        let quoting = hv_term::input::Quoting::for_program(&program);
        let text = hv_term::input::quote_path(&path.to_string_lossy(), quoting);
        pane.session.paste(&format!("{text} "));
        ui.window.focus_window();
    }

    fn on_mouse_button(&mut self, state: ElementState, button: MouseButton) {
        let Some(ui) = &mut self.ui else { return };
        if state == ElementState::Released {
            self.mouse.dragging = None;
        }
        if state == ElementState::Pressed {
            // Presses on the sidebar and tab bar belong to egui.
            if ui.over_chrome(self.mouse.x, self.mouse.y) {
                return;
            }
            if button == MouseButton::Left
                && let Some(divider) =
                    ui.layout
                        .divider_at(self.mouse.x, self.mouse.y, DIVIDER_SLACK)
            {
                self.mouse.dragging = Some(divider.clone());
                return;
            }
            // A click focuses the pane under the pointer.
            if let Some(id) = ui.layout.pane_at(self.mouse.x, self.mouse.y)
                && ui.focused_id() != Some(id)
            {
                ui.tabs.focus_pane(id);
                self.focus_changed();
            }
            self.mouse.pane = self
                .ui
                .as_ref()
                .and_then(|ui| ui.layout.pane_at(self.mouse.x, self.mouse.y));
        }
        if button == MouseButton::Left && state == ElementState::Pressed && self.link_modifier() {
            self.update_hover_link();
            if let (Some((id, .., target)), Some(ui)) = (self.mouse.hover_link.clone(), &self.ui) {
                let cwd = ui.pane(id).and_then(|p| p.session.cwd());
                open_link(&target, cwd.as_deref());
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
            if action == MouseAction::Release {
                self.mouse.pane = None;
            }
            return;
        }
        let Some((id, area)) = self.pointer_pane() else {
            return;
        };
        let Some(ui) = &mut self.ui else { return };
        let Some(pane) = ui.panes.get(&id) else {
            return;
        };
        let session = &pane.session;
        if button != MouseButton::Left {
            return;
        }
        if state == ElementState::Pressed
            && ui.renderer.hits_jump_to_bottom(self.mouse.x, self.mouse.y)
        {
            session.scroll_to_bottom();
            ui.window.request_redraw();
            return;
        }
        // Programs that asked for mouse reporting get clicks (F1-16); Shift forces selection.
        if session.mouse_mode() && !self.modifiers.shift_key() {
            return;
        }
        match state {
            ElementState::Pressed => {
                let (row, col, right) = ui.renderer.cell_at_in(area, self.mouse.x, self.mouse.y);
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
                session.select_start(row, col, right, kind);
                if kind != SelectKind::Simple && kind != SelectKind::Block {
                    session.select_update(row, col, right);
                }
                self.mouse.selecting = true;
            }
            ElementState::Released => {
                self.mouse.selecting = false;
                self.mouse.pane = None;
                // A plain click without dragging leaves no selection behind.
                if self.mouse.click_count == 1 && session.selection_text().is_none() {
                    session.select_clear();
                }
            }
        }
        ui.window.request_redraw();
    }

    fn on_cursor_moved(&mut self, x: f32, y: f32) {
        self.mouse.x = x;
        self.mouse.y = y;
        let Some(ui) = &mut self.ui else { return };
        ui.window.request_redraw(); // egui hover state, divider cursor
        if let Some(divider) = &self.mouse.dragging {
            let [sx, sy, sw, sh] = divider.span;
            let ratio = match divider.axis {
                Axis::Horizontal => (x - sx) / sw.max(1.0),
                Axis::Vertical => (y - sy) / sh.max(1.0),
            };
            if let Some(tab) = ui.tabs.active_mut() {
                tab.root.set_ratio(&divider.path, ratio);
            }
            ui.relayout();
            return;
        }
        self.mouse.over_divider = ui.layout.divider_at(x, y, DIVIDER_SLACK).map(|d| d.axis);
        self.update_hover_link();
        let held = self.mouse.reported_button.unwrap_or(TermButton::None);
        if self.report_mouse(held, MouseAction::Motion) {
            return;
        }
        if self.mouse.selecting
            && let Some((id, area)) = self.pointer_pane()
            && let Some(ui) = &self.ui
            && let Some(pane) = ui.pane(id)
        {
            let (row, col, right) = ui.renderer.cell_at_in(area, x, y);
            pane.session.select_update(row, col, right);
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
        let Some((id, _)) = self.pointer_pane() else {
            return;
        };
        let reporting = self
            .ui
            .as_ref()
            .and_then(|ui| ui.pane(id))
            .is_some_and(|p| p.session.mouse_modes().any() && !self.modifiers.shift_key());
        if reporting {
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
        let Some(ui) = &self.ui else { return };
        let Some(pane) = ui.pane(id) else { return };
        let session = &pane.session;
        if session.alt_screen() {
            // Full-screen programs (vim, less, agent TUIs) get arrow keys, like xterm's alternate scroll.
            let key = if whole > 0 {
                TermKey::Up
            } else {
                TermKey::Down
            };
            if let Some(bytes) = encode_key(&key, Mods::default(), session.key_modes()) {
                session.write(&bytes.repeat(whole.unsigned_abs() as usize));
            }
        } else {
            session.scroll(whole);
        }
        ui.window.request_redraw();
    }

    /// Applies a changed config file. Font and shortcut changes apply live; shell and
    /// scrollback apply to new panes.
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
        self.keymap = keymap_from(&config);
        #[cfg(target_os = "macos")]
        {
            // Shortcuts shown in the menu bar follow the keymap.
            self.menu = crate::menu_mac::install(&self.keymap, self.proxy.clone());
        }
        self.config = config;
    }

    fn handle_session_events(&mut self, event_loop: &ActiveEventLoop) {
        let Some(ui) = &mut self.ui else { return };
        let focused = ui.focused_id();
        let mut exited = Vec::new();
        let mut bell = false;
        for (id, pane) in &mut ui.panes {
            for event in pane.session.drain_events() {
                match event {
                    SessionEvent::Title(title) => {
                        if Some(*id) == focused {
                            ui.window.set_title(&title);
                        }
                        pane.info.title = Some(title);
                    }
                    SessionEvent::ResetTitle => {
                        if Some(*id) == focused {
                            ui.window.set_title(APP_TITLE);
                        }
                        pane.info.title = None;
                    }
                    SessionEvent::Exited => exited.push(*id),
                    SessionEvent::Bell => {
                        bell = true;
                        if Some(*id) != focused {
                            pane.info.bell = true;
                        }
                    }
                    SessionEvent::ClipboardStore(text) => {
                        if let Err(err) =
                            arboard::Clipboard::new().and_then(|mut c| c.set_text(text))
                        {
                            log::warn!("OSC 52 clipboard write failed: {err}");
                        }
                    }
                    SessionEvent::Term(term_event) => {
                        log::debug!("terminal event in pane {id}: {term_event:?}");
                        if pane.info.on_term_event(&term_event)
                            && Some(*id) != focused
                            && pane.info.needs_attention()
                        {
                            pane.info.unseen = true;
                        }
                    }
                }
            }
        }
        if bell {
            ui.window
                .request_user_attention(Some(winit::window::UserAttentionType::Informational));
            self.bell_until = Some(Instant::now() + BELL_FLASH);
            ui.renderer.set_flash(true);
        }
        ui.window.request_redraw();
        for id in exited {
            self.close_pane(id, event_loop);
        }
    }

    fn redraw(&mut self) {
        let cursor = if self.mouse.dragging.is_some() || self.mouse.over_divider.is_some() {
            match self
                .mouse
                .dragging
                .as_ref()
                .map(|d| d.axis)
                .or(self.mouse.over_divider)
            {
                Some(Axis::Horizontal) => egui::CursorIcon::ResizeHorizontal,
                _ => egui::CursorIcon::ResizeVertical,
            }
        } else if self.mouse.hover_link.is_some() {
            egui::CursorIcon::PointingHand
        } else {
            egui::CursorIcon::Text
        };
        let Some(ui) = &mut self.ui else { return };
        let (mut frame, out) = run_chrome(ui, self.sidebar_visible, &self.keymap, cursor);
        // Clicks in the chrome.
        if let Some(row) = out.clicked_session
            && let Some(&id) = ui.row_panes.get(row)
        {
            ui.tabs.focus_pane(id);
        }
        let tab_action = out.tab_action;
        if self.bell_until.is_some_and(|t| Instant::now() >= t) {
            self.bell_until = None;
            ui.renderer.set_flash(false);
        }
        let focused = ui.focused_id();
        let mut snapshots = Vec::new();
        for (id, area) in &ui.layout.panes {
            let Some(pane) = ui.panes.get(id) else {
                continue;
            };
            pane.session.flush_expired_sync();
            let mut snap = pane.session.snapshot();
            // Underline the link under the pointer while Ctrl/Cmd is held.
            if let Some((link_pane, row, start, end, _)) = &self.mouse.hover_link
                && link_pane == id
                && *row < snap.rows
            {
                let cols = snap.cols;
                for cell in &mut snap.cells[row * cols + start..row * cols + (*end).min(cols)] {
                    cell.underline = true;
                }
            }
            // Keep the IME candidate window next to the focused text cursor.
            if Some(*id) == focused
                && let Some(cursor) = snap.cursor
            {
                let (x, y, w, h) = ui.renderer.cell_rect_in(*area, cursor.row, cursor.col);
                ui.window
                    .set_ime_cursor_area(PhysicalPosition::new(x, y), PhysicalSize::new(w, h));
            }
            snapshots.push((*id, *area, snap));
        }
        let views: Vec<PaneView<'_>> = snapshots
            .iter()
            .map(|(id, area, snap)| PaneView {
                id: *id,
                snap,
                area: *area,
                focused: Some(*id) == focused,
            })
            .collect();
        let dividers: Vec<[f32; 4]> = ui.layout.dividers.iter().map(|d| d.rect).collect();
        if let Err(err) = ui
            .renderer
            .render_panes(&views, &dividers, Some(&mut frame))
        {
            log::error!("render failed: {err:#}");
        }
        drop(views);
        if out.clicked_session.is_some() {
            self.focus_changed();
        }
        if let Some(action) = tab_action {
            self.on_tab_action(action);
        }
        if let Some(id) = out.palette_choice {
            self.pending_action = crate::keymap::PALETTE_ACTIONS
                .get(id)
                .and_then(|name| Action::parse(name));
            if let Some(ui) = &self.ui {
                ui.window.request_redraw();
            }
        }
    }

    fn on_tab_action(&mut self, action: hv_ui::TabAction) {
        let Some(ui) = &mut self.ui else { return };
        match action {
            hv_ui::TabAction::Select(index) => {
                ui.tabs.active = index.min(ui.tabs.list.len().saturating_sub(1));
                self.focus_changed();
            }
            hv_ui::TabAction::Close(index) => {
                let panes = ui
                    .tabs
                    .list
                    .get(index)
                    .map(|t| t.root.panes())
                    .unwrap_or_default();
                // Closing needs the event loop only to exit; the last pane exiting does that
                // through the normal Exited path.
                for id in panes {
                    if let Some(pane) = ui.panes.remove(&id) {
                        pane.session.kill();
                    }
                    if ui.tabs.remove_pane(id) {
                        // Last tab: keep the window until the shell's exit event arrives.
                        break;
                    }
                }
                self.focus_changed();
            }
            hv_ui::TabAction::Rename(index, name) => {
                if let Some(tab) = ui.tabs.list.get_mut(index) {
                    tab.title_override = (!name.is_empty()).then_some(name);
                }
            }
            hv_ui::TabAction::Move { from, to } => {
                if from < ui.tabs.list.len() && to < ui.tabs.list.len() {
                    let active = ui.tabs.list.get(ui.tabs.active).map(|t| t.focused);
                    let tab = ui.tabs.list.remove(from);
                    ui.tabs.list.insert(to, tab);
                    if let Some(pane) = active {
                        ui.tabs.focus_pane(pane);
                    }
                }
            }
            hv_ui::TabAction::Menu(id) => {
                self.pending_action = crate::keymap::PALETTE_ACTIONS
                    .get(id)
                    .and_then(|name| Action::parse(name));
                ui.window.request_redraw();
            }
            hv_ui::TabAction::New => {
                let cwd = ui.focused().and_then(|p| p.session.cwd());
                let Some(ui) = self.ui.take() else { return };
                let spawned = self.spawn_pane(&ui, cwd);
                let mut ui = ui;
                match spawned {
                    Ok((id, pane)) => {
                        ui.panes.insert(id, pane);
                        ui.tabs.open(id);
                    }
                    Err(err) => log::error!("cannot start a new terminal: {err:#}"),
                }
                self.ui = Some(ui);
                self.focus_changed();
            }
        }
    }
}

/// Runs the chrome for this frame, lays the terminals out in the space it leaves and returns
/// what to paint plus what the user clicked.
fn run_chrome(
    ui: &mut Ui,
    sidebar_visible: bool,
    keymap: &Keymap,
    cursor: egui::CursorIcon,
) -> (hv_render::UiFrame, hv_ui::ChromeOutput) {
    let input = ui.egui.take_egui_input(&ui.window);
    let model = ui.chrome_model(sidebar_visible, keymap);
    let mut out = ui.chrome.run(input, &model);
    if ui.terminal_area != Some(out.terminal_area) {
        ui.terminal_area = Some(out.terminal_area);
        ui.renderer.set_grid_area(Some(out.terminal_area));
    }
    ui.relayout();
    let ppp = out.full.pixels_per_point;
    let pointer_over_terminal = ui
        .chrome
        .context()
        .pointer_latest_pos()
        .is_some_and(|p| !ui.over_chrome(p.x * ppp, p.y * ppp));
    if pointer_over_terminal {
        out.full.platform_output.cursor_icon = cursor;
    }
    let platform = std::mem::take(&mut out.full.platform_output);
    ui.egui.handle_platform_output(&ui.window, platform);
    let delay = out
        .full
        .viewport_output
        .values()
        .map(|v| v.repaint_delay)
        .min()
        .unwrap_or(Duration::MAX);
    if delay.is_zero() {
        ui.window.request_redraw();
    }
    ui.repaint_at = (delay < Duration::from_secs(1)).then(|| Instant::now() + delay);
    let shapes = std::mem::take(&mut out.full.shapes);
    let textures = std::mem::take(&mut out.full.textures_delta);
    let frame = hv_render::UiFrame {
        primitives: ui.chrome.tessellate(shapes, ppp),
        textures,
        pixels_per_point: ppp,
    };
    (frame, out)
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
            #[cfg(target_os = "macos")]
            AppEvent::Menu(name) => {
                if let Some(action) = Action::parse(&name) {
                    self.run_action(action, event_loop);
                }
            }
            AppEvent::AccessKit(event) => {
                use egui_winit::accesskit_winit::WindowEvent as A11y;
                if let Some(ui) = &mut self.ui {
                    match event.window_event {
                        A11y::InitialTreeRequested => ui.chrome.context().enable_accesskit(),
                        A11y::ActionRequested(request) => {
                            ui.egui.on_accesskit_action_request(request);
                        }
                        A11y::AccessibilityDeactivated => ui.chrome.context().disable_accesskit(),
                    }
                    ui.window.request_redraw();
                }
            }
            AppEvent::Hook(hook) => {
                // Route to the pane named in HIVETTY_SESSION_ID.
                let target = hook
                    .session
                    .as_deref()
                    .and_then(|s| s.parse::<PaneId>().ok());
                if let Some(ui) = &mut self.ui {
                    let focused = ui.focused_id();
                    if let Some(id) = target
                        && let Some(pane) = ui.panes.get_mut(&id)
                        && pane.info.on_hook(&hook)
                    {
                        if Some(id) != focused && pane.info.needs_attention() {
                            pane.info.unseen = true;
                        }
                        ui.window.request_redraw();
                    }
                }
            }
            AppEvent::Wakeup => {
                self.wake_pending.store(false, Ordering::Release);
                self.handle_session_events(event_loop);
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // egui sees pointer and window events; the keyboard belongs to the terminal unless an
        // egui text field (the command palette) is open.
        let palette_open = self
            .ui
            .as_ref()
            .is_some_and(|ui| ui.chrome.takes_keyboard());
        if let Some(ui) = &mut self.ui
            && !matches!(event, WindowEvent::RedrawRequested)
            && (palette_open
                || !matches!(
                    event,
                    WindowEvent::KeyboardInput { .. } | WindowEvent::Ime(_)
                ))
        {
            let _ = ui.egui.on_window_event(&ui.window, &event);
            if palette_open
                && matches!(
                    event,
                    WindowEvent::KeyboardInput { .. } | WindowEvent::Ime(_)
                )
            {
                ui.window.request_redraw();
                return;
            }
        }
        match event {
            WindowEvent::CloseRequested => {
                self.save_layout();
                if let Some(ui) = &self.ui {
                    for pane in ui.panes.values() {
                        pane.session.kill();
                    }
                }
                event_loop.exit();
            }
            WindowEvent::Resized(PhysicalSize { width, height }) => {
                if let Some(ui) = &mut self.ui {
                    ui.renderer.resize(width, height);
                    // The next frame lays out the chrome and resizes the panes to what is left.
                    ui.terminal_area = None;
                    ui.window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(ui) = &mut self.ui {
                    ui.renderer.set_scale(scale_factor as f32);
                    ui.terminal_area = None;
                    ui.resize_all();
                    ui.window.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = m.state();
                self.update_hover_link();
            }
            WindowEvent::KeyboardInput { event, .. } => self.on_key(&event, event_loop),
            WindowEvent::Ime(ime) => self.on_ime(ime),
            WindowEvent::MouseWheel { delta, .. } => self.on_wheel(delta),
            WindowEvent::MouseInput { state, button, .. } => self.on_mouse_button(state, button),
            WindowEvent::DroppedFile(path) => self.on_drop(&path),
            WindowEvent::CursorMoved { position, .. } => {
                self.on_cursor_moved(position.x as f32, position.y as f32);
            }
            WindowEvent::RedrawRequested => self.redraw(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(action) = self.pending_action.take() {
            self.run_action(action, event_loop);
        }
        let Some(ui) = &self.ui else {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        };
        let now = Instant::now();
        // A synchronized update (DECSET 2026) must be flushed when its deadline passes even if
        // no more output arrives.
        let sync = ui
            .panes
            .values()
            .filter_map(|p| p.session.sync_deadline())
            .min();
        // The sidebar's elapsed times change every second for a minute, then every minute.
        let tick = ui
            .panes
            .values()
            .filter(|_| self.sidebar_visible)
            .map(|p| {
                let since = p.info.agent.since().unwrap_or(p.info.started);
                crate::model::next_elapsed_change(since, now)
            })
            .min();
        let deadline = [sync, self.bell_until, tick, ui.repaint_at]
            .into_iter()
            .flatten()
            .min();
        event_loop.set_control_flow(deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
}

/// Whether a saved window position is still on a connected monitor (it may have been
/// unplugged since).
fn on_some_monitor(event_loop: &ActiveEventLoop, x: i32, y: i32) -> bool {
    event_loop.available_monitors().any(|m| {
        let (pos, size) = (m.position(), m.size());
        x >= pos.x && y >= pos.y && x < pos.x + size.width as i32 && y < pos.y + size.height as i32
    })
}

/// The chord a key press makes, for the keymap. Uses the key without modifiers so that
/// Shift+1 is "shift+1" and not "!".
fn pressed_chord(event: &KeyEvent, mods: ModifiersState) -> Option<Chord> {
    let key = match event.key_without_modifiers() {
        Key::Character(c) => c.to_lowercase(),
        Key::Named(named) => match named {
            NamedKey::ArrowLeft => "left".into(),
            NamedKey::ArrowRight => "right".into(),
            NamedKey::ArrowUp => "up".into(),
            NamedKey::ArrowDown => "down".into(),
            NamedKey::Enter => "enter".into(),
            NamedKey::Tab => "tab".into(),
            NamedKey::Escape => "escape".into(),
            NamedKey::Space => "space".into(),
            NamedKey::Backspace => "backspace".into(),
            NamedKey::Delete => "delete".into(),
            NamedKey::Insert => "insert".into(),
            NamedKey::Home => "home".into(),
            NamedKey::End => "end".into(),
            NamedKey::PageUp => "pageup".into(),
            NamedKey::PageDown => "pagedown".into(),
            NamedKey::F1 => "f1".into(),
            NamedKey::F2 => "f2".into(),
            NamedKey::F3 => "f3".into(),
            NamedKey::F4 => "f4".into(),
            NamedKey::F5 => "f5".into(),
            NamedKey::F6 => "f6".into(),
            NamedKey::F7 => "f7".into(),
            NamedKey::F8 => "f8".into(),
            NamedKey::F9 => "f9".into(),
            NamedKey::F10 => "f10".into(),
            NamedKey::F11 => "f11".into(),
            NamedKey::F12 => "f12".into(),
            _ => return None,
        },
        _ => return None,
    };
    Some(Chord {
        key,
        ctrl: mods.control_key(),
        shift: mods.shift_key(),
        alt: mods.alt_key(),
        super_key: mods.super_key(),
    })
}

/// Grid size for a pane drawn in `area`.
fn grid_size_in(renderer: &Renderer, area: [f32; 4]) -> GridSize {
    let (cols, rows) = renderer.grid_size_in(area);
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
    ui.resize_all();
    ui.window.request_redraw();
}

fn zoom(ui: &mut Ui, delta: f32) {
    ui.renderer.set_font_size(ui.renderer.font_size() + delta);
    ui.resize_all();
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
