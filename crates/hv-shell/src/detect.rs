//! What the prompt chips show (F3-09, F3-11, F3-12; docs/requirements.md §8): the project's
//! technology, the git branch and tool versions. Files are only **read** (at most 1 MB each);
//! no project tool is ever run (docs/security.md 2.5).

use std::path::{Path, PathBuf};

/// Largest file read for detection.
const MAX_FILE: u64 = 1 << 20;
/// Most parent directories searched for a repository root.
const MAX_DEPTH: usize = 20;

/// A project technology, in the order rules are tried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tech {
    Rust,
    Python,
    Bun,
    TypeScript,
    Node,
    JavaScript,
    Go,
    Flutter,
    Dart,
    Deno,
    Java,
    Kotlin,
    DotNet,
    Ruby,
    Php,
    Elixir,
    Swift,
    Zig,
    Cpp,
}

impl Tech {
    /// Display name (also the chip label).
    pub fn name(self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Python => "Python",
            Self::Bun => "Bun",
            Self::TypeScript => "TypeScript",
            Self::Node => "Node",
            Self::JavaScript => "JavaScript",
            Self::Go => "Go",
            Self::Flutter => "Flutter",
            Self::Dart => "Dart",
            Self::Deno => "Deno",
            Self::Java => "Java",
            Self::Kotlin => "Kotlin",
            Self::DotNet => ".NET",
            Self::Ruby => "Ruby",
            Self::Php => "PHP",
            Self::Elixir => "Elixir",
            Self::Swift => "Swift",
            Self::Zig => "Zig",
            Self::Cpp => "C/C++",
        }
    }
}

/// Everything the chips need for one directory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectInfo {
    /// The winning technology, if any marker matched.
    pub tech: Option<Tech>,
    /// Current git branch, or a short commit id when detached.
    pub branch: Option<String>,
    /// Tool versions pinned in files: `("node", "20.11")`, `("rust", "1.100.0")`…
    pub versions: Vec<(String, String)>,
    /// Root of the enclosing git repository (names the project in the sidebar).
    pub root: Option<PathBuf>,
}

/// Inspects `dir`: technology from `dir` or, if nothing matches there, from the repository
/// root (never above `home`); branch from the enclosing repository; versions from the same
/// directory the technology came from.
pub fn inspect(dir: &Path, home: Option<&Path>) -> ProjectInfo {
    let repo = find_repo(dir, home);
    let (tech, from) = match detect_tech(dir) {
        Some(tech) => (Some(tech), dir.to_path_buf()),
        None => match repo.as_ref().map(|r| r.root.clone()) {
            Some(root) => (detect_tech(&root), root),
            None => (None, dir.to_path_buf()),
        },
    };
    ProjectInfo {
        tech,
        branch: repo.as_ref().and_then(|r| branch(&r.git_dir)),
        versions: versions(&from),
        root: repo.map(|r| r.root),
    }
}

fn read_small(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

fn exists(dir: &Path, name: &str) -> bool {
    dir.join(name).is_file()
}

fn any_with_extension(dir: &Path, ext: &str) -> bool {
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().take(500).any(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case(ext))
        })
    })
}

/// The first rule of docs/requirements.md §8 that matches in `dir`.
pub fn detect_tech(dir: &Path) -> Option<Tech> {
    let has = |name: &str| exists(dir, name);
    if has("Cargo.toml") {
        return Some(Tech::Rust);
    }
    if [
        "pyproject.toml",
        "requirements.txt",
        "setup.py",
        "Pipfile",
        "uv.lock",
    ]
    .iter()
    .any(|n| has(n))
    {
        return Some(Tech::Python);
    }
    if ["bun.lock", "bun.lockb", "bunfig.toml"]
        .iter()
        .any(|n| has(n))
    {
        return Some(Tech::Bun);
    }
    let package = read_small(&dir.join("package.json"));
    let typescript_dep = package.as_deref().is_some_and(|text| {
        serde_json::from_str::<serde_json::Value>(text).is_ok_and(|json| {
            ["dependencies", "devDependencies"]
                .iter()
                .any(|key| json.get(key).and_then(|d| d.get("typescript")).is_some())
        })
    });
    if has("tsconfig.json") || typescript_dep {
        return Some(Tech::TypeScript);
    }
    if [
        "package-lock.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        ".nvmrc",
        ".node-version",
    ]
    .iter()
    .any(|n| has(n))
    {
        return Some(Tech::Node);
    }
    if package.is_some() {
        return Some(Tech::JavaScript);
    }
    if has("go.mod") {
        return Some(Tech::Go);
    }
    if let Some(pubspec) = read_small(&dir.join("pubspec.yaml")) {
        let flutter = pubspec
            .lines()
            .any(|l| l.trim_start().starts_with("flutter:"));
        return Some(if flutter { Tech::Flutter } else { Tech::Dart });
    }
    if has("deno.json") || has("deno.jsonc") {
        return Some(Tech::Deno);
    }
    if has("build.gradle.kts") {
        return Some(Tech::Kotlin);
    }
    if has("pom.xml") || has("build.gradle") {
        return Some(Tech::Java);
    }
    if any_with_extension(dir, "csproj") || any_with_extension(dir, "sln") {
        return Some(Tech::DotNet);
    }
    let rest = [
        ("Gemfile", Tech::Ruby),
        ("composer.json", Tech::Php),
        ("mix.exs", Tech::Elixir),
        ("Package.swift", Tech::Swift),
        ("build.zig", Tech::Zig),
        ("CMakeLists.txt", Tech::Cpp),
    ];
    rest.iter().find(|(n, _)| has(n)).map(|(_, t)| *t)
}

struct Repo {
    root: PathBuf,
    git_dir: PathBuf,
}

/// The repository containing `dir` (a `.git` directory, or a `.git` file of a worktree).
fn find_repo(dir: &Path, home: Option<&Path>) -> Option<Repo> {
    let mut current = Some(dir);
    for _ in 0..MAX_DEPTH {
        let here = current?;
        let dot_git = here.join(".git");
        if dot_git.is_dir() {
            return Some(Repo {
                root: here.to_path_buf(),
                git_dir: dot_git,
            });
        }
        if let Some(text) = read_small(&dot_git)
            && let Some(target) = text.trim().strip_prefix("gitdir:")
        {
            let target = Path::new(target.trim());
            let git_dir = if target.is_absolute() {
                target.to_path_buf()
            } else {
                here.join(target)
            };
            return Some(Repo {
                root: here.to_path_buf(),
                git_dir,
            });
        }
        if home.is_some_and(|h| h == here) {
            return None; // never look above the home directory
        }
        current = here.parent();
    }
    None
}

/// The checked-out branch from `HEAD`, or the first 7 characters of a detached commit.
fn branch(git_dir: &Path) -> Option<String> {
    let head = read_small(&git_dir.join("HEAD"))?;
    let head = head.trim();
    if let Some(reference) = head.strip_prefix("ref:") {
        let name = reference.trim();
        return Some(name.strip_prefix("refs/heads/").unwrap_or(name).to_owned());
    }
    let is_hash = head.len() >= 7 && head.chars().all(|c| c.is_ascii_hexdigit());
    is_hash.then(|| head[..7].to_owned())
}

/// Versions pinned in files in `dir` (never by running the tools).
pub fn versions(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let first_line = |name: &str| {
        read_small(&dir.join(name)).and_then(|t| {
            t.lines()
                .map(str::trim)
                .find(|l| !l.is_empty() && !l.starts_with('#'))
                .map(|l| l.trim_start_matches('v').to_owned())
        })
    };
    // Rust: rust-toolchain.toml `channel = "…"`, or the legacy one-line rust-toolchain.
    if let Some(toml) = read_small(&dir.join("rust-toolchain.toml"))
        && let Some(channel) = toml.lines().find_map(|l| {
            let (key, value) = l.split_once('=')?;
            (key.trim() == "channel").then(|| value.trim().trim_matches('"').to_owned())
        })
    {
        out.push(("rust".to_owned(), channel));
    } else if let Some(channel) = first_line("rust-toolchain") {
        out.push(("rust".to_owned(), channel));
    }
    if let Some(node) = first_line(".nvmrc").or_else(|| first_line(".node-version")) {
        out.push(("node".to_owned(), node));
    } else if let Some(text) = read_small(&dir.join("package.json"))
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(&text)
        && let Some(engine) = json
            .get("engines")
            .and_then(|e| e.get("node"))
            .and_then(|v| v.as_str())
    {
        out.push(("node".to_owned(), engine.to_owned()));
    }
    if let Some(python) = first_line(".python-version") {
        out.push(("python".to_owned(), python));
    }
    if let Some(gomod) = read_small(&dir.join("go.mod"))
        && let Some(version) = gomod.lines().find_map(|l| l.trim().strip_prefix("go "))
    {
        out.push(("go".to_owned(), version.trim().to_owned()));
    }
    // Keep chips short: drop absurdly long values from hand-edited files.
    out.retain(|(_, v)| !v.is_empty() && v.chars().count() <= 24);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "hv-detect-{name}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn file(&self, name: &str, text: &str) -> &Self {
            let path = self.0.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
            self
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn first_matching_rule_wins() {
        let d = TempDir::new("order");
        d.file("package.json", "{}").file("Cargo.toml", "");
        assert_eq!(detect_tech(&d.0), Some(Tech::Rust));
        let d = TempDir::new("ts");
        d.file("package.json", r#"{"devDependencies":{"typescript":"^5"}}"#);
        assert_eq!(detect_tech(&d.0), Some(Tech::TypeScript));
        let d = TempDir::new("js");
        d.file("package.json", "{}");
        assert_eq!(detect_tech(&d.0), Some(Tech::JavaScript));
        let d = TempDir::new("flutter");
        d.file(
            "pubspec.yaml",
            "name: app\nflutter:\n  uses-material-design: true\n",
        );
        assert_eq!(detect_tech(&d.0), Some(Tech::Flutter));
        let d = TempDir::new("none");
        assert_eq!(detect_tech(&d.0), None);
    }

    #[test]
    fn repo_root_fills_in_for_subdirectories_and_gives_the_branch() {
        let d = TempDir::new("repo");
        d.file("Cargo.toml", "")
            .file(".git/HEAD", "ref: refs/heads/feature/chips\n")
            .file(
                "rust-toolchain.toml",
                "[toolchain]\nchannel = \"1.100.0\"\n",
            )
            .file("src/lib.rs", "");
        let info = inspect(&d.0.join("src"), None);
        assert_eq!(info.tech, Some(Tech::Rust));
        assert_eq!(info.branch.as_deref(), Some("feature/chips"));
        assert_eq!(info.versions, vec![("rust".into(), "1.100.0".into())]);
    }

    #[test]
    fn detached_heads_and_worktrees() {
        let d = TempDir::new("detached");
        d.file(".git/HEAD", "0123456789abcdef0123456789abcdef01234567\n");
        assert_eq!(inspect(&d.0, None).branch.as_deref(), Some("0123456"));
        let w = TempDir::new("worktree");
        let git = w.0.join("main-git");
        w.file("main-git/HEAD", "ref: refs/heads/wt\n")
            .file("tree/.git", &format!("gitdir: {}\n", git.display()));
        assert_eq!(
            inspect(&w.0.join("tree"), None).branch.as_deref(),
            Some("wt")
        );
    }

    #[test]
    fn search_stops_at_home() {
        let d = TempDir::new("home");
        d.file(".git/HEAD", "ref: refs/heads/main\n")
            .file("me/x", "");
        let home = d.0.join("me");
        assert_eq!(inspect(&home, Some(&home)).branch, None);
    }

    #[test]
    fn versions_come_from_files() {
        let d = TempDir::new("versions");
        d.file(".nvmrc", "v20.11.1\n")
            .file(".python-version", "3.12\n")
            .file("go.mod", "module x\n\ngo 1.23\n");
        assert_eq!(
            versions(&d.0),
            vec![
                ("node".into(), "20.11.1".into()),
                ("python".into(), "3.12".into()),
                ("go".into(), "1.23".into()),
            ]
        );
    }
}
