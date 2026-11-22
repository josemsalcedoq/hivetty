//! `cargo xtask <command>`: project automation that should work the same on every OS.

mod conpty;
mod notices;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const HELP: &str = "\
cargo xtask <command>

COMMANDS:
    ci              Run the same checks as CI: fmt, clippy, tests, cargo-deny
    check-sources   Fail on raw control bytes (ESC, NUL...) in .rs files
    golden-update   Regenerate golden render images            (phase F1)
    fixtures        Refresh agent log / hook fixtures           (phase F4-F5)
    conpty-fetch    Download, verify and install the bundled ConPTY next to the exe
                    (Windows; --profile debug|release, default debug)
    dist            Build release packages                     (phase F6)
    notices         Write THIRD_PARTY_NOTICES.md (licenses of every shipped crate)
    bench           Run performance benchmarks (release build)
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("ci") => ci(),
        Some("check-sources") => {
            let offenders = control_bytes_in_sources(Path::new("."));
            for (path, offset, byte) in &offenders {
                println!("{}:{offset}: raw control byte {byte:#04x}", path.display());
            }
            if offenders.is_empty() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Some("notices") => match notices::generate() {
            Ok(text) => match std::fs::write("THIRD_PARTY_NOTICES.md", text) {
                Ok(()) => {
                    println!("wrote THIRD_PARTY_NOTICES.md");
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    println!("xtask notices: {err}");
                    ExitCode::FAILURE
                }
            },
            Err(err) => {
                println!("xtask notices: {err:#}");
                ExitCode::FAILURE
            }
        },
        Some("bench") => {
            let ok = Command::new("cargo")
                .args([
                    "test",
                    "--release",
                    "-p",
                    "hv-term",
                    "-p",
                    "hv-render",
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
    // Raw control bytes in sources (an ESC or NUL pasted into a literal) compile but are
    // invisible in review and make tools treat the file as binary: write them escaped.
    let offenders = control_bytes_in_sources(Path::new("."));
    if !offenders.is_empty() {
        for (path, offset, byte) in &offenders {
            println!("{}:{offset}: raw control byte {byte:#04x}", path.display());
        }
        println!("xtask ci: write control characters as escapes (\\x1b, \\0, \\r)");
        return ExitCode::FAILURE;
    }
    // Code behind cfg(unix) is invisible to clippy on Windows (and vice versa). Check the
    // workspace for the other OSes too, when their std is installed
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
        // clippy only type-checks, so no linker or C toolchain for the target is needed, except
        // for xtask itself (ring builds C code).
        let args = [
            "clippy",
            "--target",
            target,
            "--workspace",
            "--exclude",
            "xtask",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ];
        println!("==> cargo {}", args.join(" "));
        if !Command::new("cargo")
            .args(args)
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

/// `(file, offset, byte)` for every control byte other than tab, LF and CR in the `.rs` files
/// under `crates/` and `xtask/`.
fn control_bytes_in_sources(root: &Path) -> Vec<(PathBuf, usize, u8)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, usize, u8)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n != "target") {
                    walk(&path, out);
                }
            } else if path.extension().is_some_and(|e| e == "rs")
                && let Ok(bytes) = std::fs::read(&path)
            {
                for (offset, &byte) in bytes.iter().enumerate() {
                    // A CR is fine only as part of a CRLF line ending.
                    let bare_cr = byte == b'\r' && bytes.get(offset + 1) != Some(&b'\n');
                    if bare_cr || (byte < 0x20 && !matches!(byte, b'\t' | b'\n' | b'\r')) {
                        out.push((path.clone(), offset, byte));
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&root.join("crates"), &mut out);
    walk(&root.join("xtask"), &mut out);
    out
}

/// Other-OS targets checked by `xtask ci` when installed: `(std::env::consts::OS, triple)`.
const CROSS_TARGETS: &[(&str, &str)] = &[
    ("linux", "x86_64-unknown-linux-gnu"),
    ("macos", "aarch64-apple-darwin"),
    ("windows", "x86_64-pc-windows-msvc"),
];
