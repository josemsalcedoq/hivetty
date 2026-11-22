# SP-4: hook del agente → `hivetty hook` → IPC → cambio de estado

**Estado (2026-11-21):** ✅ en Windows 11, con el CI comprobando macOS y Ubuntu. Falta probar la invocación desde WSL: en este equipo solo está la distro `docker-desktop`.

## Qué se construyó
- `hv-ipc`: socket Unix (directorio `0700` del propio usuario y socket `0600`) o named pipe (DACL solo para el dueño, nombre con 64 bits aleatorios, clientes remotos rechazados). Token de 128 bits por arranque comparado en tiempo constante. Tramas `u32` + `postcard` con versión de protocolo y límite de 1 MiB. Un hilo por conexión.
- `hivetty hook --agent <id> --event <evento>`: lee stdin (máx. 1 MiB) y lo reenvía. Todo lo que puede bloquear va en un hilo auxiliar. El principal espera como mucho 500 ms y sale con `process::exit(0)`. Nunca escribe en stdout.
- `hv-agents`: modelo de estados, regla de prioridad entre fuentes (una fuente manda mientras ninguna de más prioridad haya hablado en 30 s), intérprete de hooks de Claude Code y de OSC 7501.
- La app pone `HIVETTY_IPC`, `HIVETTY_IPC_TOKEN` y `HIVETTY_SESSION_ID` en el entorno del panel, y la fila de la barra lateral muestra el estado y la acción actual ("Echo hook test string", "Needs permission: Bash(cargo test)").

## Payloads reales
Capturados de Claude Code 2.1.330 con hooks en un `.claude/settings.json` de una carpeta temporal (sin tocar la configuración global), anonimizados en `tests/fixtures/hooks/claude/2.1.330/`: `SessionStart`, `UserPromptSubmit`, `PreToolUse`, `PostToolUse`, `Stop` y `SessionEnd`.
- Campos comunes: `session_id`, `transcript_path`, `cwd`, `hook_event_name`, y en la mayoría `prompt_id`, `permission_mode` y `effort`.
- En Windows la herramienta de shell se llama `PowerShell` (no `Bash`) y trae `description`.
- `Stop` trae `last_assistant_message`; `SessionStart` trae `source` (`startup`); `SessionEnd` trae `reason`.
- `Notification` solo salta en modo interactivo; su test usa el formato documentado (`message`, `notification_type`). Falta capturarlo de verdad.

## Latencia (N-08: ≤ 300 ms)
Desde lanzar el proceso `hivetty hook` hasta que el mensaje llega al servidor (`crates/hv-app/tests/hook.rs`, 5 muestras):

| SO | Mediana | Notas |
|---|---|---|
| Windows 11, binario debug | **10–13 ms** | el primer arranque de un binario recién compilado tarda ~0,5 s (análisis del antivirus); los hooks reales usan un binario ya conocido |
| macOS / Ubuntu (CI) | < 300 ms | el test falla si la mediana pasa de 300 ms |

El contrato también tiene tests: sin hivetty no hace nada y sale con 0; con stdin abierto y sin nadie escuchando sale con 0 en menos de 1 s; nunca escribe en stdout.

## Revisión de seguridad
Sin hallazgos explotables. Endurecimiento aplicado después: comprobar que el directorio del socket pertenece al usuario (`geteuid` en `hv-platform`).

## Pendiente
- WSL: instalar una distro y probar `hivetty.exe hook` con `WSLENV` (docs/agent-integration.md 2.5).
- Responder a la consulta `?` de OSC 7501 (formato de respuesta por confirmar en el borrador del protocolo).
- Instalador de hooks (F4-07) y shim estable en `%LOCALAPPDATA%\hivetty\bin`.
