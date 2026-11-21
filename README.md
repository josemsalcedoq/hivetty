# hivetty

A native, GPU-accelerated terminal written in Rust, built for running **many coding agents at once** (Claude Code, Codex, Gemini CLI, opencode, …). It targets **macOS, Linux (Ubuntu) and Windows**.

> **Status: early development.** Planning is complete and phase F0 (foundations) is in progress. There is nothing usable yet.

## Goals for v1.0

- **A fast terminal that doesn't flicker**:
  - GPU rendering with `wgpu` (Metal / Vulkan / DirectX 12).
  - Correct synchronized output.
  - A modern bundled ConPTY on Windows.
- **Agent sidebar**:
  - Sessions grouped by project, each showing its live state (*Working*, *Waiting*, *Done*).
  - State comes from OSC 7501 and each agent's hooks.
  - An attention inbox, desktop notifications, session resume and git worktrees.
- **Usage dashboard**: tokens (input, cache and output) by model and by agent, plus tasks, agent hours, a daily chart and an activity heatmap. All of it is computed locally; nothing leaves your machine.
- **Shell integration** for zsh, bash, fish and PowerShell, with project "chips" that show the detected technology, branch and versions.
- **First-class Windows support**, not an afterthought.

## Building

```sh
cargo build
cargo run -p hv-app -- --doctor
cargo xtask ci        # same checks as CI: fmt, clippy, tests, cargo-deny
```

Requirements:
- Rust via [rustup](https://rustup.rs) (the version is pinned in `rust-toolchain.toml`).
- macOS: Xcode Command Line Tools.
- Ubuntu: `build-essential pkg-config libfontconfig1-dev libxkbcommon-dev libwayland-dev libx11-dev libvulkan-dev`.
- Windows: Visual Studio Build Tools with the "Desktop development with C++" workload.

## Documentation

The design and planning documents live in [`docs/`](docs/). They are written in Spanish; the application, code and user documentation are in English. Start with [`docs/PLAN.md`](docs/PLAN.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). To report a security issue, see [SECURITY.md](SECURITY.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT), at your option. Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
