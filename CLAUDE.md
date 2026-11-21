# termhive — instructions for agents

A Rust, GPU-accelerated, cross-platform (macOS, Ubuntu, Windows) terminal with a sidebar for coding agents and a usage dashboard.

- Read `docs/PLAN.md` to find the current phase and its acceptance criteria. Do not start work from a later phase.
- The stack and architecture are in `docs/adr/` and `docs/architecture.md`. If you want to change them, write a new ADR. Do not change them silently.
- `unsafe` is allowed only in `crates/th-platform`.
- The main thread never does blocking I/O.
- Use the `termhive-platform-check` skill before finishing a change.
- Docs are in Spanish. Code, identifiers and commits are in English.
- Never write to the user's config files (`~/.claude/settings.json`, etc.) without showing the diff and asking for confirmation.
