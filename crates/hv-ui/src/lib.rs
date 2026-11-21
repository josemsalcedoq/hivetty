//! Window chrome built with egui (ADR 0003): sidebar, and later tabs, attention inbox and the
//! usage dashboard. The terminal grid itself is drawn by `hv-render` in the space egui leaves.

pub mod strings;
mod theme;

use egui::{Color32, CornerRadius, Margin, RichText, Sense, Stroke, Vec2};

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
}

/// What the chrome produced this frame.
pub struct ChromeOutput {
    /// egui's full output (shapes, textures, platform requests).
    pub full: egui::FullOutput,
    /// Space left for the terminal grid, in physical pixels: `[x, y, width, height]`.
    pub terminal_area: [f32; 4],
    /// Index of a session row the user clicked.
    pub clicked_session: Option<usize>,
}

impl std::fmt::Debug for ChromeOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChromeOutput")
            .field("terminal_area", &self.terminal_area)
            .field("clicked_session", &self.clicked_session)
            .finish_non_exhaustive()
    }
}

/// The egui-based window chrome.
#[derive(Debug, Default)]
pub struct Chrome {
    ctx: egui::Context,
}

/// Sidebar width in points (docs/ui-spec.md 1: 300 px default; narrower until tabs exist).
const SIDEBAR_WIDTH: f32 = 260.0;

impl Chrome {
    /// Creates the chrome with hivetty's dark theme.
    pub fn new() -> Self {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        Self { ctx }
    }

    /// The egui context (for the winit integration).
    pub fn context(&self) -> &egui::Context {
        &self.ctx
    }

    /// Runs one UI frame.
    pub fn run(&mut self, input: egui::RawInput, model: &ChromeModel) -> ChromeOutput {
        let mut clicked_session = None;
        let mut central = egui::Rect::NOTHING;
        let full = self.ctx.run_ui(input, |ui| {
            if model.sidebar_visible {
                egui::Panel::left("sidebar")
                    .exact_size(SIDEBAR_WIDTH)
                    .resizable(false)
                    .frame(
                        egui::Frame::new()
                            .fill(theme::SIDEBAR_BG)
                            .inner_margin(Margin::symmetric(10, 12))
                            .stroke(Stroke::new(1.0, theme::BORDER)),
                    )
                    .show(ui, |ui| {
                        clicked_session = sidebar(ui, &model.sessions);
                    });
            }
            central = ui.available_rect_before_wrap();
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

    /// Whether egui wants keyboard input (a text field has focus).
    pub fn wants_keyboard(&self) -> bool {
        self.ctx.egui_wants_keyboard_input()
    }
}

/// The session list. Returns the index of a clicked row.
fn sidebar(ui: &mut egui::Ui, sessions: &[SessionRow]) -> Option<usize> {
    let mut clicked = None;
    ui.label(
        RichText::new(strings::SESSIONS)
            .size(12.0)
            .color(theme::TEXT_MUTED),
    );
    ui.add_space(6.0);
    let mut current_project: Option<&str> = None;
    for (i, row) in sessions.iter().enumerate() {
        if current_project != Some(row.project.as_str()) {
            current_project = Some(&row.project);
            ui.add_space(6.0);
            ui.label(
                RichText::new(&row.project)
                    .size(12.0)
                    .color(theme::TEXT_SECONDARY),
            );
        }
        if session_row(ui, row) {
            clicked = Some(i);
        }
    }
    if sessions.is_empty() {
        ui.label(RichText::new(strings::NO_SESSIONS).color(theme::TEXT_MUTED));
    }
    clicked
}

/// One two-line row: dot, title, elapsed time / state or activity.
fn session_row(ui: &mut egui::Ui, row: &SessionRow) -> bool {
    let height = 44.0;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    let painter = ui.painter_at(rect);
    let bg = if row.focused {
        theme::SELECTED_BG
    } else if response.hovered() {
        theme::HOVER_BG
    } else {
        Color32::TRANSPARENT
    };
    painter.rect_filled(rect, CornerRadius::same(6), bg);

    let dot_color = match row.state {
        RowState::Working => theme::STATE_WORKING,
        RowState::Waiting => theme::STATE_WAITING,
        RowState::Done => theme::STATE_DONE,
        RowState::Error => theme::STATE_ERROR,
        RowState::Idle => theme::TEXT_MUTED,
    };
    let left = rect.left() + 12.0;
    painter.circle_filled(egui::pos2(left + 4.0, rect.top() + 15.0), 4.0, dot_color);

    let text_left = left + 16.0;
    let font = egui::FontId::proportional(13.0);
    let small = egui::FontId::proportional(12.0);
    let right = rect.right() - 10.0;
    let time = painter.text(
        egui::pos2(right, rect.top() + 8.0),
        egui::Align2::RIGHT_TOP,
        &row.elapsed,
        small.clone(),
        theme::TEXT_MUTED,
    );
    let title_width = (time.left() - text_left - 8.0).max(20.0);
    let title = elide(ui, &row.title, &font, title_width);
    painter.text(
        egui::pos2(text_left, rect.top() + 7.0),
        egui::Align2::LEFT_TOP,
        title,
        font,
        theme::TEXT_PRIMARY,
    );
    let subtitle_color = match row.state {
        RowState::Done => theme::STATE_DONE,
        RowState::Waiting => theme::STATE_WAITING,
        RowState::Error => theme::STATE_ERROR,
        _ => theme::TEXT_SECONDARY,
    };
    let subtitle = elide(ui, &row.subtitle, &small, right - text_left);
    painter.text(
        egui::pos2(text_left, rect.top() + 25.0),
        egui::Align2::LEFT_TOP,
        subtitle,
        small,
        subtitle_color,
    );
    response.clicked()
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

    #[test]
    fn elapsed_format_matches_the_spec() {
        assert_eq!(format_elapsed(27), "27s");
        assert_eq!(format_elapsed(21 * 60 + 5), "21m");
        assert_eq!(format_elapsed(3 * 3600), "3h");
        assert_eq!(format_elapsed(2 * 86_400 + 1), "2d");
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
            sessions: vec![SessionRow {
                title: "Fix the logo layout".into(),
                subtitle: "Working".into(),
                project: "hivetty".into(),
                elapsed: "3m".into(),
                state: RowState::Working,
                focused: true,
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
