//! Incremental ingest (F5-01, F5-04; docs/adr/0006): a cache of everything parsed so far and,
//! per log file, how far it was read. Agents only append to their logs, so each refresh reads
//! just the new complete lines. Serialized with `postcard` to one file in the data directory.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Task, UsageRecord};

/// Bump when the stored format or what counts as a task changes; older caches are discarded
/// and rebuilt. 2: subagent prompts and interruptions are no longer tasks.
const VERSION: u32 = 2;
/// A cache larger than this is not ours (or is corrupt): rebuild instead of reading it.
const MAX_CACHE: u64 = 512 << 20;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Cursor {
    /// Bytes consumed (always at a line boundary).
    offset: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredRecord {
    agent: String,
    provider: String,
    model: String,
    timestamp: String,
    session_id: Option<String>,
    project: Option<String>,
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredTask {
    agent: String,
    timestamp: String,
    session_id: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Stored {
    version: u32,
    files: HashMap<PathBuf, Cursor>,
    seen: HashSet<String>,
    records: Vec<StoredRecord>,
    tasks: Vec<StoredTask>,
}

/// Parsed usage plus where reading stopped in each log.
#[derive(Debug, Default)]
pub struct Store {
    inner: Stored,
}

/// `&'static str` ids for the agents and providers we know; anything else is "other".
fn intern(s: &str) -> &'static str {
    match s {
        "claude-code" => "claude-code",
        "codex" => "codex",
        "gemini-cli" => "gemini-cli",
        "opencode" => "opencode",
        "anthropic" => "anthropic",
        "openai" => "openai",
        "google" => "google",
        "xai" => "xai",
        _ => "other",
    }
}

impl Store {
    /// Loads the cache, or starts empty if it is missing, too large, unreadable or old.
    pub fn load(path: &Path) -> Self {
        let Ok(meta) = std::fs::metadata(path) else {
            return Self::default();
        };
        if meta.len() > MAX_CACHE {
            return Self::default();
        }
        let Ok(bytes) = std::fs::read(path) else {
            return Self::default();
        };
        match postcard::from_bytes::<Stored>(&bytes) {
            Ok(inner) if inner.version == VERSION => Self { inner },
            _ => Self::default(),
        }
    }

    /// Writes the cache atomically.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let bytes = postcard::to_stdvec(&StoredRef {
            version: VERSION,
            files: &self.inner.files,
            seen: &self.inner.seen,
            records: &self.inner.records,
            tasks: &self.inner.tasks,
        })
        .map_err(std::io::Error::other)?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, path)
    }

    /// Reads what is new in Claude Code's transcripts under `root`. Returns how many records
    /// and tasks were added.
    pub fn ingest_claude(&mut self, root: &Path) -> (usize, usize) {
        let (mut added_records, mut added_tasks) = (0, 0);
        for path in crate::claude::transcript_files(root) {
            let Ok(meta) = std::fs::metadata(&path) else {
                continue;
            };
            let cursor = self.inner.files.entry(path.clone()).or_default();
            // A file that shrank was rewritten: read it again (records and tasks seen before
            // are skipped by id).
            if meta.len() < cursor.offset {
                cursor.offset = 0;
            }
            if meta.len() == cursor.offset {
                continue;
            }
            let Some((new_lines, consumed)) = read_complete_lines(&path, cursor.offset) else {
                continue;
            };
            cursor.offset += consumed;
            let (records, tasks) =
                crate::claude::parse_activity(new_lines.as_slice(), &mut self.inner.seen);
            added_records += records.len();
            added_tasks += tasks.len();
            self.inner
                .records
                .extend(records.into_iter().map(|r| StoredRecord {
                    agent: r.agent.to_owned(),
                    provider: r.provider.to_owned(),
                    model: r.model,
                    timestamp: r.timestamp,
                    session_id: r.session_id,
                    project: r.project,
                    input: r.input_tokens,
                    output: r.output_tokens,
                    cache_read: r.cache_read_tokens,
                    cache_write: r.cache_write_tokens,
                }));
            self.inner
                .tasks
                .extend(tasks.into_iter().map(|t| StoredTask {
                    agent: t.agent.to_owned(),
                    timestamp: t.timestamp,
                    session_id: t.session_id,
                }));
        }
        (added_records, added_tasks)
    }

    /// Everything ingested so far.
    pub fn records(&self) -> Vec<UsageRecord> {
        self.inner
            .records
            .iter()
            .map(|r| UsageRecord {
                agent: intern(&r.agent),
                provider: intern(&r.provider),
                model: r.model.clone(),
                timestamp: r.timestamp.clone(),
                session_id: r.session_id.clone(),
                project: r.project.clone(),
                input_tokens: r.input,
                output_tokens: r.output,
                cache_read_tokens: r.cache_read,
                cache_write_tokens: r.cache_write,
            })
            .collect()
    }

    /// Every task ingested so far.
    pub fn tasks(&self) -> Vec<Task> {
        self.inner
            .tasks
            .iter()
            .map(|t| Task {
                agent: intern(&t.agent),
                timestamp: t.timestamp.clone(),
                session_id: t.session_id.clone(),
            })
            .collect()
    }
}

/// Serializes without cloning the store.
#[derive(Serialize)]
struct StoredRef<'a> {
    version: u32,
    files: &'a HashMap<PathBuf, Cursor>,
    seen: &'a HashSet<String>,
    records: &'a Vec<StoredRecord>,
    tasks: &'a Vec<StoredTask>,
}

/// The bytes after `offset` up to the last newline, and how many bytes that is. A line still
/// being written stays for the next read.
fn read_complete_lines(path: &PathBuf, offset: u64) -> Option<(Vec<u8>, u64)> {
    let mut file = BufReader::new(File::open(path).ok()?);
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let end = bytes.iter().rposition(|b| *b == b'\n')? + 1;
    bytes.truncate(end);
    Some((bytes, end as u64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const LINE_A: &str = r#"{"type":"assistant","sessionId":"s","timestamp":"2026-11-20T10:00:05Z","requestId":"r1","message":{"id":"m1","model":"claude-opus-5-5","usage":{"input_tokens":1,"output_tokens":2}}}"#;
    const LINE_B: &str = r#"{"type":"assistant","sessionId":"s","timestamp":"2026-11-20T10:01:05Z","requestId":"r2","message":{"id":"m2","model":"claude-opus-5-5","usage":{"input_tokens":3,"output_tokens":4}}}"#;
    const PROMPT: &str = r#"{"type":"user","sessionId":"s","timestamp":"2026-11-20T10:00:00Z","message":{"role":"user","content":"fix it"}}"#;

    #[test]
    fn only_new_complete_lines_are_read_and_the_cache_round_trips() {
        let dir = std::env::temp_dir().join(format!("hv-store-{}", std::process::id()));
        let project = dir.join("projects").join("p");
        std::fs::create_dir_all(&project).unwrap();
        let log = project.join("s.jsonl");
        std::fs::write(&log, format!("{PROMPT}\n{LINE_A}\n")).unwrap();

        let mut store = Store::default();
        assert_eq!(store.ingest_claude(&dir.join("projects")), (1, 1));
        assert_eq!(
            store.ingest_claude(&dir.join("projects")),
            (0, 0),
            "nothing new"
        );

        // A half-written line is left for later.
        let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
        write!(f, "{}", &LINE_B[..40]).unwrap();
        assert_eq!(store.ingest_claude(&dir.join("projects")), (0, 0));
        writeln!(f, "{}", &LINE_B[40..]).unwrap();
        assert_eq!(store.ingest_claude(&dir.join("projects")), (1, 0));

        let cache = dir.join("usage.cache");
        store.save(&cache).unwrap();
        let loaded = Store::load(&cache);
        assert_eq!(loaded.records().len(), 2);
        assert_eq!(loaded.tasks().len(), 1);
        assert_eq!(loaded.records()[1].output_tokens, 4);
        // The loaded cursor continues where it stopped.
        let mut loaded = loaded;
        assert_eq!(loaded.ingest_claude(&dir.join("projects")), (0, 0));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn garbage_caches_are_ignored() {
        let path = std::env::temp_dir().join(format!("hv-store-bad-{}", std::process::id()));
        std::fs::write(&path, b"not a cache").unwrap();
        assert!(Store::load(&path).records().is_empty());
        std::fs::remove_file(&path).unwrap();
    }
}
