# Configuración

- Archivo: `config.toml` en el directorio de configuración (`~/Library/Application Support/hivetty/` en macOS, `$XDG_CONFIG_HOME/hivetty/` en Linux y `%APPDATA%\hivetty\` en Windows). Se puede cambiar con `--config <ruta>` o `HIVETTY_CONFIG`.
- **Recarga en caliente**: los cambios se aplican sin reiniciar, salvo los marcados con ⟳ (necesitan reinicio).
- Un error de configuración nunca impide arrancar: se usa el valor por defecto y se muestra un aviso con la línea del error.
- Se puede incluir otro archivo con `import = ["themes/mi-tema.toml"]`.
- El esquema JSON se genera desde el código (`schemars`) y se publica para autocompletado en editores (Taplo / Even Better TOML).

## Ejemplo completo con los valores por defecto

```toml
import = []

[general]
confirm_quit_with_running = true
check_updates = true           # ⟳
telemetry = false              # nunca se envía nada sin opt-in

[window]
startup_size = { cols = 160, rows = 45 }
remember_geometry = true
restore_layout = true          # reabrir pestañas, divisiones y carpetas de la última vez
decorations = "auto"           # "auto" | "full" | "none"
opacity = 1.0
blur = false                   # macOS / Windows 11
padding = { x = 8, y = 6 }
inactive_pane_dim = 0.85
macos_titlebar = "transparent" # "transparent" | "native"
windows_backdrop = "none"      # "none" | "mica" | "acrylic"

[font]
family = "JetBrains Mono"
fallback = ["Symbols Nerd Font Mono", "Noto Color Emoji", "Apple Color Emoji", "Segoe UI Emoji"]
size = 13.0
line_height = 1.15
ligatures = false
features = []                  # p. ej. ["ss01", "zero"]
builtin_box_drawing = true     # dibujar ─│┌ y powerline por código, alineados al píxel
thicken = false                # macOS: trazo más grueso

[theme]
name = "system"               # "system" (sigue al SO), "hivetty-dark", "hivetty-light"
high_contrast = false          # blanco sobre negro con colores saturados (gana a colors)
# [theme.colors]: foreground, background, cursor?, selection?, ansi (16) en #rrggbb;
# `hivetty theme import <archivo>` lo genera desde .itermcolors o un tema TOML de Alacritty
follow_system = true
light = "hivetty-light"
dark = "hivetty-dark"
minimum_contrast = 1.0         # >1 ajusta colores para garantizar el contraste

[terminal]
scrollback_lines = 10000
scroll_multiplier = 3
keep_scroll_position_on_output = true   # no saltar al final si estás leyendo arriba
cursor = { style = "block", blink = true, blink_interval_ms = 600 }
bell = { visual = true, sound = false, attention = true }
copy_on_select = false
trim_trailing_spaces_on_copy = true
word_separators = " ,│`|:\"'()[]{}<>\t"
unicode_width = "auto"         # "auto" (modo 2027 si el programa lo pide) | "wcwidth" | "grapheme"
osc52 = "write"                # "off" | "write" | "write-confirm" (leer nunca se permite)
osc52_max_bytes = 1048576
hyperlinks = true
url_detection = true
path_detection = true          # rutas archivo:línea:col
editor = "auto"                # "auto" ($VISUAL/$EDITOR, VS Code si existe) | comando con {file} {line} {col}
paste_warn_multiline = "shell" # "never" | "shell" (solo si no hay un agente) | "always"
paste_strip_escape = true
title_reporting = false        # CSI 21t: desactivado por seguridad
synchronized_output_timeout_ms = 150

[shell]
program = "auto"               # auto = $SHELL / pwsh / powershell / cmd
args = []
login = true                   # macOS: shell de login para cargar el PATH completo
integration = true
working_directory = "inherit"  # "inherit" (cwd del panel con foco) | "home" | ruta
env = { }

# Perfiles extra (aparecen en el menú de nueva pestaña)
[[profiles]]
name = "Ubuntu (WSL)"
program = "wsl.exe"
args = ["-d", "Ubuntu", "--cd", "~"]
os = "windows"

[[profiles]]
name = "Git Bash"
program = "C:\\Program Files\\Git\\bin\\bash.exe"
args = ["--login", "-i"]
os = "windows"

[sidebar]
visible = true
width = 300
group_by = "project"           # "project" | "agent" | "none"
sort = "attention"             # "attention" (Waiting>Working>resto) | "recent" | "name"
show_shell_sessions = false
show_approximate_badge = true

[notifications]
enabled = true
on = ["waiting", "done", "error"]
only_when_unfocused = true
sound = { waiting = "default", done = "none", error = "default" }
group_window_seconds = 10
long_command_seconds = 30      # notificar comandos de shell que tardan más que esto

[notifications.forward]          # reenviar al móvil (F4-33); apagado por defecto; usa curl
service = ""                     # "ntfy", "telegram" o "webhook"
url = ""                         # tema de ntfy, sendMessage de Telegram o URL del webhook
chat_id = ""                     # solo Telegram

[agents]
auto_detect = true
detect_interval_ms = 1000
waiting_idle_seconds = 5       # heurística sin hooks
default = "claude-code"
restore_on_launch = "ask"      # "ask" | "always" | "never" (reanudar sesiones)

# Sobrescribir o añadir perfiles de agente (ver docs/agent-integration.md)
[[agents.custom]]
id = "mi-agente"
name = "Mi agente"
command = "mi-agente"
process_names = ["mi-agente"]
icon = "generic"
resume = ""                    # p. ej. "mi-agente --resume {session_id}"

[agents.worktrees]
enabled = false
base_dir = "../{repo}-worktrees"
branch_prefix = "agent/"
remove_on_archive = "ask"      # "ask" | "never"

[usage]
enabled = true
sources = "auto"               # detectar las rutas de cada agente
extra_paths = []
include_wsl = true             # Windows: escanear \\wsl$\<distro>\home\*
exclude_projects = []          # rutas o globs que no se cuentan
timezone = "system"
week_starts_on = "monday"
number_format = "compact"      # "compact" (11.6B) | "locale"
heatmap_metric = "agent_time"  # "agent_time" | "tokens"
show_cost = false
pricing_file = "pricing.toml"  # editable; precios por modelo y millón de tokens
retention_days = 0             # 0 = para siempre

[narrator]
enabled = false
voice = "system"
events = ["waiting", "done"]

[renderer]
backend = "auto"               # "auto" | "metal" | "vulkan" | "dx12" | "gl" | "software"  ⟳
vsync = true
max_fps = 120
low_power_gpu = true           # preferir la iGPU en portátiles  ⟳

[ipc]
enabled = true                 # ⟳
remote_control = false         # permitir `hivetty cli send-text` y similares
allow_hooks = true

[log]
level = "warn"                 # "error" | "warn" | "info" | "debug" | "trace"
record_pty = false             # ¡captura todo lo que pasa por el terminal! solo para depurar

[update]
channel = "stable"             # "stable" | "beta"
```

## Variables de entorno

| Variable | Efecto |
|---|---|
| `HIVETTY_CONFIG` | ruta del archivo de configuración |
| `HIVETTY_LOG` | nivel de log (sintaxis de `tracing`, p. ej. `hv_agents=debug`) |
| `HIVETTY_RENDERER` | fuerza el backend (`software` para diagnosticar) |
| `HIVETTY_SAFE=1` | igual que `--safe` |

Variables que **hivetty pone** en cada PTY: `TERM`, `COLORTERM`, `TERM_PROGRAM=hivetty`, `TERM_PROGRAM_VERSION`, `HIVETTY_SESSION_ID`, `HIVETTY_IPC` y `HIVETTY_IPC_TOKEN`.
