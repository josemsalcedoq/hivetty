//! The sessions sidebar (F4-11, F4-12): a search box, filters with counts, and the sessions
//! grouped by project.

use egui::{Color32, CornerRadius, RichText, Sense, Stroke, Vec2};

use crate::{
    ArchivedRow, ProjectMenu, RowState, SessionMenu, SessionRow, elide, state_color, strings, theme,
};

/// Height kept free under the list for the agent dock.
const DOCK_HEIGHT: f32 = 44.0;

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
    /// Projects whose sessions are hidden (S-14).
    collapsed: std::collections::HashSet<String>,
    /// Put the keyboard in the search field next frame (S-03).
    pub(crate) focus_search: bool,
}

impl Sidebar {
    /// Draws the sidebar. Returns the index (in `sessions`) of a clicked row and whether the
    /// New session button was pressed.
    pub(crate) fn show(
        &mut self,
        ui: &mut egui::Ui,
        sessions: &[SessionRow],
        archived: &[ArchivedRow],
        new_session_hint: &str,
        search_hint: &str,
        announcement: Option<&str>,
    ) -> SidebarOutput {
        let mut out = SidebarOutput::default();
        let mut clicked = None;
        let mut new_session = false;
        let mut peek = None;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(strings::SESSIONS)
                    .size(12.0)
                    .color(theme::TEXT_MUTED()),
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
        let hint = if search_hint.is_empty() {
            strings::SEARCH_SESSIONS.to_owned()
        } else {
            format!("{}  {search_hint}", strings::SEARCH_SESSIONS)
        };
        let search = egui::TextEdit::singleline(&mut self.query)
            .hint_text(hint)
            .desired_width(f32::INFINITY);
        let search = ui.add(search);
        if std::mem::take(&mut self.focus_search) {
            search.request_focus();
        }
        // Enter opens the first session that matches; Esc gives the keyboard back.
        let open_first = search.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for filter in [Filter::All, Filter::NeedsYou, Filter::Working, Filter::Done] {
                let count = sessions.iter().filter(|s| filter.keeps(s.state)).count();
                let text = format!("{} {count}", filter.label());
                let selected = self.filter == filter;
                let chip = egui::Button::new(RichText::new(text).size(11.0).color(if selected {
                    theme::TEXT_PRIMARY()
                } else {
                    theme::TEXT_SECONDARY()
                }))
                .fill(if selected {
                    theme::SELECTED_BG()
                } else {
                    Color32::TRANSPARENT
                })
                .stroke(Stroke::new(1.0, theme::BORDER()))
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
        // Leave room below for the agent dock.
        let list_height = (ui.available_height() - DOCK_HEIGHT).max(40.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height(list_height)
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
                    let collapsed = self.collapsed.contains(project);
                    // Room for the fold arrow, painted below (the fonts have no triangles).
                    let header = ui.add(
                        egui::Label::new(
                            RichText::new(format!("    {project}  ·  {count}"))
                                .size(12.0)
                                .color(theme::TEXT_SECONDARY()),
                        )
                        .sense(Sense::click()),
                    );
                    let c = egui::pos2(header.rect.left() + 5.0, header.rect.center().y);
                    let points = if collapsed {
                        vec![
                            c + Vec2::new(-2.0, -4.0),
                            c + Vec2::new(-2.0, 4.0),
                            c + Vec2::new(3.0, 0.0),
                        ]
                    } else {
                        vec![
                            c + Vec2::new(-4.0, -2.0),
                            c + Vec2::new(4.0, -2.0),
                            c + Vec2::new(0.0, 3.0),
                        ]
                    };
                    ui.painter().add(egui::Shape::convex_polygon(
                        points,
                        theme::TEXT_SECONDARY(),
                        Stroke::NONE,
                    ));
                    let first = visible
                        .iter()
                        .find(|(_, s)| s.project == project)
                        .map(|(i, _)| *i);
                    let mut toggle = header.clicked();
                    header.context_menu(|ui| {
                        let fold = if collapsed {
                            strings::MENU_EXPAND
                        } else {
                            strings::MENU_COLLAPSE
                        };
                        if ui.button(strings::MENU_NEW_SESSION_HERE).clicked() {
                            out.project_menu = first.map(|i| (i, ProjectMenu::NewSession));
                            ui.close();
                        }
                        if ui.button(fold).clicked() {
                            toggle = true;
                            ui.close();
                        }
                        if ui.button(strings::MENU_SHOW_FOLDER).clicked() {
                            out.project_menu = first.map(|i| (i, ProjectMenu::ShowFolder));
                            ui.close();
                        }
                        if ui.button(strings::MENU_ARCHIVE_FINISHED).clicked() {
                            out.project_menu = first.map(|i| (i, ProjectMenu::ArchiveFinished));
                            ui.close();
                        }
                    });
                    if toggle && !self.collapsed.remove(project) {
                        self.collapsed.insert(project.to_owned());
                    }
                    if collapsed {
                        continue;
                    }
                    for (index, row) in visible.iter().filter(|(_, s)| s.project == project) {
                        let (click, card) = session_row(ui, row);
                        out.card_shown |= card;
                        match click {
                            RowClick::Open => clicked = Some(*index),
                            RowClick::Peek => peek = Some(*index),
                            RowClick::Menu(choice) => out.menu = Some((*index, choice)),
                            RowClick::None => {}
                        }
                    }
                }
                // The latest change that needs the user, as a live region: screen readers read
                // it out when it changes (F4-25).
                if let Some(text) = announcement {
                    ui.add_space(8.0);
                    let label = ui.label(RichText::new(text).size(11.0).color(theme::TEXT_MUTED()));
                    ui.ctx().accesskit_node_builder(label.id, |node| {
                        node.set_live(egui::accesskit::Live::Polite);
                    });
                }
                if sessions.is_empty() {
                    ui.label(RichText::new(strings::NO_SESSIONS).color(theme::TEXT_MUTED()));
                } else if visible.is_empty() {
                    ui.label(RichText::new(strings::NO_MATCHES).color(theme::TEXT_MUTED()));
                }
                if !archived.is_empty() {
                    ui.add_space(10.0);
                    egui::CollapsingHeader::new(
                        RichText::new(format!("{} {}", strings::ARCHIVED, archived.len()))
                            .size(12.0)
                            .color(theme::TEXT_SECONDARY()),
                    )
                    .id_salt("archived")
                    .default_open(false)
                    .show(ui, |ui| {
                        for (i, row) in archived.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&row.title).size(12.0));
                                    ui.label(
                                        RichText::new(&row.detail)
                                            .size(11.0)
                                            .color(theme::TEXT_MUTED()),
                                    );
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.small_button(strings::DELETE).clicked() {
                                            out.archive_delete = Some(i);
                                        }
                                        if ui.small_button(strings::RESTORE_SESSION).clicked() {
                                            out.archive_restore = Some(i);
                                        }
                                    },
                                );
                            });
                        }
                    });
                }
            });
        if open_first && clicked.is_none() {
            let query = self.query.trim().to_lowercase();
            clicked = sessions
                .iter()
                .enumerate()
                .find(|(_, s)| {
                    self.filter.keeps(s.state) && (query.is_empty() || matches_query(s, &query))
                })
                .map(|(i, _)| i);
        }
        out.clicked = clicked;
        out.new_session = new_session;
        out.peek = peek;
        out
    }
}

fn matches_query(row: &SessionRow, query: &str) -> bool {
    [&row.title, &row.subtitle, &row.project, &row.agent]
        .iter()
        .any(|field| field.to_lowercase().contains(query))
}

/// What the sidebar produced this frame.
#[derive(Debug, Default)]
pub(crate) struct SidebarOutput {
    pub(crate) clicked: Option<usize>,
    pub(crate) new_session: bool,
    pub(crate) peek: Option<usize>,
    pub(crate) menu: Option<(usize, SessionMenu)>,
    pub(crate) project_menu: Option<(usize, ProjectMenu)>,
    pub(crate) card_shown: bool,
    pub(crate) archive_restore: Option<usize>,
    pub(crate) archive_delete: Option<usize>,
}

/// What a click on a row asked for.
enum RowClick {
    None,
    Open,
    Peek,
    Menu(SessionMenu),
}

/// One two-line row: dot, title, elapsed time / state or activity. Waiting rows get a Peek
/// button; a right click peeks at any row.
fn session_row(ui: &mut egui::Ui, row: &SessionRow) -> (RowClick, bool) {
    let height = 44.0;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
    // What screen readers announce for the row (painted by hand, so egui cannot infer it).
    let label = format!("{}, {}, {}", row.title, row.subtitle, row.elapsed);
    response
        .widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, row.focused, &label));
    let painter = ui.painter_at(rect);
    let bg = if row.focused {
        theme::SELECTED_BG()
    } else if response.hovered() {
        theme::HOVER_BG()
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
            theme::ACCENT(),
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
        theme::TEXT_MUTED(),
    );
    let title_width = (time.left() - text_left - 8.0).max(20.0);
    let title = elide(ui, &row.title, &font, title_width);
    painter.text(
        egui::pos2(text_left, rect.top() + 7.0),
        egui::Align2::LEFT_TOP,
        title,
        font,
        theme::TEXT_PRIMARY(),
    );
    let subtitle_color = match row.state {
        RowState::Done => theme::STATE_DONE(),
        RowState::Waiting => theme::STATE_WAITING(),
        RowState::Error => theme::STATE_ERROR(),
        _ => theme::TEXT_SECONDARY(),
    };
    let subtitle = elide(ui, &row.subtitle, &small, right - text_left);
    painter.text(
        egui::pos2(text_left, rect.top() + 25.0),
        egui::Align2::LEFT_TOP,
        subtitle,
        small,
        subtitle_color,
    );
    let mut click = RowClick::None;
    if row.state == RowState::Waiting && !row.focused {
        let button_rect = egui::Rect::from_min_size(
            egui::pos2(rect.right() - 46.0, rect.top() + 24.0),
            Vec2::new(40.0, 16.0),
        );
        let peek = ui.interact(
            button_rect,
            ui.scope_id().with(("peek", &row.title, rect.top() as i32)),
            Sense::click(),
        );
        painter.rect_filled(button_rect, CornerRadius::same(4), theme::BORDER());
        painter.text(
            button_rect.center(),
            egui::Align2::CENTER_CENTER,
            strings::PEEK,
            egui::FontId::proportional(10.0),
            theme::TEXT_PRIMARY(),
        );
        if peek.clicked() {
            click = RowClick::Peek;
        }
    }
    if matches!(click, RowClick::None) && response.clicked() {
        click = RowClick::Open;
    }
    // Hover card (S-15).
    let card_shown = !row.card.is_empty() && response.hovered();
    let response = if row.card.is_empty() {
        response
    } else {
        response.on_hover_ui(|ui| {
            egui::Grid::new("session-card")
                .num_columns(2)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    for (label, value) in &row.card {
                        ui.label(RichText::new(label).size(12.0).color(theme::TEXT_MUTED()));
                        ui.label(RichText::new(value).size(12.0));
                        ui.end_row();
                    }
                });
        })
    };
    // Right click: the session menu (S-13).
    response.context_menu(|ui| {
        for (choice, label) in [
            (SessionMenu::Focus, strings::MENU_FOCUS),
            (SessionMenu::Rename, strings::MENU_RENAME),
            (SessionMenu::Restart, strings::MENU_RESTART),
            (SessionMenu::Peek, strings::MENU_PEEK),
            (SessionMenu::Archive, strings::MENU_ARCHIVE),
            (SessionMenu::CopyId, strings::MENU_COPY_ID),
            (SessionMenu::ShowFolder, strings::MENU_SHOW_FOLDER),
            (SessionMenu::OpenInEditor, strings::MENU_OPEN_IN_EDITOR),
            (SessionMenu::ShowTranscript, strings::MENU_SHOW_TRANSCRIPT),
            (SessionMenu::RemoveWorktree, strings::MENU_REMOVE_WORKTREE),
            (SessionMenu::Close, strings::MENU_CLOSE),
        ] {
            if choice == SessionMenu::RemoveWorktree && !row.worktree
                || choice == SessionMenu::ShowTranscript && !row.transcript
            {
                continue;
            }
            if ui.button(label).clicked() {
                click = RowClick::Menu(choice);
                ui.close();
            }
        }
    });
    (click, card_shown)
}
