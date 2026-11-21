# Arquitectura

## 1. Workspace

```
termhive/
├── Cargo.toml                 # [workspace], [workspace.lints], [workspace.dependencies]
├── rust-toolchain.toml
├── crates/
│   ├── th-app/                # binario `termhive`: ventana, bucle de eventos, composición, subcomandos (hook, cli, doctor)
│   ├── th-term/               # trait VtEngine; impl. con alacritty_terminal; interceptor de OSC/DCS/APC; lector de PTY
│   ├── th-pty/                # fork o envoltorio de portable-pty; ConPTY incluido; matar árboles de procesos
│   ├── th-render/             # wgpu: atlas, celdas, cursor, decoraciones, box-drawing; renderer por software
│   ├── th-ui/                 # egui: barra lateral, pestañas, bandeja de atención, diálogos, panel de uso
│   ├── th-agents/             # perfiles, detección, máquina de estados, instalador de hooks, reanudar, worktrees
│   ├── th-usage/              # parsers de logs, SQLite, agregaciones, receptor OTLP opcional
│   ├── th-shell/              # scripts de integración, detección de tecnología, chips
│   ├── th-ipc/                # servidor y cliente IPC; protocolo versionado (postcard)
│   ├── th-config/             # TOML, esquema, temas, atajos, recarga en caliente
│   ├── th-platform/           # código por SO (único crate con unsafe): AUMID, badge, ITaskbarList3, NSDockTile, TTS…
│   ├── th-i18n/               # Fluent
│   └── th-fake-agent/         # binario de pruebas (docs/testing.md §4)
├── xtask/                     # golden-update, fixtures, dist, bench, conpty-fetch
├── assets/
│   ├── shell/                 # zsh, bash, fish, pwsh
│   ├── agents/                # perfiles TOML de agentes
│   ├── icons/{agents,tech}/   # SVG
│   ├── themes/
│   ├── fonts/                 # JetBrains Mono, Symbols Nerd Font, Inter (OFL/MIT)
│   └── windows/conpty/        # conpty.dll + OpenConsole.exe (descargados por xtask, no versionados)
├── tests/fixtures/            # grabaciones .cast, logs de uso por agente y versión, payloads de hooks, dorados
└── docs/
```

Reglas de dependencias (se comprueban en CI con `cargo-deny` + un test de `cargo metadata`):
- `th-ui` y `th-render` **no** dependen de `th-pty` (hablan con los paneles por `PaneHost`).
- Solo `th-platform` usa `unsafe`.
- `th-usage` no depende de nada gráfico (se puede usar desde la CLI).

## 2. Traits principales

```rust
/// Motor VT intercambiable (alacritty_terminal hoy; libghostty-vt si SP-1b lo justifica).
trait VtEngine: Send {
    fn feed(&mut self, bytes: &[u8]);                 // ya sin las OSC interceptadas
    fn resize(&mut self, size: TermSize);
    fn snapshot(&self, dirty_only: bool) -> ScreenSnapshot;
    fn cursor_row(&self) -> usize;
    fn modes(&self) -> TermModes;                     // teclado de kitty, ratón, bracketed paste…
    fn take_responses(&mut self) -> Vec<u8>;          // DA, DSR… para escribir al PTY
    fn sync_deadline(&self) -> Option<Instant>;       // salida sincronizada
}

/// Dueño de los paneles: en el mismo proceso (F1–F6) o el servidor separado (F7, ADR 0002).
trait PaneHost {
    fn spawn(&self, spec: SpawnSpec) -> Result<PaneId>;
    fn write(&self, pane: PaneId, bytes: &[u8]);
    fn resize(&self, pane: PaneId, size: TermSize);
    fn kill(&self, pane: PaneId, mode: KillMode);
    fn subscribe(&self) -> Receiver<HostEvent>;      // Dirty(pane), Title, Cwd, PromptMark, Status7501, Notify, Exit…
    fn snapshot(&self, pane: PaneId) -> ScreenSnapshot;
}
```

## 3. Hilos y flujo de datos

```
             ┌──────────────────── hilo lector por panel ─────────────────────┐
 PTY bytes ─►│ interceptor (vte::Parser + Perform propio)                      │
             │   ├─ OSC 7/9/99/133/633/777/1337/7501, DCS, APC → HostEvent      │
             │   └─ resto ─► VtEngine.feed()  (Mutex corto por bloque)          │
             │ respuestas (DA/DSR) ─► PTY write                                 │
             └────────────────┬───────────────────────────────────────────────┘
                              │ HostEvent::Dirty / eventos (canal + EventLoopProxy, lotes de 4 ms)
┌──────────────────────┐      ▼
│ hilo principal       │  winit + egui + th-render: un frame por vsync si hay algo sucio;
│ (no hace I/O)        │  copia la instantánea con el lock corto y pinta sin el lock (patrón de Zed)
└─────────┬────────────┘
          │ comandos (escribir, resize, kill) ─► PaneHost
┌─────────┴───────┐ ┌──────────────────────┐ ┌───────────────────┐ ┌─────────────────┐
│ hilo IPC        │ │ hilo th-usage        │ │ sondeo de procesos│ │ hilo de shell/  │
│ (hooks, cli)    │ │ notify + SQLite (W)  │ │ (sysinfo, 1 s)    │ │ chips (notify)  │
└─────────────────┘ └──────────────────────┘ └───────────────────┘ └─────────────────┘
```

- Lectura del PTY en bloques de hasta 64 KiB. Si un panel produce más de lo que se pinta, se procesan lotes y se cede el lock (que `cat` de 1 GB no bloquee la UI).
- Eventos a la UI agrupados cada 4 ms (máximo 100) y `Wakeup` fusionados, como Zed.
- Salida sincronizada: mientras dura un bloque 2026, el panel no se marca sucio; se marca al cerrarse o al vencer `sync_deadline` (150 ms).

## 4. Pipeline de render (por frame)

1. Para cada panel visible y sucio: instantánea (filas sucias, cursor, selección, decoraciones y chips).
2. Buffers de instancias: fondos (rect + color) y glifos (posición, región del atlas, color, flags).
3. Glifos nuevos → `cosmic-text` (shaping y fallback) → `swash` (rasterizado) → atlas (`etagere`). Los de box-drawing, bloques, braille y powerline se generan por código.
4. Pasadas: fondos → glifos (mezcla lineal con corrección de texto; emoji en color en su propio atlas RGBA) → cursor, subrayados, selección y decoraciones → chips e iconos (atlas de SVG con `resvg`).
5. `egui-wgpu` pinta la UI en la **misma** `RenderPass` y el mismo `Device`.
6. `present()`. Modo `Fifo` (vsync) por defecto. Sin cambios no hay frame (0% de CPU en reposo).

Renderer por software: el mismo trait `Renderer` sobre `tiny-skia`, con el mismo atlas rasterizado por CPU. Sirve para los dorados y como respaldo.

## 5. Modelo de datos (UI)

```rust
struct Workspace { windows: Vec<WindowState> }
struct WindowState { tabs: Vec<Tab>, active: TabId, sidebar: SidebarState }
struct Tab { root: PaneNode, title_override: Option<String> }
enum PaneNode { Leaf(PaneId), Split { axis: Axis, ratio: f32, a: Box<PaneNode>, b: Box<PaneNode> } }

struct AgentSession {
    id: SessionId,                   // = TERMHIVE_SESSION_ID
    agent_session_id: Option<String>,// id propio del agente (para resume)
    agent: AgentId,
    pane: Option<PaneId>,            // None si está archivada
    project: ProjectId,              // raíz git
    branch: Option<String>,
    worktree: Option<PathBuf>,
    title: String,
    subtitle: String,
    state: AgentState,               // Starting | Working{progress} | Blocked{kind,msg} | Done | Error | Exited
    unread: bool,
    state_source: StateSource,       // Osc7501 | Hooks | Notify | Title | Heuristic
    started_at: SystemTime,
    state_since: SystemTime,
    archived: bool,
}
```

## 6. Persistencia

| Qué | Dónde | Formato |
|---|---|---|
| Configuración | directorio de config | `config.toml` |
| Sesiones, layout, uso, intervalos | directorio de datos | `state.sqlite` (WAL) |
| Logs de la app | directorio de estado o caché | `termhive.log` con rotación (`tracing-appender`) |
| Crashes | directorio de estado | `crash-<fecha>.txt` |
| Grabaciones de depuración | donde diga el usuario | `.cast` (asciicast v2) |
