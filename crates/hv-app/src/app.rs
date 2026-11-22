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
    /// A worktree for a new session was created (or failed) in the background (F4-22).
    WorktreeReady(
        hv_ui::NewSession,
        Result<(std::path::PathBuf, std::path::PathBuf), String>,
    ),
    /// This is the first run: show the wizard (F6-03).
    Welcome(hv_ui::WelcomeInfo),
    /// Usage logs were read in the background (F5).
    UsageLoaded(Arc<UsageData>),
    /// The agent detected in a pane's process tree changed (F4-03).
    Agent { pane: PaneId, agent: Option<String> },
    /// Project chips computed for the prompt at `line` of `pane` (F3-09…F3-12).
    Chips {
        pane: PaneId,
        line: usize,
        info: Arc<hv_shell::detect::ProjectInfo>,
    },
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
    let safe = hv_config::safe_mode();
    // Safe mode ignores the config file (and does not watch it).
    let config_path = hv_config::default_path().filter(|_| !safe);
    let (config, config_error) = if safe {
        log::warn!("safe mode: default settings, no shell integration, software renderer");
        hv_render::set_software(true);
        (hv_config::safe_config(), None)
    } else {
        config_path
            .as_deref()
            .map_or_else(|| (hv_config::Config::default(), None), hv_config::load)
    };
    if let Some(err) = &config_error {
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
    // Safe mode must not overwrite the user's layout with its one default pane.
    if !explicit_program && !safe {
        app.layout_path = crate::saved::layout_path();
        app.restore = app
            .layout_path
            .as_deref()
            .and_then(crate::saved::SavedLayout::load);
    }
    let (keymap, keymap_problem) = keymap_from(&config);
    app.keymap = keymap;
    app.config_notice = config_error
        .clone()
        .or(keymap_problem)
        .map(|e| hv_ui::strings::config_problem(&e));
    app.shell_dir = hv_config::data_dir().and_then(|dir| {
        hv_shell::install(&dir.join("shell"))
            .map_err(|err| log::warn!("shell integration unavailable: {err}"))
            .ok()
    });
    app.chip_service = {
        let proxy = std::sync::Mutex::new(app.proxy.clone());
        let home = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(std::path::PathBuf::from);
        hv_shell::ChipService::start(home, move |(pane, line), info| {
            if let Ok(proxy) = proxy.lock() {
                let _ = proxy.send_event(AppEvent::Chips { pane, line, info });
            }
        })
        .map_err(|err| log::warn!("prompt chips disabled: {err}"))
        .ok()
    };
    app.archive = crate::archive::path()
        .map(|p| crate::archive::load(&p))
        .unwrap_or_default();
    app.profiles = hv_agents::profile::builtin();
    app.agent_choices = std::iter::once(hv_ui::AgentChoice {
        id: String::new(),
        name: hv_ui::strings::DEFAULT_TITLE.to_owned(),
    })
    .chain(
        hv_agents::profile::installed(&app.profiles, std::env::var_os("PATH").as_deref())
            .into_iter()
            .map(|(p, _)| hv_ui::AgentChoice {
                id: p.id.clone(),
                name: p.name.clone(),
            }),
    )
    .collect();
    app.agent_watch = {
        let proxy = std::sync::Mutex::new(app.proxy.clone());
        hv_agents::watch::ProcessWatcher::start(
            app.profiles.clone(),
            Duration::from_secs(1),
            move |pane, agent| {
                if let Ok(proxy) = proxy.lock() {
                    let _ = proxy.send_event(AppEvent::Agent { pane, agent });
                }
            },
        )
        .map_err(|err| log::warn!("agent detection disabled: {err}"))
        .ok()
    };
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
/// The keymap with the config's overrides, and the first problem with them (to tell the user).
fn keymap_from(config: &hv_config::Config) -> (Keymap, Option<String>) {
    let (keymap, errors) = Keymap::defaults().with_overrides(&config.keybindings);
    for err in &errors {
        log::warn!("keybindings: {err}");
    }
    let problem = errors.first().map(|e| format!("keybindings: {e}"));
    (keymap, problem)
}

/// One terminal pane.
struct Pane {
    session: Session,
    info: SessionInfo,
    /// A command to type once the shell shows its first prompt (an agent started from the
    /// New session dialog or the dock), and the deadline after which it is typed anyway.
    pending_command: Option<(String, Instant)>,
    /// Project chips by prompt line (scrollback line of the OSC 133;A mark).
    chips: std::collections::BTreeMap<usize, Arc<hv_shell::detect::ProjectInfo>>,
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
    /// The usage dashboard covers the terminals (it takes the pointer and keyboard).
    usage_open: bool,
    /// The pane being peeked at from the sidebar (S-20).
    peek: Option<PaneId>,
    /// Whether hivetty draws the title bar (F1-02).
    title_bar: TitleBarMode,
    /// A pane waiting for the user to confirm closing it.
    confirm_close: Option<(PaneId, Confirm)>,
    /// A pane being renamed.
    renaming: Option<PaneId>,
    /// Space egui left for the terminals last frame, in physical pixels.
    terminal_area: Option<[f32; 4]>,
    /// Sidebar row index → pane, from the last frame.
    row_panes: Vec<PaneId>,
    /// When egui asked to be painted again (animations such as the tab spinner).
    repaint_at: Option<Instant>,
    /// This window's input state while another window is the active one.
    local: Option<Local>,
}

/// Input and UI state that belongs to one window. The active window's copy lives in the `App`
/// fields; the others keep theirs in [`Ui::local`] (see [`App::activate`]).
struct Local {
    modifiers: ModifiersState,
    scroll_remainder: f32,
    mouse: MouseState,
    ime_composing: bool,
    search: Option<SearchUi>,
    bell_until: Option<Instant>,
    sidebar_visible: bool,
    pending_action: Option<Action>,
}

impl Default for Local {
    fn default() -> Self {
        Self {
            modifiers: ModifiersState::empty(),
            scroll_remainder: 0.0,
            mouse: MouseState::default(),
            ime_composing: false,
            search: None,
            bell_until: None,
            sidebar_visible: true,
            pending_action: None,
        }
    }
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
        self.usage_open
            || self
                .terminal_area
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
    fn chrome_model(
        &mut self,
        sidebar_visible: bool,
        keymap: &Keymap,
        agents: &[hv_ui::AgentChoice],
        usage: Option<&UsageData>,
    ) -> hv_ui::ChromeModel {
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
                let project_dir = project_dir(pane);
                let mut row = crate::model::session_row(&pane.info, project_dir.as_deref());
                row.focused = Some(id) == focused;
                row.card = session_card(pane, usage);
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
            agents: agents.to_vec(),
            usage: None,
            peek: None,
            title_bar: hv_ui::TitleBar::Native,
            archived: Vec::new(),
            confirm: None,
            rename: None,
            notice: None,
            plan_limits: Vec::new(),
            new_session_hint: keymap
                .chord_for(&Action::NewAgentSession)
                .map(crate::keymap::Chord::pretty)
                .unwrap_or_default(),
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
    /// The active window (the one the current event is for).
    ui: Option<Ui>,
    /// The other windows (F2-05).
    parked: HashMap<WindowId, Ui>,
    /// The window with keyboard focus (menu bar actions go there).
    focused_window: Option<WindowId>,
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
    /// Installed shell integration scripts (F3-01…F3-04), if available.
    shell_dir: Option<std::path::PathBuf>,
    /// Reads project files for the prompt chips off the UI thread.
    chip_service: Option<hv_shell::ChipService<(PaneId, usize)>>,
    /// Agent profiles (F4-01).
    profiles: Vec<hv_agents::profile::Profile>,
    /// Watches pane process trees for agents started by hand (F4-03).
    agent_watch: Option<hv_agents::watch::ProcessWatcher>,
    /// Groups bursts of agent notifications (F4-18).
    notify_group: NotifyGroup,
    /// Agents found on PATH at start, a plain shell first (dock and New session).
    agent_choices: Vec<hv_ui::AgentChoice>,
    /// The agent last started from the dialog or the dock.
    last_agent: String,
    /// The usage dashboard (F5).
    usage: UsageState,
    /// Archived sessions (F4-15).
    archive: Vec<crate::archive::ArchivedSession>,
    /// Plan limits last reported by an agent's statusline (F4-32).
    plan_limits: PlanLimits,
    /// A problem with config.toml to show until fixed or closed (F6-02).
    config_notice: Option<String>,
    /// The count last shown on the app icon (F4-19).
    badge: usize,
    /// The worktree of the session being started (F4-22).
    next_worktree: Option<(std::path::PathBuf, std::path::PathBuf)>,
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
            Some(Ok(false)) => hv_ui::strings::FIND_NO_MATCHES,
            Some(Err(_)) => hv_ui::strings::FIND_INVALID,
            _ => "",
        };
        hv_ui::strings::find_bar(&self.query, status)
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
    /// Window edge under the pointer when hivetty draws the frame (F1-02).
    resize_edge: Option<winit::window::ResizeDirection>,
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
            parked: HashMap::new(),
            focused_window: None,
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
            shell_dir: None,
            chip_service: None,
            profiles: Vec::new(),
            agent_watch: None,
            notify_group: NotifyGroup::default(),
            agent_choices: Vec::new(),
            last_agent: "claude-code".to_owned(),
            usage: UsageState::default(),
            archive: Vec::new(),
            plan_limits: PlanLimits::default(),
            config_notice: None,
            badge: 0,
            next_worktree: None,
            #[cfg(target_os = "macos")]
            menu: None,
        }
    }

    fn create_ui(
        &mut self,
        event_loop: &ActiveEventLoop,
        adopt: &mut Option<Adopted>,
    ) -> Result<Ui> {
        let restore = self
            .restore
            .take()
            .filter(|_| self.config.window.restore_layout);
        let geometry = restore
            .as_ref()
            .and_then(|l| l.window)
            .filter(|_| self.config.window.remember_geometry);
        let title_bar = TitleBarMode::from_config(&self.config.window.decorations);
        let mut attrs = Window::default_attributes()
            .with_title(APP_TITLE)
            .with_inner_size(LogicalSize::new(1000.0, 640.0))
            // AccessKit must be attached before the window is first shown.
            .with_visible(false);
        match title_bar {
            TitleBarMode::Custom => attrs = attrs.with_decorations(false),
            TitleBarMode::MacInset => {
                #[cfg(target_os = "macos")]
                {
                    use winit::platform::macos::WindowAttributesExtMacOS;
                    attrs = attrs
                        .with_titlebar_transparent(true)
                        .with_title_hidden(true)
                        .with_fullsize_content_view(true);
                }
            }
            TitleBarMode::Native => {}
        }
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
        let mut renderer = renderer;
        renderer.set_jump_label(hv_ui::strings::JUMP_TO_BOTTOM);
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
        // F6-01: light or dark, following the OS unless the config forces one.
        let light = self
            .config
            .theme
            .is_light(window.theme().map(|t| t == winit::window::Theme::Light));
        set_palette(&self.config.theme);
        apply_theme(&chrome, light);
        #[cfg(target_os = "macos")]
        if self.menu.is_none() {
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
            usage_open: false,
            peek: None,
            title_bar,
            confirm_close: None,
            renaming: None,
            terminal_area: None,
            row_panes: Vec::new(),
            repaint_at: None,
            local: None,
        };
        apply_font(&mut ui, &self.config.font);
        if let Some((tab, panes)) = adopt.take() {
            ui.panes.extend(panes);
            ui.tabs.list.push(tab);
            ui.tabs.active = 0;
            ui.resize_all();
            return Ok(ui);
        }
        if let Some(layout) = restore {
            let mut spawned = Vec::new();
            let mode = self.config.agents.restore_on_launch.clone();
            let profiles = self.profiles.clone();
            let tabs =
                layout.restore(
                    |cwd, agent| match self.spawn_pane(&ui, cwd.map(Into::into)) {
                        Ok((id, mut pane)) => {
                            // F4-21: reopen the agent session that ran here.
                            if let Some(command) =
                                agent.and_then(|a| resume_command(&profiles, a, &mode))
                            {
                                pane.pending_command =
                                    Some((command, Instant::now() + Duration::from_millis(1500)));
                            }
                            spawned.push((id, pane));
                            Some(id)
                        }
                        Err(err) => {
                            log::warn!("cannot reopen a pane: {err:#}");
                            None
                        }
                    },
                );
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
        if self.config.shell.integration
            && let Some(scripts) = &self.shell_dir
        {
            let (program, args) = match &spec.program {
                Some(program) => (program.clone(), spec.args.clone()),
                None => hv_pty::default_shell(),
            };
            if let Some(integration) =
                hv_shell::integrate(&program, &args, scripts, |k| std::env::var(k).ok())
            {
                spec.program = Some(program);
                spec.args = integration.args;
                spec.env.extend(integration.env);
                if self.config.shell.chips {
                    spec.env.push(("HIVETTY_CHIPS".to_owned(), "1".to_owned()));
                }
            }
        }
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
                chips: std::collections::BTreeMap::new(),
                pending_command: None,
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
            Action::NewWindow => {
                let _ = self.open_window(event_loop, None);
                self.focus_changed();
            }
            Action::MoveTabToNewWindow => self.move_tab_to_new_window(event_loop),
            Action::CloseWindow => {
                if self.parked.is_empty() {
                    self.save_layout();
                }
                self.close_window(event_loop);
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
            Action::JumpToPrompt(up) => {
                if let Some(pane) = ui.focused() {
                    pane.session.jump_to_prompt(up);
                }
            }
            Action::CopyLastOutput => {
                if let Some(text) = ui.focused().and_then(|p| p.session.last_command_output())
                    && let Err(err) = arboard::Clipboard::new().and_then(|mut c| c.set_text(text))
                {
                    log::warn!("clipboard write failed: {err}");
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
            Action::NextNeedsYou => self.focus_next_needing_you(),
            Action::ClosePanesById(ids) => {
                for id in ids {
                    self.close_pane(id, event_loop);
                }
            }
            Action::ToggleUsage => self.toggle_usage(),
            Action::NewAgentSession => self.open_new_session_dialog(),
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
            let Some(pane) = ui.panes.get(&id) else {
                return crate::saved::PaneState::default();
            };
            crate::saved::PaneState {
                cwd: pane.session.cwd(),
                agent: pane
                    .info
                    .agent_id
                    .clone()
                    .map(|agent| crate::saved::SavedAgent {
                        id: agent,
                        session: pane.info.agent.agent_session().map(str::to_owned),
                    }),
            }
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

    /// Switches every window to the light or dark theme.
    fn set_theme(&mut self, light: bool) {
        for ui in self.ui.iter().chain(self.parked.values()) {
            apply_theme(&ui.chrome, light);
            ui.window.request_redraw();
        }
    }

    /// Reads the usage logs again in the background if they are older than a minute.
    fn refresh_usage(&mut self) {
        let stale = self
            .usage
            .data
            .as_ref()
            .and_then(|d| d.read_at)
            .is_none_or(|at| at.elapsed() > USAGE_REFRESH);
        if stale && !self.usage.loading {
            self.usage.loading = true;
            let proxy = self.proxy.clone();
            let _ = std::thread::Builder::new()
                .name("hv-usage-read".into())
                .spawn(move || {
                    let data = UsageData::read();
                    let _ = proxy.send_event(AppEvent::UsageLoaded(Arc::new(data)));
                });
        }
    }

    /// Opens or closes the usage dashboard; reading the logs again in the background if they
    /// are older than a minute.
    fn toggle_usage(&mut self) {
        self.usage.open = !self.usage.open;
        if self.usage.open {
            self.refresh_usage();
        }
        for ui in self.ui.iter().chain(self.parked.values()) {
            ui.window.request_redraw();
        }
    }

    /// The dashboard to draw this frame, if open.
    fn usage_view(&self) -> Option<hv_ui::UsageView> {
        if !self.usage.open {
            return None;
        }
        Some(match (&self.usage.view, self.usage.loading) {
            (Some(view), _) => {
                let filter = self.usage.filter.as_deref().map(|id| {
                    hv_agents::profile::find(&self.profiles, id)
                        .map_or_else(|| id.to_owned(), |p| p.name.clone())
                });
                hv_ui::UsageView::Ready(self.usage.range, Arc::clone(view), filter)
            }
            (None, _) => hv_ui::UsageView::Loading(self.usage.range),
        })
    }

    /// A choice in a session row's context menu (S-13).
    fn on_session_menu(&mut self, row: usize, choice: hv_ui::SessionMenu) {
        let Some(ui) = &mut self.ui else { return };
        let Some(&id) = ui.row_panes.get(row) else {
            return;
        };
        match choice {
            hv_ui::SessionMenu::Focus => {
                ui.tabs.focus_pane(id);
                self.focus_changed();
            }
            hv_ui::SessionMenu::Peek => ui.peek = Some(id),
            hv_ui::SessionMenu::Rename => ui.renaming = Some(id),
            hv_ui::SessionMenu::Archive => self.archive_pane(id),
            hv_ui::SessionMenu::CopyId => {
                if let Some(session) = ui
                    .panes
                    .get(&id)
                    .and_then(|p| p.info.agent.agent_session().map(str::to_owned))
                    && let Err(err) =
                        arboard::Clipboard::new().and_then(|mut c| c.set_text(session))
                {
                    log::warn!("clipboard write failed: {err}");
                }
            }
            hv_ui::SessionMenu::ShowFolder => {
                if let Some(dir) = ui.panes.get(&id).and_then(|p| p.session.cwd())
                    && let Err(err) = opener::open(&dir)
                {
                    log::warn!("cannot open {}: {err}", dir.display());
                }
            }
            hv_ui::SessionMenu::Close => ui.confirm_close = Some((id, Confirm::Close)),
            hv_ui::SessionMenu::RemoveWorktree => {
                ui.confirm_close = Some((id, Confirm::RemoveWorktree));
            }
        }
        if let Some(ui) = &self.ui {
            ui.window.request_redraw();
        }
    }

    /// Keeps a record of a pane (to restore it later) and closes it (F4-15).
    fn archive_pane(&mut self, id: PaneId) {
        let Some(pane) = self.ui.as_ref().and_then(|ui| ui.panes.get(&id)) else {
            return;
        };
        let row = crate::model::session_row(&pane.info, None);
        self.archive.push(crate::archive::ArchivedSession {
            title: row.title,
            agent: pane
                .info
                .agent_id
                .clone()
                .map(|agent| crate::saved::SavedAgent {
                    id: agent,
                    session: pane.info.agent.agent_session().map(str::to_owned),
                }),
            agent_name: pane.info.agent_name.clone(),
            cwd: pane.session.cwd(),
            archived_at: jiff::Timestamp::now().as_second(),
        });
        self.save_archive();
        self.queue_close(id);
    }

    /// Closes a pane after this event (several can be queued).
    fn queue_close(&mut self, id: PaneId) {
        match &mut self.pending_action {
            Some(Action::ClosePanesById(ids)) => ids.push(id),
            _ => self.pending_action = Some(Action::ClosePanesById(vec![id])),
        }
    }

    /// A choice in a project group's context menu (S-14). `row` is one of its sessions.
    fn on_project_menu(&mut self, row: usize, choice: hv_ui::ProjectMenu) {
        let Some(ui) = &self.ui else { return };
        let Some(dir) = ui
            .row_panes
            .get(row)
            .and_then(|id| ui.panes.get(id))
            .and_then(project_dir)
        else {
            return;
        };
        match choice {
            hv_ui::ProjectMenu::NewSession => {
                self.open_new_session_dialog_in(Some(dir));
            }
            hv_ui::ProjectMenu::ShowFolder => {
                if let Err(err) = opener::open(&dir) {
                    log::warn!("cannot open {}: {err}", dir.display());
                }
            }
            hv_ui::ProjectMenu::ArchiveFinished => {
                let finished: Vec<PaneId> = ui
                    .row_panes
                    .iter()
                    .filter_map(|id| Some((*id, ui.panes.get(id)?)))
                    .filter(|(_, p)| project_dir(p).as_deref() == Some(dir.as_path()))
                    .filter(|(_, p)| {
                        crate::model::session_row(&p.info, None).state == hv_ui::RowState::Done
                    })
                    .map(|(id, _)| id)
                    .collect();
                for id in finished {
                    self.archive_pane(id);
                }
            }
        }
        if let Some(ui) = &self.ui {
            ui.window.request_redraw();
        }
    }

    fn save_archive(&self) {
        if let Some(path) = crate::archive::path() {
            crate::archive::save(path, &self.archive);
        }
    }

    /// Reopens an archived session in a new tab, resuming its agent session when known.
    fn restore_archived(&mut self, session: &crate::archive::ArchivedSession) {
        let Some(ui) = self.ui.take() else { return };
        let cwd = session.cwd.clone().filter(|d| d.is_dir());
        let spawned = self.spawn_pane(&ui, cwd);
        let mut ui = ui;
        match spawned {
            Ok((id, mut pane)) => {
                if let Some(agent) = &session.agent
                    && let Some(command) = resume_command(&self.profiles, agent, "always")
                {
                    pane.pending_command =
                        Some((command, Instant::now() + Duration::from_millis(1500)));
                    pane.info.agent_id = Some(agent.id.clone());
                    pane.info.agent_name.clone_from(&session.agent_name);
                }
                ui.panes.insert(id, pane);
                ui.tabs.open(id);
            }
            Err(err) => log::error!("cannot restore the session: {err:#}"),
        }
        self.ui = Some(ui);
        self.focus_changed();
    }

    /// Opens the New session dialog with the last agent and the focused pane's folder.
    fn open_new_session_dialog(&mut self) {
        self.open_new_session_dialog_in(None);
    }

    /// The New session dialog for `folder`, or the focused pane's folder.
    fn open_new_session_dialog_in(&mut self, folder: Option<std::path::PathBuf>) {
        let agents = self.agent_choices.clone();
        let agent = self.last_agent.clone();
        let Some(ui) = &mut self.ui else { return };
        let folder = folder
            .or_else(|| ui.focused().and_then(|p| p.session.cwd()))
            .or_else(home_dir)
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default();
        ui.chrome.open_new_session(agents, &agent, folder);
        ui.window.request_redraw();
    }

    /// Starts a shell in the requested folder (new tab or split) and types the agent command
    /// once the shell is ready (S-01, S-11, A-16).
    fn start_agent_session(&mut self, request: &hv_ui::NewSession) {
        // A worktree first, off the UI thread; the session starts when it is ready.
        if let Some(branch) = &request.worktree {
            let branch = if branch.is_empty() {
                crate::worktree::default_branch(&request.prompt)
            } else {
                branch.clone()
            };
            let folder = std::path::PathBuf::from(&request.folder);
            let proxy = self.proxy.clone();
            let request = request.clone();
            let _ = std::thread::Builder::new()
                .name("hv-worktree".into())
                .spawn(move || {
                    let root = hv_shell::detect::inspect(&folder, None).root;
                    let result = match root {
                        None => Err(format!("{} is not in a git repository", folder.display())),
                        Some(root) => match crate::worktree::worktree_dir(&root, &branch) {
                            None => Err("cannot place the worktree".to_owned()),
                            Some(dir) => {
                                crate::worktree::create(&root, &branch, &dir).map(|()| {
                                    // F4-31: untracked files the repo lists, like `.env`.
                                    crate::worktree::copy_included(&root, &dir);
                                    (root.clone(), dir)
                                })
                            }
                        },
                    };
                    let _ = proxy.send_event(AppEvent::WorktreeReady(request, result));
                });
            return;
        }
        let profile = hv_agents::profile::find(&self.profiles, &request.agent).cloned();
        if let Some(p) = &profile {
            self.last_agent.clone_from(&p.id);
        }
        let Some(ui) = self.ui.take() else { return };
        let spawned = self.spawn_pane(&ui, Some(std::path::PathBuf::from(&request.folder)));
        let mut ui = ui;
        match spawned {
            Ok((id, mut pane)) => {
                if let Some(profile) = &profile {
                    let shell = self
                        .spec
                        .program
                        .clone()
                        .unwrap_or_else(|| hv_pty::default_shell().0);
                    let command = agent_command_line(profile, request, &shell);
                    // Typed at the first prompt; if the shell never reports one, after 1.5 s.
                    pane.pending_command =
                        Some((command, Instant::now() + Duration::from_millis(1500)));
                    pane.info.agent_id = Some(profile.id.clone());
                    pane.info.agent_name = Some(profile.name.clone());
                }
                pane.info.worktree = self.next_worktree.clone();
                ui.panes.insert(id, pane);
                if request.split
                    && let Some(tab) = ui.tabs.active_mut()
                {
                    tab.root.split(tab.focused, id, Axis::Horizontal);
                    tab.focused = id;
                    tab.zoomed = false;
                } else {
                    ui.tabs.open(id);
                }
            }
            Err(err) => log::error!("cannot start a new session: {err:#}"),
        }
        self.ui = Some(ui);
        self.focus_changed();
    }

    /// Agent notifications due in the active window (F4-18): one per new attention state,
    /// skipping the pane the user is looking at.
    fn notify_candidates(&mut self, window_focused: bool) -> Vec<(&'static str, String, String)> {
        let Some(ui) = &mut self.ui else {
            return Vec::new();
        };
        let focused = ui.focused_id();
        let mut out = Vec::new();
        for (id, pane) in &mut ui.panes {
            let project = pane
                .chips
                .values()
                .next_back()
                .and_then(|i| i.root.clone())
                .or_else(|| pane.session.cwd())
                .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_else(|| "~".to_owned());
            if let Some(note) = pane.info.take_notification(&project) {
                // Never for the pane the user is looking at.
                if !(window_focused && Some(*id) == focused) {
                    out.push(note);
                }
            }
        }
        if !out.is_empty() && !window_focused {
            ui.window
                .request_user_attention(Some(winit::window::UserAttentionType::Informational));
        }
        out
    }

    fn send_agent_notifications(&mut self, notes: Vec<(&'static str, String, String)>) {
        let config = &self.config.notifications;
        if !config.enabled {
            return;
        }
        let window = Duration::from_secs(config.group_window_seconds.max(1));
        for (kind, title, body) in notes {
            if !config.on.iter().any(|k| k == kind) {
                continue;
            }
            if self.notify_group.admit(Instant::now(), window) {
                notify(config, &title, &body);
            }
        }
    }

    /// Focuses the next pane (after the focused one, across windows) that waits for the user;
    /// if none does, the next one that finished unseen.
    fn focus_next_needing_you(&mut self) {
        // (window, pane, waiting, unseen) in a stable order: active window first.
        let mut all = Vec::new();
        for ui in self.ui.iter().chain(self.parked.values()) {
            for tab in &ui.tabs.list {
                for id in tab.root.panes() {
                    if let Some(p) = ui.panes.get(&id) {
                        let waiting = matches!(
                            p.info.agent.state(),
                            Some(hv_agents::AgentState::Blocked { .. })
                        );
                        all.push((ui.window.id(), id, waiting, p.info.unseen));
                    }
                }
            }
        }
        let current = self.ui.as_ref().and_then(Ui::focused_id);
        let start = all
            .iter()
            .position(|(_, id, ..)| Some(*id) == current)
            .map_or(0, |i| i + 1);
        let rotated = all[start..].iter().chain(&all[..start]);
        let target = rotated
            .clone()
            .find(|(.., waiting, _)| *waiting)
            .or_else(|| rotated.clone().find(|(.., unseen)| *unseen))
            .map(|(w, id, ..)| (*w, *id));
        let Some((window, pane)) = target else {
            return;
        };
        if self.activate(window)
            && let Some(ui) = &mut self.ui
        {
            ui.tabs.focus_pane(pane);
            ui.window.focus_window();
        }
        self.focus_changed();
    }

    /// Tells the process watcher which shells to look under (all panes of all windows).
    fn sync_watched_panes(&self) {
        let Some(watch) = &self.agent_watch else {
            return;
        };
        let panes = self
            .ui
            .iter()
            .chain(self.parked.values())
            .flat_map(|ui| ui.panes.iter())
            .filter_map(|(id, p)| p.session.pid().map(|pid| (*id, pid)))
            .collect();
        watch.set_panes(panes);
    }

    /// Makes `id` the active window, swapping the per-window state. False if unknown.
    fn activate(&mut self, id: WindowId) -> bool {
        if self.ui.as_ref().is_some_and(|ui| ui.window.id() == id) {
            return true;
        }
        let Some(mut next) = self.parked.remove(&id) else {
            return false;
        };
        let incoming = next.local.take().unwrap_or_default();
        let outgoing = self.swap_local(incoming);
        if let Some(mut current) = self.ui.take() {
            current.local = Some(outgoing);
            self.parked.insert(current.window.id(), current);
        }
        self.ui = Some(next);
        true
    }

    /// Puts `local` into the `App` fields and returns what was there.
    fn swap_local(&mut self, local: Local) -> Local {
        use std::mem::replace;
        Local {
            modifiers: replace(&mut self.modifiers, local.modifiers),
            scroll_remainder: replace(&mut self.scroll_remainder, local.scroll_remainder),
            mouse: replace(&mut self.mouse, local.mouse),
            ime_composing: replace(&mut self.ime_composing, local.ime_composing),
            search: replace(&mut self.search, local.search),
            bell_until: replace(&mut self.bell_until, local.bell_until),
            sidebar_visible: replace(&mut self.sidebar_visible, local.sidebar_visible),
            pending_action: replace(&mut self.pending_action, local.pending_action),
        }
    }

    /// Every window, the active one first.
    fn window_ids(&self) -> Vec<WindowId> {
        self.ui
            .iter()
            .map(|ui| ui.window.id())
            .chain(self.parked.keys().copied())
            .collect()
    }

    /// Opens a window (adopting a tab moved from another window, if given) and makes it active.
    /// Returns the adopted tab back if the window could not be created.
    fn open_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        adopt: Option<Adopted>,
    ) -> Option<Adopted> {
        let mut adopt = adopt;
        match self.create_ui(event_loop, &mut adopt) {
            Ok(ui) => {
                let outgoing = self.swap_local(Local::default());
                if let Some(mut current) = self.ui.take() {
                    current.local = Some(outgoing);
                    self.parked.insert(current.window.id(), current);
                }
                ui.window.request_redraw();
                self.ui = Some(ui);
            }
            Err(err) => log::error!("cannot open a window: {err:#}"),
        }
        adopt
    }

    /// Closes the active window (its panes die with it). Exits after the last one.
    fn close_window(&mut self, event_loop: &ActiveEventLoop) {
        // The last window: the process ends next, so write on this thread.
        let last = self.parked.is_empty();
        if let Some(mut ui) = self.ui.take() {
            for pane in ui.panes.values_mut() {
                let interval = pane.info.working_interval(true);
                if last {
                    record_interval_now(interval);
                } else {
                    record_interval(interval);
                }
                pane.session.kill();
            }
        }
        let _ = self.swap_local(Local::default());
        match self.parked.keys().next().copied() {
            Some(next) => {
                self.activate(next);
                self.focus_changed();
            }
            None => event_loop.exit(),
        }
    }

    /// Moves the active tab to a new window (only when the window has other tabs).
    fn move_tab_to_new_window(&mut self, event_loop: &ActiveEventLoop) {
        let Some(ui) = &mut self.ui else { return };
        if ui.tabs.list.len() < 2 {
            return;
        }
        let tab = ui.tabs.list.remove(ui.tabs.active);
        ui.tabs.active = ui.tabs.active.min(ui.tabs.list.len() - 1);
        let panes: Vec<(PaneId, Pane)> = tab
            .root
            .panes()
            .into_iter()
            .filter_map(|id| ui.panes.remove(&id).map(|p| (id, p)))
            .collect();
        for (_, pane) in &panes {
            pane.session.search_clear();
        }
        self.focus_changed();
        if let Some((tab, panes)) = self.open_window(event_loop, Some((tab, panes)))
            && let Some(ui) = &mut self.ui
        {
            // No new window: put the tab back where it was.
            ui.panes.extend(panes);
            ui.tabs.list.push(tab);
            ui.tabs.active = ui.tabs.list.len() - 1;
        }
        self.focus_changed();
    }

    /// Kills a pane and removes it; closes the window when it was its last one.
    fn close_pane(&mut self, id: PaneId, event_loop: &ActiveEventLoop) {
        let Some(ui) = &mut self.ui else { return };
        if let Some(mut pane) = ui.panes.remove(&id) {
            record_interval(pane.info.working_interval(true));
            pane.session.kill();
            if pane.info.remove_worktree
                && let Some((root, dir)) = pane.info.worktree.clone()
            {
                let _ = std::thread::Builder::new()
                    .name("hv-worktree".into())
                    .spawn(move || {
                        if let Err(err) = crate::worktree::remove(&root, &dir) {
                            log::warn!("worktree: {err}");
                            hv_platform::notify(hv_ui::strings::WORKTREE_NOT_REMOVED, &err);
                        }
                    });
            }
        }
        // Dialogs about this pane go with it.
        if ui.confirm_close.is_some_and(|(p, _)| p == id) {
            ui.confirm_close = None;
        }
        if ui.renaming == Some(id) {
            ui.renaming = None;
        }
        if ui.tabs.remove_pane(id) {
            self.close_window(event_loop);
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
        let agent = self
            .ui
            .as_ref()
            .and_then(Ui::focused)
            .and_then(|p| p.info.agent_id.clone());
        let ctx = crate::keymap::Context {
            search_open: self.search.is_some(),
            agent: agent.as_deref(),
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
        // Borderless window: the edges resize it.
        if state == ElementState::Pressed
            && button == MouseButton::Left
            && let Some(edge) = self.mouse.resize_edge
        {
            let _ = ui.window.drag_resize_window(edge);
            return;
        }
        if state == ElementState::Released && self.mouse.dragging.take().is_some() {
            return; // the end of a divider drag is not a click in a pane
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
        self.mouse.resize_edge = (ui.title_bar == TitleBarMode::Custom
            && !ui.window.is_maximized())
        .then(|| {
            let size = ui.window.inner_size();
            let band = RESIZE_BAND * ui.window.scale_factor() as f32;
            resize_edge(size.width as f32, size.height as f32, x, y, band)
        })
        .flatten();
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
    /// Shows (or clears) the config problem notice in every window.
    fn set_config_notice(&mut self, problem: Option<String>) {
        self.config_notice = problem.map(|e| hv_ui::strings::config_problem(&e));
        for ui in self.ui.iter().chain(self.parked.values()) {
            ui.window.request_redraw();
        }
    }

    fn reload_config(&mut self) {
        let Some(path) = &self.config_path else {
            return;
        };
        let (config, error) = hv_config::load(path);
        // Bad custom colors still leave a usable config (with the built-in palette).
        let mut theme_error = None;
        if let Some(err) = error {
            log::warn!("{err}");
            if !err.starts_with("theme.colors") {
                self.set_config_notice(Some(err));
                return; // keep the last good config
            }
            theme_error = Some(err);
        }
        for id in self.window_ids() {
            if self.activate(id)
                && let Some(ui) = &mut self.ui
            {
                apply_font(ui, &config.font);
            }
        }
        let (keymap, keymap_problem) = keymap_from(&config);
        self.keymap = keymap;
        self.set_config_notice(theme_error.or(keymap_problem));
        let system_light = self
            .ui
            .as_ref()
            .and_then(|ui| ui.window.theme())
            .map(|t| t == winit::window::Theme::Light);
        let light = config.theme.is_light(system_light);
        set_palette(&config.theme);
        self.set_theme(light);
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
        let window_focused = self.focused_window == Some(ui.window.id());
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
                        // The first prompt of a pane started for an agent: type its command.
                        if let hv_term::osc::TermEvent::PromptMark { kind: 'A', .. } = term_event
                            && let Some((command, _)) = pane.pending_command.take()
                        {
                            pane.session.write(command.as_bytes());
                        }
                        // A new prompt: work out its chips in the background.
                        if let hv_term::osc::TermEvent::PromptMark { kind: 'A', .. } = term_event
                            && self.config.shell.chips
                            && let Some(service) = &self.chip_service
                            && let Some(line) = pane.session.last_prompt_line()
                            && let Some(dir) = pane
                                .session
                                .cwd()
                                .or_else(|| self.spec.cwd.clone())
                                .or_else(|| std::env::current_dir().ok())
                        {
                            service.request((*id, line), dir);
                        }
                        if pane.info.on_term_event(&term_event)
                            && Some(*id) != focused
                            && pane.info.needs_attention()
                        {
                            pane.info.unseen = true;
                        }
                        record_interval(pane.info.working_interval(false));
                        // F3-13: a long command finished while the user looked elsewhere.
                        let notes = &self.config.notifications;
                        if let Some((took, exit)) =
                            pane.info.command_finished(&term_event, Instant::now())
                            && notes.enabled
                            && notes.long_command_seconds > 0
                            && took.as_secs() >= notes.long_command_seconds
                            && (!notes.only_when_unfocused
                                || !window_focused
                                || Some(*id) != focused)
                        {
                            let title = if exit.unwrap_or(0) == 0 {
                                hv_ui::strings::NOTIFY_COMMAND_DONE
                            } else {
                                hv_ui::strings::NOTIFY_COMMAND_FAILED
                            };
                            let name = pane
                                .info
                                .title
                                .clone()
                                .unwrap_or_else(|| hv_ui::strings::DEFAULT_TITLE.to_owned());
                            let body =
                                hv_ui::strings::command_finished(&name, exit, &took_text(took));
                            notify(notes, title, &body);
                            if Some(*id) != focused {
                                pane.info.unseen = true;
                            }
                        }
                    }
                }
            }
        }
        let notes = self.notify_candidates(window_focused);
        self.send_agent_notifications(notes);
        let Some(ui) = &mut self.ui else { return };
        if bell {
            ui.window
                .request_user_attention(Some(winit::window::UserAttentionType::Informational));
            self.bell_until = Some(Instant::now() + BELL_FLASH);
            ui.renderer.set_flash(true);
        }
        ui.window.request_redraw();
        // Closing the last pane closes the window; the rest of the list belonged to it.
        let window = self.ui.as_ref().map(|ui| ui.window.id());
        for id in exited {
            if self.ui.as_ref().map(|ui| ui.window.id()) != window {
                break;
            }
            self.close_pane(id, event_loop);
        }
    }

    fn redraw(&mut self) {
        let edge_cursor = self.mouse.resize_edge.map(|edge| {
            use winit::window::ResizeDirection as R;
            match edge {
                R::East | R::West => egui::CursorIcon::ResizeHorizontal,
                R::North | R::South => egui::CursorIcon::ResizeVertical,
                R::NorthEast | R::SouthWest => egui::CursorIcon::ResizeNeSw,
                R::NorthWest | R::SouthEast => egui::CursorIcon::ResizeNwSe,
            }
        });
        let cursor = if let Some(edge) = edge_cursor {
            edge
        } else if self.mouse.dragging.is_some() || self.mouse.over_divider.is_some() {
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
        let usage = self.usage_view();
        let usage_data = self.usage.data.clone();
        let archived: Vec<hv_ui::ArchivedRow> = self
            .archive
            .iter()
            .rev()
            .map(|a| {
                let project = a
                    .cwd
                    .as_deref()
                    .and_then(|d| d.file_name())
                    .map_or_else(|| "~".to_owned(), |n| n.to_string_lossy().into_owned());
                let agent = a
                    .agent_name
                    .clone()
                    .unwrap_or_else(|| hv_ui::strings::DEFAULT_TITLE.to_owned());
                hv_ui::ArchivedRow {
                    title: a.title.clone(),
                    detail: format!("{agent} · {project}"),
                }
            })
            .collect();
        let Some(ui) = &mut self.ui else { return };
        ui.usage_open = usage.is_some();
        let (mut frame, out) = run_chrome(
            ui,
            self.sidebar_visible,
            &self.keymap,
            &self.agent_choices,
            ChromeExtra {
                usage,
                usage_data: usage_data.as_deref(),
                archived,
                plan_limits: self.plan_limits.lines(),
                notice: self.config_notice.clone(),
            },
            cursor,
        );
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
                let end = (*end).min(cols);
                let start = (*start).min(end);
                for cell in &mut snap.cells[row * cols + start..row * cols + end] {
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
            // Chips go on the blank line the shell left above each prompt.
            let chip_rows: Vec<hv_render::ChipRow> = snap
                .marks
                .iter()
                .filter(|m| m.row >= 1)
                .filter_map(|m| {
                    let info = pane.chips.get(&m.line)?;
                    Some(hv_render::ChipRow {
                        row: m.row - 1,
                        chips: crate::model::chips(info),
                    })
                })
                .collect();
            // An amber ring while the agent waits for the user (S-19).
            let ring = matches!(
                pane.info.agent.state(),
                Some(hv_agents::AgentState::Blocked { .. })
            )
            .then(|| hv_term::Rgb8::hex(0xE3A008));
            snapshots.push((*id, *area, snap, chip_rows, ring));
        }
        let views: Vec<PaneView<'_>> = snapshots
            .iter()
            .map(|(id, area, snap, chips, ring)| PaneView {
                id: *id,
                snap,
                area: *area,
                focused: Some(*id) == focused,
                chips,
                ring: *ring,
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
        if let Some(row) = out.peek_request
            && let Some(ui) = &mut self.ui
            && let Some(&id) = ui.row_panes.get(row)
        {
            ui.peek = Some(id);
            ui.window.request_redraw();
        }
        if let Some(action) = out.peek_action.clone()
            && let Some(ui) = &mut self.ui
            && let Some(id) = ui.peek.take()
        {
            match action {
                hv_ui::PeekAction::Reply(text) => {
                    if let Some(pane) = ui.panes.get(&id) {
                        pane.session.scroll_to_bottom();
                        pane.session.write(text.as_bytes());
                        pane.session.write(b"\r");
                    }
                }
                hv_ui::PeekAction::GoTo => {
                    ui.tabs.focus_pane(id);
                    self.focus_changed();
                }
                hv_ui::PeekAction::Close => {}
            }
            if let Some(ui) = &self.ui {
                ui.window.request_redraw();
            }
        }
        if out.card_shown {
            self.refresh_usage();
        }
        if let Some((row, choice)) = out.session_menu {
            self.on_session_menu(row, choice);
        }
        if out.notice_closed {
            self.set_config_notice(None);
        }
        if let Some(choice) = out.welcome {
            // The config file it writes is picked up by the config watcher.
            let _ = std::thread::Builder::new()
                .name("hv-welcome".into())
                .spawn(move || {
                    if let Err(err) = crate::welcome::finish(choice.as_ref()) {
                        log::warn!("first-run setup: {err}");
                        hv_platform::notify(hv_ui::strings::SETUP_PROBLEM, &err);
                    }
                });
        }
        if let Some((row, choice)) = out.project_menu {
            self.on_project_menu(row, choice);
        }
        if let Some(answer) = out.renamed
            && let Some(ui) = &mut self.ui
            && let Some(id) = ui.renaming.take()
        {
            if let Some(name) = answer
                && let Some(pane) = ui.panes.get_mut(&id)
            {
                pane.info.custom_title = (!name.is_empty()).then_some(name);
            }
            ui.window.request_redraw();
        }
        if let Some(answer) = out.confirmed
            && let Some(ui) = &mut self.ui
            && let Some((id, kind)) = ui.confirm_close.take()
        {
            if answer
                && kind == Confirm::RemoveWorktree
                && let Some(pane) = ui.panes.get_mut(&id)
            {
                // Done by `close_pane`, after the shell and the agent are killed.
                pane.info.remove_worktree = true;
            }
            if answer {
                self.queue_close(id);
            }
            if let Some(ui) = &self.ui {
                ui.window.request_redraw();
            }
        }
        // Archived row indices count from the newest (the list is shown newest first).
        let newest_first = |i: usize, len: usize| len.checked_sub(i + 1);
        if let Some(i) = out.archive_restore
            && let Some(index) = newest_first(i, self.archive.len())
        {
            let session = self.archive.remove(index);
            self.save_archive();
            self.restore_archived(&session);
        }
        if let Some(i) = out.archive_delete
            && let Some(index) = newest_first(i, self.archive.len())
        {
            self.archive.remove(index);
            self.save_archive();
        }
        match out.usage_action {
            Some(hv_ui::UsageAction::Close) => self.toggle_usage(),
            Some(hv_ui::UsageAction::Range(range)) => {
                self.usage.range = range;
                if let Some(data) = &self.usage.data {
                    self.usage.view = Some(Arc::new(
                        data.dashboard(range, self.usage.filter.as_deref()),
                    ));
                }
            }
            Some(hv_ui::UsageAction::FilterAgent(agent)) => {
                self.usage.filter = agent;
                if let Some(data) = &self.usage.data {
                    self.usage.view = Some(Arc::new(
                        data.dashboard(self.usage.range, self.usage.filter.as_deref()),
                    ));
                }
            }
            None => {}
        }
        if out.new_session_clicked {
            self.open_new_session_dialog();
        }
        if let Some(request) = out.new_session.clone() {
            self.start_agent_session(&request);
        }
        if let Some(agent) = out.dock_choice.clone() {
            let folder = self
                .ui
                .as_ref()
                .and_then(Ui::focused)
                .and_then(|p| p.session.cwd())
                .or_else(home_dir)
                .map(|d| d.to_string_lossy().into_owned())
                .unwrap_or_default();
            self.start_agent_session(&hv_ui::NewSession {
                agent,
                folder,
                prompt: String::new(),
                args: String::new(),
                split: false,
                worktree: None,
            });
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
                for id in panes {
                    if let Some(pane) = ui.panes.remove(&id) {
                        pane.session.kill();
                    }
                    if ui.tabs.remove_pane(id) {
                        // That was the last tab: close the window once the event loop is
                        // available (about_to_wait).
                        self.pending_action = Some(Action::CloseWindow);
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
            hv_ui::TabAction::DragWindow => {
                let _ = ui.window.drag_window();
            }
            hv_ui::TabAction::ToggleMaximize => ui.window.set_maximized(!ui.window.is_maximized()),
            hv_ui::TabAction::Minimize => ui.window.set_minimized(true),
            hv_ui::TabAction::CloseWindow => self.pending_action = Some(Action::CloseWindow),
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
/// App-wide data the chrome shows besides the window's own panes.
struct ChromeExtra<'a> {
    /// The usage dashboard, when open.
    usage: Option<hv_ui::UsageView>,
    /// Usage numbers for the hover cards.
    usage_data: Option<&'a UsageData>,
    archived: Vec<hv_ui::ArchivedRow>,
    plan_limits: Vec<String>,
    notice: Option<String>,
}

fn run_chrome(
    ui: &mut Ui,
    sidebar_visible: bool,
    keymap: &Keymap,
    agents: &[hv_ui::AgentChoice],
    extra: ChromeExtra<'_>,
    cursor: egui::CursorIcon,
) -> (hv_render::UiFrame, hv_ui::ChromeOutput) {
    let input = ui.egui.take_egui_input(&ui.window);
    let mut model = ui.chrome_model(sidebar_visible, keymap, agents, extra.usage_data);
    model.usage = extra.usage;
    model.archived = extra.archived;
    model.plan_limits = extra.plan_limits;
    model.notice = extra.notice;
    model.confirm = ui.confirm_close.map(|(id, kind)| match kind {
        Confirm::Close => (
            hv_ui::strings::CONFIRM_CLOSE.to_owned(),
            hv_ui::strings::CLOSE,
        ),
        Confirm::RemoveWorktree => (
            hv_ui::strings::confirm_remove_worktree(
                &ui.panes
                    .get(&id)
                    .and_then(|p| p.info.worktree.as_ref())
                    .map(|(_, dir)| dir.display().to_string())
                    .unwrap_or_default(),
            ),
            hv_ui::strings::REMOVE,
        ),
    });
    model.rename = ui.renaming.and_then(|id| ui.panes.get(&id)).map(|p| {
        p.info
            .custom_title
            .clone()
            .or_else(|| p.info.title.clone())
            .unwrap_or_default()
    });
    model.title_bar = match ui.title_bar {
        TitleBarMode::Native => hv_ui::TitleBar::Native,
        TitleBarMode::Custom => hv_ui::TitleBar::Custom {
            maximized: ui.window.is_maximized(),
        },
        TitleBarMode::MacInset => hv_ui::TitleBar::MacInset,
    };
    model.peek = ui.peek.and_then(|id| ui.panes.get(&id)).map(|pane| {
        let row = crate::model::session_row(&pane.info, None);
        // The last lines with text: where agents print their question.
        let snap = pane.session.snapshot();
        let mut lines: Vec<String> = (0..snap.rows)
            .map(|r| snap.row_text(r).trim_end().to_owned())
            .collect();
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        let start = lines.len().saturating_sub(14);
        hv_ui::PeekView {
            title: row.title,
            subtitle: row.subtitle,
            lines: lines.split_off(start),
        }
    });
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
    // Resize cursors on the window edges win everywhere; the terminal cursor only over panes.
    let on_edge = matches!(
        cursor,
        egui::CursorIcon::ResizeNeSw | egui::CursorIcon::ResizeNwSe
    ) || (ui.title_bar == TitleBarMode::Custom
        && matches!(
            cursor,
            egui::CursorIcon::ResizeHorizontal | egui::CursorIcon::ResizeVertical
        )
        && !pointer_over_terminal);
    if pointer_over_terminal || on_edge {
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
    /// The app ends (also through the macOS Quit menu): close every open working stretch.
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        for ui in self.ui.iter_mut().chain(self.parked.values_mut()) {
            for pane in ui.panes.values_mut() {
                record_interval_now(pane.info.working_interval(true));
            }
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.ui.is_some() {
            return;
        }
        match self.create_ui(event_loop, &mut None) {
            Ok(ui) => {
                ui.window.request_redraw();
                self.ui = Some(ui);
                // F6-03: the first-run wizard, if this is the first run (checked off the UI
                // thread). Not in safe mode.
                if !hv_config::safe_mode() {
                    let proxy = self.proxy.clone();
                    let _ =
                        std::thread::Builder::new()
                            .name("hv-welcome".into())
                            .spawn(move || {
                                if let Some(info) = crate::welcome::prepare() {
                                    let _ = proxy.send_event(AppEvent::Welcome(info));
                                }
                            });
                }
            }
            Err(err) => {
                self.fatal = Some(err);
                event_loop.exit();
            }
        }
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if let StartCause::ResumeTimeReached { .. } = cause {
            for ui in self.ui.iter().chain(self.parked.values()) {
                ui.window.request_redraw();
            }
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::ConfigChanged => self.reload_config(),
            #[cfg(target_os = "macos")]
            AppEvent::Menu(name) => {
                if let Some(id) = self.focused_window {
                    self.activate(id);
                }
                if let Some(action) = Action::parse(&name) {
                    self.run_action(action, event_loop);
                }
            }
            AppEvent::AccessKit(event) => {
                use egui_winit::accesskit_winit::WindowEvent as A11y;
                if !self.activate(event.window_id) {
                    return;
                }
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
                // The pane may live in any window.
                if let Some(id) = target {
                    let owner = self
                        .parked
                        .iter()
                        .find(|(_, ui)| ui.panes.contains_key(&id))
                        .map(|(w, _)| *w);
                    if let Some(window) = owner {
                        self.activate(window);
                    }
                }
                if hook.event == "StatusLine"
                    && let Some(status) = hv_agents::statusline::parse(&hook.payload)
                {
                    self.plan_limits.update(&status);
                }
                if let Some(ui) = &mut self.ui {
                    let focused = ui.focused_id();
                    if let Some(id) = target
                        && let Some(pane) = ui.panes.get_mut(&id)
                    {
                        if pane.info.on_hook(&hook) {
                            if Some(id) != focused && pane.info.needs_attention() {
                                pane.info.unseen = true;
                            }
                            ui.window.request_redraw();
                        }
                        record_interval(pane.info.working_interval(false));
                    }
                    let window_focused = self.focused_window == Some(ui.window.id());
                    let notes = self.notify_candidates(window_focused);
                    self.send_agent_notifications(notes);
                }
            }
            AppEvent::WorktreeReady(mut request, result) => match result {
                Ok((root, dir)) => {
                    request.folder = dir.to_string_lossy().into_owned();
                    request.worktree = None;
                    self.next_worktree = Some((root, dir));
                    self.start_agent_session(&request);
                    self.next_worktree = None;
                }
                Err(err) => {
                    log::warn!("worktree: {err}");
                    hv_platform::notify(hv_ui::strings::WORKTREE_FAILED, &err);
                }
            },
            AppEvent::Welcome(info) => {
                if let Some(ui) = &mut self.ui {
                    ui.chrome.open_welcome(info);
                    ui.window.request_redraw();
                }
            }
            AppEvent::UsageLoaded(data) => {
                self.usage.loading = false;
                self.usage.view = Some(Arc::new(
                    data.dashboard(self.usage.range, self.usage.filter.as_deref()),
                ));
                self.usage.data = Some(data);
                for ui in self.ui.iter().chain(self.parked.values()) {
                    ui.window.request_redraw();
                }
            }
            AppEvent::Agent { pane, agent } => {
                let owner = self
                    .parked
                    .iter()
                    .find(|(_, ui)| ui.panes.contains_key(&pane))
                    .map(|(w, _)| *w);
                if let Some(window) = owner {
                    self.activate(window);
                }
                let name = agent
                    .as_deref()
                    .and_then(|id| hv_agents::profile::find(&self.profiles, id))
                    .map(|p| p.name.clone());
                if let Some(ui) = &mut self.ui
                    && let Some(p) = ui.panes.get_mut(&pane)
                {
                    if agent != p.info.agent_id {
                        // The stretch belongs to the agent that was running.
                        record_interval(p.info.working_interval(true));
                    }
                    if agent.is_none() && p.info.agent_id.is_some() {
                        // The agent exited: back to a plain shell.
                        p.info.agent = hv_agents::Tracker::default();
                    }
                    p.info.agent_id = agent;
                    p.info.agent_name = name;
                    ui.window.request_redraw();
                }
            }
            AppEvent::Chips { pane, line, info } => {
                let owner = self
                    .parked
                    .iter()
                    .find(|(_, ui)| ui.panes.contains_key(&pane))
                    .map(|(w, _)| *w);
                if let Some(window) = owner {
                    self.activate(window);
                }
                if let Some(ui) = &mut self.ui
                    && let Some(p) = ui.panes.get_mut(&pane)
                {
                    p.chips.insert(line, info);
                    // Keep the newest few hundred prompts.
                    while p.chips.len() > 500 {
                        p.chips.pop_first();
                    }
                    ui.window.request_redraw();
                }
            }
            AppEvent::Wakeup => {
                self.wake_pending.store(false, Ordering::Release);
                for id in self.window_ids() {
                    if self.activate(id) {
                        self.handle_session_events(event_loop);
                    }
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if !self.activate(id) {
            return;
        }
        // egui sees pointer and window events; the keyboard belongs to the terminal unless an
        // egui text field (the command palette) is open.
        let palette_open = self
            .ui
            .as_ref()
            .is_some_and(|ui| ui.chrome.takes_keyboard() || ui.usage_open);
        // A press on a resize edge starts the OS resize loop, which swallows the release: egui
        // must not see the press, or it would think the button stays down.
        let resize_press = self.mouse.resize_edge.is_some()
            && matches!(
                event,
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Left,
                    ..
                }
            );
        if let Some(ui) = &mut self.ui
            && !matches!(event, WindowEvent::RedrawRequested)
            && !resize_press
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
                // The layout of the last window closed is the one reopened next time.
                if self.parked.is_empty() {
                    self.save_layout();
                }
                self.close_window(event_loop);
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
            WindowEvent::ThemeChanged(theme) => {
                let light = self
                    .config
                    .theme
                    .is_light(Some(theme == winit::window::Theme::Light));
                self.set_theme(light);
            }
            WindowEvent::Focused(true) => self.focused_window = Some(id),
            WindowEvent::Focused(false) => {
                if self.focused_window == Some(id) {
                    self.focused_window = None;
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
        self.sync_watched_panes();
        // F4-19: how many sessions need the user, on the app icon.
        let badge = self
            .ui
            .iter()
            .chain(self.parked.values())
            .flat_map(|ui| ui.panes.values())
            .filter(|p| {
                crate::model::session_row(&p.info, None).state == hv_ui::RowState::Waiting
                    || p.info.unseen
            })
            .count();
        if badge != self.badge {
            self.badge = badge;
            hv_platform::set_badge(badge);
        }
        let group_window =
            Duration::from_secs(self.config.notifications.group_window_seconds.max(1));
        if let Some(count) = self.notify_group.due_summary(Instant::now(), group_window) {
            notify(
                &self.config.notifications,
                &hv_ui::strings::sessions_need_you(count),
                hv_ui::strings::NOTIFY_OPEN_HIVETTY,
            );
        }
        let mut deadline: Option<Instant> = self.notify_group.deadline(group_window);
        for id in self.window_ids() {
            if !self.activate(id) {
                continue;
            }
            if let Some(action) = self.pending_action.take() {
                self.run_action(action, event_loop);
            }
            if let Some(ui) = &mut self.ui {
                let now = Instant::now();
                for pane in ui.panes.values_mut() {
                    if pane
                        .pending_command
                        .as_ref()
                        .is_some_and(|(_, at)| now >= *at)
                        && let Some((command, _)) = pane.pending_command.take()
                    {
                        pane.session.write(command.as_bytes());
                    }
                }
            }
            // Panes that are not drawn (background tabs, zoomed-out panes) still need their
            // expired synchronized updates closed, or their deadline stays in the past.
            if let Some(ui) = &self.ui {
                let mut flushed = false;
                for pane in ui.panes.values() {
                    flushed |= pane.session.flush_expired_sync();
                }
                if flushed {
                    ui.window.request_redraw();
                }
            }
            if let Some(next) = self.window_deadline() {
                deadline = Some(deadline.map_or(next, |d| d.min(next)));
            }
        }
        event_loop.set_control_flow(deadline.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
}

impl App {
    /// When the active window next needs a frame without new input.
    fn window_deadline(&self) -> Option<Instant> {
        let ui = self.ui.as_ref()?;
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
        let pending = ui
            .panes
            .values()
            .filter_map(|p| p.pending_command.as_ref().map(|(_, at)| *at))
            .min();
        [sync, self.bell_until, tick, ui.repaint_at, pending]
            .into_iter()
            .flatten()
            .min()
    }
}

/// Usage records and tasks read from the agents' logs.
#[derive(Debug, Default)]
pub(crate) struct UsageData {
    records: Vec<hv_usage::UsageRecord>,
    tasks: Vec<hv_usage::Task>,
    /// The user's price table, read with the logs (U-60).
    pricing: hv_usage::pricing::Pricing,
    /// Totals by agent session id, for the hover card (S-15).
    sessions: HashMap<String, SessionTotals>,
    /// Agent time hivetty measured (F4-23).
    observed: Vec<hv_usage::observed::Interval>,
    read_at: Option<Instant>,
}

impl UsageData {
    /// Reads every supported agent's logs (Claude Code, Codex, Gemini CLI). Blocking: call
    /// off the UI thread.
    pub(crate) fn read() -> Self {
        // Incremental: the cache remembers how far each log was read (docs/adr/0006).
        let cache = hv_config::data_dir().map(|d| d.join("usage.cache"));
        let mut store = cache
            .as_deref()
            .map(hv_usage::store::Store::load)
            .unwrap_or_default();
        // Each agent's own logs (F5-03): Claude Code, Codex and Gemini CLI.
        let mut added = (0, 0);
        let mut add = |(r, t): (usize, usize)| {
            added.0 += r;
            added.1 += t;
        };
        if let Some(root) = hv_usage::claude::default_root() {
            add(store.ingest_claude(&root));
        }
        if let Some(root) = hv_usage::codex::default_root() {
            add(store.ingest_codex(&root));
        }
        if let Some(root) = hv_usage::gemini::default_root() {
            add(store.ingest_gemini(&root));
        }
        if added != (0, 0)
            && let Some(cache) = &cache
            && let Err(err) = store.save(cache)
        {
            log::warn!("cannot save the usage cache: {err}");
        }
        let records = store.records();
        let pricing = load_pricing();
        let sessions = session_totals(&records, &pricing);
        Self {
            records,
            tasks: store.tasks(),
            pricing,
            sessions,
            observed: observed_path()
                .map(|p| hv_usage::observed::load(&p))
                .unwrap_or_default(),
            read_at: Some(Instant::now()),
        }
    }

    /// The records and tasks read.
    pub(crate) fn parts(&self) -> (&[hv_usage::UsageRecord], &[hv_usage::Task]) {
        (&self.records, &self.tasks)
    }

    /// The user's price table read with the logs.
    pub(crate) fn pricing(&self) -> &hv_usage::pricing::Pricing {
        &self.pricing
    }

    /// The dashboard for `range`, ending today in local time, optionally for one agent.
    pub(crate) fn dashboard(
        &self,
        range: hv_ui::UsageRange,
        agent: Option<&str>,
    ) -> hv_usage::dashboard::Dashboard {
        let tz = jiff::tz::TimeZone::system();
        let today = jiff::Timestamp::now().to_zoned_datetime(tz.clone()).date();
        let pricing = &self.pricing;
        let mut dash = match agent {
            None => hv_usage::dashboard::with_cost(
                hv_usage::dashboard::build(&self.records, &self.tasks, range.days(), today, &tz),
                &self.records,
                pricing,
                &tz,
            ),
            Some(id) => {
                let records: Vec<_> = self
                    .records
                    .iter()
                    .filter(|r| r.agent == id)
                    .cloned()
                    .collect();
                let tasks: Vec<_> = self
                    .tasks
                    .iter()
                    .filter(|t| t.agent == id)
                    .cloned()
                    .collect();
                hv_usage::dashboard::with_cost(
                    hv_usage::dashboard::build(&records, &tasks, range.days(), today, &tz),
                    &records,
                    pricing,
                    &tz,
                )
            }
        };
        if !self.observed.is_empty() {
            dash.observed_seconds = Some(hv_usage::observed::seconds_in(
                &self.observed,
                agent,
                dash.first_day,
                dash.last_day,
                &tz,
            ));
        }
        // Agent ids to display names ("claude-code" -> "Claude Code").
        let profiles = hv_agents::profile::builtin();
        for row in &mut dash.agents {
            if let Some(profile) = hv_agents::profile::find(&profiles, &row.agent) {
                row.agent.clone_from(&profile.name);
            }
        }
        dash
    }
}

/// Dashboard state, shared by all windows.
#[derive(Debug)]
struct UsageState {
    open: bool,
    range: hv_ui::UsageRange,
    data: Option<Arc<UsageData>>,
    loading: bool,
    view: Option<Arc<hv_usage::dashboard::Dashboard>>,
    /// Agent id the panel is filtered to (U-51).
    filter: Option<String>,
}

impl Default for UsageState {
    fn default() -> Self {
        Self {
            open: false,
            range: hv_ui::UsageRange::Month,
            data: None,
            loading: false,
            view: None,
            filter: None,
        }
    }
}

/// The plan's 5-hour and weekly windows, from the agents' statuslines (F4-32, U-65).
#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct PlanLimits {
    five_hour: Option<hv_agents::statusline::Limit>,
    seven_day: Option<hv_agents::statusline::Limit>,
}

impl PlanLimits {
    fn update(&mut self, status: &hv_agents::statusline::StatusLine) {
        self.five_hour = status.five_hour.or(self.five_hour);
        self.seven_day = status.seven_day.or(self.seven_day);
    }

    /// The windows that have not reset yet, worded for the dashboard.
    fn lines(&self) -> Vec<String> {
        let now = jiff::Timestamp::now().as_second();
        [
            (hv_ui::strings::FIVE_HOUR_LIMIT, self.five_hour),
            (hv_ui::strings::WEEKLY_LIMIT, self.seven_day),
        ]
        .into_iter()
        .zip([5 * 3600, 7 * 86_400])
        .filter_map(|((name, limit), window)| {
            let limit = limit?;
            let left = u64::try_from(limit.resets_at - now)
                .ok()
                .filter(|s| *s > 0)?;
            let mut line =
                hv_ui::strings::plan_limit(name, limit.used_percentage, &countdown(left));
            if let Some(out) = runs_out_in(limit.used_percentage, window, left) {
                line = format!("{line} · {}", hv_ui::strings::runs_out_in(&countdown(out)));
            }
            Some(line)
        })
        .collect()
    }
}

/// Seconds until a plan window reaches 100 % at the pace so far, when that is before it resets.
/// `window` is its length and `left` the seconds until it resets.
fn runs_out_in(used: f64, window: u64, left: u64) -> Option<u64> {
    let elapsed = window.checked_sub(left).filter(|e| *e >= 60)? as f64;
    if !(used > 0.0 && used < 100.0) {
        return None;
    }
    let out = (100.0 - used) / (used / elapsed);
    (out < left as f64).then_some(out as u64)
}

/// `5d 11h`, `2h 10m`, `7m`.
fn countdown(seconds: u64) -> String {
    let (d, h, m) = (seconds / 86_400, seconds / 3600 % 24, seconds / 60 % 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{}m", m.max(1))
    }
}

/// A desktop notification, also forwarded to the phone when configured (F4-33).
fn notify(config: &hv_config::Notifications, title: &str, body: &str) {
    hv_platform::notify(title, body);
    crate::forward::send(&config.forward, title, body);
}

/// Where the agent time hivetty measures is kept.
fn observed_path() -> Option<std::path::PathBuf> {
    hv_config::data_dir().map(|d| d.join("agent-time.jsonl"))
}

/// Appends a finished working stretch on a background thread (F4-23).
/// The same, on this thread (when the app is about to end).
fn record_interval_now(interval: Option<hv_usage::observed::Interval>) {
    if let (Some(interval), Some(path)) = (interval, observed_path())
        && let Err(err) = hv_usage::observed::append(&path, &interval)
    {
        log::warn!("cannot record agent time: {err}");
    }
}

fn record_interval(interval: Option<hv_usage::observed::Interval>) {
    let (Some(interval), Some(path)) = (interval, observed_path()) else {
        return;
    };
    let _ = std::thread::Builder::new()
        .name("hv-agent-time".into())
        .spawn(move || {
            if let Err(err) = hv_usage::observed::append(&path, &interval) {
                log::warn!("cannot record agent time: {err}");
            }
        });
}

/// Usage of one agent session.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct SessionTotals {
    /// The model of its newest turn.
    model: String,
    newest: String,
    input: u64,
    output: u64,
    /// USD, when every turn had a price.
    cost: Option<f64>,
}

/// Sums the records of each agent session.
fn session_totals(
    records: &[hv_usage::UsageRecord],
    pricing: &hv_usage::pricing::Pricing,
) -> HashMap<String, SessionTotals> {
    let mut out: HashMap<String, SessionTotals> = HashMap::new();
    for r in records {
        let Some(id) = &r.session_id else { continue };
        let totals = out.entry(id.clone()).or_insert_with(|| SessionTotals {
            cost: (!pricing.is_empty()).then_some(0.0),
            ..SessionTotals::default()
        });
        totals.input += r.total_input();
        totals.output += r.output_tokens;
        // RFC 3339 timestamps in UTC sort as text.
        if r.timestamp >= totals.newest {
            totals.newest.clone_from(&r.timestamp);
            totals.model.clone_from(&r.model);
        }
        totals.cost = totals.cost.zip(pricing.cost(r)).map(|(a, b)| a + b);
    }
    out
}

/// The project a pane belongs to: its repository when the chips found one, else its folder.
fn project_dir(pane: &Pane) -> Option<std::path::PathBuf> {
    pane.chips
        .values()
        .next_back()
        .and_then(|info| info.root.clone())
        .or_else(|| pane.session.cwd())
}

/// The hover card of a pane (S-15): only the lines that are known.
fn session_card(pane: &Pane, usage: Option<&UsageData>) -> Vec<(String, String)> {
    use hv_ui::strings as s;
    let mut card = Vec::new();
    if let Some(agent) = &pane.info.agent_name {
        card.push((s::CARD_AGENT.to_owned(), agent.clone()));
    }
    let totals = pane
        .info
        .agent
        .agent_session()
        .and_then(|id| usage?.sessions.get(id));
    if let Some(t) = totals.filter(|t| !t.model.is_empty()) {
        card.push((s::CARD_MODEL.to_owned(), t.model.clone()));
    }
    let project = pane.chips.values().next_back();
    if let Some(branch) = project.and_then(|p| p.branch.clone()) {
        card.push((s::CARD_BRANCH.to_owned(), branch));
    }
    if let Some(dir) = pane.session.cwd() {
        card.push((s::CARD_FOLDER.to_owned(), tilde(&dir)));
    }
    if let Some(t) = totals {
        card.push((
            s::CARD_TOKENS.to_owned(),
            s::card_tokens(&hv_usage::compact(t.input), &hv_usage::compact(t.output)),
        ));
        if let Some(usd) = t.cost {
            card.push((s::CARD_COST.to_owned(), s::api_equivalent(usd)));
        }
    }
    let started = pane.info.agent.since().unwrap_or(pane.info.started);
    card.push((
        s::CARD_STARTED.to_owned(),
        s::ago(&hv_ui::format_elapsed(started.elapsed().as_secs())),
    ));
    if let Some(percent) = pane.info.context_percent {
        card.push((s::CARD_CONTEXT.to_owned(), format!("{percent:.0}%")));
    }
    if let Some(tool) = &pane.info.last_tool {
        card.push((s::CARD_LAST_TOOL.to_owned(), tool.clone()));
    }
    card
}

/// A path with the home directory shown as `~`.
fn tilde(path: &std::path::Path) -> String {
    match home_dir().and_then(|home| path.strip_prefix(home).ok().map(std::path::Path::to_owned)) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
        None => path.display().to_string(),
    }
}

/// The user's price table (`pricing.toml` next to `config.toml`), or none. Blocking.
fn load_pricing() -> hv_usage::pricing::Pricing {
    let Some(path) = hv_config::default_path().map(|p| p.with_file_name("pricing.toml")) else {
        return hv_usage::pricing::Pricing::default();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return hv_usage::pricing::Pricing::default();
    };
    hv_usage::pricing::Pricing::parse(&text).unwrap_or_else(|err| {
        log::warn!("{}: {err}", path.display());
        hv_usage::pricing::Pricing::default()
    })
}

/// Usage logs older than this are read again when the dashboard opens.
const USAGE_REFRESH: Duration = Duration::from_secs(60);

/// Merges bursts of notifications: up to three per window of time go out as they are, the rest
/// become one "N sessions need you" when the window ends (docs/ui-spec.md §7).
#[derive(Debug, Default)]
struct NotifyGroup {
    started: Option<Instant>,
    sent: u32,
    held: u32,
}

impl NotifyGroup {
    /// Whether to send this one now (otherwise it is counted for the summary).
    fn admit(&mut self, now: Instant, window: Duration) -> bool {
        if self.started.is_none_or(|s| now.duration_since(s) >= window) {
            self.started = Some(now);
            self.sent = 0;
        }
        if self.sent < 3 {
            self.sent += 1;
            true
        } else {
            self.held += 1;
            false
        }
    }

    /// The summary to send once the window is over, if anything was held back.
    fn due_summary(&mut self, now: Instant, window: Duration) -> Option<u32> {
        let started = self.started?;
        (self.held > 0 && now.duration_since(started) >= window).then(|| {
            let held = std::mem::take(&mut self.held);
            self.started = None;
            held
        })
    }

    fn deadline(&self, window: Duration) -> Option<Instant> {
        self.started.filter(|_| self.held > 0).map(|s| s + window)
    }
}

/// A tab and its panes moved into a new window.
type Adopted = (crate::workspace::Tab, Vec<(PaneId, Pane)>);

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

/// "45s", "3m 20s", "1h 5m".
fn took_text(took: Duration) -> String {
    let s = took.as_secs();
    match s {
        0..60 => format!("{s}s"),
        60..3600 => format!("{}m {}s", s / 60, s % 60),
        _ => format!("{}h {}m", s / 3600, s % 3600 / 60),
    }
}

/// The line that resumes a saved agent session, following `[agents] restore_on_launch`:
/// `"always"` runs it, `"ask"` types it without Enter so the user decides, `"never"` skips it.
/// Only ids made of letters, digits, `-` and `_` are typed (they come from a file on disk).
fn resume_command(
    profiles: &[hv_agents::profile::Profile],
    agent: &crate::saved::SavedAgent,
    mode: &str,
) -> Option<String> {
    if mode == "never" {
        return None;
    }
    let session = agent.session.as_deref()?;
    let safe = !session.is_empty()
        && session.len() <= 128
        && session
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !safe {
        return None;
    }
    let profile = hv_agents::profile::find(profiles, &agent.id)?;
    let mut line = profile
        .resume
        .as_ref()?
        .command
        .replace("{session_id}", session);
    if mode == "always" {
        line.push('\r');
    }
    Some(line)
}

/// What a confirmation dialog asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Confirm {
    Close,
    RemoveWorktree,
}

/// Who draws the title bar (F1-02).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TitleBarMode {
    /// The OS.
    Native,
    /// hivetty, in the tab bar (no OS frame).
    Custom,
    /// macOS: a transparent OS title bar over the tab bar.
    MacInset,
}

impl TitleBarMode {
    fn from_config(decorations: &str) -> Self {
        match decorations {
            "full" => Self::Native,
            "none" => Self::Custom,
            _ if cfg!(windows) => Self::Custom,
            _ if cfg!(target_os = "macos") => Self::MacInset,
            // Linux: Wayland needs client-side decorations for a borderless window; keep the
            // system's until hivetty draws those.
            _ => Self::Native,
        }
    }
}

/// Width of the resize band of a borderless window, in logical pixels.
const RESIZE_BAND: f32 = 6.0;

/// The window edge or corner at a point (physical pixels), for resizing a borderless window.
fn resize_edge(
    width: f32,
    height: f32,
    x: f32,
    y: f32,
    band: f32,
) -> Option<winit::window::ResizeDirection> {
    use winit::window::ResizeDirection as R;
    let (left, right) = (x < band, x >= width - band);
    let (top, bottom) = (y < band, y >= height - band);
    match (left, right, top, bottom) {
        (true, _, true, _) => Some(R::NorthWest),
        (_, true, true, _) => Some(R::NorthEast),
        (true, _, _, true) => Some(R::SouthWest),
        (_, true, _, true) => Some(R::SouthEast),
        (true, ..) => Some(R::West),
        (_, true, ..) => Some(R::East),
        (_, _, true, _) => Some(R::North),
        (_, _, _, true) => Some(R::South),
        _ => None,
    }
}

/// The config's custom terminal palette (or high contrast), or hivetty's own.
fn set_palette(theme: &hv_config::Theme) {
    hv_term::set_custom(theme.palette().map(|p| hv_term::CustomPalette {
        foreground: p.foreground,
        background: p.background,
        cursor: p.cursor,
        selection: p.selection,
        ansi: p.ansi,
    }));
}

/// Applies the light or dark theme to the terminals, the renderer and the chrome. The terminal
/// palette is global, so every pane follows.
fn apply_theme(chrome: &hv_ui::Chrome, light: bool) {
    hv_term::set_light(light);
    hv_render::set_light(light);
    chrome.set_light(light);
}

/// The user's home directory.
fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(std::path::PathBuf::from)
}

/// The line typed into the shell to start an agent: its command, the extra arguments as typed,
/// and the prompt as one quoted argument (agents that take a first prompt as an argument).
fn agent_command_line(
    profile: &hv_agents::profile::Profile,
    request: &hv_ui::NewSession,
    shell: &str,
) -> String {
    let mut line = profile.command.clone();
    if !request.args.is_empty() {
        line.push(' ');
        line.push_str(&request.args);
    }
    if !request.prompt.is_empty() && profile.prompt_arg {
        let quoting = hv_term::input::Quoting::for_program(shell);
        line.push(' ');
        line.push_str(&hv_term::input::quote_path(&request.prompt, quoting));
    }
    line.push('\r');
    line
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_pace_estimates_when_the_limit_runs_out() {
        // 50 % used in the first hour of 5: 1 more hour at this pace, 4 h before the reset.
        assert_eq!(runs_out_in(50.0, 5 * 3600, 4 * 3600), Some(3600));
        // 10 % in 4 h: would take far longer than the hour left.
        assert_eq!(runs_out_in(10.0, 5 * 3600, 3600), None);
        // Too early to tell, nothing used, or already out.
        assert_eq!(runs_out_in(50.0, 5 * 3600, 5 * 3600 - 10), None);
        assert_eq!(runs_out_in(0.0, 5 * 3600, 3600), None);
        assert_eq!(runs_out_in(100.0, 5 * 3600, 3600), None);
    }

    #[test]
    fn plan_limits_count_down_and_expire() {
        assert_eq!(countdown(5 * 86_400 + 11 * 3600 + 59), "5d 11h");
        assert_eq!(countdown(2 * 3600 + 10 * 60), "2h 10m");
        assert_eq!(countdown(20), "1m");
        let now = jiff::Timestamp::now().as_second();
        let mut limits = PlanLimits::default();
        limits.update(&hv_agents::statusline::StatusLine {
            five_hour: Some(hv_agents::statusline::Limit {
                used_percentage: 23.5,
                resets_at: now + 2 * 3600 + 630,
            }),
            seven_day: Some(hv_agents::statusline::Limit {
                used_percentage: 41.0,
                resets_at: now - 10, // already reset: not shown
            }),
            ..Default::default()
        });
        let lines = limits.lines();
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].starts_with("5-hour limit 24% · resets in 2h 1"),
            "{}",
            lines[0]
        );
        // A statusline without limits keeps the last ones.
        limits.update(&hv_agents::statusline::StatusLine::default());
        assert_eq!(limits.lines().len(), 1);
    }

    #[test]
    fn session_totals_sum_tokens_and_keep_the_newest_model() {
        let rec = |session: &str, ts: &str, model: &str, input: u64| hv_usage::UsageRecord {
            agent: "claude-code",
            provider: "anthropic",
            model: model.into(),
            timestamp: ts.into(),
            session_id: Some(session.into()),
            project: None,
            input_tokens: input,
            output_tokens: 10,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        };
        let records = [
            rec("a", "2026-11-20T10:00:00Z", "model-old", 100),
            rec("a", "2026-11-20T11:00:00Z", "model-new", 50),
            rec("b", "2026-11-20T09:00:00Z", "other", 1),
        ];
        let none = hv_usage::pricing::Pricing::default();
        let totals = session_totals(&records, &none);
        let a = &totals["a"];
        assert_eq!(
            (a.input, a.output, a.model.as_str()),
            (150, 20, "model-new")
        );
        assert_eq!(a.cost, None, "no price table, no cost");
        let priced = hv_usage::pricing::Pricing::parse(
            "[models.model]
input = 1.0
output = 1.0
",
        )
        .unwrap();
        assert!(session_totals(&records, &priced)["a"].cost.is_some());
        assert_eq!(
            session_totals(&records, &priced)["b"].cost,
            None,
            "unpriced model"
        );
    }

    #[test]
    fn bursts_of_notifications_are_merged() {
        let window = Duration::from_secs(10);
        let t0 = Instant::now();
        let mut group = NotifyGroup::default();
        let sent: Vec<bool> = (0..5)
            .map(|i| group.admit(t0 + Duration::from_secs(i), window))
            .collect();
        assert_eq!(sent, [true, true, true, false, false]);
        assert_eq!(group.due_summary(t0 + Duration::from_secs(5), window), None);
        assert_eq!(group.deadline(window), Some(t0 + window));
        assert_eq!(group.due_summary(t0 + window, window), Some(2));
        // A new window starts fresh.
        assert!(group.admit(t0 + Duration::from_secs(30), window));
    }

    #[test]
    fn agent_command_lines_quote_the_prompt_for_the_shell() {
        let profiles = hv_agents::profile::builtin();
        let claude = hv_agents::profile::find(&profiles, "claude-code").unwrap();
        let request = |prompt: &str, args: &str| hv_ui::NewSession {
            agent: "claude-code".into(),
            folder: ".".into(),
            prompt: prompt.into(),
            args: args.into(),
            split: false,
            worktree: None,
        };
        assert_eq!(
            agent_command_line(claude, &request("", "--model opus"), "/bin/zsh"),
            "claude --model opus\r"
        );
        assert_eq!(
            agent_command_line(claude, &request("fix it's tests", ""), "pwsh.exe"),
            "claude 'fix it''s tests'\r"
        );
        // Agents that do not take a prompt argument just start.
        let aider = hv_agents::profile::find(&profiles, "aider").unwrap();
        assert_eq!(
            agent_command_line(aider, &request("hi", ""), "bash"),
            "aider\r"
        );
    }

    #[test]
    fn resume_commands_follow_the_mode_and_refuse_odd_ids() {
        let profiles = hv_agents::profile::builtin();
        let agent = |session: &str| crate::saved::SavedAgent {
            id: "claude-code".into(),
            session: Some(session.into()),
        };
        let id = "6f1c0a52-9b9e-4a7e-8f61-2c0e5b8d1a77";
        assert_eq!(
            resume_command(&profiles, &agent(id), "always").as_deref(),
            Some(format!("claude --resume {id}\r").as_str())
        );
        assert_eq!(
            resume_command(&profiles, &agent(id), "ask").as_deref(),
            Some(format!("claude --resume {id}").as_str())
        );
        assert_eq!(resume_command(&profiles, &agent(id), "never"), None);
        assert_eq!(
            resume_command(&profiles, &agent("x; rm -rf ~"), "always"),
            None
        );
    }

    #[test]
    fn edges_and_corners_of_a_borderless_window() {
        use winit::window::ResizeDirection as R;
        assert_eq!(resize_edge(800.0, 600.0, 2.0, 300.0, 6.0), Some(R::West));
        assert_eq!(
            resize_edge(800.0, 600.0, 798.0, 598.0, 6.0),
            Some(R::SouthEast)
        );
        assert_eq!(resize_edge(800.0, 600.0, 400.0, 1.0, 6.0), Some(R::North));
        assert_eq!(resize_edge(800.0, 600.0, 400.0, 300.0, 6.0), None);
        assert_eq!(TitleBarMode::from_config("full"), TitleBarMode::Native);
        assert_eq!(TitleBarMode::from_config("none"), TitleBarMode::Custom);
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(took_text(Duration::from_secs(45)), "45s");
        assert_eq!(took_text(Duration::from_secs(200)), "3m 20s");
        assert_eq!(took_text(Duration::from_secs(3900)), "1h 5m");
    }
}
