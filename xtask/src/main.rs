//! `cargo xtask <command>`: project automation that should work the same on every OS.

mod conpty;

use std::process::{Command, ExitCode};

const HELP: &str = "\
cargo xtask <command>

COMMANDS:
    ci              Run the same checks as CI: fmt, clippy, tests, cargo-deny
    golden-update   Regenerate golden render images            (phase F1)
    fixtures        Refresh agent log / hook fixtures           (phase F4-F5)
    conpty-fetch    Download, verify and install the bundled ConPTY next to the exe
                    (Windows; --profile debug|release, default debug)
    dist            Build release packages                     (phase F6)
    bench           Run performance benchmarks (release build)
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("ci") => ci(),
        Some("bench") => {
            let ok = Command::new("cargo")
                .args([
                    "test",
                    "--release",
                    "-p",
                    "hv-term",
                    "--lib",
                    "bench_",
                    "--",
                    "--ignored",
                    "--nocapture",
                ])
                .status()
                .is_ok_and(|s| s.success());
            if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Some("conpty-fetch") => match conpty::fetch(&args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => {
                println!("xtask conpty-fetch: {err:#}");
                ExitCode::FAILURE
            }
        },
        Some(cmd @ ("golden-update" | "fixtures" | "dist")) => {
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
    // Code behind cfg(unix) is invisible to clippy on Windows (and vice versa). Check the crates
    // without native build dependencies for the other OSes too, when their std is installed
    // (`rustup target add x86_64-unknown-linux-gnu aarch64-apple-darwin x86_64-pc-windows-msvc`).
    let installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let host = std::env::consts::OS;
    for (os, target) in CROSS_TARGETS {
        if *os == host || !installed.lines().any(|l| l.trim() == *target) {
            continue;
        }
        let mut args = vec!["clippy", "--target", target, "--all-targets", "--locked"];
        for krate in PORTABLE_CRATES {
            args.extend(["-p", krate]);
        }
        args.extend(["--", "-D", "warnings"]);
        println!("==> cargo {}", args.join(" "));
        if !Command::new("cargo")
            .args(&args)
            .status()
            .is_ok_and(|s| s.success())
        {
            println!("xtask ci: clippy for {target} failed");
            return ExitCode::FAILURE;
        }
    }
    println!("xtask ci: all checks passed");
    ExitCode::SUCCESS
}

/// Other-OS targets checked by `xtask ci` when installed: `(std::env::consts::OS, triple)`.
const CROSS_TARGETS: &[(&str, &str)] = &[
    ("linux", "x86_64-unknown-linux-gnu"),
    ("macos", "aarch64-apple-darwin"),
    ("windows", "x86_64-pc-windows-msvc"),
];

/// Crates whose dependencies build without a C toolchain for the target.
const PORTABLE_CRATES: &[&str] = &[
    "hv-ipc",
    "hv-platform",
    "hv-agents",
    "hv-grid",
    "hv-config",
    "hv-usage",
];
