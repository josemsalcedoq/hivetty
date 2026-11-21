# Arquitectura

## Workspace

```
termhive/
├── Cargo.toml                 # [workspace]
├── crates/
│   ├── th-app/                # binario `termhive`: ventana, bucle de eventos, composición
│   ├── th-term/               # emulación VT (envuelve alacritty_terminal), instantáneas de celdas
│   ├── th-pty/                # PTY multiplataforma (portable-pty), detección de shell
│   ├── th-render/             # wgpu: atlas de glifos, celdas, cursor; renderer por software
│   ├── th-ui/                 # egui: barra lateral, pestañas, barra de estado, panel de uso
│   ├── th-agents/             # perfiles de agentes, detección, máquina de estados
│   ├── th-usage/              # lectores de logs, SQLite, agregaciones
│   ├── th-shell/              # OSC 133/7, detección de tecnología del proyecto, chips
│   ├── th-ipc/                # servidor IPC local + subcomando `termhive hook`
│   ├── th-config/             # TOML, temas, atajos, recarga en caliente
│   └── th-platform/           # código específico por SO (único crate con `unsafe` permitido)
├── assets/
│   ├── shell/                 # scripts de integración: zsh, bash, fish, pwsh
│   ├── icons/                 # logos de tecnologías y agentes (SVG → atlas)
│   └── themes/
├── tests/fixtures/            # logs de agentes, pantallas de referencia
└── docs/
```

## Hilos

```
                ┌───────────────┐   bytes    ┌──────────────┐
  shell/agente ─┤ hilo PTY (xN) ├───────────►│ th-term::Term│ (Mutex por panel)
                └───────────────┘            └──────┬───────┘
                                                    │ "dirty" + instantánea
┌───────────────────┐  eventos winit     ┌──────────▼──────────┐
│ hilo principal    │◄──────────────────►│ th-render + th-ui   │
│ (winit, UI, GPU)  │                    │ (un frame por vsync)│
└────────┬──────────┘                    └─────────────────────┘
         │ EventLoopProxy (despertar)
┌────────┴──────────┐   ┌────────────────────┐   ┌───────────────────┐
│ hilo IPC          │   │ hilo th-usage      │   │ hilo sondeo procs │
│ (hooks agentes)   │   │ (notify + SQLite)  │   │ (sysinfo, 1 s)    │
└───────────────────┘   └────────────────────┘   └───────────────────┘
```

- El hilo principal **nunca** hace I/O bloqueante.
- Los demás hilos mandan mensajes al principal con `EventLoopProxy<AppEvent>`.
- Cada panel tiene su `Term` protegido por un `parking_lot::FairMutex` (mismo patrón que Alacritty). El renderer copia solo las filas sucias.

## Pipeline de render (por frame)

1. Para cada panel visible: tomar la instantánea (celdas, cursor, selección).
2. Construir los buffers de instancias: fondos (rect + color) y glifos (posición, coords del atlas, color).
3. Glifos no presentes → rasterizar con `swash` y subir al atlas (`etagere`).
4. Pasada 1: fondos. Pasada 2: glifos (texto en escala de grises o subpíxel; emoji en color). Pasada 3: cursor y decoraciones.
5. Pasada de UI: `egui-wgpu` dibuja la barra lateral, las pestañas, los chips y el panel de uso en la misma `RenderPass`.
6. `present()`.

El renderer por software implementa el mismo trait `Renderer` sobre `tiny-skia` para los tests dorados y como respaldo sin GPU.

## Modelo de datos

```rust
struct Workspace { tabs: Vec<Tab>, active: TabId }
struct Tab { root: PaneNode, title: String }
enum PaneNode { Leaf(PaneId), Split { dir: Axis, ratio: f32, a: Box<PaneNode>, b: Box<PaneNode> } }
struct Pane { pty: PtyHandle, term: Arc<FairMutex<Term>>, cwd: Option<PathBuf>, session: Option<AgentSessionId> }

struct AgentSession {
    id: AgentSessionId,         // = TERMHIVE_SESSION_ID
    agent: AgentKind,           // ClaudeCode, Codex, Gemini, ...
    project: ProjectId,         // raíz git del directorio
    title: String,
    subtitle: String,           // última acción ("Writing crates/…/vision.rs")
    state: AgentState,          // Starting | Working | Waiting | Done | Error | Exited
    started_at: Instant,
    state_since: Instant,
    archived: bool,
}
```

## Persistencia

- `config.toml` en el directorio de configuración de la plataforma (`directories::ProjectDirs`).
- `state.sqlite` en el directorio de datos: sesiones, layout y eventos de uso.
- Logs de la app en el directorio de caché, con rotación.
