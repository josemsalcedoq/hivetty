# Integración con agentes

> **Nivel de verificación.** ✔ = confirmado en documentación oficial o en el código (nov 2026). ◐ = de fuentes secundarias. ✗ = pendiente de verificar contra la versión instalada (en SP-4, SP-5 o F5-03). **Ningún parser se implementa sin fixtures reales de la versión soportada.**

## 1. Perfil de agente (TOML, `assets/agents/*.toml`)

```toml
id = "claude-code"
name = "Claude Code"
provider = "anthropic"
command = "claude"
process_names = ["claude", "claude.exe", "node"]   # node + argv que contenga claude
icon = "claude.svg"

[state]
osc7501 = true                    # emite OSC 7501 (≥ 2.1.295)
hooks = "claude-settings"         # instalador soportado
title_from_osc = true             # su título (OSC 2) es el resumen de la tarea, p. ej. "✳ Arreglar logos"

[keys]
shift_enter = "\u001b\r"
image_paste = { macos = "ctrl+v", linux = "ctrl+v", windows = "alt+v" }

[resume]
command = "claude --resume {session_id}"

[usage]
format = "claude-jsonl"
paths = ["~/.claude/projects/**/*.jsonl"]
```

Añadir un agente nuevo = un archivo TOML + (si tiene logs propios) un parser con fixtures. Skill propia `termhive-agent-hooks` (ver `docs/skills.md`).

## 2. Estado en vivo

### 2.1 Modelo de estados

```
Starting ─► Working ◄──► Blocked(kind) ─► …
              │   ▲
              ▼   │
             Idle/Done ──► (no leído hasta que se mira)
              │
              ▼
        Error · Exited
```

- `Blocked.kind` ∈ { `permission`, `question`, `auth` } (de OSC 7501). En la UI se muestra como **Waiting** más el motivo ("Necesita permiso: Bash(cargo test)").
- **`Done` persiste como no leído** hasta que el usuario enfoca la sesión. Mientras tanto, la fila y la pestaña llevan el punto.
- El estado se **agrega**: panel → pestaña → proyecto → icono de la app (badge = sesiones Waiting más Done sin leer).

### 2.2 Fuentes, por prioridad

Una fuente de más prioridad manda mientras haya emitido algo en los últimos 30 s. Las de menos prioridad solo rellenan huecos.

| # | Fuente | Fiabilidad | Funciona por SSH/WSL | Detalle |
|---|---|---|---|---|
| 1 | **OSC 7501** (Program Status Protocol, borrador rev 0.4) | alta | ✔ (viaja por el terminal; **no** dentro de tmux, que lo descarta) | `ESC]7501;state=blocked:kind=permission:msg=<b64>ST`. Estados `idle`, `working` (con `progress` 0–100), `blocked`, `done`, `error` y `clear`; varios `id` por panel; hay que **responder a la consulta `?`**. Emisores ✔: Claude Code ≥ 2.1.295, Pi y Crush; Codex en main ◐ |
| 2 | **Hooks** del agente → `termhive hook` → IPC | alta | WSL ✔ (vía `termhive.exe`); SSH ✗ | detalles en §2.3. Aportan lo que OSC 7501 no da: la acción actual (subtítulo), el id de sesión, la ruta de la transcripción y el modelo |
| 3 | OSC 9 / 99 / 777 (notificaciones), OSC 9;4 (progreso) | media | ✔ | la notificación pasa a `Blocked` sin motivo; no distingue "done" de "blocked" |
| 4 | Título (OSC 0/2) | media | ✔ | un spinner o cambio en el título → `Working` |
| 5 | BEL | baja | ✔ | sin foco → `Blocked` probable |
| 6 | Heurística de salida y proceso | baja | ✔ | salida en los últimos 2 s → `Working`; 5 s sin salida con el proceso vivo → `Idle?`; proceso terminado → `Exited`/`Error`. La fila lleva el badge "≈" |

Opcional (C): reglas de pantalla por agente (como hace Herdr), con un manifiesto actualizable que reconoce prompts de permiso conocidos. Siempre se etiquetan como estimación.

### 2.3 Hooks por agente

**Claude Code** ✔ (`~/.claude/settings.json` → `hooks`; también `.claude/settings.json` del proyecto)

| Evento | → estado / dato |
|---|---|
| `SessionStart` | `Starting`; guarda `session_id`, `transcript_path`, `cwd` y el modelo si viene |
| `UserPromptSubmit` | `Working`; nueva **tarea** (+1); título provisional si no hay OSC 2 |
| `PreToolUse` | `Working`; subtítulo desde `tool_input` (`description` en Bash → "Converting screenshot to PNG"; `file_path` en Write/Edit → "Writing crates/…/vision.rs") |
| `PostToolUse` | `Working` |
| `Notification` | `Blocked` (permiso o inactividad); mensaje → motivo |
| `Stop` | `Done` (fin de la tarea) |
| `SubagentStop` | subtarea terminada (solo subtítulo) |
| `SessionEnd` | `Exited` |
| `agent_needs_input` / `agent_completed` ◐ | del supervisor de "agent view" (`claude agents`); equivalentes a `Blocked` / `Done` |

Problemas conocidos ◐: el aviso de inactividad llega a los ~60 s y el de permiso a los ~6 s; `AskUserQuestion` no disparaba hook en algunas versiones. **Por eso OSC 7501 va por delante.**

Payload común por stdin ✔: `session_id`, `transcript_path`, `cwd`, `hook_event_name` y, según el evento, `tool_name`, `tool_input`, `message` y `prompt`. Comprobar los campos exactos en la versión instalada (SP-4) y guardar ejemplos en `tests/fixtures/hooks/claude/<versión>/`.

**Codex CLI** ◐: hooks propios o la clave `notify` de `~/.codex/config.toml`, que ejecuta un comando con un JSON al terminar un turno. Verificar el nombre de los eventos y el payload ✗.

**Gemini CLI** ◐: eventos `AfterAgent` y `Notification` (sistema de hooks en `~/.gemini/settings.json`) ✗.

**opencode** ◐: sistema de plugins y eventos (Rex y Warp lo integran) ✗.

**Grok CLI, Junie, Antigravity, Copilot CLI, Cursor CLI, Amp y Aider**: ✗. Hasta verificarlos, usar las fuentes 1, 3, 4, 5 y 6.

### 2.4 Instalador de hooks

`termhive agents install-hooks [--agent <id>] [--dry-run]`:
1. Lee la configuración actual del agente. Si no existe, propone crearla.
2. Calcula las entradas que faltan (cada una con el marcador `termhive`).
3. **Muestra el diff y pide confirmación.** Nunca toca las entradas del usuario.
4. Hace una copia `*.termhive-bak` y escribe de forma atómica (archivo temporal + renombrar).
5. `uninstall-hooks` quita solo las entradas con el marcador.
6. `--doctor` comprueba que siguen instalados y que apuntan al binario actual (las rutas cambian al actualizar en Windows: usar un *shim* estable, `%LOCALAPPDATA%\termhive\bin\termhive.exe`).

Contrato de `termhive hook --agent <id> --event <evento>`:
- Lee stdin (máximo 1 MiB), añade `TERMHIVE_SESSION_ID` y lo envía por IPC con el token.
- **Siempre sale con 0 en menos de 1 s.** Si no hay IPC (agente fuera de termhive), no hace nada.
- Sin stdout (algunos agentes interpretan la salida de los hooks).

### 2.5 Casos especiales
- **tmux** dentro de termhive: tmux descarta OSC 7501 y, sin `allow-passthrough on`, también las notificaciones. Los hooks siguen funcionando si el entorno conserva `TERMHIVE_*`: documentar `set -g update-environment "TERMHIVE_SESSION_ID TERMHIVE_IPC TERMHIVE_IPC_TOKEN"`.
- **SSH:** OSC 7501, notificaciones y título sí llegan; los hooks no. La sesión queda con estado de fuentes 1 y 3–6.
- **WSL:** OSC 7501 sí. Los hooks funcionan si se instalan dentro de WSL apuntando a `termhive.exe` (interoperabilidad) y se reenvían las variables con `WSLENV=TERMHIVE_SESSION_ID/u:TERMHIVE_IPC/u:TERMHIVE_IPC_TOKEN/u`.
- **Agente lanzado a mano** en un panel: se detecta por el árbol de procesos (`sysinfo`, cada 1 s). Se crea una sesión "observada" y, si emite OSC 7501, el estado es fiable sin hooks.

## 3. Uso (tokens, tareas y tiempo)

Ver ADR 0005: logs = verdad para los tokens; tiempo de agente = intervalos observados por termhive.

### 3.1 Fuentes por agente

| Agente | Ubicación típica | Formato y campos | Verif. |
|---|---|---|---|
| Claude Code | `~/.claude/projects/<proyecto-codificado>/<session>.jsonl` (y subagentes) | JSONL; los mensajes del asistente llevan `message.model`, `message.usage.{input_tokens, output_tokens, cache_read_input_tokens, cache_creation_input_tokens}`, `message.id`, `requestId`, `timestamp`, `sessionId`, `cwd`, `isSidechain` | ◐ (confirmar con SP-5 frente a `ccusage`) |
| Codex CLI | `~/.codex/sessions/AAAA/MM/DD/rollout-*.jsonl` | JSONL con eventos de recuento de tokens | ✗ |
| Gemini CLI | `~/.gemini/tmp/<hash>/…` | JSON por sesión | ✗ |
| opencode | almacenamiento local propio (JSON o SQLite) | — | ✗ |
| Resto | — | — | ✗ |

Referencias para implementarlos: **[ccusage](https://github.com/ryoppippi/ccusage)** (más de 20 agentes, dedupe y precios de LiteLLM) y **[tokscale](https://github.com/junhoyeo/tokscale)** (núcleo en Rust, más de 60 agentes, tokens de razonamiento). Leer sus parsers antes de escribir los nuestros (respetando sus licencias).

Notas:
- **Dedupe** por `message.id` + `requestId` (Claude repite líneas por streaming y reintentos). Seguir exactamente el criterio de `ccusage` para que los totales cuadren.
- En Windows, `~` = `%USERPROFILE%`, y además las distros WSL (`\\wsl.localhost\<distro>\home\*\…`).
- El proyecto sale del `cwd` del log; si no viene, se decodifica del nombre de la carpeta.
- **Tokens de razonamiento**: guardarlos aparte si el agente los informa (Codex, Gemini).

### 3.2 OpenTelemetry (opcional, F5-15)
Claude Code exporta por OTel ✔: `claude_code.token.usage` (por tipo y modelo), `cost.usage`, **`active_time.total`** (distingue usuario y CLI), `session.count`, líneas de código, commits y PRs. termhive puede levantar un receptor OTLP local (`127.0.0.1`, puerto aleatorio) y pasar `CLAUDE_CODE_ENABLE_TELEMETRY=1` y `OTEL_*` **solo** a las sesiones que lanza (nunca tocar la configuración global). Así hay datos al instante; los logs siguen siendo la verdad.

### 3.3 Límites del plan (5 h y semana) (S)
- La statusline de Claude Code recibe por stdin un JSON que incluye `rate_limits` ◐ (lo usa Claude-Code-Usage-Monitor). termhive puede ofrecer una statusline propia que, además de pintar, reenvíe ese JSON por IPC → barra "week ▬ 15% ↻ 5d 11h" en la UI.
- Mostrar el ritmo de gasto y la hora estimada de agotamiento, como ccusage `blocks --live`.

### 3.4 Esquema SQLite

```sql
CREATE TABLE usage_event (
  id              TEXT PRIMARY KEY,      -- agente + message.id + requestId (dedupe)
  ts              INTEGER NOT NULL,      -- unix ms UTC
  agent           TEXT NOT NULL,
  provider        TEXT NOT NULL,
  model           TEXT,
  session_id      TEXT,
  project         TEXT,
  input_tokens    INTEGER NOT NULL DEFAULT 0,
  cache_read      INTEGER NOT NULL DEFAULT 0,
  cache_write     INTEGER NOT NULL DEFAULT 0,
  output_tokens   INTEGER NOT NULL DEFAULT 0,
  reasoning_tokens INTEGER NOT NULL DEFAULT 0,
  kind            TEXT NOT NULL          -- 'turn' | 'task'
);
CREATE INDEX usage_ts ON usage_event(ts);
CREATE INDEX usage_agent_ts ON usage_event(agent, ts);

CREATE TABLE session (
  session_id TEXT PRIMARY KEY, agent TEXT, project TEXT, title TEXT,
  started_ts INTEGER, ended_ts INTEGER, archived INTEGER DEFAULT 0,
  resume_cmd TEXT, worktree TEXT
);

CREATE TABLE agent_interval (            -- tiempo Working observado por termhive
  session_id TEXT NOT NULL, agent TEXT NOT NULL,
  start_ts INTEGER NOT NULL, end_ts INTEGER NOT NULL,
  source TEXT NOT NULL                   -- 'osc7501' | 'hooks' | 'heuristic' | 'logs'
);

CREATE TABLE ingest_cursor (path TEXT PRIMARY KEY, offset INTEGER, mtime INTEGER, parser_version INTEGER);
```

- SQLite en modo WAL. Escribe un único hilo; la UI lee con conexiones de solo lectura.
- Migraciones versionadas (`rusqlite_migration`). Cambiar la versión de un parser → reingestar sus archivos.

## 4. API para automatizar (F7-08, opcional)
CLI y socket con salida JSON, desactivados por defecto (`remote_control = false`): `termhive cli list`, `spawn`, `split`, `send-text`, `capture`, `notify`, `set-status` y `wait --until blocked|done <sesión>`. Opcionalmente, servidor MCP con las mismas operaciones (de solo lectura por defecto).
