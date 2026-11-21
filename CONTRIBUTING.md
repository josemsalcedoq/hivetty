# Contributing to hivetty

Thanks for your interest! hivetty is in early development, so please open an issue to discuss larger changes before you start.

## Workflow

1. Pick a task from [`docs/backlog.md`](docs/backlog.md). Work only on tasks from the current phase.
2. Create a short-lived branch: `feat/<task-id>`, `fix/…`, `docs/…` or `spike/…`.
3. Use [Conventional Commits](https://www.conventionalcommits.org/), for example `feat(hv-render): glyph atlas`.
4. Run `cargo xtask ci` locally before opening a pull request.
5. Fill in the pull request template, including what you tested on each platform.

## Rules

- All user-facing text (UI, CLI, errors and docs for users) is in **English**.
- `unsafe` code is only allowed in `crates/hv-platform`.
- Architectural changes need an ADR in `docs/adr/`.
- Code ported from other projects (for example Alacritty) must keep its license header and attribution.

## Setup

See the "Building" section of the [README](README.md).

## License

By contributing, you agree that your contributions are dual licensed under MIT OR Apache-2.0, as described in the README.
