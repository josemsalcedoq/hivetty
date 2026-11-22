//! Saving and restoring the window layout (F2-08): tabs, splits, each pane's directory and the
//! window geometry, in `layout.json` in the data directory. Versioned; anything unreadable is
//! ignored and hivetty starts with one shell.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::workspace::{Axis, PaneId, PaneNode, Tab, Tabs};

/// Bump on incompatible changes; older files are then ignored.
const VERSION: u32 = 1;
/// Largest layout file read (a layout is a few KiB; anything bigger is not ours).
const MAX_BYTES: u64 = 1 << 20;
/// Most panes restored, so a corrupted or hostile file cannot spawn hundreds of shells.
const MAX_PANES: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SavedLayout {
    version: u32,
    pub(crate) tabs: Vec<SavedTab>,
    pub(crate) active: usize,
    pub(crate) window: Option<SavedWindow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SavedTab {
    pub(crate) title: Option<String>,
    pub(crate) root: SavedNode,
    /// Index of the focused pane in reading order.
    pub(crate) focused: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum SavedNode {
    Pane {
        cwd: Option<PathBuf>,
        /// The agent that ran there and its own session id, for resuming (F4-21).
        #[serde(default)]
        agent: Option<SavedAgent>,
        /// A name the user gave the session (S-13).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    Split {
        horizontal: bool,
        ratio: f32,
        a: Box<SavedNode>,
        b: Box<SavedNode>,
    },
}

/// An agent running in a pane when the layout was saved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct SavedAgent {
    /// Profile id (`claude-code`).
    pub(crate) id: String,
    /// The agent's own session id, if it reported one.
    pub(crate) session: Option<String>,
}

/// What is saved for one pane.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PaneState {
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) agent: Option<SavedAgent>,
    pub(crate) title: Option<String>,
}

/// Longest saved session name restored.
const MAX_TITLE: usize = 200;

/// A saved name as it may be shown: one line, no control characters, not too long.
fn clean_title(title: &str) -> Option<String> {
    let clean: String = title
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(MAX_TITLE)
        .collect();
    let clean = clean.trim();
    (!clean.is_empty()).then(|| clean.to_owned())
}

/// Window size and position in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct SavedWindow {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) x: Option<i32>,
    pub(crate) y: Option<i32>,
    pub(crate) maximized: bool,
}

impl SavedLayout {
    /// Captures the tabs, asking `cwd` for each pane's directory.
    pub(crate) fn capture(
        tabs: &Tabs,
        window: Option<SavedWindow>,
        pane: impl Fn(PaneId) -> PaneState,
    ) -> Self {
        let saved = tabs
            .list
            .iter()
            .map(|tab| SavedTab {
                title: tab.title_override.clone(),
                root: save_node(&tab.root, &pane),
                focused: tab
                    .root
                    .panes()
                    .iter()
                    .position(|p| *p == tab.focused)
                    .unwrap_or(0),
            })
            .collect();
        Self {
            version: VERSION,
            tabs: saved,
            active: tabs.active,
            window,
        }
    }

    /// Rebuilds the tabs, calling `spawn` with each pane's directory; panes that fail to
    /// start are left out. Returns no tabs if nothing could start.
    pub(crate) fn restore(
        &self,
        mut spawn: impl FnMut(Option<&Path>, Option<&SavedAgent>, Option<String>) -> Option<PaneId>,
    ) -> Tabs {
        let mut budget = MAX_PANES;
        let mut out = Tabs::default();
        for saved in &self.tabs {
            let Some(root) = restore_node(&saved.root, &mut spawn, &mut budget) else {
                continue;
            };
            let panes = root.panes();
            let focused = panes.get(saved.focused).copied().unwrap_or(panes[0]);
            out.list.push(Tab {
                root,
                focused,
                zoomed: false,
                title_override: saved.title.clone(),
            });
        }
        out.active = self.active.min(out.list.len().saturating_sub(1));
        out
    }

    /// Reads `path`. `None` if missing, too large, unreadable or from another version.
    pub(crate) fn load(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        if meta.len() > MAX_BYTES {
            log::warn!("ignoring {}: too large", path.display());
            return None;
        }
        let text = std::fs::read_to_string(path).ok()?;
        match serde_json::from_str::<Self>(&text) {
            Ok(layout) if layout.version == VERSION && !layout.tabs.is_empty() => Some(layout),
            Ok(_) => None,
            Err(err) => {
                log::warn!("ignoring {}: {err}", path.display());
                None
            }
        }
    }

    /// Writes `path` atomically (temporary file + rename).
    pub(crate) fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)
    }
}

fn save_node(node: &PaneNode, pane: &impl Fn(PaneId) -> PaneState) -> SavedNode {
    match node {
        PaneNode::Leaf(id) => {
            let state = pane(*id);
            SavedNode::Pane {
                cwd: state.cwd,
                agent: state.agent,
                title: state.title,
            }
        }
        PaneNode::Split { axis, ratio, a, b } => SavedNode::Split {
            horizontal: *axis == Axis::Horizontal,
            ratio: *ratio,
            a: Box::new(save_node(a, pane)),
            b: Box::new(save_node(b, pane)),
        },
    }
}

fn restore_node(
    node: &SavedNode,
    spawn: &mut impl FnMut(Option<&Path>, Option<&SavedAgent>, Option<String>) -> Option<PaneId>,
    budget: &mut usize,
) -> Option<PaneNode> {
    match node {
        SavedNode::Pane { cwd, agent, title } => {
            if *budget == 0 {
                return None;
            }
            *budget -= 1;
            // A directory that no longer exists falls back to the default one.
            let cwd = cwd.as_deref().filter(|d| d.is_dir());
            let title = title.as_deref().and_then(clean_title);
            spawn(cwd, agent.as_ref(), title).map(PaneNode::Leaf)
        }
        SavedNode::Split {
            horizontal,
            ratio,
            a,
            b,
        } => {
            let a = restore_node(a, spawn, budget);
            let b = restore_node(b, spawn, budget);
            match (a, b) {
                (Some(a), Some(b)) => Some(PaneNode::Split {
                    axis: if *horizontal {
                        Axis::Horizontal
                    } else {
                        Axis::Vertical
                    },
                    ratio: if ratio.is_finite() {
                        ratio.clamp(0.1, 0.9)
                    } else {
                        0.5
                    },
                    a: Box::new(a),
                    b: Box::new(b),
                }),
                (Some(only), None) | (None, Some(only)) => Some(only),
                (None, None) => None,
            }
        }
    }
}

/// Where the layout is kept.
pub(crate) fn layout_path() -> Option<PathBuf> {
    hv_config::data_dir().map(|d| d.join("layout.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_tabs() -> Tabs {
        let mut tabs = Tabs::default();
        tabs.open(1);
        let tab = tabs.active_mut().unwrap();
        tab.root.split(1, 2, Axis::Horizontal);
        tab.focused = 2;
        tab.title_override = Some("api".into());
        tabs.open(3);
        tabs.active = 0;
        tabs
    }

    #[test]
    fn layout_round_trips_through_json() {
        let tabs = sample_tabs();
        let dir = std::env::temp_dir();
        let mut saved = SavedLayout::capture(&tabs, None, |id| PaneState {
            cwd: (id == 2).then(|| dir.clone()),
            agent: (id == 3).then(|| SavedAgent {
                id: "claude-code".into(),
                session: Some("abc-123".into()),
            }),
            title: (id == 3).then(|| "Fix the\nlogin bug".into()),
        });
        saved.active = tabs.active;
        let json = serde_json::to_string(&saved).unwrap();
        let back: SavedLayout = serde_json::from_str(&json).unwrap();
        assert_eq!(back, saved);

        let mut next = 10;
        let mut cwds = Vec::new();
        let mut agents = Vec::new();
        let mut titles = Vec::new();
        let restored = back.restore(|cwd, agent, title| {
            titles.push(title);
            cwds.push(cwd.map(Path::to_path_buf));
            agents.push(agent.cloned());
            next += 1;
            Some(next)
        });
        assert_eq!(
            agents[2].as_ref().unwrap().session.as_deref(),
            Some("abc-123")
        );
        assert_eq!(restored.list.len(), 2);
        assert_eq!(restored.list[0].root.panes(), vec![11, 12]);
        assert_eq!(restored.list[0].focused, 12);
        assert_eq!(restored.list[0].title_override.as_deref(), Some("api"));
        assert_eq!(cwds, vec![None, Some(std::env::temp_dir()), None]);
        assert!(
            titles.contains(&Some("Fix the login bug".to_owned())),
            "{titles:?}"
        );
    }

    #[test]
    fn layouts_saved_before_agents_existed_still_load() {
        let old = r#"{"version":1,"tabs":[{"title":null,"root":{"kind":"pane","cwd":null},"focused":0}],"active":0,"window":null}"#;
        let layout: SavedLayout = serde_json::from_str(old).unwrap();
        assert_eq!(layout.tabs.len(), 1);
    }

    #[test]
    fn failed_panes_collapse_and_missing_dirs_fall_back() {
        let saved = SavedLayout {
            version: VERSION,
            tabs: vec![SavedTab {
                title: None,
                root: SavedNode::Split {
                    horizontal: false,
                    ratio: f32::NAN,
                    a: Box::new(SavedNode::Pane {
                        cwd: Some("/definitely/not/here".into()),
                        agent: None,
                        title: None,
                    }),
                    b: Box::new(SavedNode::Pane {
                        cwd: None,
                        agent: None,
                        title: None,
                    }),
                },
                focused: 7,
            }],
            active: 5,
            window: None,
        };
        let mut calls = 0;
        let tabs = saved.restore(|cwd, _, _| {
            assert!(cwd.is_none());
            calls += 1;
            (calls == 2).then_some(42) // the first pane fails to start
        });
        assert_eq!(tabs.list[0].root, PaneNode::Leaf(42));
        assert_eq!(tabs.list[0].focused, 42);
        assert_eq!(tabs.active, 0);
    }

    #[test]
    fn hostile_files_are_bounded() {
        let mut node = SavedNode::Pane {
            cwd: None,
            agent: None,
            title: None,
        };
        for _ in 0..200 {
            node = SavedNode::Split {
                horizontal: true,
                ratio: 0.5,
                a: Box::new(node),
                b: Box::new(SavedNode::Pane {
                    cwd: None,
                    agent: None,
                    title: None,
                }),
            };
        }
        let saved = SavedLayout {
            version: VERSION,
            tabs: vec![SavedTab {
                title: None,
                root: node,
                focused: 0,
            }],
            active: 0,
            window: None,
        };
        let mut spawned = 0;
        saved.restore(|_, _, _| {
            spawned += 1;
            Some(spawned)
        });
        assert_eq!(spawned, MAX_PANES as u64);
    }

    #[test]
    fn save_and_load_use_the_file_system() {
        let path = std::env::temp_dir().join(format!("hv-layout-{}.json", std::process::id()));
        let saved = SavedLayout::capture(&sample_tabs(), None, |_| PaneState::default());
        saved.save(&path).unwrap();
        assert_eq!(SavedLayout::load(&path), Some(saved));
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(SavedLayout::load(&path), None);
        std::fs::remove_file(&path).unwrap();
    }
}
