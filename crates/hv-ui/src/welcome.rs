//! The first-run wizard (F6-03): theme, shell integration, update checks and, for each agent
//! found, hivetty's hooks and statusline. Each option says in one line what it turns on. As
//! soon as an agent option is ticked, the exact changes to its settings are listed under the
//! options (CLAUDE.md: never write the user's config without showing the diff).

use egui::{Color32, CornerRadius, Key, Margin, RichText, Stroke, Vec2};

use crate::{strings, theme};

/// A change to an agent's settings the wizard can make.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentSetup {
    /// Claude Code's hooks: live session state.
    ClaudeHooks,
    /// Claude Code's statusline: context use and plan limits.
    ClaudeStatusline,
    /// Codex's hooks.
    CodexHooks,
    /// Gemini CLI's hooks.
    GeminiHooks,
}

impl AgentSetup {
    fn title(self) -> &'static str {
        match self {
            Self::ClaudeHooks => strings::WELCOME_CLAUDE_HOOKS,
            Self::ClaudeStatusline => strings::WELCOME_CLAUDE_STATUSLINE,
            Self::CodexHooks => strings::WELCOME_CODEX_HOOKS,
            Self::GeminiHooks => strings::WELCOME_GEMINI_HOOKS,
        }
    }

    fn detail(self) -> &'static str {
        match self {
            Self::ClaudeHooks => strings::WELCOME_CLAUDE_HOOKS_DETAIL,
            Self::ClaudeStatusline => strings::WELCOME_CLAUDE_STATUSLINE_DETAIL,
            Self::CodexHooks => strings::WELCOME_CODEX_HOOKS_DETAIL,
            Self::GeminiHooks => strings::WELCOME_GEMINI_HOOKS_DETAIL,
        }
    }
}

/// One agent change the wizard offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentOffer {
    pub setup: AgentSetup,
    /// The settings file, as shown to the user.
    pub path: String,
    /// The exact changes; empty when it is already set up.
    pub changes: Vec<String>,
}

/// What the app found, to show in the wizard.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WelcomeInfo {
    /// The agent changes on offer, for the agents found on this machine.
    pub agents: Vec<AgentOffer>,
}

/// What the user chose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WelcomeChoice {
    /// "system", "hivetty-dark" or "hivetty-light".
    pub theme: String,
    pub shell_integration: bool,
    /// Look for new versions once a day (F6-11).
    pub check_updates: bool,
    /// The agent changes to make (only ones with changes).
    pub agents: Vec<AgentSetup>,
}

/// Wizard state while open.
#[derive(Debug, Clone)]
pub(crate) struct Welcome {
    info: WelcomeInfo,
    theme: usize,
    shell_integration: bool,
    check_updates: bool,
    /// Ticked agent offers, by index in `info.agents`.
    agents: Vec<bool>,
}

/// What happened this frame.
#[derive(Debug)]
pub(crate) enum Outcome {
    Open,
    Skipped,
    Done(WelcomeChoice),
}

const THEMES: [(&str, &str); 3] = [
    ("system", strings::THEME_SYSTEM),
    ("hivetty-dark", strings::THEME_DARK),
    ("hivetty-light", strings::THEME_LIGHT),
];

/// Widest the dialog gets.
const WIDTH: f32 = 520.0;
/// Space kept between the dialog and the window edges.
const EDGE: f32 = 24.0;
/// Padding inside the dialog.
const PAD: i8 = 20;

impl Welcome {
    pub(crate) fn new(info: WelcomeInfo) -> Self {
        let agents = vec![false; info.agents.len()];
        Self {
            info,
            theme: 0,
            shell_integration: true,
            // Off until the user ticks it: it contacts GitHub.
            check_updates: false,
            agents,
        }
    }

    pub(crate) fn show(&mut self, ctx: &egui::Context) -> Outcome {
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            return Outcome::Skipped;
        }
        let mut outcome = Outcome::Open;
        let screen = ctx.content_rect();
        let width = WIDTH.min(screen.width() - 2.0 * EDGE).max(240.0);
        let frame = egui::Frame::new()
            .fill(theme::ELEVATED_BG())
            .stroke(Stroke::new(1.0, theme::BORDER()))
            .corner_radius(CornerRadius::same(12))
            .shadow(egui::Shadow {
                offset: [0, 8],
                blur: 28,
                spread: 0,
                color: Color32::from_black_alpha(90),
            });
        let id = egui::Id::unique("welcome");
        egui::Modal::new(id)
            .area(egui::Modal::default_area(id).fade_in(false))
            .frame(frame)
            .backdrop_color(Color32::from_black_alpha(140))
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing.y = 0.0;
                egui::Frame::new()
                    .inner_margin(Margin {
                        left: PAD,
                        right: PAD,
                        top: PAD,
                        bottom: 10,
                    })
                    .show(ui, |ui| {
                        ui.label(RichText::new(strings::WELCOME_TITLE).size(20.0).strong());
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(strings::WELCOME_INTRO)
                                .size(13.0)
                                .color(theme::TEXT_SECONDARY()),
                        );
                    });
                // The options scroll when the window is too short for them; the buttons stay.
                let footer = 64.0;
                let header = ui.min_rect().height();
                let room = (screen.height() - 2.0 * EDGE - header - footer).max(80.0);
                // Both bounds: the modal's own area would otherwise keep it as short as the
                // first frame left it.
                egui::ScrollArea::vertical()
                    .max_height(room)
                    .min_scrolled_height(room)
                    .scroll_bar_visibility(
                        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                    )
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        egui::Frame::new()
                            .inner_margin(Margin::symmetric(PAD, 0))
                            .show(ui, |ui| self.options(ui));
                    });
                separator(ui);
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(PAD, 14))
                    .show(ui, |ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.spacing_mut().item_spacing.x = 8.0;
                            let start = egui::Button::new(
                                RichText::new(strings::WELCOME_START)
                                    .strong()
                                    .color(Color32::WHITE),
                            )
                            .fill(theme::ACCENT_STRONG())
                            .corner_radius(CornerRadius::same(6))
                            .min_size(Vec2::new(88.0, 30.0));
                            // No Enter shortcut: a key pressed for the terminal as the wizard
                            // appears must not answer it.
                            if ui.add(start).clicked() {
                                outcome = Outcome::Done(self.choice());
                            }
                            let skip = egui::Button::new(
                                RichText::new(strings::WELCOME_SKIP).color(theme::TEXT_SECONDARY()),
                            )
                            .frame_when_inactive(false)
                            .corner_radius(CornerRadius::same(6))
                            .min_size(Vec2::new(64.0, 30.0));
                            if ui.add(skip).clicked() {
                                outcome = Outcome::Skipped;
                            }
                        });
                    });
            });
        outcome
    }

    /// The options, in sections.
    fn options(&mut self, ui: &mut egui::Ui) {
        section(ui, strings::WELCOME_APPEARANCE);
        segmented(ui, &mut self.theme, &THEMES.map(|(_, label)| label));
        ui.add_space(10.0);

        section(ui, strings::WELCOME_TERMINAL);
        option(
            ui,
            &mut self.shell_integration,
            strings::WELCOME_SHELL,
            strings::WELCOME_SHELL_DETAIL,
        );
        option(
            ui,
            &mut self.check_updates,
            strings::WELCOME_UPDATES,
            strings::WELCOME_UPDATES_DETAIL,
        );

        if self.info.agents.is_empty() {
            return;
        }
        section(ui, strings::WELCOME_AGENTS);
        for (offer, on) in self.info.agents.iter().zip(self.agents.iter_mut()) {
            if offer.changes.is_empty() {
                let mut done = true;
                ui.add_enabled_ui(false, |ui| {
                    option(ui, &mut done, offer.setup.title(), strings::WELCOME_ALREADY);
                });
            } else {
                option(ui, on, offer.setup.title(), offer.setup.detail());
            }
        }
        // The exact changes for what is ticked, folded away until asked for.
        let ticked: Vec<&AgentOffer> = self
            .info
            .agents
            .iter()
            .zip(&self.agents)
            .filter(|(offer, on)| **on && !offer.changes.is_empty())
            .map(|(offer, _)| offer)
            .collect();
        if ticked.is_empty() {
            ui.label(
                RichText::new(strings::WELCOME_AGENTS_NOTE)
                    .size(12.0)
                    .color(theme::TEXT_MUTED()),
            );
        } else {
            changes(ui, &ticked);
        }
        ui.add_space(14.0);
    }

    fn choice(&self) -> WelcomeChoice {
        WelcomeChoice {
            theme: THEMES[self.theme].0.to_owned(),
            shell_integration: self.shell_integration,
            check_updates: self.check_updates,
            agents: self
                .info
                .agents
                .iter()
                .zip(&self.agents)
                .filter(|(offer, on)| **on && !offer.changes.is_empty())
                .map(|(offer, _)| offer.setup)
                .collect(),
        }
    }
}

/// A section heading.
fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(
        RichText::new(title)
            .size(12.0)
            .strong()
            .color(theme::TEXT_SECONDARY()),
    );
    ui.add_space(8.0);
}

/// A checkbox with what it turns on in one line under it, aligned with the label.
fn option(ui: &mut egui::Ui, on: &mut bool, title: &str, detail: &str) {
    ui.checkbox(on, RichText::new(title).size(13.0));
    let indent = ui.spacing().icon_width + ui.spacing().icon_spacing;
    ui.horizontal(|ui| {
        ui.add_space(indent);
        ui.add(
            egui::Label::new(RichText::new(detail).size(12.0).color(theme::TEXT_MUTED())).wrap(),
        );
    });
    ui.add_space(10.0);
}

/// Equal-width buttons in one frame, one of them selected.
fn segmented(ui: &mut egui::Ui, selected: &mut usize, labels: &[&str]) {
    let segment = 96.0;
    egui::Frame::new()
        .fill(theme::WINDOW_BG())
        .stroke(Stroke::new(1.0, theme::BORDER()))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::same(2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (i, label) in labels.iter().enumerate() {
                    let on = *selected == i;
                    let text = RichText::new(*label).size(12.0).color(if on {
                        theme::TEXT_PRIMARY()
                    } else {
                        theme::TEXT_SECONDARY()
                    });
                    let button = egui::Button::new(text)
                        .fill(if on {
                            theme::SELECTED_BG()
                        } else {
                            Color32::TRANSPARENT
                        })
                        .stroke(Stroke::NONE)
                        .corner_radius(CornerRadius::same(6))
                        .min_size(Vec2::new(segment, 26.0));
                    let response = ui.add(button);
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(egui::Role::RadioButton, true, on, *label)
                    });
                    if response.clicked() {
                        *selected = i;
                    }
                }
            });
        });
}

/// The exact lines each ticked option adds, by file: on screen before the user agrees
/// (CLAUDE.md), in a short box that scrolls.
fn changes(ui: &mut egui::Ui, ticked: &[&AgentOffer]) {
    let lines: usize = ticked.iter().map(|o| o.changes.len()).sum();
    ui.label(
        RichText::new(strings::welcome_see_changes(lines))
            .size(12.0)
            .color(theme::TEXT_SECONDARY()),
    );
    ui.add_space(6.0);
    let mut files: Vec<&str> = Vec::new();
    for offer in ticked {
        if !files.contains(&offer.path.as_str()) {
            files.push(&offer.path);
        }
    }
    egui::Frame::new()
        .fill(theme::WINDOW_BG())
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::ScrollArea::both()
                .id_salt("welcome-changes-lines")
                .max_height(120.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for (i, file) in files.iter().enumerate() {
                        if i > 0 {
                            ui.add_space(8.0);
                        }
                        ui.label(
                            RichText::new(strings::welcome_changes(file))
                                .size(12.0)
                                .color(theme::TEXT_SECONDARY()),
                        );
                        for offer in ticked.iter().filter(|o| o.path == *file) {
                            for change in &offer.changes {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(change)
                                            .monospace()
                                            .size(11.0)
                                            .color(theme::TEXT_MUTED()),
                                    )
                                    .extend(),
                                );
                            }
                        }
                    }
                });
        });
}

/// A full-width line between the options and the buttons.
fn separator(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, theme::BORDER());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer(setup: AgentSetup, changes: &[&str]) -> AgentOffer {
        AgentOffer {
            setup,
            path: "~/.claude/settings.json".into(),
            changes: changes.iter().map(|c| (*c).to_owned()).collect(),
        }
    }

    #[test]
    fn only_ticked_offers_with_changes_are_chosen() {
        let mut wizard = Welcome::new(WelcomeInfo {
            agents: vec![
                offer(AgentSetup::ClaudeHooks, &["+ Stop"]),
                offer(AgentSetup::ClaudeStatusline, &[]),
                offer(AgentSetup::CodexHooks, &["+ Stop"]),
            ],
        });
        assert!(
            wizard.choice().agents.is_empty(),
            "nothing is ticked at first"
        );
        wizard.agents = vec![true, true, false];
        assert_eq!(wizard.choice().agents, vec![AgentSetup::ClaudeHooks]);
    }
}
