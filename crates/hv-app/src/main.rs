//! hivetty: a GPU-accelerated terminal for running many coding agents side by side.

// Release builds on Windows are GUI apps (no console window). Debug builds keep the console for logs.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod headless;

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
    --find <PATTERN>     With --screenshot, search the scrollback (regex) and show the result
    --doctor             Print diagnostic information about this system
    usage [--json]       Print Claude Code token usage per day from local transcripts
    -V, --version        Print version
    -h, --help           Print help

ENVIRONMENT:
    HIVETTY_LOG          Log level (error, warn, info, debug, trace). Default: warn
";

/// What the command line asked for.
#[derive(Debug, PartialEq)]
enum Command {
    Run(SpawnSpec),
    Screenshot {
        spec: SpawnSpec,
        out: PathBuf,
        cols: u16,
        rows: u16,
        wait: Duration,
        events: bool,
        scroll: i32,
        find: Option<String>,
    },
    Doctor,
    Usage {
        json: bool,
    },
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
            "usage" => {
                return Ok(Command::Usage {
                    json: args[i + 1..].iter().any(|a| a == "--json"),
                });
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
            "--find" => {
                find = Some(value(i)?.clone());
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
        },
        None => Command::Run(spec),
    })
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(
        env_logger::Env::new().filter_or("HIVETTY_LOG", "warn,wgpu_hal=off,wgpu_core=error"),
    )
    .init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = parse_args(&args);
    // Everything except opening the window prints to the console; in Windows release builds
    // (GUI subsystem) that needs the parent console attached first.
    if !matches!(parsed, Ok(Command::Run(_))) {
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
        Command::Run(spec) => app::run(spec),
        Command::Usage { json } => {
            print!("{}", usage_report(json));
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
        } => {
            let opts = headless::Capture {
                cols,
                rows,
                wait,
                print_events: events,
                scroll_up: scroll,
                find,
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

/// `hivetty usage`: Claude Code token usage per day, read from the local transcripts.
fn usage_report(json: bool) -> String {
    use std::fmt::Write as _;
    let Some(root) = hv_usage::claude::default_root() else {
        return "No Claude Code transcripts found (~/.claude/projects).
"
        .to_owned();
    };
    let records = hv_usage::claude::scan(&root);
    let daily = hv_usage::daily(&records);
    let mut out = String::new();
    if json {
        out.push_str("{\"daily\":[");
        for (i, (day, t)) in daily.iter().enumerate() {
            let sep = if i == 0 { "" } else { "," };
            let _ = write!(
                out,
                "{sep}{{\"date\":\"{day}\",\"input\":{},\"output\":{},\"cacheRead\":{},\"cacheWrite\":{},\"turns\":{}}}",
                t.input, t.output, t.cache_read, t.cache_write, t.turns
            );
        }
        out.push_str(
            "]}
",
        );
        return out;
    }
    let _ = writeln!(
        out,
        "{:<12}{:>12}{:>12}{:>14}{:>14}{:>8}",
        "Date", "Input", "Output", "Cache read", "Cache write", "Turns"
    );
    let mut total = hv_usage::Totals::default();
    for (day, t) in &daily {
        let _ = writeln!(
            out,
            "{:<12}{:>12}{:>12}{:>14}{:>14}{:>8}",
            day.to_string(),
            t.input,
            t.output,
            t.cache_read,
            t.cache_write,
            t.turns
        );
        total.input += t.input;
        total.output += t.output;
        total.cache_read += t.cache_read;
        total.cache_write += t.cache_write;
        total.turns += t.turns;
    }
    let _ = writeln!(
        out,
        "
Total input {} ({:.0}% from cache), output {}, {} turns",
        hv_usage::compact(total.total_input()),
        total.cache_share() * 100.0,
        hv_usage::compact(total.output),
        total.turns
    );
    out
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
        assert_eq!(parse_args(&[]), Ok(Command::Run(SpawnSpec::default())));
    }

    #[test]
    fn command_takes_the_rest_of_the_line() {
        let Ok(Command::Run(spec)) = parse_args(&args("-e git log --oneline")) else {
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
