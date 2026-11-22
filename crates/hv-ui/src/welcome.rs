//! The first-run wizard (F6-03): theme, shell integration and, if Claude Code is installed,
//! its hooks and statusline. The changes to the agent's settings are listed before the user
//! agrees to them (CLAUDE.md: never write the user's config without showing the diff).

use egui::{CornerRadius, Key, Margin, RichText, Stroke};

use crate::{strings, theme};

/// What the app found, to show in the wizard.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WelcomeInfo {
    /// Claude Code's settings file and the changes installing the hooks would make (empty when
    /// they are already installed); `None` when Claude Code was not found.
    pub hooks: Option<(String, Vec<String>)>,
    /// The same for the statusline; `None` also when the user already has a statusline.
    pub statusline: Option<(String, Vec<String>)>,
}

/// What the user chose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WelcomeChoice {
    /// "system", "hivetty-dark" or "hivetty-light".
    pub theme: String,
    pub shell_integration: bool,
    pub install_hooks: bool,
    pub install_statusline: bool,
}

/// Wizard state while open.
#[derive(Debug, Clone)]
pub(crate) struct Welcome {
    info: WelcomeInfo,
    theme: usize,
    shell_integration: bool,
    install_hooks: bool,
    install_statusline: bool,
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

impl Welcome {
    pub(crate) fn new(info: WelcomeInfo) -> Self {
        Self {
            info,
            theme: 0,
            shell_integration: true,
            install_hooks: false,
            install_statusline: false,
        }
    }

    pub(crate) fn show(&mut self, ctx: &egui::Context) -> Outcome {
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            return Outcome::Skipped;
        }
        let mut outcome = Outcome::Open;
        let screen = ctx.content_rect();
        let width = 520.0_f32.min(screen.width() - 32.0);
        egui::Area::new(egui::Id::unique("welcome"))
            .order(egui::Order::Foreground)
            .fade_in(false)
            .fixed_pos(egui::pos2(
                screen.center().x - width / 2.0,
                screen.top() + 60.0,
            ))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::ELEVATED_BG())
                    .stroke(Stroke::new(1.0, theme::BORDER()))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::same(18))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        ui.label(RichText::new(strings::WELCOME_TITLE).size(18.0).strong());
                        ui.label(
                            RichText::new(strings::WELCOME_INTRO).color(theme::TEXT_SECONDARY()),
                        );
                        ui.add_space(12.0);

                        ui.label(RichText::new(strings::WELCOME_THEME).strong());
                        ui.horizontal(|ui| {
                            for (i, (_, label)) in THEMES.iter().enumerate() {
                                ui.radio_value(&mut self.theme, i, *label);
                            }
                        });
                        ui.add_space(8.0);

                        ui.checkbox(&mut self.shell_integration, strings::WELCOME_SHELL);
                        ui.label(
                            RichText::new(strings::WELCOME_SHELL_DETAIL)
                                .size(12.0)
                                .color(theme::TEXT_MUTED()),
                        );
                        ui.add_space(8.0);

                        if let Some((path, changes)) = &self.info.hooks {
                            agent_change(
                                ui,
                                &mut self.install_hooks,
                                strings::WELCOME_HOOKS,
                                path,
                                changes,
                            );
                        }
                        if let Some((path, changes)) = &self.info.statusline {
                            agent_change(
                                ui,
                                &mut self.install_statusline,
                                strings::WELCOME_STATUSLINE,
                                path,
                                changes,
                            );
                        }

                        ui.add_space(12.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(strings::WELCOME_START).clicked() {
                                outcome = Outcome::Done(self.choice());
                            }
                            if ui.button(strings::WELCOME_SKIP).clicked() {
                                outcome = Outcome::Skipped;
                            }
                        });
                    });
            });
        outcome
    }

    fn choice(&self) -> WelcomeChoice {
        WelcomeChoice {
            theme: THEMES[self.theme].0.to_owned(),
            shell_integration: self.shell_integration,
            install_hooks: self.install_hooks
                && self.info.hooks.as_ref().is_some_and(|(_, c)| !c.is_empty()),
            install_statusline: self.install_statusline
                && self
                    .info
                    .statusline
                    .as_ref()
                    .is_some_and(|(_, c)| !c.is_empty()),
        }
    }
}

/// A checkbox to change an agent's settings, with the file and the exact changes under it.
fn agent_change(ui: &mut egui::Ui, on: &mut bool, label: &str, path: &str, changes: &[String]) {
    if changes.is_empty() {
        ui.label(
            RichText::new(format!("{label}: {}", strings::WELCOME_ALREADY))
                .color(theme::TEXT_SECONDARY()),
        );
        return;
    }
    ui.checkbox(on, label);
    ui.label(
        RichText::new(strings::welcome_changes(path))
            .size(12.0)
            .color(theme::TEXT_MUTED()),
    );
    for change in changes {
        ui.label(
            RichText::new(format!("  {change}"))
                .monospace()
                .size(11.0)
                .color(theme::TEXT_SECONDARY()),
        );
    }
    ui.add_space(6.0);
}
