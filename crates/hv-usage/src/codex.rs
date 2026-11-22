//! Codex CLI's session logs (F5-03): `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`
//! (`$CODEX_HOME` if set). Format checked against version 0.171.0
//! (`tests/fixtures/codex/0.171.0`).
//!
//! - `session_meta`: the session id and its folder.
//! - `turn_context`: the model of each turn.
//! - `token_usage_record`: one per model response, with `usage`. OpenAI counts cached tokens
//!   inside `input_tokens`, so the uncached part is `input_tokens - cached_input_tokens`; the
//!   reasoning tokens are already part of `output_tokens`.
//! - `response_item` messages with `role: "user"`: the prompts (tasks), except the
//!   `<environment_context>` the CLI adds.

use std::collections::HashSet;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::{FileContext, Task, UsageRecord};

/// Lines longer than this are skipped (docs/security.md 2.6).
const MAX_LINE: usize = 16 * 1024 * 1024;

#[derive(Deserialize)]
struct Line {
    timestamp: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    payload: Option<serde_json::Value>,
}

#[derive(Deserialize, Default)]
struct Usage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cached_input_tokens: u64,
    #[serde(default)]
    cache_write_input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
}

/// `~/.codex/sessions` (or `$CODEX_HOME/sessions`), if it exists.
pub fn default_root() -> Option<PathBuf> {
    let base = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .map(|h| PathBuf::from(h).join(".codex"))
        })?;
    let root = base.join("sessions");
    root.is_dir().then_some(root)
}

/// All `.jsonl` logs under `root`.
pub fn session_files(root: &Path) -> Vec<PathBuf> {
    crate::claude::transcript_files(root)
}

fn text(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key)?.as_str().map(str::to_owned)
}

/// Parses new lines of one log. `ctx` carries what earlier lines said (session, folder, model)
/// across incremental reads.
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
        let (Some(kind), Some(p)) = (parsed.kind.as_deref(), parsed.payload.as_ref()) else {
            continue;
        };
        match kind {
            "session_meta" => {
                ctx.session = text(p, "session_id").or_else(|| text(p, "id"));
                ctx.cwd = text(p, "cwd").or(ctx.cwd.take());
            }
            "turn_context" => {
                if let Some(model) = text(p, "model") {
                    ctx.model = Some(model);
                }
                ctx.cwd = text(p, "cwd").or(ctx.cwd.take());
            }
            "token_usage_record" => {
                let Some(usage) = p
                    .get("usage")
                    .and_then(|u| serde_json::from_value::<Usage>(u.clone()).ok())
                else {
                    continue;
                };
                if let Some(id) = text(p, "response_id")
                    && !seen.insert(format!("codex:{id}"))
                {
                    continue;
                }
                let cached = usage.cached_input_tokens.min(usage.input_tokens);
                let write = usage
                    .cache_write_input_tokens
                    .min(usage.input_tokens - cached);
                records.push(UsageRecord {
                    agent: "codex",
                    provider: "openai",
                    model: ctx.model.clone().unwrap_or_else(|| "unknown".to_owned()),
                    timestamp: parsed.timestamp.clone().unwrap_or_default(),
                    session_id: text(p, "session_id").or_else(|| ctx.session.clone()),
                    project: ctx.cwd.clone(),
                    input_tokens: usage.input_tokens - cached - write,
                    output_tokens: usage.output_tokens,
                    cache_read_tokens: cached,
                    cache_write_tokens: write,
                });
            }
            "response_item" => {
                if p.get("role").and_then(|r| r.as_str()) != Some("user") {
                    continue;
                }
                let typed = p
                    .get("content")
                    .and_then(|c| c.as_array())
                    .is_some_and(|items| {
                        items.iter().any(|i| {
                            i.get("type").and_then(|t| t.as_str()) == Some("input_text")
                                && i.get("text").and_then(|t| t.as_str()).is_some_and(|t| {
                                    !t.trim().is_empty() && !t.trim_start().starts_with('<')
                                })
                        })
                    });
                let new = text(p, "id").is_none_or(|id| seen.insert(format!("codex-task:{id}")));
                if typed && new {
                    tasks.push(Task {
                        agent: "codex",
                        timestamp: parsed.timestamp.clone().unwrap_or_default(),
                        session_id: ctx.session.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    (records, tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/codex/0.171.0/session.jsonl");

    #[test]
    fn real_session_parses() {
        let mut seen = HashSet::new();
        let mut ctx = FileContext::default();
        let (records, tasks) = parse_activity(FIXTURE.as_bytes(), &mut seen, &mut ctx);
        // Three model responses in the first turn and one in the second; two typed prompts.
        assert_eq!(records.len(), 4, "{records:#?}");
        assert_eq!(tasks.len(), 2);
        let first = &records[0];
        assert_eq!((first.agent, first.provider), ("codex", "openai"));
        assert_eq!(first.model, "gpt-6.2-sol");
        assert_eq!(first.project.as_deref(), Some("/home/me/proj"));
        // 16 392 input of which 14 208 cached.
        assert_eq!((first.input_tokens, first.cache_read_tokens), (2184, 14208));
        assert_eq!(first.output_tokens, 44);
        assert!(first.session_id.is_some());
        // Read again (a rewritten log): nothing counts twice.
        let (again, again_tasks) = parse_activity(FIXTURE.as_bytes(), &mut seen, &mut ctx);
        assert!(again.is_empty() && again_tasks.is_empty());
    }

    #[test]
    fn context_carries_across_incremental_reads() {
        let mut seen = HashSet::new();
        let mut ctx = FileContext::default();
        let lines: Vec<&str> = FIXTURE.lines().collect();
        let mid = lines.len() / 2;
        let (a, _) = parse_activity(lines[..mid].join("\n").as_bytes(), &mut seen, &mut ctx);
        let (b, _) = parse_activity(lines[mid..].join("\n").as_bytes(), &mut seen, &mut ctx);
        assert_eq!(a.len() + b.len(), 4);
        assert!(b.iter().all(|r| r.model == "gpt-6.2-sol"));
    }
}
