//! hivetty: a GPU-accelerated terminal for running many coding agents side by side.

// Release builds on Windows are GUI apps (no console window). Debug builds keep the console for logs.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod agents_cli;
mod app;
mod archive;
mod crash;
mod forward;
mod headless;
mod hook;
mod keymap;
#[cfg(target_os = "macos")]
mod menu_mac;
mod model;
mod saved;
mod statusline;
mod usage_cli;
mod welcome;
mod workspace;
mod worktree;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use hv_term::SpawnSpec;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
hivetty - a terminal for running many coding agents side by side

USAGE:
    hivetty [OPTIONS] [-e <PROGRAM> [ARGS]...]

OPTIONS:
    -e, --command <PROGRAM> [ARGS]...
                         Run PROGRAM instead of the default shell (must be last)
    --screenshot <FILE>  Run without a window and save the final screen as a PNG
    --size <COLSxROWS>   Grid size for --screenshot (default 100x30)
    --wait <SECONDS>     Max time --screenshot waits for the program to exit (default 5)
    --events             With --screenshot, print the terminal events seen (OSC 7/9/133/777/7501)
    --scroll <LINES>     With --screenshot, scroll up into the scrollback before capturing
    --record <FILE>      Record all terminal output to an asciicast v2 file (debugging)
    --ui                 With --screenshot, also draw the window chrome (sidebar)
    --find <PATTERN>     With --screenshot, search the scrollback (regex) and show the result
    --preedit <TEXT>     With --screenshot, draw TEXT as input-method composition at the cursor
    --split              With --screenshot, run the program in two panes side by side
    --palette            With --screenshot --ui, show the command palette
    --welcome            With --screenshot --ui, show the first-run wizard (sample data)
    --usage              With --screenshot --ui, show the usage dashboard (this machine's logs)
    --light              With --screenshot, use the light theme
    --high-contrast      With --screenshot, use the high-contrast palette
    --doctor             Print diagnostic information about this system
    --safe               Start with default settings, no shell integration and the software
                         renderer (also HIVETTY_SAFE=1)
    usage [--json|--csv] [--project NAME] [--days N]
                         Print token usage per day from the agents' local logs
    agents [list]        List the coding agents hivetty knows and which are installed
    agents install-hooks [--agent claude-code|gemini-cli] [--dry-run]
                         Add hivetty's hooks to the agent's settings (shows the changes first)
    agents uninstall-hooks [--agent claude-code|gemini-cli] [--dry-run]
                         Remove them again
    agents install-statusline [--dry-run]
                         Use hivetty's statusline in Claude Code (context use, plan limits)
    agents uninstall-statusline [--dry-run]
                         Remove it again
    statusline           The statusline command itself (reads Claude Code's JSON on stdin)
    config schema        Print the JSON schema of config.toml (for editor validation)
    theme import <FILE>  Convert an iTerm2 (.itermcolors) or Alacritty (.toml) theme and print
                         the [theme.colors] block to paste into config.toml
    hook --agent <ID> --event <NAME>
                         Forward an agent hook (stdin) to the running hivetty; always exits 0
    -V, --version        Print version
    -h, --help           Print help

ENVIRONMENT:
    HIVETTY_LOG          Log level (error, warn, info, debug, trace). Default: warn
    HIVETTY_SAFE=1       Same as --safe
";

/// What the command line asked for.
#[derive(Debug, PartialEq)]
enum Command {
    Run(SpawnSpec, Option<PathBuf>),
    Screenshot {
        spec: SpawnSpec,
        out: PathBuf,
        cols: u16,
        rows: u16,
        wait: Duration,
        events: bool,
        scroll: i32,
        find: Option<String>,
        ui: bool,
        preedit: Option<String>,
        split: bool,
        palette: bool,
        welcome: bool,
        usage: bool,
        light: bool,
    },
    Doctor,
    Usage(usage_cli::UsageArgs),
    Agents(agents_cli::AgentsCommand),
    /// `theme import <file>`: print `[theme.colors]` converted from another terminal's theme.
    ThemeImport(PathBuf),
    /// `config schema`: print the JSON schema of config.toml.
    ConfigSchema,
    Version,
    Help,
}

fn parse_args(args: &[String]) -> Result<Command, String> {
    let mut spec = SpawnSpec::default();
    let mut screenshot: Option<PathBuf> = None;
    let (mut cols, mut rows, mut wait) = (100, 30, Duration::from_secs(5));
    let mut events = false;
    let mut scroll = 0;
    let mut find = None;
    let mut ui = false;
    let mut preedit = None;
    let mut split = false;
    let mut palette = false;
    let mut welcome = false;
    let mut usage_flag = false;
    let mut light = false;
    let mut record: Option<PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        let value = |i: usize| {
            args.get(i + 1)
                .ok_or_else(|| format!("{} needs a value", args[i]))
        };
        match args[i].as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "-V" | "--version" => return Ok(Command::Version),
            "--doctor" => return Ok(Command::Doctor),
            "agents" => {
                return agents_cli::AgentsCommand::parse(&args[i + 1..]).map(Command::Agents);
            }
            "usage" => {
                return usage_cli::UsageArgs::parse(&args[i + 1..]).map(Command::Usage);
            }
            "config" => {
                return match args.get(i + 1).map(String::as_str) {
                    Some("schema") => Ok(Command::ConfigSchema),
                    _ => Err("usage: hivetty config schema".into()),
                };
            }
            "theme" => {
                return match (args.get(i + 1).map(String::as_str), args.get(i + 2)) {
                    (Some("import"), Some(file)) => Ok(Command::ThemeImport(PathBuf::from(file))),
                    _ => {
                        Err("usage: hivetty theme import <file.itermcolors|alacritty.toml>".into())
                    }
                };
            }
            "-e" | "--command" => {
                let program = value(i)?.clone();
                spec.program = Some(program);
                spec.args = args[i + 2..].to_vec();
                break;
            }
            "--screenshot" => {
                screenshot = Some(PathBuf::from(value(i)?));
                i += 1;
            }
            "--size" => {
                let v = value(i)?;
                let (c, r) = v.split_once('x').ok_or("--size must look like 100x30")?;
                cols = c.parse().map_err(|_| "invalid --size columns")?;
                rows = r.parse().map_err(|_| "invalid --size rows")?;
                if cols == 0 || rows == 0 {
                    return Err("--size must be at least 1x1".into());
                }
                i += 1;
            }
            "--events" => events = true,
            "--record" => {
                record = Some(PathBuf::from(value(i)?));
                i += 1;
            }
            "--ui" => ui = true,
            "--find" => {
                find = Some(value(i)?.clone());
                i += 1;
            }
            "--split" => split = true,
            "--palette" => palette = true,
            "--welcome" => welcome = true,
            "--usage" => usage_flag = true,
            "--light" => light = true,
            "--preedit" => {
                preedit = Some(value(i)?.clone());
                i += 1;
            }
            "--scroll" => {
                scroll = value(i)?.parse().map_err(|_| "invalid --scroll")?;
                i += 1;
            }
            "--wait" => {
                let secs: f64 = value(i)?.parse().map_err(|_| "invalid --wait")?;
                wait = Duration::from_secs_f64(secs.clamp(0.0, 600.0));
                i += 1;
            }
            other => return Err(format!("unknown argument '{other}'")),
        }
        i += 1;
    }
    Ok(match screenshot {
        Some(out) => Command::Screenshot {
            spec,
            out,
            cols,
            rows,
            wait,
            events,
            scroll,
            find,
            ui,
            preedit,
            split,
            palette,
            welcome,
            usage: usage_flag,
            light,
        },
        None => Command::Run(spec, record),
    })
}

/// `hivetty theme import <file>`: prints the converted theme; never writes the config.
fn theme_import(file: &std::path::Path) -> anyhow::Result<()> {
    use anyhow::Context;
    let size = std::fs::metadata(file)
        .with_context(|| format!("cannot read {}", file.display()))?
        .len();
    if size > hv_config::theme_import::MAX_BYTES as u64 {
        anyhow::bail!("{} is too large for a theme", file.display());
    }
    let text =
        std::fs::read_to_string(file).with_context(|| format!("cannot read {}", file.display()))?;
    let iterm = file
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("itermcolors"))
        || text.trim_start().starts_with("<?xml");
    let colors = if iterm {
        hv_config::theme_import::from_itermcolors(&text)
    } else {
        hv_config::theme_import::from_alacritty(&text)
    }
    .map_err(|e| anyhow::anyhow!("{}: {e}", file.display()))?;
    println!("# From {}. Paste this into config.toml.", file.display());
    print!("{}", hv_config::theme_import::to_toml(&colors));
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Agent hooks come first and bypass everything else: no logging setup, no console, exit 0.
    if args.first().map(String::as_str) == Some("hook") {
        hook::run(&args[1..]);
        // Exit right away: a helper thread may still be blocked on stdin or a pipe.
        std::process::exit(0);
    }
    // Claude Code's statusline: print the line, exit 0, never block (F4-32).
    if args.first().map(String::as_str) == Some("statusline") {
        let line = statusline::run();
        if !line.is_empty() {
            use std::io::Write;
            let mut out = std::io::stdout().lock();
            let _ = writeln!(out, "{line}").and_then(|()| out.flush());
        }
        std::process::exit(0);
    }
    env_logger::Builder::from_env(
        env_logger::Env::new().filter_or("HIVETTY_LOG", "warn,wgpu_hal=off,wgpu_core=error"),
    )
    .init();
    crash::install();
    // `--safe` applies to every command (N-23).
    let args: Vec<String> = if args.iter().any(|a| a == "--safe") {
        hv_config::set_safe_mode();
        args.into_iter().filter(|a| a != "--safe").collect()
    } else {
        args
    };
    if hv_config::safe_mode() {
        hv_render::set_software(true);
    }
    // Screenshots in the high-contrast palette (what `[theme] high_contrast` does).
    let args: Vec<String> = if args.iter().any(|a| a == "--high-contrast") {
        let p = hv_config::Palette::HIGH_CONTRAST;
        hv_term::set_custom(Some(hv_term::CustomPalette {
            foreground: p.foreground,
            background: p.background,
            cursor: p.cursor,
            selection: p.selection,
            ansi: p.ansi,
        }));
        args.into_iter()
            .filter(|a| a != "--high-contrast")
            .collect()
    } else {
        args
    };
    let parsed = parse_args(&args);
    // Everything except opening the window prints to the console; in Windows release builds
    // (GUI subsystem) that needs the parent console attached first.
    if !matches!(parsed, Ok(Command::Run(..))) {
        hv_platform::attach_parent_console();
    }
    let command = match parsed {
        Ok(c) => c,
        Err(message) => {
            println!("error: {message}\n\n{HELP}");
            return ExitCode::from(2);
        }
    };
    let result = match command {
        Command::Help => {
            print!("{HELP}");
            Ok(())
        }
        Command::Version => {
            println!("hivetty {VERSION}");
            Ok(())
        }
        Command::Doctor => {
            print!("{}", doctor_report());
            Ok(())
        }
        Command::Run(spec, record) => app::run(spec, record),
        Command::Agents(command) => agents_cli::run(&command),
        Command::ThemeImport(file) => theme_import(&file),
        Command::ConfigSchema => {
            print!("{}", hv_config::schema_json());
            Ok(())
        }
        Command::Usage(args) => {
            print!("{}", usage_cli::run(&args));
            Ok(())
        }
        Command::Screenshot {
            spec,
            out,
            cols,
            rows,
            wait,
            events,
            scroll,
            find,
            ui,
            preedit,
            split,
            palette,
            welcome,
            usage,
            light,
        } => {
            let opts = headless::Capture {
                cols,
                rows,
                wait,
                print_events: events,
                scroll_up: scroll,
                find,
                ui,
                preedit,
                split,
                palette,
                welcome,
                usage,
                light,
                ..headless::Capture::default()
            };
            headless::screenshot(&spec, &opts, &out).map(|saved| println!("saved {saved}"))
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            log::error!("{err:#}");
            println!("hivetty: {err:#}");
            ExitCode::FAILURE
        }
    }
}

/// Builds the `--doctor` report. GPU, hook and ConPTY checks are added in later phases.
fn doctor_report() -> String {
    let (shell, _) = hv_pty::default_shell();
    let mut report = format!(
        "hivetty {VERSION}\nos: {}\narch: {}\nfamily: {}\nshell: {shell}\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::env::consts::FAMILY,
    );
    if cfg!(windows) {
        report.push_str(&format!("conpty: {}\n", conpty_status()));
    }
    if hv_config::safe_mode() {
        report.push_str(
            "safe mode: on (default settings, no shell integration, software renderer)
",
        );
    }
    match hv_config::default_path() {
        Some(path) => {
            let state = if path.is_file() {
                "found"
            } else {
                "not created yet, using defaults"
            };
            report.push_str(&format!("config: {} ({state})\n", path.display()));
        }
        None => report.push_str("config: no config directory found\n"),
    }
    let (config, config_error) = hv_config::default_path()
        .as_deref()
        .map_or_else(|| (hv_config::Config::default(), None), hv_config::load);
    if let Some(err) = config_error {
        report.push_str(&format!("config error: {err}\n"));
    }
    // GPU: the adapter the renderer would pick (a headless device, no window).
    match hv_render::Renderer::new_headless(1, 1, 1.0, 14.0) {
        Ok(renderer) => report.push_str(&format!("gpu: {}\n", renderer.adapter_description())),
        Err(err) => report.push_str(&format!("gpu: NOT AVAILABLE ({err:#})\n")),
    }
    if let Some(dir) = hv_config::data_dir() {
        report.push_str(&format!("data: {}\n", dir.display()));
        if let Some(latest) = crash::reports(&dir).last() {
            report.push_str(&format!("last crash report: {}\n", latest.display()));
        }
        let cache = dir.join("usage.cache");
        if let Ok(meta) = std::fs::metadata(&cache) {
            report.push_str(&format!("usage cache: {} KB\n", meta.len() / 1024));
        }
    }
    report.push_str(&format!(
        "shell integration: {}\n",
        if config.shell.integration {
            "on (zsh, bash, fish, PowerShell)"
        } else {
            "off ([shell] integration = false)"
        }
    ));
    // Agents on PATH and whether hivetty's Claude Code hooks are installed.
    let profiles = hv_agents::profile::builtin();
    let installed = hv_agents::profile::installed(&profiles, std::env::var_os("PATH").as_deref());
    let names: Vec<&str> = installed.iter().map(|(p, _)| p.name.as_str()).collect();
    report.push_str(&format!(
        "agents on PATH: {}\n",
        if names.is_empty() {
            "none found".to_owned()
        } else {
            names.join(", ")
        }
    ));
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    if let Some(home) = home {
        let settings = hv_agents::install::claude_settings_path(std::path::Path::new(&home));
        let text = std::fs::read_to_string(&settings).unwrap_or_default();
        let state = match hv_agents::install::plan_install(
            &text,
            &std::env::current_exe().unwrap_or_default(),
        ) {
            Ok(plan) if plan.changes.is_empty() => "installed".to_owned(),
            Ok(plan) if plan.changes.iter().all(|c| c.starts_with('~')) => {
                "installed for another hivetty binary (run `hivetty agents install-hooks`)"
                    .to_owned()
            }
            Ok(_) => "not installed (`hivetty agents install-hooks`)".to_owned(),
            Err(err) => format!("cannot read {}: {err}", settings.display()),
        };
        report.push_str(&format!("claude code hooks: {state}\n"));
    }
    report
}

/// Which ConPTY host will be used (docs/platforms.md: the bundled one is preferred).
fn conpty_status() -> &'static str {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf));
    let has = |name: &str| dir.as_ref().is_some_and(|d| d.join(name).is_file());
    match (has("conpty.dll"), has("OpenConsole.exe")) {
        (true, true) => "bundled (conpty.dll + OpenConsole.exe)",
        (true, false) => {
            "WARNING: conpty.dll without OpenConsole.exe; falls back to the system conhost"
        }
        _ => "system (run `cargo xtask conpty-fetch` to bundle the newer one)",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn doctor_report_includes_version_and_os() {
        let report = doctor_report();
        assert!(report.starts_with(&format!("hivetty {VERSION}\n")));
        assert!(report.contains(&format!("os: {}", std::env::consts::OS)));
        assert!(report.contains("shell: "));
    }

    #[test]
    fn no_args_runs_the_default_shell() {
        assert_eq!(
            parse_args(&[]),
            Ok(Command::Run(SpawnSpec::default(), None))
        );
    }

    #[test]
    fn command_takes_the_rest_of_the_line() {
        let Ok(Command::Run(spec, _)) = parse_args(&args("-e git log --oneline")) else {
            panic!()
        };
        assert_eq!(spec.program.as_deref(), Some("git"));
        assert_eq!(spec.args, ["log", "--oneline"]);
    }

    #[test]
    fn screenshot_with_size_and_wait() {
        let Ok(Command::Screenshot {
            out,
            cols,
            rows,
            wait,
            spec,
            ..
        }) = parse_args(&args("--screenshot a.png --size 80x24 --wait 2 -e echo hi"))
        else {
            panic!()
        };
        assert_eq!(
            (out, cols, rows, wait),
            (PathBuf::from("a.png"), 80, 24, Duration::from_secs(2))
        );
        assert_eq!(spec.program.as_deref(), Some("echo"));
    }

    #[test]
    fn bad_arguments_are_rejected() {
        assert!(parse_args(&args("--size 80")).is_err());
        assert!(parse_args(&args("--size 0x10")).is_err());
        assert!(parse_args(&args("--bogus")).is_err());
        assert!(parse_args(&args("-e")).is_err());
    }
}
