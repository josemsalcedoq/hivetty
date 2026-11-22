//! The sessions sidebar (F4-11, F4-12): a search box, filters with counts, and the sessions
//! grouped by project.

use egui::{Color32, CornerRadius, RichText, Sense, Stroke, Vec2};

use crate::{RowState, SessionRow, elide, state_color, strings, theme};

/// Which sessions the list shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Filter {
    #[default]
    All,
    NeedsYou,
    Working,
    Done,
}

impl Filter {
    fn keeps(self, state: RowState) -> bool {
        match self {
            Self::All => true,
            Self::NeedsYou => state == RowState::Waiting,
            Self::Working => state == RowState::Working,
            Self::Done => matches!(state, RowState::Done | RowState::Error),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::All => strings::FILTER_ALL,
            Self::NeedsYou => strings::FILTER_NEEDS_YOU,
            Self::Working => strings::FILTER_WORKING,
            Self::Done => strings::FILTER_DONE,
        }
    }
}

/// State kept between frames.
#[derive(Debug, Default)]
pub(crate) struct Sidebar {
    query: String,
    filter: Filter,
}

impl Sidebar {
    /// Draws the sidebar. Returns the index (in `sessions`) of a clicked row and whether the
    /// New session button was pressed.
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        sessions: &[SessionRow],
        new_session_hint: &str,
    ) -> (Option<usize>, bool) {
        let mut clicked = None;
        let mut new_session = false;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(strings::SESSIONS)
                    .size(12.0)
                    .color(theme::TEXT_MUTED),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let button = egui::Button::new(RichText::new(strings::NEW_SESSION).size(12.0))
                    .shortcut_text(new_session_hint)
                    .corner_radius(CornerRadius::same(6));
                if ui.add(button).clicked() {
                    new_session = true;
                }
            });
        });
        ui.add_space(6.0);
        let search = egui::TextEdit::singleline(&mut self.query)
            .hint_text(strings::SEARCH_SESSIONS)
            .desired_width(f32::INFINITY);
        ui.add(search);
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for filter in [Filter::All, Filter::NeedsYou, Filter::Working, Filter::Done] {
                let count = sessions.iter().filter(|s| filter.keeps(s.state)).count();
                let text = format!("{} {count}", filter.label());
                let selected = self.filter == filter;
                let chip = egui::Button::new(RichText::new(text).size(11.0).color(if selected {
                    theme::TEXT_PRIMARY
                } else {
                    theme::TEXT_SECONDARY
                }))
                .fill(if selected {
                    theme::SELECTED_BG
                } else {
                    Color32::TRANSPARENT
                })
                .stroke(Stroke::new(1.0, theme::BORDER))
                .corner_radius(CornerRadius::same(10));
                if ui.add(chip).clicked() {
                    self.filter = filter;
                }
            }
        });
        ui.add_space(4.0);

        let query = self.query.trim().to_lowercase();
        let visible: Vec<(usize, &SessionRow)> = sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| self.filter.keeps(s.state))
            .filter(|(_, s)| query.is_empty() || matches_query(s, &query))
            .collect();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Group by project in order of first appearance.
                let mut projects: Vec<&str> = Vec::new();
                for (_, s) in &visible {
                    if !projects.contains(&s.project.as_str()) {
                        projects.push(&s.project);
                    }
                }
                for project in projects {
                    ui.add_space(6.0);
                    let count = visible.iter().filter(|(_, s)| s.project == project).count();
                    ui.label(
                        RichText::new(format!("{project}  ·  {count}"))
                            .size(12.0)
                            .color(theme::TEXT_SECONDARY),
                    );
                    for (index, row) in visible.iter().filter(|(_, s)| s.project == project) {
                        if session_row(ui, row) {
                            clicked = Some(*index);
                        }
                    }
                }
                if sessions.is_empty() {
                    ui.label(RichText::new(strings::NO_SESSIONS).color(theme::TEXT_MUTED));
                } else if visible.is_empty() {
                    ui.label(RichText::new(strings::NO_MATCHES).color(theme::TEXT_MUTED));
                }
            });
        (clicked, new_session)
    }
}

fn matches_query(row: &SessionRow, query: &str) -> bool {
    [&row.title, &row.subtitle, &row.project, &row.agent]
        .iter()
        .any(|field| field.to_lowercase().contains(query))
}

/// One two-line row: dot, title, elapsed time / state or activity.
fn session_row(ui: &mut egui::Ui, row: &SessionRow) -> bool {
    let height = 44.0;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    // What screen readers announce for the row (painted by hand, so egui cannot infer it).
    let label = format!("{}, {}, {}", row.title, row.subtitle, row.elapsed);
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, row.focused, &label));
    let painter = ui.painter_at(rect);
    let bg = if row.focused {
        theme::SELECTED_BG
    } else if response.hovered() {
        theme::HOVER_BG
    } else {
        Color32::TRANSPARENT
    };
    painter.rect_filled(rect, CornerRadius::same(6), bg);

    let left = rect.left() + 12.0;
    painter.circle_filled(
        egui::pos2(left + 4.0, rect.top() + 15.0),
        4.0,
        state_color(row.state),
    );
    if row.unseen && !row.focused {
        painter.circle_filled(
            egui::pos2(rect.right() - 6.0, rect.top() + 6.0),
            3.0,
            theme::ACCENT,
        );
    }

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
