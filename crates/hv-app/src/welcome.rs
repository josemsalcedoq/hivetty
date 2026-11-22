//! First run (F6-03): decides whether to show the wizard, gathers what it shows, and applies
//! the choices. Everything here does file I/O: call it off the UI thread.

use std::path::{Path, PathBuf};

use hv_ui::{WelcomeChoice, WelcomeInfo};

/// Written once the wizard was answered or skipped.
const MARKER: &str = "welcomed";

fn marker() -> Option<PathBuf> {
    hv_config::data_dir().map(|d| d.join(MARKER))
}

/// What the wizard shows, or `None` when this is not the first run (the marker exists, or
/// there is a config file already: the user set hivetty up by hand).
pub(crate) fn prepare() -> Option<WelcomeInfo> {
    if marker()?.exists() || hv_config::default_path()?.exists() {
        return None;
    }
    Some(agent_changes(claude_installed()))
}

/// Claude Code is on the PATH or has its settings folder.
fn claude_installed() -> bool {
    let profiles = hv_agents::profile::builtin();
    let on_path = hv_agents::profile::installed(&profiles, std::env::var_os("PATH").as_deref())
        .iter()
        .any(|(p, _)| p.id == "claude-code");
    on_path
        || crate::agents_cli::settings_path()
            .ok()
            .and_then(|p| p.parent().map(Path::is_dir))
            .unwrap_or(false)
}

/// The changes installing the hooks and the statusline would make.
fn agent_changes(installed: bool) -> WelcomeInfo {
    let mut info = WelcomeInfo::default();
    if !installed {
        return info;
    }
    let (Ok(path), Ok(exe)) = (crate::agents_cli::settings_path(), std::env::current_exe()) else {
        return info;
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(_) => return info,
    };
    let shown = path.display().to_string();
    if let Ok(plan) = hv_agents::install::plan_install(&text, &exe) {
        info.hooks = Some((shown.clone(), plan.changes));
    }
    // An error here means the user has a statusline of their own: not offered.
    if let Ok(plan) = hv_agents::install::plan_statusline_install(&text, &exe) {
        info.statusline = Some((shown, plan.changes));
    }
    info
}

/// The `config.toml` the choices make.
fn config_text(choice: &WelcomeChoice) -> String {
    format!(
        "# Written by the first-run wizard. All settings: docs/user/guide.md\n\n\
         [theme]\nname = \"{}\"\n\n[shell]\nintegration = {}\n",
        choice.theme, choice.shell_integration
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
        if choice.install_hooks || choice.install_statusline {
            // Remember the wizard ran even if this fails, and report it.
            problem = install_agent(choice).err();
        }
    }
    let marker = marker().ok_or("no data directory")?;
    if let Some(dir) = marker.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(marker, "").map_err(|e| e.to_string())?;
    problem.map_or(Ok(()), Err)
}

/// Installs what the user ticked, planned again from the file as it is now (both changes
/// together, one backup).
fn install_agent(choice: &WelcomeChoice) -> Result<(), String> {
    let path = crate::agents_cli::settings_path().map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err.to_string()),
    };
    let mut changed = false;
    if choice.install_hooks {
        let plan = hv_agents::install::plan_install(&text, &exe)?;
        changed |= !plan.changes.is_empty();
        text = plan.new_text;
    }
    // The statusline can fail on its own (the user set one up meanwhile): the hooks still go in.
    let mut skipped = None;
    if choice.install_statusline {
        match hv_agents::install::plan_statusline_install(&text, &exe) {
            Ok(plan) => {
                changed |= !plan.changes.is_empty();
                text = plan.new_text;
            }
            Err(err) => skipped = Some(err),
        }
    }
    if changed {
        let plan = hv_agents::install::Plan {
            new_text: text,
            changes: Vec::new(),
        };
        hv_agents::install::apply(&path, &plan).map_err(|e| e.to_string())?;
    }
    skipped.map_or(Ok(()), |err| {
        Err(format!("statusline not installed: {err}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_written_is_valid() {
        let choice = WelcomeChoice {
            theme: "hivetty-light".into(),
            shell_integration: false,
            install_hooks: false,
            install_statusline: false,
        };
        let (config, error) = hv_config::parse(&config_text(&choice));
        assert_eq!(error, None);
        assert_eq!(config.theme.name, "hivetty-light");
        assert!(!config.shell.integration);
    }

    #[test]
    fn nothing_is_offered_without_the_agent() {
        assert_eq!(agent_changes(false), WelcomeInfo::default());
    }
}
