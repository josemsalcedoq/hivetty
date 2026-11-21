# Integración con agentes

> Las rutas y formatos de logs de esta página son **los conocidos a fecha de escritura (nov 2026)** y cambian entre versiones. Verificar contra la versión instalada antes de implementar cada parser y guardar fixtures por versión.

## 1. Perfil de agente (TOML)

```toml
[[agent]]
id = "claude-code"
name = "Claude Code"
provider = "anthropic"
command = "claude"
icon = "claude.svg"
process_names = ["claude", "claude.exe"]
# Cómo leer el uso
usage.format = "claude-jsonl"
usage.paths = ["~/.claude/projects/**/*.jsonl"]
# Cómo obtener el estado
state.hooks = "claude-settings"   # instalador de hooks soportado
state.title_spinner = true        # el agente pone un spinner en el título (OSC 2)
```

## 2. Estado en vivo

Tres fuentes, de más fiable a menos. La de mayor prioridad gana durante 10 s.

### 2.1 Hooks del agente (preferida)

El PTY lleva `TERMHIVE_SESSION_ID` y `TERMHIVE_IPC` (ruta del socket o named pipe) en el entorno. El hook ejecuta:

```
termhive hook --event <evento>   # lee el JSON del hook por stdin y lo reenvía por IPC
```

Mapa para **Claude Code** (`~/.claude/settings.json` → `hooks`):

| Hook | Estado |
|---|---|
| `SessionStart` | `Starting` |
| `UserPromptSubmit` | `Working` (+ título = primeras palabras del prompt si no hay) |
| `PreToolUse` / `PostToolUse` | `Working` + subtítulo ("Editing src/foo.rs", "Running cargo test") |
| `Notification` | `Waiting` (pide permiso o lleva tiempo inactivo) |
| `Stop` | `Done` |
| `SessionEnd` | `Exited` |

Otros agentes: revisar su sistema de hooks/notify (Codex tiene `notify` en su config; Gemini CLI y otros, verificar). Sin hooks → fuentes 2 y 3.

El instalador (`termhive agents install-hooks`) **añade** entradas sin tocar las existentes, enseña el diff y pide confirmación. Desinstalar quita solo lo que se añadió (las entradas llevan un marcador).

### 2.2 Secuencias del terminal

- OSC 9 / OSC 777 `notify` → `Waiting` + notificación.
- OSC 0/2 (título): muchos agentes ponen un spinner o un estado en el título. Si el título cambia con un spinner → `Working`.
- BEL → `Waiting` si la pestaña no tiene foco.

### 2.3 Heurística

- Hay salida en los últimos 2 s → `Working`.
- Sin salida 5 s y el proceso del agente sigue vivo → `Waiting`.
- Proceso terminado → `Exited` (código ≠ 0 → `Error`).

## 3. Uso (tokens, tareas y tiempo)

| Agente | Ubicación típica | Formato |
|---|---|---|
| Claude Code | `~/.claude/projects/<proyecto>/<sesión>.jsonl` | JSONL; los mensajes del asistente llevan `message.usage` (`input_tokens`, `cache_read_input_tokens`, `cache_creation_input_tokens`, `output_tokens`) y `message.model` |
| Codex | `~/.codex/sessions/AAAA/MM/DD/*.jsonl` | JSONL con eventos de uso de tokens |
| Gemini CLI | `~/.gemini/tmp/<hash>/` | JSON por sesión |
| Otros | por investigar | — |

Notas:
- En Windows `~` es `%USERPROFILE%`. Algunas apps usan `%APPDATA%`; el perfil puede declarar varias rutas.
- **Deduplicar** por id de mensaje: Claude Code puede escribir el mismo mensaje varias veces (streaming y reintentos).
- Usar [`ccusage`](https://github.com/ryoppippi/ccusage) como referencia para validar los totales.
- El proyecto se deduce del `cwd` registrado en el log o del nombre de carpeta.

### Esquema SQLite

```sql
CREATE TABLE usage_event (
  id            TEXT PRIMARY KEY,      -- agente + id de mensaje (dedupe)
  ts            INTEGER NOT NULL,      -- unix ms UTC
  agent         TEXT NOT NULL,
  provider      TEXT NOT NULL,
  model         TEXT,
  session_id    TEXT,
  project       TEXT,
  input_tokens  INTEGER DEFAULT 0,
  cache_read    INTEGER DEFAULT 0,
  cache_write   INTEGER DEFAULT 0,
  output_tokens INTEGER DEFAULT 0,
  kind          TEXT NOT NULL          -- 'turn' | 'task'
);
CREATE INDEX usage_ts ON usage_event(ts);

CREATE TABLE agent_interval (           -- de los hooks: tiempo real trabajando
  session_id TEXT, agent TEXT, start_ts INTEGER, end_ts INTEGER
);

CREATE TABLE ingest_cursor (path TEXT PRIMARY KEY, offset INTEGER, mtime INTEGER);
```
