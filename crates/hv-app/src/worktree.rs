//! New sessions in their own git worktree (F4-22, A-14): `git worktree add -b <branch>` next
//! to the repository, run because the user asked for it in the New session dialog.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A helper program run without a console window: the release build on Windows has no
/// console, so each console program would otherwise flash one.
pub(crate) fn background(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

/// A branch name we are willing to pass to git: letters, digits and `/ _ . -`, no `..`, not
/// starting with `-` or `/`, at most 100 characters.
pub(crate) fn valid_branch(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && !name.starts_with(['-', '/', '.'])
        && !name.ends_with(['/', '.'])
        && !name.contains("..")
        && !name.contains("//")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '.' | '-'))
}

/// `agent/<slug>` from a prompt ("Fix the login bug" → `agent/fix-the-login-bug`), or from the
/// time when there is no prompt.
pub(crate) fn default_branch(prompt: &str) -> String {
    let slug: String = prompt
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        format!("agent/{}", jiff::Timestamp::now().strftime("%Y%m%d-%H%M%S"))
    } else {
        format!("agent/{slug}")
    }
}

/// Where the worktree for `branch` of the repository at `root` goes: a sibling directory,
/// `<repo>-<last part of the branch>`, made unique with a number if it exists.
pub(crate) fn worktree_dir(root: &Path, branch: &str) -> Option<PathBuf> {
    let parent = root.parent()?;
    let repo = root.file_name()?.to_string_lossy();
    let leaf = branch.rsplit('/').next().unwrap_or(branch);
    let base = format!("{repo}-{leaf}");
    let mut dir = parent.join(&base);
    let mut n = 2;
    while dir.exists() {
        dir = parent.join(format!("{base}-{n}"));
        n += 1;
    }
    Some(dir)
}

/// Runs `git worktree add -b <branch> <dir>` in `root`. Blocking: call off the UI thread.
pub(crate) fn create(root: &Path, branch: &str, dir: &Path) -> Result<(), String> {
    if !valid_branch(branch) {
        return Err(format!("invalid branch name '{branch}'"));
    }
    let git = hv_pty::resolve_program("git");
    let output = background(&mut Command::new(git))
        .arg("-C")
        .arg(root)
        .args(["worktree", "add", "-b", branch, "--"])
        .arg(dir)
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

/// Most bytes copied by [`copy_included`] (a `.env` is small; a stray `node_modules` is not).
const MAX_INCLUDE_BYTES: u64 = 50 << 20;

/// Copies the files listed in the repository's `.worktreeinclude` into a new worktree (F4-31):
/// files git does not track, like `.env`, that a fresh checkout lacks. One relative path per
/// line (`#` comments); a directory is copied whole. Paths that leave the repository, and
/// symbolic links, are skipped. Returns what was copied. Blocking: call off the UI thread.
pub(crate) fn copy_included(root: &Path, dir: &Path) -> Vec<PathBuf> {
    let Ok(list) = std::fs::read_to_string(root.join(".worktreeinclude")) else {
        return Vec::new();
    };
    let mut budget = MAX_INCLUDE_BYTES;
    let mut copied = Vec::new();
    for line in list.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let rel = Path::new(line.trim_start_matches("./"));
        let inside = rel
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)));
        if !inside {
            log::warn!(".worktreeinclude: skipping '{line}' (outside the repository)");
            continue;
        }
        copy_tree(&root.join(rel), &dir.join(rel), &mut budget, &mut copied);
    }
    copied
}

/// Copies a file or a directory tree, without following symbolic links, within `budget` bytes.
fn copy_tree(from: &Path, to: &Path, budget: &mut u64, copied: &mut Vec<PathBuf>) {
    let Ok(meta) = std::fs::symlink_metadata(from) else {
        return;
    };
    if meta.is_dir() {
        let Ok(entries) = std::fs::read_dir(from) else {
            return;
        };
        for entry in entries.flatten() {
            copy_tree(&entry.path(), &to.join(entry.file_name()), budget, copied);
        }
    } else if meta.is_file()
        && meta.len() <= *budget
        && !to.exists()
        && let Some(parent) = to.parent()
        && std::fs::create_dir_all(parent).is_ok()
        && std::fs::copy(from, to).is_ok()
    {
        *budget -= meta.len();
        copied.push(to.to_path_buf());
    }
}

/// Runs `git worktree remove <dir>` in `root` (no `--force`: git refuses when the worktree
/// has uncommitted or untracked changes). Retries for a few seconds while the closed shell
/// still holds the folder (Windows). Blocking: call off the UI thread.
pub(crate) fn remove(root: &Path, dir: &Path) -> Result<(), String> {
    let git = hv_pty::resolve_program("git");
    let mut last = String::new();
    for _ in 0..10 {
        let output = background(&mut Command::new(&git))
            .arg("-C")
            .arg(root)
            .args(["worktree", "remove", "--"])
            .arg(dir)
            .output()
            .map_err(|e| format!("cannot run git: {e}"))?;
        if output.status.success() {
            return Ok(());
        }
        last = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        // Changes in the worktree: retrying will not help.
        if last.contains("modified or untracked") || last.contains("is dirty") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    Err(last)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_names_are_checked() {
        assert!(valid_branch("agent/fix-the-login-bug"));
        assert!(valid_branch("feature/x_1.2"));
        assert!(!valid_branch("-rf"));
        assert!(!valid_branch("a/../b"));
        assert!(!valid_branch("a b"));
        assert!(!valid_branch("a;rm"));
        assert!(!valid_branch(""));
        assert!(!valid_branch("x/"));
    }

    #[test]
    fn default_branches_come_from_the_prompt() {
        assert_eq!(
            default_branch("Fix the login bug!"),
            "agent/fix-the-login-bug"
        );
        assert!(default_branch("").starts_with("agent/2"));
        assert!(valid_branch(&default_branch("¿Qué tal? über")));
    }

    #[test]
    fn creates_a_real_worktree() {
        let base = std::env::temp_dir().join(format!("hv-wt-{}", std::process::id()));
        let repo = base.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str]| {
            let status = Command::new(hv_pty::resolve_program("git"))
                .arg("-C")
                .arg(&repo)
                .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
                // A throwaway repo: the machine's global hooks do not apply to it.
                .arg("-c")
                .arg(format!(
                    "core.hooksPath={}",
                    base.join("no-hooks").display()
                ))
                .args(args)
                .output()
                .unwrap();
            assert!(status.status.success(), "{status:?}");
        };
        git(&["init", "-q"]);
        std::fs::write(repo.join("README"), "x").unwrap();
        git(&["add", "README"]);
        git(&["commit", "-q", "-m", "init"]);
        let dir = worktree_dir(&repo, "agent/test").unwrap();
        create(&repo, "agent/test", &dir).unwrap();
        assert!(dir.join("README").is_file());
        // Untracked files listed in .worktreeinclude follow; paths outside do not.
        std::fs::write(repo.join(".env"), "KEY=1").unwrap();
        std::fs::create_dir_all(repo.join("conf")).unwrap();
        std::fs::write(repo.join("conf/local.toml"), "x").unwrap();
        std::fs::write(
            repo.join(".worktreeinclude"),
            "# local files\n.env\nconf\n../secret\n",
        )
        .unwrap();
        let copied = copy_included(&repo, &dir);
        assert_eq!(copied.len(), 2, "{copied:?}");
        assert_eq!(std::fs::read_to_string(dir.join(".env")).unwrap(), "KEY=1");
        assert!(dir.join("conf/local.toml").is_file());
        std::fs::remove_file(dir.join(".env")).unwrap();
        std::fs::remove_dir_all(dir.join("conf")).unwrap();
        assert!(
            create(&repo, "agent/test", &base.join("other")).is_err(),
            "branch exists"
        );
        assert!(create(&repo, "--bad", &base.join("x")).is_err());
        // A worktree with changes is kept; a clean one is removed.
        std::fs::write(dir.join("new.txt"), "x").unwrap();
        assert!(remove(&repo, &dir).is_err());
        assert!(dir.exists());
        std::fs::remove_file(dir.join("new.txt")).unwrap();
        remove(&repo, &dir).unwrap();
        assert!(!dir.exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn worktree_dirs_are_siblings_and_unique() {
        let root = std::env::temp_dir().join(format!("hv-wt-repo-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let first = worktree_dir(&root, "agent/fix").unwrap();
        assert_eq!(first.parent(), root.parent());
        assert!(
            first
                .file_name()
                .unwrap()
                .to_string_lossy()
                .ends_with("-fix")
        );
        std::fs::create_dir_all(&first).unwrap();
        let second = worktree_dir(&root, "agent/fix").unwrap();
        assert!(second.to_string_lossy().ends_with("-fix-2"));
        std::fs::remove_dir_all(&first).unwrap();
        std::fs::remove_dir_all(&root).unwrap();
    }
}
