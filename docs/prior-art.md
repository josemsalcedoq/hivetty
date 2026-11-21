# Proyectos parecidos y qué aprender de ellos

Investigación de nov 2026. Los datos de licencias y detalles internos marcados con "?" no están confirmados.

## Hallazgo clave: OSC 7501 (Program Status Protocol)

Borrador rev 0.4 (oct 2026), escrito por el autor de Rex (Mitchell Hashimoto): `ESC ] 7501 ; state=blocked:kind=permission:msg=<base64> ST`.
- Estados: `idle`, `working` (con progreso 0–100), `blocked` (`kind` = `permission` | `question` | `auth`), `done` (persiste tras salir el proceso), `error` y `clear`. Varios `id` por panel. El terminal responde a la consulta `?`.
- **Emisores:** Claude Code ≥ 2.1.295, Pi y Crush; Codex lo tiene en main.
- **Receptores:** cmux, Rex, Solo, Zellij 0.46 y kitty (master). No lo soportan todavía Ghostty (app), WezTerm, Windows Terminal ni Warp; tmux lo rechazó.
- Los hooks de Claude **no** pueden emitirlo (no escriben en el terminal).
- Fuente: https://stackness.dev/blog/osc-7501-program-status-terminal-protocol-for-coding-agents

**Consecuencia para hivetty:** OSC 7501 pasa a ser la **fuente de estado nº 1** (ver `docs/agent-integration.md`), por delante de los hooks. Además se puede ver dentro de SSH, porque viaja por el propio terminal. Hay que verificar pronto (SP-2) que ConPTY lo deja pasar.

## Terminales y multiplexores centrados en agentes

| Proyecto | Plataformas / licencia | Qué tomar | Cómo detecta el estado |
|---|---|---|---|
| [cmux](https://github.com/manaflow-ai/cmux) | macOS; GPL-3.0 | pestañas verticales con rama, PR, cwd, puertos y última notificación; anillo azul de "atención"; ⌘⇧U salta a lo último sin leer; CLI y API por socket; restaurar con reanudación de agentes por id; `cmux ssh` | OSC 9/99/777, OSC 7501, hooks → `cmux notify` |
| [Herdr](https://herdr.dev/docs/agents) | macOS/Linux, Windows beta; Apache-2.0? | servidor que mantiene los paneles; estado agregado panel → pestaña → workspace; "done" sigue visible hasta que lo miras; esperar a que otro agente se bloquee | proceso en primer plano + reglas de pantalla por agente; "blocked" solo en prompts conocidos |
| Rex (Superlogical) | macOS beta, servidor multiplataforma | servidor de sesiones; `rex split/send/capture/run --wait` con JSON; `rex skill setup` instala hooks | hooks + OSC 7501 |
| [Solo](https://soloterm.com/docs/terminal/program-status) | macOS/Windows/Linux; comercial | agentes junto a los procesos de desarrollo; MCP, API HTTP y CLI | OSC 7501 por encima de su heurística |
| [Zellij 0.46](https://alternativeto.net/news/2026/11/zellij-0-46-brings-built-in-terminal-attention-inbox-for-ai-agents-and-session-previews/) | todas; MIT | **bandeja de atención** con OSC 7501; vistas previas de sesión | OSC 7501 |
| [Claude Squad](https://github.com/smtg-ai/claude-squad) | Unix/WSL; AGPL | worktree por agente, pestaña de diff, pausar y reanudar | sondeo de `tmux capture-pane` |
| [agent-deck](https://github.com/asheshgoplani/agent-deck) | Unix/WSL; MIT | grupos, búsqueda, *fork*, panel de costes con cuotas, web móvil | sondeo de tmux |

## Apps de escritorio para agentes

| Proyecto | Qué tomar |
|---|---|
| [Conductor](https://www.conductor.build/docs) (solo macOS) | workspace = worktree + rama + terminal + diff; revisión de diff, PR y checks |
| [Paradev](https://paradev.com/llms.txt) (antes Superset) | scripts de preparación por repo, panel de cambios y PR, app iOS |
| [vibe-kanban](https://github.com/BloopAI/vibe-kanban) (en cierre) | tareas en kanban con worktree; comentarios en el diff que vuelven al agente |
| Sculptor (Imbue) | un contenedor por agente; *pairing mode* que refleja la rama en local |
| [Paseo](https://github.com/getpaseo/paseo) | demonio con clientes de escritorio, móvil y web |
| [tlbx / MidTerm](https://github.com/tlbx-ai/MidTerm) | entorno web autoalojado (**no** es el de las capturas ni de midudev) |
| [Terax](https://github.com/crynta/terax-ai) (Tauri; promocionado por midudev) | ligero (~7 MB), agente propio, panel git, vista previa del servidor de desarrollo |

No se encontró ningún proyecto público "midterm" de midudev: el terminal de las capturas parece privado o inédito.

## Apps oficiales
- **[Claude Code Desktop](https://code.claude.com/docs/en/desktop):** barra lateral por proyecto o estado, entornos (local, nube, SSH, WSL), worktree, diff con comentarios, estado de CI y PR, anillo de uso.
- **[Claude Code "agent view"](https://code.claude.com/docs/en/agent-view) (`claude agents`):** grupos *Needs input / Ready for review / Working / Completed*; hooks `agent_needs_input` y `agent_completed`; worktrees en `.claude/worktrees`.
- **[Codex (dentro de ChatGPT desktop)](https://learn.chatgpt.com/docs/environments/git-worktrees):** modos local, worktree y nube; `.worktreeinclude`; instantánea antes de borrar un worktree.
- **[Zed Terminal Threads](https://zed.dev/blog/terminal-threads):** alojan los CLIs de Claude y Codex, avisan por campana y no detectan el final de turno.
- **[Warp](https://docs.warp.dev/agents/cli-agents/overview/)** (cliente AGPL desde abril de 2026): detecta agentes de terceros y muestra el estado en paneles y pestañas verticales con rama, worktree y PR.

## Terminales generales
- **Ghostty:** OSC 9/777, notificaciones al terminar un comando (OSC 133) y barras OSC 9;4. libghostty es la base de cmux, Rex y Terax.
- **kitty:** las notificaciones más ricas (OSC 99 con ids y consultas), socket de control remoto y su protocolo de teclado.
- **iTerm2 3.7:** "Session Status" para Claude Code (color de pestaña, punto y subtítulo) y archivo de sesiones.
- **Wave:** `wsh badge` alimentado por hooks de Claude.
- **Windows Terminal:** sin funciones de agentes, aunque su fork "Intelligent Terminal" añade un panel ACP.

## Paneles de uso
- **[ccusage](https://github.com/ryoppippi/ccusage):** vistas diaria, semanal, mensual y por sesión; bloques de 5 h con ritmo de gasto; precios de LiteLLM con caché offline; más de 20 agentes. **Referencia para validar los totales.**
- **[Claude-Code-Usage-Monitor](https://github.com/Maciek-roboblog/Claude-Code-Usage-Monitor):** lee `rate_limits` de la statusline y una API OAuth de uso; ETA de agotamiento.
- **[tokscale](https://github.com/junhoyeo/tokscale)** (núcleo en Rust, más de 60 agentes): pestañas de modelos, diario, por hora, sesiones, proyectos y agentes; mapa de calor tipo contribuciones; tokens de razonamiento. **Buena referencia para los parsers de muchos agentes.**
- **[CodexBar](https://github.com/steipete/CodexBar):** límites de sesión y semanales con cuenta atrás.
- **Claude Code integrado:** `/stats` (mapa de 12 semanas) y `/usage`. Por OTel exporta `token.usage`, `cost.usage`, **`active_time.total` (user frente a cli)**, `session.count`, líneas de código, commits y PRs.

## Funcionalidades añadidas al plan gracias a esta investigación
- Receptor de **OSC 7501** como fuente principal (A-05) y estado `Blocked` con su motivo (permiso, pregunta, autenticación).
- **Bandeja de atención** dentro de la app, enlazada a los paneles, y anillo en el panel que necesita atención.
- **"Done" persiste como no leído** hasta que se mira.
- **Peek**: leer la pregunta que bloquea y contestar sin cambiar de pestaña.
- Metadatos de fila: rama, PR y estado de CI, diff (+/-), puertos y % de contexto.
- Worktrees: script de preparación por repo, copia de archivos ignorados (`.worktreeinclude`, `.env`), puerto por worktree, límite y limpieza.
- API por CLI y socket con salida JSON (`spawn`, `split`, `send`, `capture`, `notify`, `set-status`) y servidor MCP opcional.
- Reenvío de notificaciones al móvil (ntfy, Telegram o Slack).
- Uso: precios de LiteLLM con caché offline, ventanas de 5 h y cuota semanal con cuenta atrás, ritmo de gasto, tokens de razonamiento y separar tiempo de usuario frente al del agente.

## Problemas conocidos que hivetty debe evitar
- **Parpadeo y saltos de scroll** con TUIs de Ink (peor con ConPTY): salida sincronizada correcta (incluido `CSI 2J` dentro del bloque) y no mover el scroll si el usuario está arriba.
- **Windows olvidado** por casi todos los competidores: **es la oportunidad de hivetty**.
- **Notificaciones poco fiables o tardías** (el aviso de inactividad de Claude tarda unos 60 s y el de permiso unos 6 s; la campana no distingue "done" de "blocked"): OSC 7501 + hooks + agrupado.
- **Perder la pista** con 3–4 o más agentes: barra lateral y bandeja de atención.
- **Coste de los worktrees** (node_modules, `.env`, puertos y worktrees abandonados): script de preparación y limpieza.
- **Restauración superficial**: servidor de PTYs (ADR 0002) y reanudación por id.
- **Peso de Electron**: nativo en Rust.
- **Riesgo de facturación** en GUIs que usan el SDK del agente: hivetty envuelve el CLI en un PTY y no tiene ese problema.
