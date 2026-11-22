//! Archived sessions (F4-15, S-12): closed panes kept as a record (title, agent, its session
//! id, folder) so they can be reopened with the agent's resume command. Stored in
//! `archive.json` in the data directory.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::saved::SavedAgent;

/// Most sessions kept; the oldest are dropped first.
const MAX_ARCHIVED: usize = 200;
/// Largest archive file read.
const MAX_BYTES: u64 = 4 << 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ArchivedSession {
    pub(crate) title: String,
    pub(crate) agent: Option<SavedAgent>,
    /// Agent display name when archived.
    pub(crate) agent_name: Option<String>,
    pub(crate) cwd: Option<PathBuf>,
    /// Unix seconds.
    pub(crate) archived_at: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    version: u32,
    sessions: Vec<ArchivedSession>,
}

/// Where the archive lives.
pub(crate) fn path() -> Option<PathBuf> {
    hv_config::data_dir().map(|d| d.join("archive.json"))
}

/// Reads the archive; an unreadable or oversized file reads as empty.
pub(crate) fn load(path: &Path) -> Vec<ArchivedSession> {
    if std::fs::metadata(path).map_or(true, |m| m.len() > MAX_BYTES) {
        return Vec::new();
    }
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<File>(&t).ok())
        .map(|f| f.sessions)
        .unwrap_or_default()
}

/// Writes the archive on a background thread (atomically), newest last, capped.
pub(crate) fn save(path: PathBuf, sessions: &[ArchivedSession]) {
    let start = sessions.len().saturating_sub(MAX_ARCHIVED);
    let file = File {
        version: 1,
        sessions: sessions[start..].to_vec(),
    };
    let _ = std::thread::Builder::new()
        .name("hv-archive".into())
        .spawn(move || {
            let result = (|| -> std::io::Result<()> {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                let text = serde_json::to_string_pretty(&file).map_err(std::io::Error::other)?;
                let tmp = path.with_extension("json.tmp");
                std::fs::write(&tmp, text)?;
                std::fs::rename(&tmp, &path)
            })();
            if let Err(err) = result {
                log::warn!("cannot save the archive: {err}");
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_round_trips() {
        let path = std::env::temp_dir().join(format!("hv-archive-{}.json", std::process::id()));
        let session = ArchivedSession {
            title: "Fix the login bug".into(),
            agent: Some(SavedAgent {
                id: "claude-code".into(),
                session: Some("abc".into()),
            }),
            agent_name: Some("Claude Code".into()),
            cwd: Some(std::env::temp_dir()),
            archived_at: 1_700_000_000,
        };
        save(path.clone(), std::slice::from_ref(&session));
        // The write happens on a thread.
        for _ in 0..50 {
            if path.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(load(&path), vec![session]);
        std::fs::write(&path, "garbage").unwrap();
        assert!(load(&path).is_empty());
        std::fs::remove_file(&path).unwrap();
    }
}
