//! Gemini CLI's chat logs (F5-03): `~/.gemini/tmp/<project>/chats/session-*.jsonl`, with the
//! project's folder in `~/.gemini/tmp/<project>/.project_root`. Format checked against version
//! 0.70.0 (`tests/fixtures/gemini-cli/0.70.0`).
//!
//! - The first line has the `sessionId`.
//! - Messages are `type: "user"` or `"gemini"`. A model message is written again as it updates
//!   (same `id`), so it is counted once. Its `tokens` count cached tokens inside `input`, and
//!   the thinking tokens (`thoughts`) apart from `output`; both are billed as output.
//! - Lines with `$set` update the header and are ignored.

use std::collections::HashSet;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{FileContext, Task, UsageRecord};

/// Lines longer than this are skipped (docs/security.md 2.6).
const MAX_LINE: usize = 16 * 1024 * 1024;
/// Largest `.project_root` read.
const MAX_ROOT_FILE: u64 = 4096;

#[derive(Deserialize)]
struct Line {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    id: Option<String>,
    timestamp: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    content: Option<serde_json::Value>,
    tokens: Option<Tokens>,
    model: Option<String>,
}

#[derive(Deserialize)]
struct Tokens {
    #[serde(default)]
    input: u64,
    #[serde(default)]
    output: u64,
    #[serde(default)]
    cached: u64,
    #[serde(default)]
    thoughts: u64,
}

/// `~/.gemini/tmp`, if it exists.
pub fn default_root() -> Option<PathBuf> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })?;
    let root = PathBuf::from(home).join(".gemini").join("tmp");
    root.is_dir().then_some(root)
}

/// Every chat log under `root`, with the folder of its project when known.
pub fn session_files(root: &Path) -> Vec<(PathBuf, Option<String>)> {
    let mut out = Vec::new();
    let Ok(projects) = std::fs::read_dir(root) else {
        return out;
    };
    for project in projects.flatten() {
        let dir = project.path();
        let project_root = std::fs::metadata(dir.join(".project_root"))
            .ok()
            .filter(|m| m.is_file() && m.len() <= MAX_ROOT_FILE)
            .and_then(|_| std::fs::read_to_string(dir.join(".project_root")).ok())
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());
        let Ok(chats) = std::fs::read_dir(dir.join("chats")) else {
            continue;
        };
        for chat in chats.flatten() {
            let path = chat.path();
            if path.extension().is_some_and(|e| e == "jsonl") {
                out.push((path, project_root.clone()));
            }
        }
    }
    out
}

/// A prompt the user typed: text that is not the CLI's own `<session_context>`, and no tool
/// results.
fn is_prompt(content: &serde_json::Value) -> bool {
    let Some(items) = content.as_array() else {
        return content
            .as_str()
            .is_some_and(|t| !t.trim().is_empty() && !t.trim_start().starts_with('<'));
    };
    items.iter().all(|i| i.get("functionResponse").is_none())
        && items.iter().any(|i| {
            i.get("text")
                .and_then(|t| t.as_str())
                .is_some_and(|t| !t.trim().is_empty() && !t.trim_start().starts_with('<'))
        })
}

/// Parses new lines of one chat log; `ctx` keeps the session id across incremental reads and
/// holds the project folder.
pub fn parse_activity(
    reader: impl BufRead,
    seen: &mut HashSet<String>,
    ctx: &mut FileContext,
) -> (Vec<UsageRecord>, Vec<Task>) {
    let mut records = Vec::new();
    let mut tasks = Vec::new();
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if line.len() > MAX_LINE {
            continue;
        }
        let Ok(parsed) = serde_json::from_str::<Line>(&line) else {
            continue;
        };
        if let Some(session) = parsed.session_id {
            ctx.session = Some(session);
            continue;
        }
        let Some(id) = parsed.id else { continue };
        match parsed.kind.as_deref() {
            Some("gemini") => {
                let Some(tokens) = parsed.tokens else {
                    continue;
                };
                if !seen.insert(format!("gemini:{id}")) {
                    continue;
                }
                let cached = tokens.cached.min(tokens.input);
                records.push(UsageRecord {
                    agent: "gemini-cli",
                    provider: "google",
                    model: parsed.model.unwrap_or_else(|| "unknown".to_owned()),
                    timestamp: parsed.timestamp.unwrap_or_default(),
                    session_id: ctx.session.clone(),
                    project: ctx.cwd.clone(),
                    input_tokens: tokens.input - cached,
                    output_tokens: tokens.output + tokens.thoughts,
                    cache_read_tokens: cached,
                    cache_write_tokens: 0,
                });
            }
            Some("user")
                if parsed.content.as_ref().is_some_and(is_prompt)
                    && seen.insert(format!("gemini-task:{id}")) =>
            {
                tasks.push(Task {
                    agent: "gemini-cli",
                    timestamp: parsed.timestamp.unwrap_or_default(),
                    session_id: ctx.session.clone(),
                });
            }
            _ => {}
        }
    }
    (records, tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/gemini-cli/0.70.0/session.jsonl");

    #[test]
    fn real_session_parses() {
        let mut seen = HashSet::new();
        let mut ctx = FileContext {
            cwd: Some("/home/me/proj".into()),
            ..FileContext::default()
        };
        let (records, tasks) = parse_activity(FIXTURE.as_bytes(), &mut seen, &mut ctx);
        // Two model messages; the first is written three times as it updates.
        assert_eq!(records.len(), 2, "{records:#?}");
        assert_eq!(
            tasks.len(),
            1,
            "the session context and the tool result are not tasks"
        );
        let first = &records[0];
        assert_eq!((first.agent, first.provider), ("gemini-cli", "google"));
        assert_eq!(first.model, "gemini-3.5-flash-lite");
        assert_eq!((first.input_tokens, first.output_tokens), (8412, 18 + 437));
        let second = &records[1];
        assert_eq!(
            (second.input_tokens, second.cache_read_tokens),
            (8960 - 6090, 6090)
        );
        assert_eq!(first.session_id, ctx.session);
        assert_eq!(first.project.as_deref(), Some("/home/me/proj"));
        let (again, _) = parse_activity(FIXTURE.as_bytes(), &mut seen, &mut ctx);
        assert!(again.is_empty());
    }

    #[test]
    fn project_roots_are_read_from_the_marker_file() {
        let base = std::env::temp_dir().join(format!("hv-gemini-{}", std::process::id()));
        let chats = base.join("proj").join("chats");
        std::fs::create_dir_all(&chats).unwrap();
        std::fs::write(base.join("proj").join(".project_root"), "/work/proj\n").unwrap();
        std::fs::write(chats.join("session-1.jsonl"), FIXTURE).unwrap();
        let files = session_files(&base);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].1.as_deref(), Some("/work/proj"));
        std::fs::remove_dir_all(&base).unwrap();
    }
}
