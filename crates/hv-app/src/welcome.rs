//! First run (F6-03): decides whether to show the wizard, gathers what it shows, and applies
//! the choices. Everything here does file I/O: call it off the UI thread.

use std::path::{Path, PathBuf};

use hv_agents::install::Plan;
use hv_ui::{AgentOffer, AgentSetup, WelcomeChoice, WelcomeInfo};

/// Written once the wizard was answered or skipped.
const MARKER: &str = "welcomed";

/// Every agent change the wizard knows, in the order it lists them.
const SETUPS: [AgentSetup; 4] = [
    AgentSetup::ClaudeHooks,
    AgentSetup::ClaudeStatusline,
    AgentSetup::CodexHooks,
    AgentSetup::GeminiHooks,
];

fn marker() -> Option<PathBuf> {
    hv_config::data_dir().map(|d| d.join(MARKER))
}

/// What the wizard shows, or `None` when this is not the first run (the marker exists, or
/// there is a config file already: the user set hivetty up by hand).
pub(crate) fn prepare() -> Option<WelcomeInfo> {
    if marker()?.exists() || hv_config::default_path()?.exists() {
        return None;
    }
    Some(WelcomeInfo { agents: offers() })
}

/// The agent changes on offer: for each agent found on this machine, what installing hivetty's
/// hooks (and Claude Code's statusline) would change. Only reads files.
pub(crate) fn offers() -> Vec<AgentOffer> {
    let (Ok(home), Ok(exe)) = (crate::agents_cli::home(), std::env::current_exe()) else {
        return Vec::new();
    };
    let profiles = hv_agents::profile::builtin();
    let on_path: Vec<String> =
        hv_agents::profile::installed(&profiles, std::env::var_os("PATH").as_deref())
            .into_iter()
            .map(|(p, _)| p.id.clone())
            .collect();
    SETUPS
        .into_iter()
        .filter_map(|setup| {
            let path = settings_path(setup, &home);
            // On the PATH, or its settings folder exists (installed some other way).
            let found = on_path.iter().any(|id| id == agent_id(setup))
                || path.parent().is_some_and(Path::is_dir);
            if !found {
                return None;
            }
            let text = read_or_empty(&path).ok()?;
            // An error means the file cannot be changed safely (or, for the statusline, the
            // user has one of their own): not offered.
            let plan = plan(setup, &text, &exe).ok()?;
            Some(AgentOffer {
                setup,
                path: shown(&path, &home),
                changes: plan.changes,
            })
        })
        .collect()
}

fn agent_id(setup: AgentSetup) -> &'static str {
    match setup {
        AgentSetup::ClaudeHooks | AgentSetup::ClaudeStatusline => "claude-code",
        AgentSetup::CodexHooks => "codex",
        AgentSetup::GeminiHooks => "gemini-cli",
    }
}

fn settings_path(setup: AgentSetup, home: &Path) -> PathBuf {
    match setup {
        AgentSetup::ClaudeHooks | AgentSetup::ClaudeStatusline => {
            hv_agents::install::claude_settings_path(home)
        }
        AgentSetup::CodexHooks => hv_agents::codex::config_path(home),
        AgentSetup::GeminiHooks => hv_agents::install::gemini_settings_path(home),
    }
}

/// The change `setup` makes to `text` (the settings file as it is now).
fn plan(setup: AgentSetup, text: &str, exe: &Path) -> Result<Plan, String> {
    match setup {
        AgentSetup::ClaudeHooks => hv_agents::install::plan_install(text, exe),
        AgentSetup::ClaudeStatusline => hv_agents::install::plan_statusline_install(text, exe),
        AgentSetup::CodexHooks => hv_agents::codex::plan_install(text, exe),
        AgentSetup::GeminiHooks => {
            hv_agents::install::plan_install_for(hv_agents::install::GEMINI, text, exe)
        }
    }
}

/// `path` with the home folder written as `~`, to keep it short.
fn shown(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) => {
            let sep = std::path::MAIN_SEPARATOR;
            format!("~{sep}{}", rest.display())
        }
        Err(_) => path.display().to_string(),
    }
}

fn read_or_empty(path: &Path) -> std::io::Result<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(err),
    }
}

/// The `config.toml` the choices make.
fn config_text(choice: &WelcomeChoice) -> String {
    format!(
        "# Written by the first-run wizard. All settings: docs/user/guide.md\n\n\
         [theme]\nname = \"{}\"\n\n[shell]\nintegration = {}\n\n[updates]\ncheck = {}\n",
        choice.theme, choice.shell_integration, choice.check_updates
    )
}

/// Applies the choices (`None` = skipped) and remembers that the wizard ran.
pub(crate) fn finish(choice: Option<&WelcomeChoice>) -> Result<(), String> {
    let mut problem = None;
    if let Some(choice) = choice {
        if let Some(path) = hv_config::default_path()
            && !path.exists()
        {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            std::fs::write(&path, config_text(choice)).map_err(|e| e.to_string())?;
        }
        if !choice.agents.is_empty() {
            // Remember the wizard ran even if this fails, and report it.
            problem = install_agents(&choice.agents).err();
        }
    }
    let marker = marker().ok_or("no data directory")?;
    if let Some(dir) = marker.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(marker, "").map_err(|e| e.to_string())?;
    problem.map_or(Ok(()), Err)
}

/// Makes the chosen changes, planned again from each file as it is now: one write and one
/// backup per file. A file that fails does not stop the others.
fn install_agents(chosen: &[AgentSetup]) -> Result<(), String> {
    let home = crate::agents_cli::home().map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut files: Vec<PathBuf> = Vec::new();
    for setup in chosen {
        let path = settings_path(*setup, &home);
        if !files.contains(&path) {
            files.push(path);
        }
    }
    let mut problems = Vec::new();
    for path in files {
        let setups: Vec<AgentSetup> = chosen
            .iter()
            .copied()
            .filter(|s| settings_path(*s, &home) == path)
            .collect();
        if let Err(err) = install_file(&path, &setups, &exe) {
            problems.push(format!("{}: {err}", shown(&path, &home)));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems.join("; "))
    }
}

/// What `setup` is called in an error message.
fn name(setup: AgentSetup) -> &'static str {
    match setup {
        AgentSetup::ClaudeHooks => "Claude Code hooks",
        AgentSetup::ClaudeStatusline => "Claude Code statusline",
        AgentSetup::CodexHooks => "Codex hooks",
        AgentSetup::GeminiHooks => "Gemini CLI hooks",
    }
}

/// Applies `setups` (all for `path`) together.
fn install_file(path: &Path, setups: &[AgentSetup], exe: &Path) -> Result<(), String> {
    let mut text = read_or_empty(path).map_err(|e| e.to_string())?;
    let mut changed = false;
    // One can fail on its own (the user set up a statusline meanwhile): the rest still go in.
    let mut skipped = Vec::new();
    for setup in setups {
        match plan(*setup, &text, exe) {
            Ok(plan) => {
                changed |= !plan.changes.is_empty();
                text = plan.new_text;
            }
            Err(err) => skipped.push(format!("{} not set up: {err}", name(*setup))),
        }
    }
    if changed {
        let plan = Plan {
            new_text: text,
            changes: Vec::new(),
        };
        hv_agents::install::apply(path, &plan).map_err(|e| e.to_string())?;
    }
    if skipped.is_empty() {
        Ok(())
    } else {
        Err(skipped.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_written_is_valid() {
        let choice = WelcomeChoice {
            theme: "hivetty-light".into(),
            shell_integration: false,
            check_updates: true,
            agents: Vec::new(),
        };
        let (config, error) = hv_config::parse(&config_text(&choice));
        assert_eq!(error, None);
        assert_eq!(config.theme.name, "hivetty-light");
        assert!(!config.shell.integration);
        assert!(config.updates.check);
    }

    #[test]
    fn paths_under_home_are_shortened() {
        let home = std::env::temp_dir().join("hv-home");
        let path = home.join(".claude").join("settings.json");
        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(
            shown(&path, &home),
            format!("~{sep}.claude{sep}settings.json")
        );
        let elsewhere = std::env::temp_dir().join("other.json");
        assert_eq!(shown(&elsewhere, &home), elsewhere.display().to_string());
    }

    #[test]
    fn claude_changes_to_one_file_go_in_together() {
        let dir = std::env::temp_dir().join(format!("hv-welcome-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.json");
        std::fs::write(&path, "{}").unwrap();
        let exe = std::env::current_exe().unwrap();
        install_file(
            &path,
            &[AgentSetup::ClaudeHooks, AgentSetup::ClaudeStatusline],
            &exe,
        )
        .unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("SessionStart") && text.contains("statusLine"));
        // Planned again from the file now: no hook left to add.
        assert!(
            plan(AgentSetup::ClaudeHooks, &text, &exe)
                .unwrap()
                .changes
                .is_empty()
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
