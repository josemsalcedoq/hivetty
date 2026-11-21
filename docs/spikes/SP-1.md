# SP-1 — Shell real con alacritty_terminal + wgpu

- **Fecha:** 2026-11-21
- **Estado:** ✅ superado en Windows; macOS y Linux se verifican con el render headless del CI (ver más abajo)
- **Rama:** directamente en `main`. El código no es desechable: forma la base de la F1.

## Qué se construyó
| Crate | Qué hay |
|---|---|
| `hv-pty` | `Pty::spawn` sobre `portable-pty` 0.9 (ConPTY / openpty), shell por defecto por SO (`pwsh` → `powershell` en Windows, `$SHELL -l` en macOS), variables `TERM*` |
| `hv-term` | `Session`: hilo lector propio (sin el `EventLoop` de alacritty), **interceptor** de OSC 7/9/9;4/133/777/7501 con un segundo `vte::Parser`, `Term` + `Processor<StdSyncHandler>` (salida sincronizada), respuestas `PtyWrite` (CSI 6n de ConPTY), pegado con bracketed paste saneado, codificación de teclado xterm, `Snapshot` |
| `hv-grid` | tipos de datos puros (`Snapshot`, `SnapCell`, `Rgb8`) para que `hv-render` no dependa de `hv-pty` |
| `hv-render` | wgpu 30: rectángulos instanciados (fondos, cursor, subrayados) + texto con glyphon 0.12 con avance monoespaciado fijo; caché de filas; modo **headless** (`render_to_rgba`) |
| `hv-app` | ventana winit 0.30, teclado, pegar (`Ctrl+Shift+V` / `Cmd+V`), zoom, rueda (scrollback o flechas en pantalla alternativa), títulos, campana; `--screenshot`, `--size`, `--wait`, `--events`, `-e` |

## Resultados
- **Funciona** en Windows 11 (26200), con RTX 4060 por **DX12**: PowerShell, colores de 16, 256 y truecolor, inverso, Unicode (ñ, CJK, ✓ → λ), ajuste de línea, cursor y salida de git en color.
- Tests: 17 en `hv-term` (OSC, teclado, snapshot, salida sincronizada, sesión end-to-end por ConPTY), 3 en `hv-pty`, 2 en `hv-render` y 5 en `hv-app`.
- **Vulkan en Windows con capas de terceros** (OBS, Overwolf) llenaba el log de errores del cargador → se fuerza **DX12 en Windows** (`WGPU_BACKEND` sigue pudiendo cambiarlo), como decía `platforms.md`.
- Al verificar no se debe automatizar el escritorio (enviar teclas o hacer capturas de pantalla): el usuario puede estar usando el PC. Por eso existe `--screenshot`, que ejecuta y renderiza **sin ventana**. Es también la base de los tests dorados.

## Decisiones
- El interceptor con `vte::Parser` propio funciona y es barato. Se descarta `termwiz` para esto.
- Texto con glyphon **solo como paso intermedio**: el atlas propio, el box-drawing por código y el damage tracking siguen en F1 (F1-09…F1-12).
- El motor VT se queda en `alacritty_terminal` hasta SP-1b.

## Pendiente
- Medir N-01…N-12 (F1-28).
- IME (F1-15), selección y copia (F1-18), ratón (F1-16), protocolo de teclado de Kitty (F1-30).
