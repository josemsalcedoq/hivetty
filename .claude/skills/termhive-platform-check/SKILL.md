---
name: termhive-platform-check
description: Checklist to run before finishing any termhive change that touches PTY, input, windows, paths, IPC, fonts, notifications or packaging, so it keeps working on macOS, Ubuntu and Windows.
---

# Cross-platform check for termhive

Before you call a change done, go through every item that applies and say which ones you checked.

## Code
- Paths: use `std::path` and `directories::ProjectDirs`. Never use hardcoded `/` or `~`; expand `~` with `dirs::home_dir()`.
- Line endings: the PTY can send `\r\n` on Windows. Parsers must accept both.
- Platform-specific code goes in `th-platform` behind `#[cfg(target_os = "...")]`, with an implementation (even a no-op) for all three systems.
- IPC: Unix socket on macOS/Linux, named pipe on Windows, both through `interprocess`.
- Shortcuts: use `Primary` (Cmd on macOS, Ctrl+Shift elsewhere). Never hardcode `Cmd` or `Ctrl`.
- Processes: executable names end in `.exe` on Windows (`claude` vs `claude.exe`).
- Shells: if you change a shell integration, update all four scripts (`zsh`, `bash`, `fish`, `pwsh`).

## Verification
- `cargo clippy --all-targets -- -D warnings` and `cargo nextest run` pass locally.
- If you changed rendering: run the golden test (software renderer vs reference).
- If you changed escape sequences: test under ConPTY (Windows) or note in the PR that it is still unverified there.
- CI has to pass on `macos-latest`, `ubuntu-24.04` and `windows-latest`.

## Report
End with a list of what you tested on which system and what is still unverified.
