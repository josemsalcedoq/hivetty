//! Detecting agents started by hand in a pane (F4-03): every second, look at the processes
//! below each pane's shell and match them against the profiles.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::profile::Profile;

/// How deep below the shell to look (shell → npx → node → agent is common).
const MAX_DEPTH: usize = 6;

/// One process as the matcher sees it.
#[derive(Debug, Clone)]
pub struct Proc {
    pub pid: u32,
    pub parent: Option<u32>,
    pub name: String,
    pub args: Vec<String>,
}

/// The agent running below `root` (breadth first, so the outermost agent wins), if any.
pub fn agent_below(root: u32, procs: &[Proc], profiles: &[Profile]) -> Option<String> {
    let mut children: HashMap<u32, Vec<&Proc>> = HashMap::new();
    for p in procs {
        if let Some(parent) = p.parent {
            children.entry(parent).or_default().push(p);
        }
    }
    let mut level = vec![root];
    for _ in 0..MAX_DEPTH {
        let mut next = Vec::new();
        for pid in &level {
            for child in children.get(pid).into_iter().flatten() {
                if let Some(profile) = profiles
                    .iter()
                    .find(|pr| pr.matches(&child.name, &child.args))
                {
                    return Some(profile.id.clone());
                }
                next.push(child.pid);
            }
        }
        if next.is_empty() {
            break;
        }
        level = next;
    }
    None
}

/// Watches pane process trees on a background thread.
#[derive(Debug)]
pub struct ProcessWatcher {
    panes: Arc<Mutex<Vec<(u64, u32)>>>,
}

impl ProcessWatcher {
    /// Starts watching. `report(pane, agent)` runs on the watcher thread whenever the agent
    /// detected in a pane changes (`None` = the agent exited).
    pub fn start(
        profiles: Vec<Profile>,
        interval: Duration,
        report: impl Fn(u64, Option<String>) + Send + 'static,
    ) -> std::io::Result<Self> {
        let panes: Arc<Mutex<Vec<(u64, u32)>>> = Arc::default();
        // The thread ends when the watcher is dropped.
        let watched = Arc::downgrade(&panes);
        std::thread::Builder::new()
            .name("hv-agent-watch".into())
            .spawn(move || {
                // Without access to the process list there is nothing to detect.
                let Ok(mut system) = System::new() else {
                    return;
                };
                let mut last: HashMap<u64, Option<String>> = HashMap::new();
                loop {
                    std::thread::sleep(interval);
                    let Some(watched) = watched.upgrade() else {
                        return;
                    };
                    let current = match watched.lock() {
                        Ok(p) => p.clone(),
                        Err(_) => return,
                    };
                    drop(watched);
                    if current.is_empty() {
                        last.clear();
                        continue;
                    }
                    system.refresh_processes_specifics(
                        ProcessesToUpdate::All,
                        true,
                        ProcessRefreshKind::nothing()
                            .with_cmd(UpdateKind::OnlyIfNotSet)
                            .with_exe(UpdateKind::OnlyIfNotSet),
                    );
                    let procs: Vec<Proc> = system
                        .processes()
                        .iter()
                        .map(|(pid, p)| Proc {
                            pid: pid.as_u32(),
                            parent: p.parent().map(|pp| pp.as_u32()),
                            name: p.name().to_string_lossy().into_owned(),
                            args: p
                                .cmd()
                                .iter()
                                .map(|a| a.to_string_lossy().into_owned())
                                .collect(),
                        })
                        .collect();
                    last.retain(|pane, _| current.iter().any(|(p, _)| p == pane));
                    for (pane, root) in current {
                        let agent = agent_below(root, &procs, &profiles);
                        if last.get(&pane) != Some(&agent) {
                            last.insert(pane, agent.clone());
                            report(pane, agent);
                        }
                    }
                }
            })?;
        Ok(Self { panes })
    }

    /// Replaces the list of `(pane, shell pid)` to watch.
    pub fn set_panes(&self, panes: Vec<(u64, u32)>) {
        if let Ok(mut p) = self.panes.lock() {
            *p = panes;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, parent: u32, name: &str, args: &str) -> Proc {
        Proc {
            pid,
            parent: Some(parent),
            name: name.into(),
            args: args.split_whitespace().map(String::from).collect(),
        }
    }

    #[test]
    fn finds_the_agent_under_the_shell() {
        let profiles = crate::profile::builtin();
        let procs = vec![
            proc(10, 1, "pwsh.exe", "pwsh"),
            proc(
                11,
                10,
                "node.exe",
                "node C:/npm/node_modules/@openai/codex/bin/codex.js",
            ),
            proc(12, 11, "codex.exe", "codex"),
            proc(20, 1, "zsh", "zsh"),
            proc(21, 20, "vim", "vim"),
        ];
        assert_eq!(agent_below(10, &procs, &profiles).as_deref(), Some("codex"));
        assert_eq!(agent_below(20, &procs, &profiles), None);
        assert_eq!(agent_below(99, &procs, &profiles), None);
    }

    #[test]
    fn real_process_list_can_be_read() {
        let watcher = ProcessWatcher::start(
            crate::profile::builtin(),
            Duration::from_millis(50),
            |_, _| {},
        );
        let watcher = watcher.unwrap();
        watcher.set_panes(vec![(1, std::process::id())]);
        std::thread::sleep(Duration::from_millis(200));
    }
}
