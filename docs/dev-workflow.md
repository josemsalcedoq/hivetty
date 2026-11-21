# Flujo de desarrollo (con agentes)

## Ramas y commits
- *Trunk-based*: `main` siempre compila y pasa CI. Ramas cortas `feat/…`, `fix/…`, `docs/…`, `spike/…`.
- Commits con **Conventional Commits** (`feat(th-render): glyph atlas`), en inglés. El changelog se genera con `git-cliff`.
- Versionado SemVer. `0.x` hasta que la F6 esté cerrada.
- Sin atribución de IA en commits ni PRs (regla del autor).

## Definición de terminado (DoD)
Un PR está terminado cuando:
1. Cumple el criterio de aceptación de su tarea (`docs/backlog.md`).
2. Tiene tests (unitarios y, si toca PTY, render o UI, del nivel que corresponda según `docs/testing.md`).
3. CI en verde en macOS, Ubuntu y Windows.
4. Se ha pasado la skill `termhive-platform-check` y el PR dice qué se probó en qué SO.
5. Si cambia la arquitectura o una decisión: ADR nuevo o actualizado.
6. Si cambia algo visible: captura o GIF en el PR y documentación de usuario actualizada.
7. `CHANGELOG` generado con los commits.

## Trabajar con Claude Code en este repo
- **Una tarea del backlog = una sesión**, idealmente en su propio `git worktree` (`git worktree add ../termhive-wt/<id> -b feat/<id>`), para poder tener varias en paralelo sin pisarse.
- Paralelizar solo tareas de **crates distintos** o sin dependencias entre sí (el backlog marca las dependencias).
- Para tareas grandes: primero *plan mode* (el agente propone el plan y lo apruebas), después la implementación.
- Antes de abrir el PR: `/code-review` sobre el diff y la skill `termhive-platform-check`.
- PRs que tocan IPC, parsers, hooks o actualizaciones: además, `/security-review`.
- `CLAUDE.md` en la raíz con las reglas del proyecto; skills del proyecto en `.claude/skills/`; skills de terceros solo tras revisarlas (`docs/skills.md`).
- Nunca pedir a un agente "haz la fase X entera". Mejor tareas de 0.5 a 3 días con criterio de aceptación claro.

## Revisiones en hardware real
Las cosas que CI no ve (latencia, IME, DPI, notificaciones, lectores de pantalla) se prueban a mano en los tres SO al cerrar cada fase, con la checklist de `docs/testing.md` §6.

## GitHub
- Etiquetas: `phase:F0…F7`, `crate:th-*`, `os:macos|linux|windows`, `type:feat|bug|spike|docs`, `prio:M|S|C`, `good-first-issue`.
- Un milestone por fase.
- Plantillas de issue: bug (con `termhive --doctor` adjunto), petición de funcionalidad y soporte de un agente nuevo.
- Plantilla de PR con la checklist de plataformas.
