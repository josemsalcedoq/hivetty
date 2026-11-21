# SP-5 — Parser de uso de Claude Code frente a ccusage

- **Fecha:** 2026-11-21
- **Estado:** ✅ superado

## Método
- `hv-usage::claude` lee `~/.claude/projects/**/*.jsonl`. Toma las líneas `type: "assistant"` con `message.usage`, ignora el modelo `<synthetic>` y hace **dedupe por `message.id:requestId`** entre todos los archivos.
- `hivetty usage --json` frente a `npx ccusage@latest daily --json --offline`, sobre los datos reales de este PC (36 transcripciones, 91 MB).
- Se excluye el día en curso, porque la sesión activa sigue escribiendo mientras se comparan.

## Resultado
| Campo | ccusage | hivetty | Diferencia |
|---|---|---|---|
| input | 3,834 | 3,834 | 0.0000 % |
| output | 1,448,131 | 1,448,131 | 0.0000 % |
| cache read | 313,142,120 | 313,142,120 | 0.0000 % |
| cache write | 4,962,543 | 4,962,543 | 0.0000 % |

18 días comparados, todos idénticos campo a campo. El criterio de aceptación de F5-16 (±1 %) se cumple sobradamente.

## Hallazgos sobre el formato (Claude Code, nov 2026)
- Tipos de línea vistos: `user`, `assistant`, `system`, `attachment`, `ai-title`, `last-prompt`, `tools-used`, `api-request`, `cost-state`, `mode`, `permission-mode`, `session-owner`, `file-history-*`, `queue-operation` y otros.
- **`ai-title`** contiene el título de la sesión: útil como título en la barra lateral (A-07) cuando no hay OSC 2.
- `usage` trae además `cache_creation` (desglosado en 5 m y 1 h), `service_tier`, `speed` y `server_tool_use`. Se ignoran por ahora.
- Las líneas llevan `cwd` y `gitBranch`: sirven para agrupar por proyecto y mostrar la rama (S-21).

## Pendiente
- Ingesta incremental con cursores y SQLite (F5-01, F5-04).
- Parsers de Codex, Gemini y el resto (F5-03).
