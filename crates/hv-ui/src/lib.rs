//! Window chrome built with egui (ADR 0003): sidebar, and later tabs, attention inbox and the
//! usage dashboard. The terminal grid itself is drawn by `hv-render` in the space egui leaves.

mod new_session;
mod palette;
mod peek;
mod sidebar;
pub mod strings;
mod tabs;
mod theme;
mod usage;

pub use new_session::{AgentChoice, NewSession};
pub use palette::PaletteItem;
pub use peek::{PeekAction, PeekView};
pub use tabs::{TAB_BAR_HEIGHT, TabAction, TabItem, TitleBar};
pub use usage::{UsageAction, UsageRange, UsageView};

use egui::{Color32, Margin, Stroke};

/// What the sidebar shows for one session (filled by the app from the session state).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRow {
    /// Title (from the program's OSC 2 title, or the shell name).
    pub title: String,
    /// Second line: current activity or state.
    pub subtitle: String,
    /// Project the session belongs to (git root or folder name).
    pub project: String,
    /// Elapsed time, already formatted (`27s`, `21m`).
    pub elapsed: String,
    /// Coarse state, for the colored dot.
    pub state: RowState,
    /// Whether this is the focused session.
    pub focused: bool,
    /// Agent name, if one runs there (also searchable).
    pub agent: String,
    /// Finished or needs the user and has not been looked at yet.
    pub unseen: bool,
}

/// Coarse session state shown in the sidebar (docs/ui-spec.md 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    /// Program running.
    Working,
    /// Needs the user.
    Waiting,
    /// Finished successfully.
    Done,
    /// Finished with an error.
    Error,
    /// Plain shell, idle.
    Idle,
}

/// Everything the chrome needs for one frame.
#[derive(Debug, Clone, Default)]
pub struct ChromeModel {
    /// Sessions to list.
    pub sessions: Vec<SessionRow>,
    /// Whether the sidebar is visible.
    pub sidebar_visible: bool,
    /// Tabs of the window. The bar is hidden when empty.
    pub tabs: Vec<TabItem>,
    /// Sections of the ☰ menu in the tab bar; empty = no menu button (macOS uses the system
    /// menu bar instead).
    pub menu: Vec<Vec<PaletteItem>>,
    /// Installed agents for the dock and the New session dialog (a plain shell first).
    pub agents: Vec<AgentChoice>,
    /// Shortcut shown on the New session button ("Ctrl+Shift+A").
    pub new_session_hint: String,
    /// The usage dashboard, drawn over the terminals when open (F5).
    pub usage: Option<UsageView>,
    /// A session being peeked at (S-20).
    pub peek: Option<PeekView>,
    /// Whether the tab bar is also the window's title bar (F1-02).
    pub title_bar: TitleBar,
}

/// What the chrome produced this frame.
pub struct ChromeOutput {
    /// egui's full output (shapes, textures, platform requests).
    pub full: egui::FullOutput,
    /// Space left for the terminal grid, in physical pixels: `[x, y, width, height]`.
    pub terminal_area: [f32; 4],
    /// Index of a session row the user clicked.
    pub clicked_session: Option<usize>,
    /// A click in the tab bar.
    pub tab_action: Option<TabAction>,
    /// The command chosen in the palette (its [`PaletteItem::id`]).
    pub palette_choice: Option<usize>,
    /// The New session button was pressed (the app opens the dialog with its defaults).
    pub new_session_clicked: bool,
    /// The dialog was confirmed.
    pub new_session: Option<NewSession>,
    /// An agent in the dock was clicked (profile id).
    pub dock_choice: Option<String>,
    /// Something done in the usage dashboard.
    pub usage_action: Option<UsageAction>,
    /// Index of a session row whose Peek was asked for.
    pub peek_request: Option<usize>,
    /// Something done in the peek popover.
    pub peek_action: Option<PeekAction>,
}

impl std::fmt::Debug for ChromeOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChromeOutput")
            .field("terminal_area", &self.terminal_area)
            .field("clicked_session", &self.clicked_session)
            .field("tab_action", &self.tab_action)
            .finish_non_exhaustive()
    }
}

/// The egui-based window chrome.
#[derive(Debug, Default)]
pub struct Chrome {
    ctx: egui::Context,
    palette: Option<palette::Palette>,
    tab_bar: tabs::TabBar,
    sidebar: sidebar::Sidebar,
    new_session: Option<new_session::Dialog>,
    peek: Option<peek::Peek>,
}

/// Sidebar width in points (docs/ui-spec.md 1: 300 px default; narrower until tabs exist).
const SIDEBAR_WIDTH: f32 = 260.0;

impl Chrome {
    /// Creates the chrome with hivetty's dark theme.
    pub fn new() -> Self {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        Self {
            ctx,
            palette: None,
            tab_bar: tabs::TabBar::default(),
            sidebar: sidebar::Sidebar::default(),
            new_session: None,
            peek: None,
        }
    }

    /// The egui context (for the winit integration).
    pub fn context(&self) -> &egui::Context {
        &self.ctx
    }

    /// Runs one UI frame.
    pub fn run(&mut self, input: egui::RawInput, model: &ChromeModel) -> ChromeOutput {
        let mut clicked_session = None;
        let mut tab_action = None;
        let mut palette_choice = None;
        let mut central = egui::Rect::NOTHING;
        let palette = &mut self.palette;
        let tab_bar = &mut self.tab_bar;
        let sidebar_state = &mut self.sidebar;
        let dialog = &mut self.new_session;
        let mut new_session_clicked = false;
        let mut new_session = None;
        let mut dock_choice = None;
        let mut usage_action = None;
        let mut peek_request = None;
        let mut peek_action = None;
        if model.peek.is_none() {
            self.peek = None;
        } else if self.peek.is_none() {
            self.peek = Some(peek::Peek::opened());
        }
        let peek_state = &mut self.peek;
        let full = self.ctx.run_ui(input, |ui| {
            if model.sidebar_visible {
                egui::Panel::left("sidebar")
                    .exact_size(SIDEBAR_WIDTH)
                    .resizable(false)
                    .frame(
                        egui::Frame::new()
                            .fill(theme::SIDEBAR_BG())
                            .inner_margin(Margin::symmetric(10, 12))
                            .stroke(Stroke::new(1.0, theme::BORDER())),
                    )
                    .show(ui, |ui| {
                        let out = sidebar_state.show(ui, &model.sessions, &model.new_session_hint);
                        clicked_session = out.0;
                        new_session_clicked = out.1;
                        peek_request = out.2;
                        // The dock sits under the list (which leaves room for it).
                        ui.separator();
                        dock_choice = new_session::dock(ui, &model.agents);
                    });
            }
            if !model.tabs.is_empty() {
                egui::Panel::top("tabs")
                    .exact_size(TAB_BAR_HEIGHT)
                    .resizable(false)
                    .frame(
                        egui::Frame::new()
                            .fill(theme::SIDEBAR_BG())
                            .inner_margin(Margin::symmetric(6, 0))
                            .stroke(Stroke::new(1.0, theme::BORDER())),
                    )
                    .show(ui, |ui| {
                        tab_action = tab_bar.show(ui, &model.tabs, &model.menu, model.title_bar);
                    });
            }
            central = ui.available_rect_before_wrap();
            if let Some(view) = &model.usage {
                usage_action = usage::show(ui.ctx(), central, view);
            }
            if let (Some(view), Some(state)) = (&model.peek, peek_state.as_mut()) {
                peek_action = state.show(ui.ctx(), view);
            }
            if let Some(open) = dialog.as_mut() {
                match open.show(ui.ctx()) {
                    new_session::Outcome::Open => {}
                    new_session::Outcome::Cancelled => *dialog = None,
                    new_session::Outcome::Create(request) => {
                        new_session = Some(request);
                        *dialog = None;
                    }
                }
            }
            if let Some(open) = palette.as_mut() {
                match open.show(ui.ctx()) {
                    palette::PaletteOutcome::Open => {}
                    palette::PaletteOutcome::Dismissed => *palette = None,
                    palette::PaletteOutcome::Chosen(id) => {
                        palette_choice = Some(id);
                        *palette = None;
                    }
                }
            }
        });
        let ppp = full.pixels_per_point;
        let terminal_area = [
            central.min.x * ppp,
            central.min.y * ppp,
            central.width() * ppp,
            central.height() * ppp,
        ];
        ChromeOutput {
            full,
            terminal_area,
            clicked_session,
            tab_action,
            palette_choice,
            new_session_clicked,
            new_session,
            dock_choice,
            usage_action,
            peek_request,
            peek_action,
        }
    }

    /// Turns shapes into GPU primitives.
    pub fn tessellate(
        &self,
        shapes: Vec<egui::epaint::ClippedShape>,
        pixels_per_point: f32,
    ) -> Vec<egui::ClippedPrimitive> {
        self.ctx.tessellate(shapes, pixels_per_point)
    }

    /// Opens the command palette with these entries.
    pub fn open_palette(&mut self, items: Vec<PaletteItem>) {
        self.palette = Some(palette::Palette::new(items));
    }

    /// Whether the command palette is open (it takes the keyboard).
    pub fn palette_open(&self) -> bool {
        self.palette.is_some()
    }

    /// Opens the New session dialog with an agent (profile id) and folder preselected.
    pub fn open_new_session(&mut self, agents: Vec<AgentChoice>, agent: &str, folder: String) {
        self.new_session = Some(new_session::Dialog::new(agents, agent, folder));
    }

    /// Whether a text field of the chrome (palette, dialog, tab rename, search) takes the
    /// keyboard and IME.
    pub fn takes_keyboard(&self) -> bool {
        self.palette.is_some()
            || self.new_session.is_some()
            || self.peek.is_some()
            || self.tab_bar.editing()
            || self.ctx.egui_wants_keyboard_input()
    }

    /// Switches the chrome to the light or dark theme (F6-01).
    pub fn set_light(&self, on: bool) {
        theme::set_light(&self.ctx, on);
    }

    /// Whether egui wants keyboard input (a text field has focus).
    pub fn wants_keyboard(&self) -> bool {
        self.ctx.egui_wants_keyboard_input()
    }
}

fn state_color(state: RowState) -> Color32 {
    match state {
        RowState::Working => theme::STATE_WORKING(),
        RowState::Waiting => theme::STATE_WAITING(),
        RowState::Done => theme::STATE_DONE(),
        RowState::Error => theme::STATE_ERROR(),
        RowState::Idle => theme::TEXT_MUTED(),
    }
}

/// Shortens `text` with an ellipsis so it fits in `width` points.
fn elide(ui: &egui::Ui, text: &str, font: &egui::FontId, width: f32) -> String {
    let fits = |s: &str| {
        ui.fonts_mut(|f| {
            f.layout_no_wrap(s.to_owned(), font.clone(), Color32::WHITE)
                .size()
                .x
        }) <= width
    };
    if fits(text) {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let candidate: String = chars[..mid].iter().collect::<String>() + "…";
        if fits(&candidate) {
            lo = mid
        } else {
            hi = mid - 1
        }
    }
    chars[..lo].iter().collect::<String>() + "…"
}

/// Formats a duration the sidebar way: `27s`, `21m`, `3h`, `2d` (docs/requirements.md S-06).
pub fn format_elapsed(secs: u64) -> String {
    match secs {
        0..60 => format!("{secs}s"),
        60..3600 => format!("{}m", secs / 60),
        3600..86_400 => format!("{}h", secs / 3600),
        _ => format!("{}d", secs / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Vec2;

    #[test]
    fn elapsed_format_matches_the_spec() {
        assert_eq!(format_elapsed(27), "27s");
        assert_eq!(format_elapsed(21 * 60 + 5), "21m");
        assert_eq!(format_elapsed(3 * 3600), "3h");
        assert_eq!(format_elapsed(2 * 86_400 + 1), "2d");
    }

    #[test]
    fn palette_filters_by_typing_and_runs_the_choice_on_enter() {
        let mut chrome = Chrome::new();
        let item = |id: usize, label: &str| PaletteItem {
            id,
            label: label.into(),
            hint: String::new(),
        };
        chrome.open_palette(vec![
            item(0, "New tab"),
            item(1, "Split right"),
            item(2, "Split down"),
        ]);
        let frame = |chrome: &mut Chrome, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(900.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let mut out = chrome.run(input, &ChromeModel::default());
            out.full.textures_delta.clear();
            out.palette_choice
        };
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        assert_eq!(frame(&mut chrome, vec![]), None); // focuses the text field
        assert_eq!(
            frame(&mut chrome, vec![egui::Event::Text("spd".into())]),
            None
        );
        assert_eq!(frame(&mut chrome, vec![key(egui::Key::Enter)]), Some(2));
        assert!(!chrome.palette_open());

        chrome.open_palette(vec![item(0, "New tab")]);
        assert_eq!(frame(&mut chrome, vec![key(egui::Key::Escape)]), None);
        assert!(!chrome.palette_open());
    }

    #[test]
    fn peek_sends_the_typed_reply_on_enter() {
        let mut chrome = Chrome::new();
        let model = ChromeModel {
            peek: Some(PeekView {
                title: "Fix the login bug".into(),
                subtitle: "Needs permission: Bash".into(),
                lines: vec!["Do you want to proceed?".into(), "1. Yes".into()],
            }),
            ..ChromeModel::default()
        };
        let frame = |chrome: &mut Chrome, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(900.0, 600.0),
                )),
                events,
                ..Default::default()
            };
            let mut out = chrome.run(input, &model);
            out.full.textures_delta.clear();
            out.peek_action
        };
        assert_eq!(frame(&mut chrome, vec![]), None);
        assert!(chrome.takes_keyboard());
        assert_eq!(
            frame(&mut chrome, vec![egui::Event::Text("1".into())]),
            None
        );
        let enter = egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        assert_eq!(
            frame(&mut chrome, vec![enter]),
            Some(PeekAction::Reply("1".into()))
        );
    }

    #[test]
    fn screen_readers_get_each_row_as_a_labelled_button() {
        let mut chrome = Chrome::new();
        chrome.context().enable_accesskit();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(800.0, 400.0),
            )),
            ..Default::default()
        };
        let model = ChromeModel {
            sidebar_visible: true,
            tabs: Vec::new(),
            menu: Vec::new(),
            agents: Vec::new(),
            new_session_hint: String::new(),
            usage: None,
            peek: None,
            title_bar: TitleBar::Native,
            sessions: vec![SessionRow {
                title: "Fix the login bug".into(),
                subtitle: "Needs permission: Bash".into(),
                project: "hivetty".into(),
                elapsed: "1m".into(),
                state: RowState::Waiting,
                focused: true,
                agent: String::new(),
                unseen: false,
            }],
        };
        let mut out = chrome.run(input, &model);
        out.full.textures_delta.clear();
        let update = out
            .full
            .platform_output
            .accesskit_update
            .expect("accessibility tree");
        let button = update
            .nodes
            .iter()
            .map(|(_, node)| node)
            .find(|node| {
                node.role() == egui::Role::Button
                    && node
                        .label()
                        .is_some_and(|l| l.starts_with("Fix the login bug"))
            })
            .expect("the session row as a button");
        assert_eq!(
            button.label(),
            Some("Fix the login bug, Needs permission: Bash, 1m")
        );
    }

    #[test]
    fn chrome_reserves_the_sidebar_and_reports_the_terminal_area() {
        let mut chrome = Chrome::new();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                Vec2::new(1000.0, 600.0),
            )),
            ..Default::default()
        };
        let model = ChromeModel {
            sidebar_visible: true,
            tabs: Vec::new(),
            menu: Vec::new(),
            agents: Vec::new(),
            new_session_hint: String::new(),
            usage: None,
            peek: None,
            title_bar: TitleBar::Native,
            sessions: vec![SessionRow {
                title: "Fix the logo layout".into(),
                subtitle: "Working".into(),
                project: "hivetty".into(),
                elapsed: "3m".into(),
                state: RowState::Working,
                focused: true,
                agent: String::new(),
                unseen: false,
            }],
        };
        let mut out = chrome.run(input.clone(), &model);
        out.full.textures_delta.clear();
        let [x, _, w, _] = out.terminal_area;
        assert!(
            x >= SIDEBAR_WIDTH - 1.0,
            "terminal starts after the sidebar (x = {x})"
        );
        assert!((x + w - 1000.0).abs() < 1.0);

        let mut hidden = chrome.run(
            input,
            &ChromeModel {
                sidebar_visible: false,
                ..model
            },
        );
        hidden.full.textures_delta.clear();
        assert_eq!(hidden.terminal_area[0], 0.0);
    }
}
