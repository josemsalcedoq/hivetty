//! hivetty: a GPU-accelerated terminal for running many coding agents side by side.

use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
hivetty - a terminal for running many coding agents side by side

USAGE:
    hivetty [OPTIONS]

OPTIONS:
    --doctor       Print diagnostic information about this system
    -V, --version  Print version
    -h, --help     Print help
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => {
            println!(
                "hivetty {VERSION}: the terminal window is not implemented yet (see docs/PLAN.md)."
            );
            ExitCode::SUCCESS
        }
        Some("-V" | "--version") => {
            println!("hivetty {VERSION}");
            ExitCode::SUCCESS
        }
        Some("-h" | "--help") => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Some("--doctor") => {
            print!("{}", doctor_report());
            ExitCode::SUCCESS
        }
        Some(other) => {
            println!("error: unknown argument '{other}'\n\n{HELP}");
            ExitCode::from(2)
        }
    }
}

/// Builds the `--doctor` report. GPU, shell, hook and ConPTY checks are added in later phases.
fn doctor_report() -> String {
    format!(
        "hivetty {VERSION}\nos: {}\narch: {}\nfamily: {}\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::env::consts::FAMILY,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_report_includes_version_and_os() {
        let report = doctor_report();
        assert!(report.starts_with(&format!("hivetty {VERSION}\n")));
        assert!(report.contains(&format!("os: {}", std::env::consts::OS)));
    }
}
