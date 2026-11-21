# termhive

Terminal nativo escrito en Rust, acelerado por GPU, pensado para trabajar con **varios agentes de código a la vez** (Claude Code, Codex, Gemini CLI, Grok CLI…).

Funciona en **macOS, Linux (Ubuntu) y Windows**.

> Estado: **planificación**. Todavía no hay código; ver [`docs/PLAN.md`](docs/PLAN.md).

## Qué hace (objetivo)

- Terminal rápido: render por GPU con `wgpu` (Metal / Vulkan / DirectX 12), PTY nativo (ConPTY en Windows).
- Pestañas y paneles divididos.
- **Barra lateral de agentes**: sesiones agrupadas por proyecto, con estado en vivo (*Working*, *Waiting*, *Done*), búsqueda y filtros.
- **Panel de uso**: tokens de entrada/salida, uso de caché, tareas, horas de agente, gráficos por día, mapa de calor por hora y tablas por modelo y por agente.
- Integración con la shell (zsh, bash, fish, PowerShell): marcas de prompt, directorio actual y *chips* con la tecnología del proyecto.

## Documentación

| Documento | Contenido |
|---|---|
| [docs/PLAN.md](docs/PLAN.md) | Plan detallado por fases, con tareas y criterios de aceptación |
| [docs/architecture.md](docs/architecture.md) | Crates, flujo de datos e hilos |
| [docs/agent-integration.md](docs/agent-integration.md) | Cómo se detecta el estado de cada agente y de dónde salen las métricas |
| [docs/platforms.md](docs/platforms.md) | Diferencias entre macOS, Linux y Windows |
| [docs/skills.md](docs/skills.md) | Skills de agentes recomendadas para desarrollar este proyecto |
| [docs/adr/](docs/adr/) | Registro de decisiones de arquitectura |

## Requisitos de desarrollo

- Rust estable (vía [rustup](https://rustup.rs)).
- macOS: Xcode Command Line Tools.
- Ubuntu: `build-essential pkg-config libfontconfig1-dev libxkbcommon-dev libwayland-dev libx11-dev libvulkan-dev`.
- Windows: Visual Studio Build Tools (MSVC) y Windows 10 1809 o superior (ConPTY).
