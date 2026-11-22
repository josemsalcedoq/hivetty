//! New sessions in their own git worktree (F4-22, A-14): `git worktree add -b <branch>` next
//! to the repository, run because the user asked for it in the New session dialog.

use std::path::{Path, PathBuf};
use std::process::Command;

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
    let output = Command::new(git)
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

/// Runs `git worktree remove <dir>` in `root` (no `--force`: git refuses when the worktree
/// has uncommitted or untracked changes). Retries for a few seconds while the closed shell
/// still holds the folder (Windows). Blocking: call off the UI thread.
pub(crate) fn remove(root: &Path, dir: &Path) -> Result<(), String> {
    let git = hv_pty::resolve_program("git");
    let mut last = String::new();
    for _ in 0..10 {
        let output = Command::new(&git)
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
