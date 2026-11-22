//! Claude Code transcripts: `~/.claude/projects/<project>/<session>.jsonl`.
//!
//! Each assistant message carries `message.model`, `message.usage` and ids. The same message
//! can be written more than once (streaming, retries, resumed sessions), so records are
//! de-duplicated by `message.id` + `requestId`, the same key ccusage uses (docs/adr/0005).

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{Task, UsageRecord};

/// Lines longer than this are skipped (docs/security.md 2.6).
const MAX_LINE: usize = 16 * 1024 * 1024;

#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type")]
    kind: Option<String>,
    timestamp: Option<String>,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    cwd: Option<String>,
    #[serde(rename = "isMeta")]
    is_meta: Option<bool>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    id: Option<String>,
    model: Option<String>,
    usage: Option<Usage>,
    content: Option<serde_json::Value>,
}

/// Whether a user message is a prompt the user typed (a task), not a tool result, a meta
/// message or the output of a local command.
fn is_prompt(content: &serde_json::Value) -> bool {
    match content {
        serde_json::Value::String(text) => {
            let text = text.trim_start();
            !text.is_empty() && !text.starts_with('<')
        }
        serde_json::Value::Array(items) => {
            let kind = |item: &serde_json::Value| {
                item.get("type")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            };
            items
                .iter()
                .all(|i| kind(i).as_deref() != Some("tool_result"))
                && items.iter().any(|i| {
                    kind(i).as_deref() == Some("text")
                        && i.get("text")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|t| !t.trim_start().starts_with('<'))
                })
        }
        _ => false,
    }
}

#[derive(Deserialize, Default)]
struct Usage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
}

/// Default transcript directory for the current user.
pub fn default_root() -> Option<PathBuf> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })?;
    let root = PathBuf::from(home).join(".claude").join("projects");
    root.is_dir().then_some(root)
}

/// All `.jsonl` files under `root` (one level of project folders, plus nested subagent logs).
pub fn transcript_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "jsonl") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Parses one transcript, skipping records whose key is already in `seen`.
pub fn parse_reader(reader: impl BufRead, seen: &mut HashSet<String>) -> Vec<UsageRecord> {
    parse_activity(reader, seen).0
}

/// Like [`parse_reader`], also returning the tasks (prompts the user sent).
pub fn parse_activity(
    reader: impl BufRead,
    seen: &mut HashSet<String>,
) -> (Vec<UsageRecord>, Vec<Task>) {
    let mut out = Vec::new();
    let mut tasks = Vec::new();
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        let user = line.contains("\"type\":\"user\"");
        if line.len() > MAX_LINE || !(user || line.contains("\"usage\"")) {
            continue;
        }
        let Ok(parsed) = serde_json::from_str::<Line>(&line) else {
            continue;
        };
        if parsed.kind.as_deref() == Some("user") {
            let prompt = parsed
                .message
                .as_ref()
                .and_then(|m| m.content.as_ref())
                .is_some_and(is_prompt);
            if prompt && parsed.is_meta != Some(true) {
                tasks.push(Task {
                    agent: "claude-code",
                    timestamp: parsed.timestamp.unwrap_or_default(),
                    session_id: parsed.session_id,
                });
            }
            continue;
        }
        if parsed.kind.as_deref() != Some("assistant") {
            continue;
        }
        let Some(message) = parsed.message else {
            continue;
        };
        let Some(usage) = message.usage else { continue };
        let model = message.model.unwrap_or_default();
        if model.is_empty() || model == "<synthetic>" {
            continue;
        }
        // Without both ids there is no reliable way to de-duplicate; count it once as is.
        if let (Some(id), Some(req)) = (&message.id, &parsed.request_id)
            && !seen.insert(format!("{id}:{req}"))
        {
            continue;
        }
        out.push(UsageRecord {
            agent: "claude-code",
            provider: "anthropic",
            model,
            timestamp: parsed.timestamp.unwrap_or_default(),
            session_id: parsed.session_id,
            project: parsed.cwd,
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cache_read_tokens: usage.cache_read_input_tokens,
            cache_write_tokens: usage.cache_creation_input_tokens,
        });
    }
    (out, tasks)
}

/// Parses every transcript under `root`: usage records (de-duplicated across files) and tasks.
pub fn scan_activity(root: &Path) -> (Vec<UsageRecord>, Vec<Task>) {
    let mut seen = HashSet::new();
    let (mut records, mut tasks) = (Vec::new(), Vec::new());
    for path in transcript_files(root) {
        if let Ok(file) = File::open(&path) {
            let (r, t) = parse_activity(BufReader::new(file), &mut seen);
            records.extend(r);
            tasks.extend(t);
        }
    }
    (records, tasks)
}

/// Parses every transcript under `root`, de-duplicated across files.
pub fn scan(root: &Path) -> Vec<UsageRecord> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for path in transcript_files(root) {
        if let Ok(file) = File::open(&path) {
            out.extend(parse_reader(BufReader::new(file), &mut seen));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/claude-basic.jsonl");

    #[test]
    fn parses_assistant_usage_and_skips_the_rest() {
        let records = parse_reader(FIXTURE.as_bytes(), &mut HashSet::new());
        // 3 assistant lines with usage, one of them duplicated, one synthetic, plus noise.
        assert_eq!(records.len(), 2);
        let first = &records[0];
        assert_eq!(first.model, "claude-opus-5-5");
        assert_eq!(
            (
                first.input_tokens,
                first.output_tokens,
                first.cache_read_tokens,
                first.cache_write_tokens
            ),
            (10, 200, 3000, 400)
        );
        assert_eq!(first.session_id.as_deref(), Some("s1"));
        assert_eq!(first.project.as_deref(), Some("/home/me/proj"));
    }

    #[test]
    fn duplicates_are_dropped_across_files() {
        let mut seen = HashSet::new();
        let a = parse_reader(FIXTURE.as_bytes(), &mut seen);
        let b = parse_reader(FIXTURE.as_bytes(), &mut seen);
        assert_eq!(a.len(), 2);
        assert!(
            b.is_empty(),
            "the same messages in another file must not count twice"
        );
    }

    #[test]
    fn corrupt_lines_are_ignored() {
        let input = "{not json\n\n{\"type\":\"assistant\",\"usage\":1}\n";
        assert!(parse_reader(input.as_bytes(), &mut HashSet::new()).is_empty());
    }
}
