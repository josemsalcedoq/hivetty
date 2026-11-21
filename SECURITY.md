# Security policy

hivetty runs your shell and your coding agents, so we take security reports seriously.

## Reporting a vulnerability

Please **do not open a public issue**. Report it privately through
[GitHub Security Advisories](https://github.com/josemsalcedoq/hivetty/security/advisories/new).

Include:
- the affected version (`hivetty --doctor`) and OS;
- steps to reproduce, or a proof of concept;
- the impact as you understand it.

You will get an acknowledgement within 7 days. Please allow time for a fix before disclosing publicly.

## Scope

Areas of particular interest:
- Escape-sequence handling: OSC 8, OSC 52, notifications, title reporting.
- Paste injection.
- The local IPC channel and the `hivetty hook` command.
- The hook installer, which edits agent configuration files.
- Parsing of agent logs.
- The update mechanism.

See [`docs/security.md`](docs/security.md) (in Spanish) for the threat model.

## Supported versions

Until 1.0, only the latest release receives security fixes.
