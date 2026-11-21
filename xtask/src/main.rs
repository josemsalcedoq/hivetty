//! `cargo xtask <command>`: project automation that should work the same on every OS.

use std::process::{Command, ExitCode};

const HELP: &str = "\
cargo xtask <command>

COMMANDS:
    ci              Run the same checks as CI: fmt, clippy, tests, cargo-deny
    golden-update   Regenerate golden render images            (phase F1)
    fixtures        Refresh agent log / hook fixtures           (phase F4-F5)
    conpty-fetch    Download and verify the bundled ConPTY      (phase F1, Windows)
    dist            Build release packages                     (phase F6)
    bench           Run performance benchmarks                 (phase F1)
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("ci") => ci(),
        Some(cmd @ ("golden-update" | "fixtures" | "conpty-fetch" | "dist" | "bench")) => {
            println!("xtask {cmd}: not implemented yet (see docs/backlog.md)");
            ExitCode::FAILURE
        }
        _ => {
            print!("{HELP}");
            ExitCode::from(2)
        }
    }
}

/// Runs the CI checks in order and stops at the first failure.
fn ci() -> ExitCode {
    let steps: &[&[&str]] = &[
        &["fmt", "--all", "--check"],
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ],
        &[
            "nextest",
            "run",
            "--workspace",
            "--locked",
            "--no-tests=pass",
        ],
        &["deny", "check"],
    ];
    for step in steps {
        println!("==> cargo {}", step.join(" "));
        let ok = Command::new("cargo")
            .args(*step)
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            println!("xtask ci: 'cargo {}' failed", step.join(" "));
            return ExitCode::FAILURE;
        }
    }
    println!("xtask ci: all checks passed");
    ExitCode::SUCCESS
}
