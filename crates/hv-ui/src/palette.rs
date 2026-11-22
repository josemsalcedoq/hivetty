//! Command palette (F2-07): a filter box over every action, with its shortcut.

use egui::{Color32, CornerRadius, Key, Margin, RichText, Stroke, Vec2};

use crate::{strings, theme};

/// One entry the palette can run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteItem {
    /// What the app runs when chosen (opaque to the UI).
    pub id: usize,
    /// Shown text, e.g. "Split right".
    pub label: String,
    /// Shortcut hint, e.g. "Ctrl+Shift+D".
    pub hint: String,
}

/// Open palette state.
#[derive(Debug, Clone, Default)]
pub(crate) struct Palette {
    pub(crate) items: Vec<PaletteItem>,
    query: String,
    selected: usize,
    /// Focus the text field on the first frame.
    needs_focus: bool,
}

/// What happened in the palette this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaletteOutcome {
    /// Still open.
    Open,
    /// Closed without choosing.
    Dismissed,
    /// The user chose the item with this id.
    Chosen(usize),
}

impl Palette {
    pub(crate) fn new(items: Vec<PaletteItem>) -> Self {
        Self {
            items,
            query: String::new(),
            selected: 0,
            needs_focus: true,
        }
    }

    /// Items matching the query, best first.
    fn matches(&self) -> Vec<&PaletteItem> {
        let mut scored: Vec<(i32, &PaletteItem)> = self
            .items
            .iter()
            .filter_map(|item| fuzzy_score(&self.query, &item.label).map(|s| (s, item)))
            .collect();
        scored.sort_by_key(|a| std::cmp::Reverse(a.0));
        scored.into_iter().map(|(_, item)| item).collect()
    }

    pub(crate) fn show(&mut self, ctx: &egui::Context) -> PaletteOutcome {
        let mut outcome = PaletteOutcome::Open;
        let (up, down, enter, escape) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, Key::Enter),
                i.consume_key(egui::Modifiers::NONE, Key::Escape),
            )
        });
        if escape {
            return PaletteOutcome::Dismissed;
        }
        let matches: Vec<PaletteItem> = self.matches().into_iter().cloned().collect();
        if matches.is_empty() {
            self.selected = 0;
        } else {
            if down {
                self.selected = (self.selected + 1) % matches.len();
            }
            if up {
                self.selected = (self.selected + matches.len() - 1) % matches.len();
            }
            self.selected = self.selected.min(matches.len() - 1);
        }
        if enter && let Some(item) = matches.get(self.selected) {
            return PaletteOutcome::Chosen(item.id);
        }

        let screen = ctx.content_rect();
        let width = 520.0_f32.min(screen.width() - 32.0);
        egui::Area::new(egui::Id::unique("command-palette"))
            .order(egui::Order::Foreground)
            .fade_in(false)
            .fixed_pos(egui::pos2(
                screen.center().x - width / 2.0,
                screen.top() + 64.0,
            ))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::ELEVATED_BG)
                    .stroke(Stroke::new(1.0, theme::BORDER))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        let edit = egui::TextEdit::singleline(&mut self.query)
                            .hint_text(strings::PALETTE_HINT)
                            .desired_width(f32::INFINITY)
                            .frame(egui::Frame::NONE)
                            .font(egui::FontId::proportional(15.0));
                        let response = ui.add(edit);
                        if self.needs_focus {
                            response.request_focus();
                            self.needs_focus = false;
                        }
                        if response.changed() {
                            self.selected = 0;
                        }
                        ui.separator();
                        if matches.is_empty() {
                            ui.label(
                                RichText::new(strings::PALETTE_EMPTY).color(theme::TEXT_MUTED),
                            );
                        }
                        egui::ScrollArea::vertical()
                            .max_height(320.0)
                            .show(ui, |ui| {
                                for (i, item) in matches.iter().enumerate() {
                                    if palette_row(ui, item, i == self.selected) {
                                        outcome = PaletteOutcome::Chosen(item.id);
                                    }
                                }
                            });
                    });
            });
        outcome
    }
}

fn palette_row(ui: &mut egui::Ui, item: &PaletteItem, selected: bool) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::ListItem, true, selected, &item.label)
    });
    if selected {
        response.scroll_to_me(None);
    }
    let painter = ui.painter_at(rect);
    let bg = if selected {
        theme::SELECTED_BG
    } else if response.hovered() {
        theme::HOVER_BG
    } else {
        Color32::TRANSPARENT
    };
    painter.rect_filled(rect, CornerRadius::same(6), bg);
    painter.text(
        egui::pos2(rect.left() + 10.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        &item.label,
        egui::FontId::proportional(14.0),
        theme::TEXT_PRIMARY,
    );
    painter.text(
        egui::pos2(rect.right() - 10.0, rect.center().y),
        egui::Align2::RIGHT_CENTER,
        &item.hint,
        egui::FontId::proportional(12.0),
        theme::TEXT_MUTED,
    );
    response.clicked()
}

/// Subsequence match score: `None` if the letters of `query` do not appear in order in
/// `text`. Matches at word starts and runs of consecutive letters score higher.
pub(crate) fn fuzzy_score(query: &str, text: &str) -> Option<i32> {
    let query: Vec<char> = query
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if query.is_empty() {
        return Some(0);
    }
    let text: Vec<char> = text.to_lowercase().chars().collect();
    let mut score = 0;
    let mut qi = 0;
    let mut previous: Option<usize> = None;
    for (ti, c) in text.iter().enumerate() {
        if qi < query.len() && *c == query[qi] {
            score += 1;
            if ti == 0 || text[ti - 1] == ' ' {
                score += 3; // word start
            }
            if previous.is_some_and(|p| p + 1 == ti) {
                score += 2; // consecutive
            }
            previous = Some(ti);
            qi += 1;
        }
    }
    (qi == query.len()).then_some(score - text.len() as i32 / 8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_matches_in_order_and_prefers_word_starts() {
        assert!(fuzzy_score("spr", "Split right").is_some());
        assert!(fuzzy_score("rsp", "Split right").is_none());
        let word_start = fuzzy_score("nt", "New tab").unwrap();
        let middle = fuzzy_score("nt", "Font smaller").unwrap();
        assert!(word_start > middle, "{word_start} vs {middle}");
        assert_eq!(fuzzy_score("", "anything"), Some(0));
    }
}
