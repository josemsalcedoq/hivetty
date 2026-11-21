# termhive — instructions for agents

A GPU-accelerated, cross-platform (macOS, Ubuntu, Windows) terminal written in Rust, with a sidebar for coding agents and a usage dashboard. The docs are in Spanish.

## Before you start
- Read `docs/PLAN.md` (index) and find the task in `docs/backlog.md`. Work only on tasks from the current phase. Do not start a phase until the previous one is closed.
- Each task has a requirement ID (`docs/requirements.md`) and acceptance criteria. Meet them exactly.

## Fixed rules
- The stack and architecture are in `docs/adr/` and `docs/architecture.md`. To change them, write a new ADR. Never change them silently.
- `unsafe` is allowed only in `crates/th-platform`.
- The main thread never does blocking I/O. Painting never holds the terminal lock (copy a snapshot first).
- `th-ui`/`th-render` never depend on `th-pty`; they use `PaneHost`.
- Never run project tools to detect technologies; only read files (`docs/security.md` §2.5).
- Never write to the user's config files (`~/.claude/settings.json`, etc.) without showing the diff and asking for confirmation.
- `termhive hook` must always exit with code 0 in under 1 s and print nothing to stdout.
- Code ported from Alacritty or other projects keeps its license header and attribution.

## Conventions
- Code, identifiers and commits are in English (Conventional Commits). Docs are in Spanish.
- No AI attribution in commits or PRs.
- Before finishing: the `termhive-platform-check` skill, `/code-review` and, if you touch IPC, parsers, hooks or updates, `/security-review`.
- Tests at the right level (`docs/testing.md`). New agent-log parsers need real fixtures for each version.
