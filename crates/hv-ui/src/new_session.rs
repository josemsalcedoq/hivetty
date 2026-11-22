//! The "New session" dialog (F4-13, docs/ui-spec.md §4) and what it asks the app to start.

use egui::{CornerRadius, Key, Margin, RichText, Stroke};

use crate::{strings, theme};

/// An agent the dialog and the dock can start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentChoice {
    /// Profile id (`claude-code`), or empty for a plain shell.
    pub id: String,
    /// Display name.
    pub name: String,
}

/// What the user asked to start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewSession {
    /// Profile id, or empty for a plain shell.
    pub agent: String,
    /// Directory to start in (validated to exist).
    pub folder: String,
    /// Optional first prompt.
    pub prompt: String,
    /// Extra arguments, as typed.
    pub args: String,
    /// Open next to the focused pane instead of in a new tab.
    pub split: bool,
    /// Start in a new git worktree on this branch (`Some("")` = pick a name).
    pub worktree: Option<String>,
}

/// Dialog state while open.
#[derive(Debug, Clone)]
pub(crate) struct Dialog {
    agents: Vec<AgentChoice>,
    selected: usize,
    folder: String,
    prompt: String,
    args: String,
    split: bool,
    worktree: bool,
    branch: String,
    error: Option<String>,
    needs_focus: bool,
    /// Folders used before, newest first.
    recent: Vec<String>,
}

/// What happened in the dialog this frame.
#[derive(Debug)]
pub(crate) enum Outcome {
    Open,
    Cancelled,
    Create(NewSession),
}

impl Dialog {
    pub(crate) fn new(
        agents: Vec<AgentChoice>,
        agent: &str,
        folder: String,
        recent: Vec<String>,
    ) -> Self {
        let selected = agents.iter().position(|a| a.id == agent).unwrap_or(0);
        Self {
            agents,
            selected,
            folder,
            prompt: String::new(),
            args: String::new(),
            split: false,
            worktree: false,
            branch: String::new(),
            error: None,
            needs_focus: true,
            recent,
        }
    }

    pub(crate) fn show(&mut self, ctx: &egui::Context) -> Outcome {
        let mut outcome = Outcome::Open;
        let (enter, escape) =
            ctx.input(|i| (i.key_pressed(Key::Enter), i.key_pressed(Key::Escape)));
        if escape {
            return Outcome::Cancelled;
        }
        let screen = ctx.content_rect();
        let width = 520.0_f32.min(screen.width() - 32.0);
        egui::Area::new(egui::Id::unique("new-session"))
            .order(egui::Order::Foreground)
            .fade_in(false)
            .fixed_pos(egui::pos2(
                screen.center().x - width / 2.0,
                screen.top() + 64.0,
            ))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::ELEVATED_BG())
                    .stroke(Stroke::new(1.0, theme::BORDER()))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(Margin::same(14))
                    .show(ui, |ui| {
                        ui.set_width(width);
                        ui.label(RichText::new(strings::NEW_SESSION).size(15.0).strong());
                        ui.add_space(8.0);
                        egui::Grid::new("new-session-grid")
                            .num_columns(2)
                            .spacing([10.0, 8.0])
                            .show(ui, |ui| {
                                ui.label(strings::FIELD_AGENT);
                                let current = self
                                    .agents
                                    .get(self.selected)
                                    .map_or(strings::DEFAULT_TITLE, |a| a.name.as_str())
                                    .to_owned();
                                egui::ComboBox::from_id_salt("agent")
                                    .selected_text(current)
                                    .width(width - 120.0)
                                    .show_ui(ui, |ui| {
                                        for (i, agent) in self.agents.iter().enumerate() {
                                            ui.selectable_value(&mut self.selected, i, &agent.name);
                                        }
                                    });
                                ui.end_row();

                                ui.label(strings::FIELD_FOLDER);
                                ui.horizontal(|ui| {
                                    let field_width = if self.recent.is_empty() {
                                        width - 120.0
                                    } else {
                                        width - 190.0
                                    };
                                    let folder = ui.add(
                                        egui::TextEdit::singleline(&mut self.folder)
                                            .desired_width(field_width),
                                    );
                                    if folder.changed() {
                                        self.error = None;
                                    }
                                    if !self.recent.is_empty() {
                                        ui.menu_button(strings::RECENT, |ui| {
                                            for recent in &self.recent {
                                                if ui.button(recent).clicked() {
                                                    self.folder.clone_from(recent);
                                                    self.error = None;
                                                    ui.close();
                                                }
                                            }
                                        });
                                    }
                                });
                                ui.end_row();

                                ui.label(strings::FIELD_PROMPT);
                                let prompt = ui.add(
                                    egui::TextEdit::singleline(&mut self.prompt)
                                        .hint_text(strings::OPTIONAL)
                                        .desired_width(width - 120.0),
                                );
                                if self.needs_focus {
                                    prompt.request_focus();
                                    self.needs_focus = false;
                                }
                                ui.end_row();

                                ui.label(strings::FIELD_ARGUMENTS);
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.args)
                                        .hint_text(strings::OPTIONAL)
                                        .desired_width(width - 120.0),
                                );
                                ui.end_row();

                                ui.label(strings::FIELD_WORKTREE);
                                ui.horizontal(|ui| {
                                    ui.checkbox(&mut self.worktree, strings::CREATE_WORKTREE);
                                    if self.worktree {
                                        ui.add(
                                            egui::TextEdit::singleline(&mut self.branch)
                                                .hint_text(strings::BRANCH_HINT)
                                                .desired_width(200.0),
                                        );
                                    }
                                });
                                ui.end_row();

                                ui.label(strings::FIELD_OPEN_IN);
                                ui.horizontal(|ui| {
                                    ui.radio_value(&mut self.split, false, strings::OPEN_NEW_TAB);
                                    ui.radio_value(
                                        &mut self.split,
                                        true,
                                        strings::OPEN_SPLIT_RIGHT,
                                    );
                                });
                                ui.end_row();
                            });
                        if let Some(error) = &self.error {
                            ui.add_space(4.0);
                            ui.label(RichText::new(error).color(theme::STATE_ERROR()));
                        }
                        ui.add_space(10.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let create = ui.button(strings::CREATE).clicked();
                            let cancel = ui.button(strings::CANCEL).clicked();
                            if cancel {
                                outcome = Outcome::Cancelled;
                            } else if create || enter {
                                outcome = self.try_create();
                            }
                        });
                    });
            });
        outcome
    }

    fn try_create(&mut self) -> Outcome {
        let folder = self.folder.trim();
        if !std::path::Path::new(folder).is_dir() {
            self.error = Some(strings::FOLDER_MISSING.to_owned());
            return Outcome::Open;
        }
        Outcome::Create(NewSession {
            agent: self
                .agents
                .get(self.selected)
                .map(|a| a.id.clone())
                .unwrap_or_default(),
            folder: folder.to_owned(),
            prompt: self.prompt.trim().to_owned(),
            args: self.args.trim().to_owned(),
            split: self.split,
            worktree: self.worktree.then(|| self.branch.trim().to_owned()),
        })
    }
}

/// The dock: one button per installed agent; a click starts it in the current folder (S-11).
pub(crate) fn dock(ui: &mut egui::Ui, agents: &[AgentChoice]) -> Option<String> {
    let mut clicked = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        for agent in agents.iter().filter(|a| !a.id.is_empty()) {
            // Initials until the agent icons exist (F3-14).
            let initials: String = agent
                .name
                .split_whitespace()
                .filter_map(|w| w.chars().next())
                .take(2)
                .collect();
            let button = egui::Button::new(RichText::new(initials).size(11.0))
                .min_size(egui::vec2(28.0, 24.0))
                .corner_radius(CornerRadius::same(6));
            if ui.add(button).on_hover_text(&agent.name).clicked() {
                clicked = Some(agent.id.clone());
            }
        }
    });
    clicked
}
