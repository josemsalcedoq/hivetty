# Plan detallado — termhive

## 0. Visión

Un terminal nativo y multiplataforma donde los agentes de código son ciudadanos de primera clase. El usuario abre varias sesiones de agentes en paralelo y el terminal sabe en todo momento:

- qué agente corre en cada pestaña y en qué proyecto,
- si está trabajando, esperando una respuesta o ha terminado,
- cuántos tokens, tareas y horas ha consumido cada agente y cada modelo.

### Referencia visual (lo que queremos replicar)

**Ventana principal**

```
┌──────────────────┬──────────────────────────────┬──────────────────────────────┐
│ + New session    │ [tab] [tab] [tab] [tab]  +   │                              │
│ 🔍 Search agents │                              │                              │
│ All 6 Waiting    │   Panel izquierdo (PTY)      │   Panel derecho (PTY)        │
│       Working 5  │   agente / shell             │   agente / shell             │
│                  │                              │                              │
│ ▸ proyecto-a     │                              │                              │
│   ✳ Sesión  3m   │                              │                              │
│     Done         │                              │                              │
│ ▸ proyecto-b     │                              │                              │
│   ✳ Claude 21m   │                              │                              │
│     Working…     │                              │                              │
│                  │ ❯ _                          │ ❯ _                          │
│ [iconos agentes] │ ▸▸ auto mode · week ▬ 15%    │ status bar                   │
│ Archived     19  │                              │                              │
└──────────────────┴──────────────────────────────┴──────────────────────────────┘
```

**Panel de uso**: 4 tarjetas KPI (input tokens con % de caché, output tokens por turno, tareas/turnos, horas de agente con concurrencia máxima), cada una con su delta respecto al periodo anterior y un sparkline. Debajo: barras de tokens por día (input arriba, output abajo), mapa de calor día×hora, tabla de modelos agrupada por proveedor y tabla de agentes filtrable. Selector de rango: 7 / 30 / 90 días / todo.

### No-objetivos (por ahora)

- No es un IDE ni un editor.
- No reimplementa los agentes: los lanza y observa, no habla con las APIs de los modelos.
- No hay sincronización en la nube. Todos los datos son locales.

---

## 1. Decisiones de stack (resumen; detalle en `docs/adr/`)

| Área | Elección | Alternativa considerada |
|---|---|---|
| Lenguaje | Rust estable | — |
| Ventana / eventos | `winit` | GPUI (Zed), SDL |
| Render | `wgpu` (Metal / Vulkan / DX12 / GL de respaldo) | OpenGL directo (Alacritty) |
| Texto | `cosmic-text` + `swash` (shaping, fallback, emoji) y `glyphon` para el atlas | `parley` + `vello` (CuTTY) |
| Emulación VT | `alacritty_terminal` | `libghostty-vt` (Zig, ABI C), crate `wezterm-term` |
| PTY | `portable-pty` (openpty en Unix, ConPTY en Windows) | crates propios por SO |
| UI de la ventana (barra lateral, panel de uso) | `egui` + `egui-wgpu` sobre el mismo dispositivo GPU | UI propia dibujada a mano |
| Renderer por software (tests y respaldo) | `tiny-skia` | — |
| Base de datos local | SQLite (`rusqlite`, feature `bundled`) | `redb` |
| IPC local | `interprocess` (Unix socket / named pipe) | TCP en localhost |
| Config | TOML (`serde`, `toml`) + recarga con `notify` | — |
| Portapapeles | `arboard` | — |
| Notificaciones | `notify-rust` (Linux/Windows) + `mac-notification-sys` | — |
| Procesos | `sysinfo` (árbol de procesos para detectar agentes) | — |
| Logs | `tracing` + `tracing-subscriber` | — |

---

## 2. Fases

Cada fase termina con algo usable y su propio criterio de aceptación. **Todas las fases deben pasar CI en las tres plataformas** antes de cerrarse.

### Fase 0 — Cimientos (1 semana)

**Objetivo:** repo listo para trabajar con CI en macOS, Ubuntu y Windows.

Tareas:
1. Instalar Rust con rustup en las máquinas de desarrollo (Windows: toolchain MSVC).
2. Crear el workspace de Cargo con los crates vacíos de `docs/architecture.md`.
3. Configurar `rustfmt.toml`, `clippy.toml` y `[workspace.lints]` (prohibir `unsafe_code` salvo en `th-platform`).
4. CI en GitHub Actions con matriz `macos-latest`, `ubuntu-24.04` y `windows-latest`:
   - `cargo fmt --check`
   - `cargo clippy --all-targets -- -D warnings`
   - `cargo nextest run`
   - `cargo deny check` (licencias y advisories)
5. Instalar los paquetes de sistema de Ubuntu en CI (ver README).
6. Plantillas de issues/PR y `CHANGELOG.md`.
7. Instalar las skills de `docs/skills.md` después de revisarlas.

**Aceptación:** un PR vacío pasa CI en las tres plataformas.

---

### Fase 1 — Terminal mínimo usable (3–4 semanas)

**Objetivo:** una ventana con una shell que funcione bien con `vim`, `htop`, `less`, `tmux` y con un agente (Claude Code).

Tareas:
1. **Ventana** (`th-app`): bucle de eventos de `winit`, DPI por monitor, redimensionado, foco, cierre.
2. **PTY** (`th-pty`):
   - Abrir el PTY con el tamaño en celdas y en píxeles.
   - Detectar la shell por defecto: `$SHELL` en Unix; en Windows, `pwsh` → `powershell` → `cmd`.
   - Hilo lector que manda bytes al terminal por un canal (`crossbeam-channel`).
   - Propagar el redimensionado (en Windows, `ResizePseudoConsole`).
   - Variables de entorno: `TERM=xterm-256color`, `COLORTERM=truecolor`, `TERM_PROGRAM=termhive` y `TERMHIVE_SESSION_ID`.
3. **Emulación** (`th-term`): envolver `alacritty_terminal::Term` y exponer una "instantánea" de las celdas visibles para el renderer, sin bloquear el hilo del PTY.
4. **Render** (`th-render`):
   - Inicializar `wgpu` eligiendo el backend por plataforma, con GL como respaldo.
   - Atlas de glifos: rasterizar con `swash` y empaquetar con `etagere`.
   - Dibujar las celdas con *instancing*: un quad por celda (fondo) y uno por glifo.
   - Cursor (bloque, barra o subrayado; parpadeo), subrayados y tachado.
   - Truecolor, 256 colores y paleta del tema.
   - Redibujar solo con cambios (*damage tracking*), a 60 fps como máximo.
5. **Entrada**: traducir las teclas al protocolo xterm, incluyendo el modo de teclado de Kitty (CSI u) si la app lo pide; IME (pinyin, japonés y tildes muertas en layouts españoles); ratón (SGR 1006); pegado con *bracketed paste*.
6. **Scrollback** con rueda y barra; búsqueda de texto (Ctrl/Cmd+F).
7. **Selección y portapapeles**: selección por palabra/línea/bloque; copiar y pegar con `arboard`; OSC 52.
8. **Fuentes**: fuente configurable, fallback a emoji y CJK, ligaduras opcionales.
9. **Renderer por software** (`tiny-skia`) con la misma lógica de layout que el de GPU.

**Aceptación:**
- `vttest` pasa las secciones 1–3 y 5.
- `vim`, `htop`, `less`, `tmux` y `claude` funcionan sin artefactos en las tres plataformas.
- Escribir con eco local tarda <10 ms de latencia de entrada a pantalla en un portátil medio.
- `cat` de un archivo de 100 MB no congela la UI.
- El test dorado CPU frente a GPU da 0 píxeles distintos en una pantalla de referencia.

---

### Fase 2 — Pestañas, paneles y sesiones (2 semanas)

Tareas:
1. Modelo `Workspace → Tab → árbol de Panes` (división horizontal/vertical, redimensionado con arrastre).
2. Barra de pestañas: título dinámico (OSC 0/2), icono del agente, cerrar, reordenar, `+`.
3. Atajos multiplataforma con la tecla `Primary` (Cmd en macOS, Ctrl+Shift en Linux y Windows). Ver `docs/platforms.md`.
4. Paleta de comandos.
5. Guardar el layout y los directorios al cerrar y restaurarlos al abrir (sin restaurar procesos).

**Aceptación:** abrir 10 pestañas con 2 paneles cada una mantiene 60 fps y menos de 300 MB de RAM.

---

### Fase 3 — Integración con la shell y chips (2–3 semanas)

Tareas:
1. Scripts en `assets/shell/` para `zsh`, `bash`, `fish` y `pwsh`, inyectados automáticamente (ZDOTDIR en zsh, `--rcfile` en bash, `-NoExit -Command` en pwsh):
   - OSC 133 (A/B/C/D): inicio y fin del prompt y del comando, con código de salida.
   - OSC 7: directorio actual.
   - OSC 1337 / variable propia para mandar los *chips* al terminal.
2. **Detección de tecnología** del proyecto. Se mira el directorio actual y, si no hay proyecto, la raíz del repo git. Gana la primera en este orden:

   Rust (`Cargo.toml`) → Python (`pyproject.toml`, `requirements.txt`) → Bun (`bun.lock`, `bun.lockb`, `bunfig.toml`) → TypeScript (`tsconfig.json` o `typescript` en dependencias) → Node (`package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`, `.nvmrc`, `.node-version`) → JavaScript (`package.json`) → Go → Dart/Flutter → otros.

   Se hace **en el terminal (Rust)**, no en la shell, para que funcione igual en las cuatro shells. La shell solo informa del directorio (OSC 7).
3. **Chips** dibujados por el terminal en una fila sobre el prompt: logo y versión. Si un chip no cabe en la fila, no se dibuja (no salta de línea).
4. Navegar entre prompts (saltar al comando anterior o siguiente) y copiar la salida del último comando.
5. Notificación cuando termina un comando largo con la ventana sin foco.

**Aceptación:** en un repo Rust, uno Bun, uno TS y uno Python se muestra el chip correcto en las 4 shells y las 3 plataformas.

> **Riesgo:** ConPTY en Windows filtra algunas secuencias de escape. Verificar pronto (semana 1 de esta fase) que OSC 133, OSC 7 y OSC 9/777 llegan; si no, plan B: mandar esos datos por la IPC (`th hook …`).

---

### Fase 4 — Barra lateral de agentes (3–4 semanas)

Detalle técnico en `docs/agent-integration.md`.

Tareas:
1. **Registro de agentes** (`th-agents`): perfiles declarativos en TOML (nombre, comando, icono, cómo se detecta y de dónde se leen sus logs). Perfiles iniciales: Claude Code, Codex, Gemini CLI, Grok CLI, opencode, Junie, Antigravity.
2. **Detección**:
   - Al lanzar desde "New session", se conoce el agente directamente.
   - Si el usuario lo lanza a mano: árbol de procesos del PTY (`sysinfo`), sondeado cada 1 s.
3. **Máquina de estados** por sesión: `Starting → Working ⇄ Waiting → Done | Error | Exited`.
   - Fuente 1, hooks del agente: `th hook <evento>` por IPC.
   - Fuente 2, secuencias del terminal: OSC 9/777 (notificaciones), título (OSC 2) y campana (BEL).
   - Fuente 3, heurística: sin salida durante N segundos con el cursor en el prompt del agente → `Waiting`.
4. **UI de la barra lateral**:
   - Botón "New session" (`Primary+T`) con selector de agente y directorio.
   - Búsqueda (`/`) por título, proyecto o resumen.
   - Filtros `All · Waiting · Working` con contadores.
   - Grupos por proyecto (nombre de la raíz git del directorio).
   - Fila: icono, título (el prompt inicial o el título del agente), subtítulo con la última acción, tiempo transcurrido y color de estado.
   - Iconos de los agentes instalados abajo (lanzar rápido) y sección "Archived" con contador.
5. **Notificaciones del sistema** cuando una sesión pasa a `Waiting` o `Done` sin foco. Clic → enfoca esa pestaña.
6. Instalador de hooks: `termhive agents install-hooks` muestra el diff en `~/.claude/settings.json` (y equivalentes) y pide confirmación antes de escribir. Nunca sobrescribe, solo añade entradas.

**Aceptación:**
- 6 sesiones de Claude Code en paralelo: el estado cambia en menos de 500 ms con hooks y en menos de 5 s solo con heurística.
- Pulsar una sesión en `Waiting` enfoca su panel.
- Funciona igual en las tres plataformas.

---

### Fase 5 — Panel de uso (3 semanas)

Tareas:
1. **Ingesta** (`th-usage`): lectores incrementales de los logs locales de cada agente (ver `docs/agent-integration.md`). Recordar la posición de lectura de cada archivo para no releerlo entero. Vigilar los cambios con `notify`.
2. **Normalización** a una tabla `events(ts, agent, model, provider, session_id, project, input_tokens, cache_read, cache_write, output_tokens, kind)`.
3. **Métricas**:
   - Input tokens = input + cache_read + cache_write; "% from cache" = cache_read / input total.
   - Output tokens y "per turn" = output / turnos.
   - **Tarea** = un prompt del usuario que provoca trabajo. **Turno** = un mensaje del asistente.
   - **Agent time** = suma de intervalos `Working` (de los hooks) o, sin hooks, intervalos entre eventos con huecos menores de 5 min. "Up to N at once" = concurrencia máxima.
   - Delta frente al periodo anterior de la misma duración.
4. **Vistas**: tarjetas KPI con sparkline, barras de tokens por día (input arriba, output abajo, escala independiente), mapa de calor día×hora en hora local, tabla de modelos (proveedor, barra proporcional, input, output, tareas; "N more" agrupado), tabla de agentes (pulsar uno filtra todo el panel), selector 7/30/90/All y `Esc` para cerrar.
5. Exportar a CSV/JSON.

**Aceptación:** los números de Claude Code cuadran (±1 %) con `ccusage` sobre los mismos logs. El panel abre en menos de 300 ms con 90 días de datos.

---

### Fase 6 — Pulido y distribución (3 semanas)

1. Temas (claro/oscuro, importar temas de iTerm2/Alacritty), configuración con recarga en caliente.
2. Barra de estado configurable: modo del agente, barra de uso semanal y "narrator".
3. **Narrador** opcional: lee en voz alta cuando un agente termina o pide algo (macOS `say` / AVSpeechSynthesizer, Windows SAPI, Linux `speech-dispatcher`).
4. Empaquetado:
   - macOS: `.app` + `.dmg` universal (arm64 + x86_64), firmado y notarizado.
   - Ubuntu: `.deb` y AppImage, archivo `.desktop` e icono.
   - Windows: MSI (WiX) o MSIX, firma de código y manifiesto winget.
5. Actualizaciones automáticas (`self_update` o feed propio).
6. Página de ayuda y `termhive --doctor` (comprueba GPU, fuentes, shell y hooks).

**Aceptación:** instalar desde cero en máquinas limpias de las tres plataformas y completar el flujo "abrir → nueva sesión de Claude Code → ver estado → ver uso" sin tocar la terminal.

---

### Fase 7 — Extras (sin fecha)

- Protocolo de gráficos de Kitty e imágenes inline (iTerm2 OSC 1337 / sixel).
- Panel de "goal" o resumen del agente leyendo su transcripción.
- Límite de uso semanal del plan, si el agente lo expone de forma fiable.
- Plugins (WASM) y control remoto del terminal por IPC (`termhive cli`).

---

## 3. Estrategia de pruebas

| Tipo | Herramienta | Qué cubre |
|---|---|---|
| Unitarias | `cargo nextest` | parsers de logs, máquina de estados, detección de tecnología, keymaps |
| Dorados de render | renderer por software + `image` | comparar CPU frente a GPU píxel a píxel y frente a imágenes de referencia |
| PTY de integración | por plataforma en CI | lanzar la shell, mandar comandos, leer la salida |
| Conformidad VT | `vttest`, `esctest` | secuencias de escape |
| Fuzzing | `cargo-fuzz` | parsers de logs y de OSC propios |
| Rendimiento | `criterion` + `vtebench` | throughput y latencia |
| Fixtures de agentes | `tests/fixtures/<agente>/` | logs reales anonimizados de cada versión de agente |

---

## 4. Riesgos principales

| Riesgo | Impacto | Mitigación |
|---|---|---|
| ConPTY filtra o reordena secuencias | Fase 3–4 en Windows | probar pronto; plan B por IPC |
| `alacritty_terminal` no soporta imágenes | chips con logos, Fase 7 | chips dibujados por el terminal fuera de la rejilla; para imágenes, interceptar APC/OSC antes del parser |
| Formato de logs de agentes no documentado y cambiante | panel de uso | parsers tolerantes (`serde(default)`), fixtures por versión, avisar en `--doctor` |
| IME en Linux (ibus/fcitx) y Wayland | entrada de texto | probar con `winit` en GNOME/KDE; seguir sus issues |
| Fuentes y fallback distintos por SO | aspecto | `cosmic-text` con fallback propio y pruebas doradas por SO |
| Drivers GPU antiguos | arranque | respaldo GL y renderer por software |
| Alcance demasiado grande | todo | cada fase es entregable; no empezar la siguiente sin cerrar la anterior |

---

## 5. Calendario orientativo

| Semana | Fase |
|---|---|
| 1 | F0 |
| 2–5 | F1 |
| 6–7 | F2 |
| 8–10 | F3 |
| 11–14 | F4 |
| 15–17 | F5 |
| 18–20 | F6 |

Unos 5 meses a tiempo parcial con agentes ayudando. El hito con más valor es el final de **F4** (terminal + barra de agentes).
