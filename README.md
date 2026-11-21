# termhive

Terminal nativo escrito en Rust, acelerado por GPU, pensado para trabajar con **muchos agentes de código a la vez** (Claude Code, Codex, Gemini CLI, opencode…). Funciona en **macOS, Ubuntu y Windows**.

> Estado: **planificación completa, sin código todavía**. Empezar por [`docs/PLAN.md`](docs/PLAN.md).

## Qué hace (objetivo v1.0)

- **Terminal rápido y sin parpadeo**: render por GPU con `wgpu` (Metal / Vulkan / DirectX 12), salida sincronizada y ConPTY moderno incluido en Windows.
- **Barra lateral de agentes**: sesiones agrupadas por proyecto con estado en vivo (*Working*, *Waiting*, *Done*), por medio de OSC 7501 y de los hooks de cada agente. Incluye bandeja de atención, notificaciones, reanudación de sesiones y worktrees.
- **Panel de uso**: tokens (input, caché, output) por modelo y agente, tareas, horas de agente, gráfico diario y mapa de calor. Todo local.
- **Integración con la shell** (zsh, bash, fish, PowerShell) y *chips* con la tecnología del proyecto.

## Documentación

Todo el plan está en [`docs/`](docs/), con [`docs/PLAN.md`](docs/PLAN.md) como índice: requisitos, UI, arquitectura, conformidad VT, integración con agentes, plataformas, seguridad, pruebas, distribución, backlog, skills y ADRs.

## Requisitos de desarrollo

- Rust estable vía [rustup](https://rustup.rs) (versión fijada en `rust-toolchain.toml` en la F0).
- macOS: Xcode Command Line Tools.
- Ubuntu: `build-essential pkg-config libfontconfig1-dev libxkbcommon-dev libwayland-dev libx11-dev libvulkan-dev`.
- Windows: Visual Studio Build Tools (MSVC). El ConPTY incluido se descarga con `cargo xtask conpty-fetch`.
