# hivetty FAQ

Short answers to common questions. The [user guide](guide.md) has the details.

## General

**What is hivetty?**
A terminal for running many coding agents side by side. It is a regular GPU-accelerated terminal, plus a sidebar that shows what each agent session is doing (working, waiting for you, done) and a dashboard of how many tokens your agents used.

**Which agents does it support?**
Any program runs in it. hivetty knows Claude Code, Codex, Gemini CLI, opencode, Copilot CLI, Cursor CLI, Amp and Aider by name (`hivetty agents list`). Claude Code, Codex and Gemini CLI report their exact state through hooks. The others get a state guessed from their output, marked with **≈**.

**Does hivetty send my data anywhere?**
No. There is no telemetry. The usage dashboard reads the logs your agents already keep on your machine. hivetty only talks to the network if you turn on forwarding notifications to your phone (`[notifications.forward]`, only to the URL you configure) or the update check (`[updates] check`, once a day to GitHub's release list; nothing is downloaded). `[usage] otlp` only listens on your own machine.

## Agents and the sidebar

**A session says "Running" and never changes.**
hivetty knows a program is running there but not what it is doing. Install the agent's hooks (`hivetty agents install-hooks`, add `--agent codex` or `--agent gemini-cli` for those). For other agents the state is guessed from their output once hivetty recognizes the agent.

**What does "≈" mean?**
The state is a guess from the agent's output: recent output means *working*, and a few seconds of quiet after real work means *done*. `[agents] waiting_idle_seconds` sets how long the quiet is. Agents that report their state never show it.

**Is installing hooks safe? What do they change?**
`install-hooks` shows every line it adds to the agent's settings and asks before writing. It keeps a backup next to the file (`settings.json.hivetty-bak`, `config.toml.hivetty-bak`) and never touches your own hooks. The hook command only forwards the event to hivetty: outside hivetty it does nothing, and it always exits immediately. `hivetty agents uninstall-hooks` removes exactly what was added.

**Codex asks me to trust hooks.**
That is Codex's own safety check for new hooks. Approve hivetty's once.

**Does it work in tmux, over SSH or in WSL?**
Partly; see "Agents inside tmux, SSH or WSL" in the guide. Over SSH, Claude Code still reports its state; hooks need hivetty on the same machine.

## Usage dashboard

**Why does the dashboard show no cost?**
hivetty ships no prices, because they change and a wrong number would be worse than none. Write your own `pricing.toml` next to `config.toml` (the guide has an example).

**The numbers differ from my bill.**
They are tokens from the agents' local logs, the same ones tools like `ccusage` read. Plans with a monthly fee are not billed per token, so the cost is an "API equivalent", not what you pay.

**Can I export the data?**
`hivetty usage --csv` or `--json`, optionally with `--days N` and `--project NAME`.

## Files and settings

**Where are my settings and data?**

| | Settings (`config.toml`) | Data (layout, caches, crash reports) |
|---|---|---|
| Windows | `%APPDATA%\hivetty` | `%LOCALAPPDATA%\hivetty` |
| macOS | `~/Library/Application Support/hivetty` | `~/Library/Application Support/hivetty` |
| Linux | `~/.config/hivetty` | `~/.local/share/hivetty` |

`HIVETTY_CONFIG` and `HIVETTY_DATA_DIR` override them. `hivetty --doctor` prints both.

**I made a mistake in `config.toml`.**
A bar over the terminal tells you what is wrong, and hivetty keeps the last settings that worked until you fix it. Editors with TOML schema support can check the file as you type (see the guide).

**How do I see the first-run wizard again?**
Delete `config.toml` and the file named `welcomed` in the data folder.

**How do I make hivetty look like my old terminal?**
`hivetty theme import <file>` converts an iTerm2 (`.itermcolors`) or Alacritty theme into a `[theme.colors]` block for `config.toml`. `[theme] high_contrast = true` gives white on black with saturated colors.

## Troubleshooting

**hivetty does not start, or shows a black window.**
Start it with `hivetty --safe`: default settings, no shell integration and the software renderer. If that works, the problem is the GPU driver or the config. `hivetty --doctor` shows which GPU is used.

**Windows says the app is unrecognized.**
Test builds are not signed yet. In the SmartScreen dialog choose *More info*, then *Run anyway*.

**My shell prints an error about running scripts (Windows PowerShell).**
hivetty's PowerShell integration works with the default "Restricted" policy. If you still see the error, set `[shell] integration = false` and tell us your PowerShell version.

**hivetty crashed.**
The newest `crash-*.txt` in the data folder has the details (no terminal content). `hivetty --doctor` shows its path; please attach it to the report.
