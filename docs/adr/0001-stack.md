# ADR 0001 — Stack base

- **Estado:** propuesto; se confirma o cambia al cerrar SP-1 (y SP-1b)
- **Fecha:** 2026-11-21 (versiones verificadas ese día en crates.io)

## Contexto

Necesitamos un terminal nativo para macOS, Ubuntu y Windows, con render por GPU y una interfaz rica (barra lateral y panel de uso) alrededor de la rejilla.

## Decisión

| Pieza | Crate (versión) | Licencia | Notas |
|---|---|---|---|
| Ventana | `winit` 0.30.13 | Apache-2.0 | **Quedarse en 0.30** hasta que egui-winit y accesskit_winit soporten 0.31 (en beta; trae mejoras de IME). |
| GPU | `wgpu` 30.0.x | MIT/Apache | |
| Emulación VT | `alacritty_terminal` 0.26.0 (vte 0.15) | Apache-2.0 | con un **interceptor propio** de OSC/DCS/APC (`docs/vt-compliance.md` §1), sin su `EventLoop` |
| PTY | `portable-pty` 0.9.0 | MIT | lleva meses sin publicarse: los arreglos de Windows (paso a windows-sys, `kill()`) solo están en main de WezTerm. Usar **dependencia git fijada a un commit** o un fork propio (`hv-pty`) |
| ConPTY | **`conpty.dll` + `OpenConsole.exe` incluidos** (NuGet `Microsoft.Windows.Console.ConPTY` 1.25.x, MIT, Windows 1809+) | MIT | como hace Zed. Deja pasar las secuencias sin modificarlas (ver `docs/platforms.md`) |
| UI | `egui` / `egui-wgpu` / `egui-winit` 0.37 | MIT/Apache | ADR 0003. egui-wgpu 0.37 usa wgpu 30: **un único Device** compartido |
| Texto | `cosmic-text` 0.19 (shaping con harfrust/skrifa) + `swash` 0.2.10 (rasterizado); `glyphon` 0.12 como referencia o para el texto de la UI | MIT/Apache | glyphon 0.12 usa wgpu 30 y cosmic-text 0.19 |
| Software / dorados | `tiny-skia` 0.12 | BSD-3 | |
| Iconos SVG | `resvg` 0.50 | Apache/MIT | |
| Accesibilidad | `accesskit` 0.25 / `accesskit_winit` 0.34 (siempre activo en egui) | MIT/Apache | `Role::Terminal` existe, pero la rejilla se añade al árbol a mano |
| DB | `rusqlite` 0.43 (bundled) | MIT | |
| IPC | `interprocess` 2.4 | 0BSD/Apache | |
| Portapapeles | `arboard` 3.7 | MIT/Apache | |
| Notificaciones | `notify-rust` 4.19 | MIT/Apache | en Windows necesita AUMID (ver plataformas) |
| Menús | `muda` 0.21 | Apache/MIT | **solo macOS y Windows**: en Linux necesita una ventana GTK, así que el menú se pinta con egui |

## Alternativa seria: libghostty-vt 1.0

Publicada el **2026-11-06** con ABI C estable (handles opacos, comprobada en CI con libabigail), licencia MIT y probada en Linux, macOS y **Windows x86_64**. Ventajas frente a AT: gráficos de kitty incluidos, conformidad de referencia, OSC tipadas, y teclado, búsqueda y selección incluidos. La usan los terminales de los IDEs de JetBrains.

Inconvenientes hoy:
- Solo hay un crate de la comunidad (`libghostty-vt` 0.2.2), anterior a la 1.0, con tipos `!Send`/`!Sync` y "breaking changes expected".
- Hace falta el toolchain de Zig para compilar.
- No es thread-safe: hay que encerrarla en un hilo dueño.

**Decisión:** empezar con AT (Rust puro, ecosistema probado en Zed), pero encapsulado tras el trait `VtEngine` en `hv-term`. **SP-1b** (2 días) monta libghostty-vt con bindings propios mínimos sobre la ABI 1.0 en los 3 SO. Si el coste es razonable, se reconsidera antes de F3, porque ahorraría el interceptor, la codificación de teclado y las imágenes.

## Otras alternativas descartadas
| Opción | Motivo |
|---|---|
| `wezterm-term` | la más completa en eventos (`AlertHandler`: notificaciones, cwd, progreso, user vars), pero **no está en crates.io**, la última release es de 2024 y su API cambia; habría que mantener un fork |
| `termwiz` | útil solo como parser de OSC tipadas (opción del interceptor) |
| `vt100` | pantalla en memoria; útil para tests sin GPU, no como emulador completo |
| GPUI | adoptar todo el framework de Zed |
| Tauri + xterm.js | no es nativo; más latencia y memoria (es lo que hace Terax) |

## Consecuencias
- Hay que **portar del frontend de Alacritty** (Apache-2.0, con atribución): codificación de teclado (incluido Kitty), codificación del ratón y `builtin_font.rs` (box-drawing).
- Imágenes inline: capa propia en F7, o resueltas si se migra a libghostty-vt.
- Revisar esta decisión con mediciones al final de SP-1/SP-1b.
