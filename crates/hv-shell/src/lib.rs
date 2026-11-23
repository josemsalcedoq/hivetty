//! Shell integration (F3-01…F3-04): scripts that make zsh, bash, fish and PowerShell report
//! prompt marks (OSC 133) and the current directory (OSC 7), injected without touching the
//! user's dotfiles. Later: project technology detection and prompt chips.
//!
//! The scripts are embedded in the binary and written to the data directory on first use
//! ([`install`]); [`integrate`] turns a shell command line into one that loads them.

pub mod detect;
pub mod git_index;
mod service;

pub use service::ChipService;

use std::io;
use std::path::{Path, PathBuf};

/// The scripts, by path relative to the install directory.
const FILES: &[(&str, &str)] = &[
    (
        "hivetty.zsh",
        include_str!("../../../assets/shell/hivetty.zsh"),
    ),
    (
        "zsh/.zshenv",
        include_str!("../../../assets/shell/zsh/.zshenv"),
    ),
    (
        "hivetty.bash",
        include_str!("../../../assets/shell/hivetty.bash"),
    ),
    (
        "fish/vendor_conf.d/hivetty.fish",
        include_str!("../../../assets/shell/fish/vendor_conf.d/hivetty.fish"),
    ),
    (
        "hivetty.ps1",
        include_str!("../../../assets/shell/hivetty.ps1"),
    ),
];

/// Writes the scripts under `dir` (a versioned subdirectory of the data directory). Files that
/// already have the right contents are left alone. Returns the directory to pass to
/// [`integrate`].
pub fn install(dir: &Path) -> io::Result<PathBuf> {
    let root = dir.join(env!("CARGO_PKG_VERSION"));
    for (relative, contents) in FILES {
        let path = root.join(relative);
        if std::fs::read_to_string(&path).is_ok_and(|current| current == *contents) {
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Atomic replace, so a shell starting at the same time never reads half a script.
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, contents)?;
        std::fs::rename(&tmp, &path)?;
    }
    Ok(root)
}

/// The shell families with an integration script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Zsh,
    Bash,
    Fish,
    /// Windows PowerShell 5.1 or PowerShell 7 (`pwsh`).
    PowerShell,
}

impl Shell {
    /// Recognizes a shell from its program path or name (`/bin/zsh`, `pwsh.exe`…).
    pub fn detect(program: &str) -> Option<Self> {
        let file = program.rsplit(['/', '\\']).next().unwrap_or(program);
        let name = file
            .strip_suffix(".exe")
            .or_else(|| file.strip_suffix(".EXE"))
            .unwrap_or(file)
            .to_ascii_lowercase();
        match name.as_str() {
            "zsh" => Some(Self::Zsh),
            "bash" => Some(Self::Bash),
            "fish" => Some(Self::Fish),
            "pwsh" | "powershell" => Some(Self::PowerShell),
            _ => None,
        }
    }
}

/// How to start a shell so it loads the integration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Integration {
    /// Arguments to use instead of the original ones.
    pub args: Vec<String>,
    /// Environment variables to add.
    pub env: Vec<(String, String)>,
}

/// The command line that loads the integration for `program args`, or `None` when the shell
/// is not supported or the arguments ask for something we must not change (a script, a
/// command, a non-interactive shell).
///
/// `env` reads the current environment (for the user's `ZDOTDIR` and `XDG_DATA_DIRS`).
pub fn integrate(
    program: &str,
    args: &[String],
    scripts: &Path,
    env: impl Fn(&str) -> Option<String>,
) -> Option<Integration> {
    let shell = Shell::detect(program)?;
    let dir = scripts.to_string_lossy().into_owned();
    let mut extra = vec![("HIVETTY_SHELL_DIR".to_owned(), dir.clone())];
    match shell {
        Shell::Zsh => {
            // Only plain interactive starts: `zsh -c …` or `zsh script` are left alone.
            if args.iter().any(|a| a == "-c" || !a.starts_with('-')) {
                return None;
            }
            if let Some(user) = env("ZDOTDIR") {
                extra.push(("HIVETTY_ZDOTDIR".to_owned(), user));
            }
            extra.push((
                "ZDOTDIR".to_owned(),
                scripts.join("zsh").to_string_lossy().into_owned(),
            ));
            Some(Integration {
                args: args.to_vec(),
                env: extra,
            })
        }
        Shell::Bash => {
            // bash takes long options only before short ones, and a login shell ignores
            // --rcfile (the script sources the profile itself when HIVETTY_BASH_LOGIN is set).
            let mut login = false;
            let (mut long, mut short) = (Vec::new(), Vec::new());
            for arg in args {
                if let Some(name) = arg.strip_prefix("--") {
                    match name {
                        "login" => login = true,
                        // Files we would override, or modes we must not change.
                        n if n == "rcfile"
                            || n == "init-file"
                            || n.starts_with("norc")
                            || n.starts_with("posix") =>
                        {
                            return None;
                        }
                        _ => long.push(arg.clone()),
                    }
                } else {
                    // Anything else is a script to run: do not interfere.
                    let flags = arg.strip_prefix('-').filter(|f| !f.is_empty())?;
                    // Split clusters like `-il`.
                    for flag in flags.chars() {
                        match flag {
                            'l' => login = true,
                            'i' => {}
                            // A command to run (or read from stdin): do not interfere.
                            'c' | 's' => return None,
                            other => short.push(format!("-{other}")),
                        }
                    }
                }
            }
            if login {
                extra.push(("HIVETTY_BASH_LOGIN".to_owned(), "1".to_owned()));
            }
            let mut new_args = long;
            new_args.extend([
                "--rcfile".to_owned(),
                scripts.join("hivetty.bash").to_string_lossy().into_owned(),
                "-i".to_owned(),
            ]);
            new_args.extend(short);
            Some(Integration {
                args: new_args,
                env: extra,
            })
        }
        Shell::Fish => {
            if args
                .iter()
                .any(|a| a == "-c" || a == "--command" || !a.starts_with('-'))
            {
                return None;
            }
            let fish_dir = scripts.join("fish").to_string_lossy().into_owned();
            let user = env("XDG_DATA_DIRS").filter(|d| !d.is_empty());
            // The vendor script restores the user's value (empty = it was unset).
            extra.push((
                "HIVETTY_XDG_DATA_DIRS".to_owned(),
                user.clone().unwrap_or_default(),
            ));
            let system = user.unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
            extra.push(("XDG_DATA_DIRS".to_owned(), format!("{fish_dir}:{system}")));
            Some(Integration {
                args: args.to_vec(),
                env: extra,
            })
        }
        Shell::PowerShell => {
            let lower: Vec<String> = args.iter().map(|a| a.to_ascii_lowercase()).collect();
            let runs_something = lower.iter().any(|a| {
                matches!(
                    a.as_str(),
                    "-command"
                        | "-c"
                        | "-file"
                        | "-f"
                        | "-encodedcommand"
                        | "-ec"
                        | "-noninteractive"
                )
            });
            if runs_something {
                return None;
            }
            let script = scripts
                .join("hivetty.ps1")
                .to_string_lossy()
                .replace('\'', "''");
            let mut new_args = args.to_vec();
            if !lower.iter().any(|a| a == "-noexit") {
                new_args.push("-NoExit".to_owned());
            }
            // Dot-source the script's text, not the file: Windows PowerShell's default
            // execution policy (Restricted) refuses to run .ps1 files but not script blocks.
            new_args.push("-Command".to_owned());
            new_args.push(format!(
                ". ([ScriptBlock]::Create([IO.File]::ReadAllText('{script}')))"
            ));
            Some(Integration {
                args: new_args,
                env: extra,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &str) -> Option<String> {
        None
    }

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn shells_are_recognized_by_name() {
        assert_eq!(Shell::detect("/bin/zsh"), Some(Shell::Zsh));
        assert_eq!(
            Shell::detect(r"C:\Program Files\PowerShell\7\pwsh.exe"),
            Some(Shell::PowerShell)
        );
        assert_eq!(Shell::detect("powershell.exe"), Some(Shell::PowerShell));
        assert_eq!(Shell::detect("cmd.exe"), None);
        assert_eq!(Shell::detect("nu"), None);
    }

    #[test]
    fn zsh_keeps_the_users_zdotdir() {
        let dir = Path::new("/data/shell/1.0");
        let env = |k: &str| (k == "ZDOTDIR").then(|| "/home/me/.config/zsh".to_owned());
        let i = integrate("/bin/zsh", &args("-l"), dir, env).unwrap();
        assert_eq!(i.args, args("-l"));
        assert!(
            i.env
                .contains(&("HIVETTY_ZDOTDIR".into(), "/home/me/.config/zsh".into()))
        );
        assert!(
            i.env
                .contains(&("ZDOTDIR".into(), dir.join("zsh").to_string_lossy().into()))
        );
        assert!(integrate("zsh", &args("-c ls"), dir, none).is_none());
    }

    #[test]
    fn bash_uses_rcfile_and_remembers_login() {
        let dir = Path::new("/d");
        let i = integrate("bash", &args("--login"), dir, none).unwrap();
        assert_eq!(i.args[0], "--rcfile");
        assert!(i.args[1].ends_with("hivetty.bash"));
        assert!(i.env.contains(&("HIVETTY_BASH_LOGIN".into(), "1".into())));
        assert!(integrate("bash", &args("--norc"), dir, none).is_none());
        assert!(integrate("bash", &args("script.sh"), dir, none).is_none());
        // Long options go first; clusters are split.
        let i = integrate("bash", &args("-ilx --noprofile"), dir, none).unwrap();
        assert_eq!(i.args[0], "--noprofile");
        assert_eq!(i.args[1], "--rcfile");
        assert_eq!(&i.args[3..], ["-i", "-x"]);
        assert!(i.env.contains(&("HIVETTY_BASH_LOGIN".into(), "1".into())));
        assert!(integrate("bash", &args("-ic"), dir, none).is_none());
    }

    #[test]
    fn fish_prepends_its_data_dir() {
        let env = |k: &str| (k == "XDG_DATA_DIRS").then(|| "/usr/share".to_owned());
        let i = integrate("fish", &[], Path::new("/d"), env).unwrap();
        let (_, dirs) = i.env.iter().find(|(k, _)| k == "XDG_DATA_DIRS").unwrap();
        assert!(dirs.ends_with(":/usr/share"), "{dirs}");
    }

    #[test]
    fn powershell_dot_sources_the_script_and_stays_open() {
        let dir = Path::new(r"C:\Users\o'neil\AppData\Local\hivetty\shell");
        let i = integrate("pwsh.exe", &args("-NoLogo"), dir, none).unwrap();
        assert_eq!(i.args[0], "-NoLogo");
        assert_eq!(i.args[1], "-NoExit");
        assert_eq!(i.args[2], "-Command");
        assert!(
            i.args[3].starts_with(". ([ScriptBlock]::Create(") && i.args[3].contains("o''neil")
        );
        assert!(integrate("pwsh", &args("-File x.ps1"), dir, none).is_none());
        assert!(integrate("powershell", &args("-Command Get-Date"), dir, none).is_none());
    }

    #[test]
    fn install_writes_the_scripts_once() {
        let dir = std::env::temp_dir().join(format!("hv-shell-{}", std::process::id()));
        let root = install(&dir).unwrap();
        for (relative, contents) in FILES {
            assert_eq!(
                std::fs::read_to_string(root.join(relative)).unwrap(),
                *contents
            );
        }
        // Second run: nothing to rewrite.
        let modified = std::fs::metadata(root.join("hivetty.ps1"))
            .unwrap()
            .modified()
            .unwrap();
        install(&dir).unwrap();
        let again = std::fs::metadata(root.join("hivetty.ps1"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(modified, again);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn scripts_contain_no_raw_control_bytes() {
        for (name, contents) in FILES {
            assert!(
                !contents
                    .bytes()
                    .any(|b| b < 0x20 && !matches!(b, b'\t' | b'\n' | b'\r')),
                "{name}"
            );
        }
    }
}
