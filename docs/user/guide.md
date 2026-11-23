# hivetty user guide

hivetty is a GPU-accelerated terminal for macOS, Linux and Windows, built for running many coding agents side by side. This guide covers what works today. hivetty is pre-release: build it from source, or take an unsigned test package (`.msi`, `.dmg`, `.deb`, AppImage) from the latest run of the *package* workflow in the project's GitHub Actions. Questions? See the [FAQ](faq.md).

## Install from source

1. Install Rust with [rustup](https://rustup.rs). The repository pins the toolchain version.
2. Install the platform tools:
   - **macOS:** Xcode Command Line Tools (`xcode-select --install`).
   - **Ubuntu:** `sudo apt install build-essential pkg-config libfontconfig1-dev libxkbcommon-dev libwayland-dev libx11-dev libvulkan-dev`.
   - **Windows:** Visual Studio Build Tools with the "Desktop development with C++" workload.
3. Build and run:

```sh
git clone https://github.com/josemsalcedoq/hivetty
cd hivetty
cargo run --release -p hv-app
```

On Windows, also run `cargo xtask conpty-fetch --profile release` once. It downloads Microsoft's current ConPTY next to the executable. hivetty works without it, but the bundled ConPTY passes every escape sequence through untouched and fixes old Windows 10 bugs.

To check your setup, run `hivetty --doctor`.

## The window

- **Sidebar** (left): every session, grouped by project (the git repository it runs in), with its state:
  - grey: running or working;
  - amber: waiting for you;
  - green: done;
  - red: error.

  At the top are a search box and the filters *All*, *Needs you*, *Working* and *Done*. A blue dot marks a session that changed while you were looking elsewhere. On a waiting session, **Peek** (or a right click on any row) shows what it is asking, and you can answer it without leaving your tab. The row of buttons at the bottom is the **dock**: one button per coding agent installed on your machine.
- **Tabs** (top): one per workspace. Inside a tab you can split panes as you like.
  - A spinner means an agent is working, and a thin bar shows a program's progress (OSC 9;4).
  - A blue dot means something happened in that tab while you were looking elsewhere.
  - Double-click a tab to rename it, and drag it to reorder.
  - On Windows and Linux, the ☰ menu lists every action. macOS has the same actions in the menu bar.
- **Panes**: the pane with focus is bright and the others are slightly dimmed. A pane whose agent is waiting for you gets an amber ring.

## Keyboard shortcuts

macOS uses ⌘; Windows and Linux use Ctrl+Shift, so the shortcuts never take Ctrl+letter away from the shell or the agents.

| Action | macOS | Windows / Linux |
|---|---|---|
| New agent session (choose agent, folder, prompt) | ⇧⌘T | Ctrl+Shift+A |
| New tab | ⌘T | Ctrl+Shift+T |
| New window | ⌘N | Ctrl+Shift+N |
| Split right / down | ⌘D / ⇧⌘D | Ctrl+Shift+D / Ctrl+Shift+E |
| Close pane | ⌘W | Ctrl+Shift+W |
| Move focus between panes | ⌥⌘ + arrows | Alt + arrows |
| Resize pane | ⌃⌘ + arrows | Ctrl+Shift+Alt + arrows |
| Maximize or restore pane | ⇧⌘↩ | Ctrl+Shift+Enter |
| Next / previous tab | ⇧⌘] / ⇧⌘[ | Ctrl+Tab / Ctrl+Shift+Tab |
| Go to tab 1–9 | ⌘1…9 | Alt+1…9 |
| Go to the next session that needs you | ⌘J | Ctrl+Shift+J |
| Search sessions (Enter opens the first match, Esc goes back) | ⇧⌘S | Ctrl+Shift+S |
| Command palette | ⇧⌘P | Ctrl+Shift+P |
| Agent usage dashboard | ⌥⌘U | Ctrl+Alt+U |
| Show or hide the sidebar | ⌘B | Ctrl+Shift+B |
| Find in scrollback (regex) | ⌘F | Ctrl+Shift+F |
| Jump to previous / next prompt | ⌘↑ / ⌘↓ | Ctrl+Shift+↑ / ↓ |
| Copy / paste | ⌘C / ⌘V | Ctrl+Shift+C / Ctrl+Shift+V (Ctrl+C also copies when text is selected on Windows) |
| Bigger / smaller / reset font | ⌘= / ⌘- / ⌘0 | Ctrl+= / Ctrl+- / Ctrl+0 |

The command palette lists actions that have no default shortcut too, such as "Copy last command output" and "Move tab to new window". Ctrl+click (⌘-click on macOS) opens URLs, and opens `file:line` references in VS Code when it is installed.

## Configuration

The first time hivetty opens (with no `config.toml` yet) it asks for a theme and whether to use shell integration, and, if Claude Code is installed, offers its hooks and statusline, listing the exact changes to `~/.claude/settings.json` first. Nothing is changed unless you tick it. **Skip** leaves everything as it is; the wizard does not come back.

hivetty reads `config.toml` from:
- Windows: `%APPDATA%\hivetty\config.toml`
- macOS: `~/Library/Application Support/hivetty/config.toml`
- Linux: `~/.config/hivetty/config.toml`

Editors that validate TOML against a JSON schema (Taplo, VS Code's Even Better TOML) can check the file as you type. Add this first line:

```toml
#:schema https://raw.githubusercontent.com/josemsalcedoq/hivetty/main/docs/config.schema.json
```

`hivetty config schema` prints the same schema for the version you have. A mistake in the file shows in a bar over the terminal, and hivetty keeps the last settings that worked.

Changes apply as soon as you save. Errors are logged, and the last good configuration stays in use. Every key is optional. This example shows each key with its default value:

```toml
[font]
# family = "JetBrains Mono"   # default: the first installed of a built-in list
size = 14

[shell]
# program = "pwsh"            # default: pwsh/powershell on Windows, $SHELL elsewhere
args = []
integration = true            # prompt marks and current directory, see below
chips = true                  # project chips above the prompt

[terminal]
scrollback_lines = 10000

[window]
remember_geometry = true
restore_layout = true         # reopen tabs, splits and directories

[usage]
otlp = false                  # Claude Code sessions started in hivetty report lines of code, commits and PRs to it
wsl_distros = []              # Windows: also read agents' logs inside these WSL distributions ("Ubuntu")

[accessibility]
speak = false                 # say out loud when a session needs you or finishes (system voice)

[updates]
check = false                 # true: once a day, ask GitHub for the latest release and say when there is a newer one

[editor]
command = "code"              # what "Open in editor" runs, with the folder last ("zed", "subl -n"…)

[theme]
name = "system"               # "system" follows the OS; "hivetty-dark" or "hivetty-light"
high_contrast = false         # white on black with saturated colors
# Your own palette. `hivetty theme import <file>` converts an iTerm2 (.itermcolors) or
# Alacritty theme into this block. The sidebar follows its background (light or dark).
# [theme.colors]
# foreground = "#abb2bf"
# background = "#282c34"
# cursor = "#ffffff"          # optional
# selection = "#3e4451"       # optional
# ansi = [ ...16 colors: black, red, green, yellow, blue, magenta, cyan, white, then bright... ]

[agents]
restore_on_launch = "ask"     # "ask" types the resume command and waits for Enter; "always"; "never"

[notifications]
enabled = true
only_when_unfocused = true
on = ["waiting", "done", "error"]
sounds = ["waiting", "error"] # these also play the system sound
group_window_seconds = 10
long_command_seconds = 30     # notify when a shell command runs longer than this; 0 = off

# Also send notifications to your phone (off by default). Uses the system's curl.
[notifications.forward]
service = "ntfy"              # "ntfy", "telegram" or "webhook"
url = "https://ntfy.sh/your-private-topic"
# Telegram: url = "https://api.telegram.org/bot<token>/sendMessage" and chat_id = "<your chat id>"
# Webhook: POSTs {"source":"hivetty","title":…,"body":…} as JSON to url

# Change or remove shortcuts. "primary" is ⌘ on macOS and Ctrl elsewhere.
[[keybindings]]
key = "primary+shift+t"
action = "new_tab"

[[keybindings]]
key = "ctrl+shift+d"
action = "none"               # free the shortcut

[[keybindings]]
key = "shift+enter"
action = { send_text = "\u001b\r" }
when = "agent == 'claude-code'"
```

Action names are the ones in the command palette, in snake case (`split_right`, `toggle_usage`, `go_to_tab_3`…). Conditions you can use in `when`:
- `os == 'macos'`, `os == 'windows'`, `os == 'linux'`
- `search_open`
- `terminal_focused`
- `agent == '<id>'`

Prefix any of them with `!` to negate it.

## Shell integration

For zsh, bash, fish and PowerShell, hivetty loads a small script when the shell starts. It never edits your dotfiles: your own configuration still loads first. The script reports:
- where each prompt starts and whether the last command succeeded. A green or red bar appears in the left margin, and you can jump between prompts;
- the current directory. New tabs and splits open in it, and the layout remembers it.

With `chips = true` the shell leaves a blank line above each prompt. hivetty fills it with the project's technology, git branch and pinned tool versions (from `rust-toolchain.toml`, `.nvmrc`, `.python-version`, `go.mod`…). hivetty only reads files to do this; it never runs anything in your project. A chip that does not fit on the line is not drawn.

To turn this off, set `integration = false`.

## Coding agents

hivetty recognizes these agents when they run in a pane: Claude Code, Codex, Gemini CLI, opencode, Copilot CLI, Cursor CLI, Amp and Aider. Grok CLI, Junie and Antigravity have profiles too, but their command names have not been verified yet. `hivetty agents list` shows which of them are installed on your machine.

The state of each session (working, waiting, done) comes from three sources:
1. **OSC 7501**: Claude Code 2.1.295 and later reports its state directly. It works with no setup, even over SSH.
2. **Hooks**: these also report what the agent is doing right now ("Editing src/main.rs"). Install them for Claude Code with:

   ```sh
   hivetty agents install-hooks --dry-run   # see what would change
   hivetty agents install-hooks             # shows the changes and asks before writing
   ```

   For Gemini CLI, add `--agent gemini-cli` (the entries go to `~/.gemini/settings.json`). For Codex, add `--agent codex` (they go to `[hooks]` in `~/.codex/config.toml`, keeping your comments); Codex then asks you once to trust them. Their sessions show their state and current action the same way.

   This adds entries to `~/.claude/settings.json` and keeps a backup next to it. Your own hooks are never touched. `hivetty agents uninstall-hooks` removes only hivetty's entries. The hook command does nothing outside hivetty and always exits immediately.
3. **Notifications and the window title** that the agent sends.

Agents with none of these (Aider, opencode, Cursor CLI, or one whose hooks are not installed) still get a state, guessed from their output: recent output means *working*, and a few seconds of quiet after real work means *done*. Guessed states show with **≈** (for example "≈ Working"). `[agents] waiting_idle_seconds` (5 by default) sets how much quiet counts as done.

hivetty can also be Claude Code's statusline. It prints the model, the context window in use and your plan's limits under Claude Code's prompt (`Opus · ctx 8% · 5h 24% · 7d 41%`), and passes the same numbers to hivetty: the context shows in the session's hover card, and the 5-hour and weekly limits with their reset countdown in the usage dashboard. `hivetty agents install-statusline` sets it up the same way as the hooks (it shows the change and asks). If you already have a statusline, hivetty leaves it alone.

When a session needs you or finishes while you are looking elsewhere, you get a desktop notification. Click it to go to that session (Windows and Linux). Several at once are grouped ("4 sessions need you"). If you close hivetty while agents are running, it offers to resume them the next time it opens (`[agents] restore_on_launch`).

### Agents inside tmux, SSH or WSL

What hivetty can see depends on where the agent runs:

| | State (OSC 7501) | Hooks (current action, permissions) | Guessed state (≈) |
|---|---|---|---|
| In a hivetty pane | yes | yes | yes |
| Inside **tmux** | no: tmux drops it | yes, if tmux passes hivetty's variables (below) | no: the agent runs under the tmux server |
| Over **SSH** | yes: it travels with the output | no: hooks need the local hivetty | no: locally the pane only runs `ssh` |
| Inside **WSL** (Windows) | yes | yes, with hooks that run `hivetty.exe` (below) | no: Windows only sees `wsl.exe` |

**tmux.** Make tmux hand hivetty's variables to new windows, in `~/.tmux.conf`:

```
set -g update-environment "HIVETTY_SESSION_ID HIVETTY_IPC HIVETTY_IPC_TOKEN"
```

tmux copies them when you attach, so the state shows on the hivetty pane where you attached last.

**WSL.** hivetty adds its variables to `WSLENV`, so a `wsl` started in a pane receives them. Inside WSL, install the agent's hooks so that they run the Windows binary, for example in `~/.claude/settings.json` of your WSL home:

```json
"command": "'/mnt/c/Program Files/hivetty/hivetty.exe' hook --agent claude-code --event Stop"
```

(one entry per event, as `hivetty agents install-hooks --dry-run` lists them on Windows).

## Agent usage dashboard

Press Ctrl+Alt+U (⌥⌘U) to open it. Choose 7 days, 30 days, 90 days or All. Press `esc` to close it. The dashboard shows:
- input tokens (including cache), output tokens, tasks and agent time, each compared with the previous period of the same length;
- tokens by day;
- a heatmap of when your agents work;
- your models and agents. Click an agent to see only its usage.

Everything comes from the logs your agents already write on your machine: Claude Code for now, in `~/.claude/projects`. Nothing is uploaded anywhere. hivetty keeps a small cache, so it reads only the new lines each time.

To see an estimated cost, write your own price table in `pricing.toml`, next to `config.toml`. hivetty ships no prices because they change. Prices are in USD per million tokens; a key matches every model whose name starts with it, and the longest match wins. Cache tokens without their own price are charged as input.

```toml
[models."claude-opus"]
input = 15.0
output = 75.0
cache_read = 1.5
cache_write = 18.75
```

The input card then shows "≈ $X API equivalent" (and says so when some models have no price), the models table shows each model's cost, and `hivetty usage` adds a cost column.

The same data is available on the command line:

```sh
hivetty usage                          # table with totals
hivetty usage --days 7 --project api   # last week, one project
hivetty usage --csv > usage.csv        # or --json
```

## Privacy

hivetty does not send telemetry, and it only goes online for what you turn on (notification forwarding, the update check). It reads your agents' local logs to build the usage dashboard, and the files of your current project to draw the chips. It talks to local processes only through a private socket (a named pipe on Windows) that requires a random per-launch token. It never writes your agents' configuration without showing you the change and asking first.

## Troubleshooting

- `hivetty --doctor` prints the version, the GPU, the ConPTY in use and the configuration status.
- Set `HIVETTY_LOG=debug` to see what hivetty is doing (on Windows, run it from a terminal to see the log).
- If no session state appears for Claude Code, check that it is version 2.1.295 or later, or install the hooks.
- To start from scratch, delete `layout.json` and `usage.cache` from the data directory:
  - Windows: `%LOCALAPPDATA%\hivetty`
  - macOS: `~/Library/Application Support/hivetty`
  - Linux: `~/.local/share/hivetty`
