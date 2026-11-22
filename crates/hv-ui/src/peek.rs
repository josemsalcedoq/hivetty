//! Peek (F4-29, S-20): see what a session is asking and answer it without leaving the pane
//! you are in.

use egui::{CornerRadius, Key, Margin, RichText, Stroke};

use crate::{strings, theme};

/// What the peek popover shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeekView {
    /// Session title.
    pub title: String,
    /// State and reason ("Needs permission: Bash(cargo test)").
    pub subtitle: String,
    /// The last lines of its screen.
    pub lines: Vec<String>,
}

/// What the user did in the popover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeekAction {
    /// Send this text (and Enter) to the session.
    Reply(String),
    /// Focus the session.
    GoTo,
    Close,
}

/// Reply box state.
#[derive(Debug, Default)]
pub(crate) struct Peek {
    reply: String,
    needs_focus: bool,
}

impl Peek {
    pub(crate) fn opened() -> Self {
        Self {
            reply: String::new(),
            needs_focus: true,
        }
    }

    pub(crate) fn show(&mut self, ctx: &egui::Context, view: &PeekView) -> Option<PeekAction> {
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            return Some(PeekAction::Close);
        }
        let mut action = None;
        let screen = ctx.content_rect();
        let width = 560.0_f32.min(screen.width() - 32.0);
        egui::Area::new(egui::Id::unique("peek"))
            .order(egui::Order::Foreground)
            .fade_in(false)
            .fixed_pos(egui::pos2(
                screen.center().x - width / 2.0,
                screen.top() + 64.0,
            ))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::ELEVATED_BG)
                    .stroke(Stroke::new(1.0, theme::STATE_WAITING))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::same(14))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        ui.label(RichText::new(&view.title).size(15.0).strong());
                        ui.label(
                            RichText::new(&view.subtitle)
                                .size(12.0)
                                .color(theme::STATE_WAITING),
                        );
                        ui.add_space(8.0);
                        egui::Frame::new()
                            .fill(theme::WINDOW_BG)
                            .corner_radius(CornerRadius::same(6))
                            .inner_margin(Margin::same(8))
                            .show(ui, |ui| {
                                ui.set_width(width - 16.0);
                                for line in &view.lines {
                                    ui.label(
                                        RichText::new(line)
                                            .monospace()
                                            .size(12.0)
                                            .color(theme::TEXT_PRIMARY),
                                    );
                                }
                            });
                        ui.add_space(8.0);
                        let edit = ui.add(
                            egui::TextEdit::singleline(&mut self.reply)
                                .hint_text(strings::PEEK_REPLY_HINT)
                                .desired_width(f32::INFINITY),
                        );
                        if self.needs_focus {
                            edit.request_focus();
                            self.needs_focus = false;
                        }
                        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                        ui.add_space(8.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(strings::PEEK_SEND).clicked() || enter {
                                action = Some(PeekAction::Reply(std::mem::take(&mut self.reply)));
                            }
                            if ui.button(strings::PEEK_GO_TO).clicked() {
                                action = Some(PeekAction::GoTo);
                            }
                            if ui.button(strings::CANCEL).clicked() {
                                action = Some(PeekAction::Close);
                            }
                        });
                    });
            });
        action
    }
}
