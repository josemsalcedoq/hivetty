//! Whether a git worktree has uncommitted changes to tracked files (F3-11), by reading only
//! files (`docs/security.md` 2.5): the index (`.git/index`, versions 2 and 3) against the
//! size and modification time of each file, git's own quick check. Nothing is run, and no
//! file content is read, so a repository's filters and hooks never come into play.
//!
//! Limits: untracked files are not seen, a change that keeps size and time is missed, and an
//! index that is too large, of another version or with SHA-256 ids gives no answer.

use std::path::{Component, Path};
use std::time::{Duration, Instant, UNIX_EPOCH};

/// Largest index read.
const MAX_INDEX: u64 = 32 << 20;
/// Most entries checked.
const MAX_ENTRIES: usize = 20_000;
/// Time budget for the stat calls.
const BUDGET: Duration = Duration::from_millis(200);

/// One tracked file as the index saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    path: String,
    mtime_s: u32,
    mtime_ns: u32,
    size: u32,
    mode: u32,
    /// assume-unchanged, skip-worktree or intent-to-add: not compared.
    skip: bool,
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

/// Parses the entries of an index file (version 2 or 3, SHA-1 ids).
fn parse(bytes: &[u8]) -> Option<Vec<Entry>> {
    if bytes.get(..4)? != b"DIRC" {
        return None;
    }
    let version = be32(bytes, 4)?;
    if !(2..=3).contains(&version) {
        return None;
    }
    let count = usize::try_from(be32(bytes, 8)?).ok()?;
    if count > MAX_ENTRIES {
        return None;
    }
    let mut entries = Vec::with_capacity(count);
    let mut at = 12;
    for _ in 0..count {
        let start = at;
        let mtime_s = be32(bytes, at + 8)?;
        let mtime_ns = be32(bytes, at + 12)?;
        let mode = be32(bytes, at + 24)?;
        let size = be32(bytes, at + 36)?;
        let flags = be16(bytes, at + 60)?;
        at += 62;
        let mut skip = flags & 0x8000 != 0; // assume-unchanged
        if flags & 0x4000 != 0 {
            // Extended flags (version 3): skip-worktree and intent-to-add.
            let extended = be16(bytes, at)?;
            skip |= extended & 0x6000 != 0;
            at += 2;
        }
        let end = at + bytes.get(at..)?.iter().position(|&b| b == 0)?;
        let path = std::str::from_utf8(bytes.get(at..end)?).ok()?.to_owned();
        // Entries are NUL-padded to a multiple of 8 bytes.
        let length = end + 1 - start;
        at = start + length.div_ceil(8) * 8;
        entries.push(Entry {
            path,
            mtime_s,
            mtime_ns,
            size,
            mode,
            skip,
        });
    }
    Some(entries)
}

/// A path inside the worktree: relative, without `..`.
fn inside(path: &str) -> bool {
    Path::new(path)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
}

/// Whether the file at `full` differs from what the index recorded.
fn changed(entry: &Entry, full: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(full) else {
        return true; // deleted
    };
    // The index keeps the size modulo 2^32.
    if (meta.len() & 0xFFFF_FFFF) != u64::from(entry.size) {
        return true;
    }
    let Some(modified) = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
    else {
        return false;
    };
    if modified.as_secs() & 0xFFFF_FFFF != u64::from(entry.mtime_s) {
        return true;
    }
    // Nanoseconds only when git recorded them.
    entry.mtime_ns != 0 && modified.subsec_nanos() != entry.mtime_ns
}

/// `Some(true)` when some tracked file changed, `Some(false)` when none did, `None` when there
/// is no answer (no index, unsupported, too large or too slow).
pub fn is_dirty(root: &Path, git_dir: &Path) -> Option<bool> {
    let index = git_dir.join("index");
    if std::fs::metadata(&index).ok()?.len() > MAX_INDEX {
        return None;
    }
    let entries = parse(&std::fs::read(&index).ok()?)?;
    let started = Instant::now();
    for entry in &entries {
        // Submodules (gitlinks) and the excluded ones are not compared.
        if entry.skip || entry.mode == 0o160_000 || !inside(&entry.path) {
            continue;
        }
        if changed(entry, &root.join(&entry.path)) {
            return Some(true);
        }
        if started.elapsed() > BUDGET {
            return None;
        }
    }
    Some(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A git repository with one committed file, made with the real git (CI runners have it).
    fn repo(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("hv-gitidx-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
                .arg("-c")
                .arg(format!("core.hooksPath={}", dir.join("no-hooks").display()))
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success(), "{out:?}");
        };
        git(&["init", "-q"]);
        std::fs::write(dir.join("a.txt"), "one").unwrap();
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/b.rs"), "fn main() {}").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "init"]);
        dir
    }

    #[test]
    fn a_clean_tree_then_an_edit_then_a_deletion() {
        let dir = repo("edit");
        let git_dir = dir.join(".git");
        assert_eq!(is_dirty(&dir, &git_dir), Some(false));
        std::fs::write(dir.join("src/b.rs"), "fn main() { println!(); }").unwrap();
        assert_eq!(is_dirty(&dir, &git_dir), Some(true));
        std::fs::write(dir.join("src/b.rs"), "fn main() {}").unwrap();
        std::fs::remove_file(dir.join("a.txt")).unwrap();
        assert_eq!(is_dirty(&dir, &git_dir), Some(true));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn garbage_and_hostile_paths() {
        assert_eq!(parse(b"nope"), None);
        assert_eq!(
            parse(b"DIRC\0\0\0\x04\0\0\0\0"),
            None,
            "version 4 is not read"
        );
        assert!(!inside("../etc/passwd"));
        assert!(!inside("/etc/passwd"));
        assert!(inside("src/main.rs"));
        let dir = std::env::temp_dir().join(format!("hv-gitidx-none-{}", std::process::id()));
        assert_eq!(is_dirty(&dir, &dir.join(".git")), None);
    }
}
