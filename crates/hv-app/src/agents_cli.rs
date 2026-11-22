//! `hivetty agents …`: list installed agents and install or remove hivetty's hooks
//! (F4-02, F4-07). Changes to the agents' config are shown first and need a yes.

use std::io::{BufRead, Write};

use anyhow::{Context, Result, bail};

/// What `hivetty agents` was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AgentsCommand {
    List,
    InstallHooks { dry_run: bool },
    UninstallHooks { dry_run: bool },
    InstallStatusline { dry_run: bool },
    UninstallStatusline { dry_run: bool },
}

impl AgentsCommand {
    pub(crate) fn parse(args: &[String]) -> Result<Self, String> {
        let dry_run = args.iter().any(|a| a == "--dry-run");
        match args.first().map(String::as_str) {
            None | Some("list") => Ok(Self::List),
            Some("install-hooks") => Ok(Self::InstallHooks { dry_run }),
            Some("uninstall-hooks") => Ok(Self::UninstallHooks { dry_run }),
            Some("install-statusline") => Ok(Self::InstallStatusline { dry_run }),
            Some("uninstall-statusline") => Ok(Self::UninstallStatusline { dry_run }),
            Some(other) => Err(format!("unknown agents command '{other}'")),
        }
    }
}

pub(crate) fn run(command: &AgentsCommand) -> Result<()> {
    match command {
        AgentsCommand::List => {
            let profiles = hv_agents::profile::builtin();
            let installed =
                hv_agents::profile::installed(&profiles, std::env::var_os("PATH").as_deref());
            println!("Agents hivetty knows ({}):", profiles.len());
            for profile in &profiles {
                let found = installed.iter().find(|(p, _)| p.id == profile.id);
                let status = match found {
                    Some((_, path)) => format!("installed  {}", path.display()),
                    None => "not found".to_owned(),
                };
                let note = if profile.verified {
                    ""
                } else {
                    "  (profile not verified)"
                };
                println!("  {:<14} {:<12} {status}{note}", profile.id, profile.name);
            }
            Ok(())
        }
        AgentsCommand::InstallHooks { dry_run } => {
            let path = settings_path()?;
            let exe = std::env::current_exe().context("cannot find the hivetty executable")?;
            let text = read_or_empty(&path)?;
            let plan = hv_agents::install::plan_install(&text, &exe)
                .map_err(|e| anyhow::anyhow!("{e} ({})", path.display()))?;
            confirm_and_apply(
                &path,
                &plan,
                *dry_run,
                "Install hivetty's Claude Code hooks",
            )
        }
        AgentsCommand::UninstallHooks { dry_run } => {
            let path = settings_path()?;
            let text = read_or_empty(&path)?;
            let plan = hv_agents::install::plan_uninstall(&text)
                .map_err(|e| anyhow::anyhow!("{e} ({})", path.display()))?;
            confirm_and_apply(&path, &plan, *dry_run, "Remove hivetty's Claude Code hooks")
        }
        AgentsCommand::InstallStatusline { dry_run } => {
            let path = settings_path()?;
            let exe = std::env::current_exe().context("cannot find the hivetty executable")?;
            let text = read_or_empty(&path)?;
            let plan = hv_agents::install::plan_statusline_install(&text, &exe)
                .map_err(|e| anyhow::anyhow!("{e} ({})", path.display()))?;
            confirm_and_apply(
                &path,
                &plan,
                *dry_run,
                "Use hivetty's Claude Code statusline",
            )
        }
        AgentsCommand::UninstallStatusline { dry_run } => {
            let path = settings_path()?;
            let text = read_or_empty(&path)?;
            let plan = hv_agents::install::plan_statusline_uninstall(&text)
                .map_err(|e| anyhow::anyhow!("{e} ({})", path.display()))?;
            confirm_and_apply(&path, &plan, *dry_run, "Remove hivetty's statusline")
        }
    }
}

fn settings_path() -> Result<std::path::PathBuf> {
    let home = std::env::var_os("USERPROFILE")
        .filter(|_| cfg!(windows))
        .or_else(|| std::env::var_os("HOME"))
        .context("cannot find the home directory")?;
    Ok(hv_agents::install::claude_settings_path(
        std::path::Path::new(&home),
    ))
}

fn read_or_empty(path: &std::path::Path) -> Result<String> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(err) => Err(err).with_context(|| format!("cannot read {}", path.display())),
    }
}

fn confirm_and_apply(
    path: &std::path::Path,
    plan: &hv_agents::install::Plan,
    dry_run: bool,
    what: &str,
) -> Result<()> {
    if plan.changes.is_empty() {
        println!("Nothing to change in {}.", path.display());
        return Ok(());
    }
    println!("{what} in {}:", path.display());
    for change in &plan.changes {
        println!("  {change}");
    }
    if dry_run {
        println!("\nDry run: nothing was written.");
        return Ok(());
    }
    print!("\nA backup goes to settings.json.hivetty-bak. Apply these changes? [y/N] ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        bail!("cancelled; nothing was written");
    }
    hv_agents::install::apply(path, plan)
        .with_context(|| format!("cannot write {}", path.display()))?;
    println!("Done.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subcommands_parse() {
        let args = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
        assert_eq!(AgentsCommand::parse(&[]), Ok(AgentsCommand::List));
        assert_eq!(
            AgentsCommand::parse(&args("install-hooks --dry-run")),
            Ok(AgentsCommand::InstallHooks { dry_run: true })
        );
        assert!(AgentsCommand::parse(&args("explode")).is_err());
    }
}
