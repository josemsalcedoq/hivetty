//! Agent profiles (F4-01, docs/agent-integration.md §1): one TOML per agent in
//! `assets/agents/`, embedded in the binary. Detection of installed agents (F4-02).

use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The built-in profiles, by file name.
const BUILTIN: &[(&str, &str)] = &[
    (
        "claude-code",
        include_str!("../../../assets/agents/claude-code.toml"),
    ),
    ("codex", include_str!("../../../assets/agents/codex.toml")),
    (
        "gemini-cli",
        include_str!("../../../assets/agents/gemini-cli.toml"),
    ),
    (
        "opencode",
        include_str!("../../../assets/agents/opencode.toml"),
    ),
    (
        "grok-cli",
        include_str!("../../../assets/agents/grok-cli.toml"),
    ),
    ("junie", include_str!("../../../assets/agents/junie.toml")),
    (
        "antigravity",
        include_str!("../../../assets/agents/antigravity.toml"),
    ),
    (
        "copilot-cli",
        include_str!("../../../assets/agents/copilot-cli.toml"),
    ),
    (
        "cursor-cli",
        include_str!("../../../assets/agents/cursor-cli.toml"),
    ),
    ("amp", include_str!("../../../assets/agents/amp.toml")),
    ("aider", include_str!("../../../assets/agents/aider.toml")),
];

/// One agent profile.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Stable id (`claude-code`), used by `hivetty hook --agent` and in the config.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Who makes it (`anthropic`, `openai`…).
    pub provider: String,
    /// Command that starts it.
    pub command: String,
    /// Executable names (without `.exe`) that identify a running instance.
    #[serde(default)]
    pub process_names: Vec<String>,
    /// When it runs under `node`, words in the arguments that identify it.
    #[serde(default)]
    pub node_args: Vec<String>,
    /// When it runs under `python`, words in the arguments that identify it.
    #[serde(default)]
    pub python_args: Vec<String>,
    /// Whether the command and process names were checked against a real install.
    #[serde(default)]
    pub verified: bool,
    /// The command takes a first prompt as its last argument (`claude "fix the tests"`).
    #[serde(default)]
    pub prompt_arg: bool,
    /// How its live state is obtained.
    #[serde(default)]
    pub state: StateSources,
    /// How to reopen a session after a restart.
    #[serde(default)]
    pub resume: Option<Resume>,
    /// Where its token usage lives.
    #[serde(default)]
    pub usage: Option<Usage>,
}

/// `[state]` of a profile.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateSources {
    /// Emits OSC 7501.
    #[serde(default)]
    pub osc7501: bool,
    /// Hook installer kind (`claude-settings`), if supported.
    #[serde(default)]
    pub hooks: Option<String>,
    /// Its window title (OSC 2) summarizes the current task.
    #[serde(default)]
    pub title_from_osc: bool,
}

/// `[resume]` of a profile.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resume {
    /// Command with `{session_id}`.
    pub command: String,
}

/// `[usage]` of a profile.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Usage {
    /// Parser to use (`claude-jsonl`, `codex-jsonl`).
    pub format: String,
    /// Glob patterns of the log files (`~` = home).
    pub paths: Vec<String>,
}

impl Profile {
    /// Whether a process with this executable name and arguments is this agent.
    pub fn matches(&self, exe: &str, args: &[String]) -> bool {
        let exe = exe_stem(exe);
        if self
            .process_names
            .iter()
            .any(|n| n.eq_ignore_ascii_case(&exe))
        {
            return true;
        }
        let mentions = |words: &[String]| {
            args.iter().any(|a| {
                let a = a.to_ascii_lowercase();
                words.iter().any(|w| a.contains(&w.to_ascii_lowercase()))
            })
        };
        match exe.as_str() {
            "node" | "bun" => mentions(&self.node_args),
            "python" | "python3" | "py" => mentions(&self.python_args),
            _ => false,
        }
    }
}

/// `C:\x\claude.exe` → `claude`, lowercase.
fn exe_stem(exe: &str) -> String {
    let file = exe.rsplit(['/', '\\']).next().unwrap_or(exe);
    let file = file.to_ascii_lowercase();
    file.strip_suffix(".exe").unwrap_or(&file).to_owned()
}

/// The built-in profiles.
pub fn builtin() -> Vec<Profile> {
    BUILTIN
        .iter()
        .map(|(name, text)| {
            toml::from_str(text).unwrap_or_else(|e| panic!("assets/agents/{name}.toml: {e}"))
        })
        .collect()
}

/// The profile for an id (also accepts `claude` for Claude Code, as hooks may be installed
/// with the short name).
pub fn find<'a>(profiles: &'a [Profile], id: &str) -> Option<&'a Profile> {
    let id = if id == "claude" { "claude-code" } else { id };
    profiles.iter().find(|p| p.id == id)
}

/// Profiles whose command is on `PATH` (F4-02), with where it was found.
pub fn installed<'a>(
    profiles: &'a [Profile],
    path: Option<&std::ffi::OsStr>,
) -> Vec<(&'a Profile, PathBuf)> {
    let Some(path) = path else {
        return Vec::new();
    };
    let dirs: Vec<PathBuf> = std::env::split_paths(path).collect();
    let extensions: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_owned())
            .split(';')
            .filter(|e| !e.is_empty())
            .map(str::to_ascii_lowercase)
            .collect()
    } else {
        vec![String::new()]
    };
    profiles
        .iter()
        .filter_map(|p| find_command(&p.command, &dirs, &extensions).map(|found| (p, found)))
        .collect()
}

fn find_command(command: &str, dirs: &[PathBuf], extensions: &[String]) -> Option<PathBuf> {
    dirs.iter().find_map(|dir| {
        extensions.iter().find_map(|ext| {
            let candidate = dir.join(format!("{command}{ext}"));
            is_executable(&candidate).then_some(candidate)
        })
    })
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_eleven_builtin_profiles_parse_with_unique_ids() {
        let profiles = builtin();
        assert_eq!(profiles.len(), 11);
        let mut ids: Vec<&str> = profiles.iter().map(|p| p.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 11);
        let claude = find(&profiles, "claude").unwrap();
        assert!(claude.state.osc7501);
        assert_eq!(
            claude.resume.as_ref().unwrap().command,
            "claude --resume {session_id}"
        );
    }

    #[test]
    fn processes_match_by_name_or_by_runtime_arguments() {
        let profiles = builtin();
        let claude = find(&profiles, "claude-code").unwrap();
        assert!(claude.matches(r"C:\Users\me\.local\bin\claude.exe", &[]));
        let args = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
        assert!(claude.matches(
            "/usr/bin/node",
            &args("node /usr/lib/node_modules/@anthropic-ai/claude-code/cli.js")
        ));
        assert!(!claude.matches("node", &args("node server.js")));
        let aider = find(&profiles, "aider").unwrap();
        assert!(aider.matches("python3", &args("python3 -m aider")));
    }

    #[test]
    fn installed_agents_are_found_on_path() {
        let dir = std::env::temp_dir().join(format!("hv-agents-path-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let name = if cfg!(windows) { "codex.cmd" } else { "codex" };
        let file = dir.join(name);
        std::fs::write(&file, "").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let profiles = builtin();
        let path = std::env::join_paths([&dir]).unwrap();
        let found = installed(&profiles, Some(&path));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].0.id, "codex");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
