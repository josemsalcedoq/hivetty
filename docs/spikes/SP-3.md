# SP-3: egui y la rejilla propia en la misma pasada

**Estado (2026-11-21):** 🟡 parcial. La parte central del ADR 0003 está validada; faltan IME dentro de un campo de egui y AccessKit.

## Qué se probó
- `hv-render` pinta el terminal (rectángulos + glyphon) y después egui (`egui-wgpu` 0.37) en **la misma render pass** (`RenderPass::forget_lifetime`). Una sola presentación por frame.
- `hv-ui::Chrome` dibuja la barra lateral (`egui::Panel::left`) y devuelve el área libre. `hv-render` coloca la rejilla en esa área (`set_grid_area`) y el PTY se redimensiona a lo que cabe.
- Ventana real (`hv-app/src/app.rs`): `egui-winit` recibe los eventos de puntero y de ventana. El teclado y el IME van solo al terminal mientras la interfaz no tenga campos de texto. Clics y rueda sobre la barra no llegan al terminal, y el cursor del ratón se fuerza a I-beam o mano sobre la rejilla.
- `Ctrl+Shift+B` / `⌘B` muestra u oculta la barra (docs/keybindings.md).
- Captura sin ventana: `hivetty --screenshot out.png --ui -e …`.

## Resultados
- Windows 11, RTX 4060, DX12: la barra y el texto del terminal salen bien alineados, sin parpadeo.
- Error encontrado y corregido: la primera pasada de egui en la captura se hacía con la barra oculta, así que la rejilla empezaba en x = 0, debajo de la barra.
- El atlas de fuentes de egui llega en el primer `TexturesDelta`; hay que acumularlo hasta que se pinte el primer frame.

## Pendiente
- IME en un campo de texto de egui (búsqueda de agentes, F2). Probarlo con el IME japonés de Windows y con el de macOS.
- AccessKit: activar la feature `accesskit` de `egui-winit` y comprobar con Narrador / VoiceOver / Orca que se leen las filas de la barra.
- Medir el coste por frame de egui con 50 sesiones en la barra (objetivo de docs/perf.md).
- Fuentes de la interfaz: ahora son las de egui (excepción de licencia en `deny.toml`). Cambiar a Inter.
