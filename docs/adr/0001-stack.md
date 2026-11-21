# ADR 0001 — Stack base

- **Estado:** propuesto
- **Fecha:** 2026-11-21

## Contexto

Necesitamos un terminal nativo que funcione en macOS, Ubuntu y Windows, con render por GPU y una interfaz rica (barra lateral, panel de uso) alrededor de la rejilla del terminal.

## Decisión

- `winit` + `wgpu` para la ventana y la GPU: un único código para Metal, Vulkan y DX12.
- `alacritty_terminal` para la emulación VT: maduro, usado por Zed y Alacritty, y publicado como crate.
- `portable-pty` para el PTY: abstrae ConPTY y openpty.
- `egui` (sobre `egui-wgpu`) para la interfaz de la ventana, y la rejilla del terminal con renderer propio. Permite construir la barra lateral y el panel de uso rápido; si limita el aspecto, se sustituye después pieza a pieza.

## Alternativas

| Opción | Pros | Contras |
|---|---|---|
| `libghostty-vt` | conformidad VT excelente, imágenes de Kitty | Zig en la cadena de compilación, ABI C, API joven |
| GPUI (Zed) | UI y terminal ya resueltos juntos | framework grande, ligado al ritmo de Zed |
| Tauri + xterm.js | desarrollo de UI muy rápido | no es "nativo", más latencia y RAM |

## Consecuencias

- Las imágenes inline (protocolo de Kitty) no vienen de serie: hay que interceptarlas antes del parser (Fase 7).
- Revisar esta decisión al final de la Fase 1 con mediciones de latencia y conformidad.
