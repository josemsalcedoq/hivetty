# Conformidad VT y secuencias de escape

Datos de `alacritty_terminal` 0.26.0 (vte 0.15), verificados el 2026-11-21 contra su código y su página `alacritty-escapes(7)`.

**Columnas:**
- **Prio:** M = imprescindible en la v1.0 · S = debería · C = después.
- **Quién lo hace:**
  - `AT` = `alacritty_terminal` lo resuelve.
  - `INT` = nuestro **interceptor** previo al parser (ver §1).
  - `FE` = nuestro frontend (teclado, ratón, render).
  - `PORT` = código que hay que portar del frontend de Alacritty (Apache-2.0, con atribución).

## 1. Arquitectura del interceptor

`alacritty_terminal` **solo** atiende las OSC 0, 1, 2, 4, 8, 10, 11, 12, 50, 52, 104, 110, 111 y 112. El resto (7, 9, 133, 633, 777, 1337 y 7501) va a un log de depuración y se pierde. DCS no hace nada y APC/SOS/PM se tragan. El enum `Event` no tiene variantes para nada de eso.

Por eso:
1. **No se usa el `EventLoop` de serie** (es dueño del bucle de lectura del PTY). `hv-term` tiene su propio hilo lector.
2. Cada bloque de bytes pasa primero por un **segundo `vte::Parser` con un `Perform` propio** que solo mira `osc_dispatch`, `hook`/`put`/`unhook` (DCS) y APC, y emite `TermEvent`s tipados (`Cwd`, `PromptMark`, `Notify`, `Progress`, `ProgramStatus`, `UserVar`…).
3. Para anclar las marcas (OSC 133, chips, OSC 7501) a una fila, el bloque se **parte en el límite de cada OSC**: se alimenta a `Term` la parte anterior, se lee la fila del cursor y se registra el evento con esa fila.
4. Después se alimenta todo a `vte::ansi::Processor<StdSyncHandler>` → `Term`. El hilo respeta `sync_timeout()` para la salida sincronizada.

Alternativa evaluada para el paso 2: el parser de `termwiz`, que ya devuelve OSC tipadas (`CurrentWorkingDirectory`, `FinalTermSemanticPrompt`, `SystemNotification`, `RxvtExtension`). Se decide en SP-1.

## 2. Controles y CSI básicos

| Función | Prio | Quién |
|---|---|---|
| C0 (BEL, BS, HT, LF, CR…), ESC 7/8, IND, NEL, RI, RIS, DECSC/DECRC | M | AT |
| Movimiento del cursor (CUU…CUP, HVP, CHA, VPA, CHT, CBT) | M | AT |
| Borrado (ED, EL, ECH, DCH, ICH, IL, DL) y scroll (SU, SD, DECSTBM, DECSLRM) | M | AT (verificar DECSLRM) |
| Tabuladores (HTS, TBC) | M | AT |
| Juegos de caracteres (G0/G1, DEC special graphics) | M | AT |
| REP | S | AT |

## 3. SGR

| Función | Prio | Quién |
|---|---|---|
| Negrita, tenue, cursiva, subrayado, parpadeo, inverso, oculto, tachado | M | AT |
| 16, 256 y truecolor (`38;2`/`48;2` con `;` y `:`) | M | AT |
| Estilos de subrayado `4:0…4:5` (simple, doble, ondulado, punteado, discontinuo) | M | AT + FE (pintado) |
| Color de subrayado `58`/`59` | M | AT |
| Sobrerrayado `53` | C | verificar |

## 4. Modos DEC privados

| Modo | Función | Prio | Quién |
|---|---|---|---|
| 1 | teclas de cursor en modo aplicación | M | AT + FE |
| 6, 7 | origen, autowrap | M | AT |
| 12, 25 | parpadeo y visibilidad del cursor | M | AT |
| 47 / 1047 / 1049 | pantalla alternativa | M | AT |
| 1000 / 1002 / 1003 / 1006 / 1016 | ratón (clic, arrastre, movimiento, SGR, SGR en píxeles) | M | AT (modos) + PORT (codificación) |
| 1004 | eventos de foco | M | AT + FE |
| 1007 | scroll alternativo | S | AT |
| 2004 | bracketed paste | M | AT + FE |
| **2026** | **salida sincronizada** (búfer de hasta 2 MiB, 150 ms) | **M** | AT (`StdSyncHandler`). Requisito de la investigación: `CSI 2J` dentro de un bloque sincronizado no debe hacer parpadear |
| 2027 | anchura por grafemas | S | **no soportado** en AT → ver §8 |
| 2031 | aviso de cambio de tema claro/oscuro | S | INT/FE (verificar) |
| DECSCUSR | forma del cursor | M | AT |
| DECRQM | consulta de modos | M | AT (verificar cobertura) |

## 5. OSC

| OSC | Función | Prio | Quién | Notas |
|---|---|---|---|---|
| 0 / 1 / 2 | título | M | AT | |
| 4 / 10 / 11 / 12 / 104 / 110–112 | consultar y fijar la paleta y los colores | M | AT | los agentes consultan el fondo para elegir su tema |
| 7 | directorio actual | M | **INT** | |
| 8 | hiperenlaces | M | AT + FE | |
| 9 | notificación (estilo iTerm2/ConEmu) | M | **INT** | ojo: `9;4` = progreso, `9;9` = cwd (ConEmu) |
| 9;4 | barra de progreso | S | **INT** | pestaña + barra de tareas o dock |
| 52 | portapapeles | M | AT (`Event::ClipboardStore`) | lectura desactivada (`docs/security.md`) |
| 99 | notificaciones de kitty | C | INT | |
| 133 | marcas semánticas de prompt | M | **INT** | |
| 633 | integración de shell de VS Code | C | INT | algunas herramientas lo emiten |
| 777 | notificación (rxvt) | M | **INT** | |
| 1337 | iTerm2 (SetUserVar, CurrentDir, imágenes) | S / C | INT | variables de usuario sí; imágenes en F7 |
| **7501** | **estado de programa para agentes** | **M** | **INT** | ver `docs/agent-integration.md`. Responder a la consulta `?` |

## 6. Consultas y respuestas

| Secuencia | Prio | Quién |
|---|---|---|
| DA1 / DA2 / DA3 | M | AT (revisar qué anuncia) |
| DSR 5/6 (estado y posición del cursor) | M | AT. **Obligatorio con ConPTY**: con `INHERIT_CURSOR` envía `CSI 6n` y **bloquea la entrada hasta que respondamos** |
| XTVERSION (`CSI > q`) | S | INT/FE → `hivetty <versión>` |
| `CSI 14/16/18 t` (tamaños en píxeles y celdas) | S | AT/FE |
| `CSI 21 t` (reportar el título) | — | **desactivado** (seguridad) |
| DECRQSS, XTGETTCAP (DCS) | S | INT (DCS es un no-op en AT) |

## 7. Teclado

| Función | Prio | Quién |
|---|---|---|
| Codificación xterm clásica (modificadores, teclado de aplicación) | M | PORT (`alacritty/src/input/keyboard.rs`) |
| **Protocolo de teclado de Kitty** (`CSI = u`, `> u`, `< u`, `? u`) | M | AT lleva la pila de modos; **la codificación (`build_sequence`) se porta del frontend** |
| `Shift+Enter` para agentes | M | FE (ver `docs/keybindings.md`) |
| Option/Alt como Meta | M | FE (`winit` `set_option_as_alt` en macOS) |

## 8. Unicode y anchura

- Por defecto: **anchura por punto de código compatible con wcwidth** (como `alacritty_terminal`), para coincidir con lo que asumen las shells y TUIs.
- El modo **2027** (anchura por grafemas, propuesta de Contour; lo soportan Ghostty, foot, Contour y WezTerm) no está en AT. Se implementa en F7 o se reconsidera con libghostty-vt (ADR 0001).
- En Windows, crear ConPTY con el flag `PSEUDOCONSOLE_GLYPH_WIDTH_*` que coincida con nuestro modelo (0x10 = WCSWIDTH) para que ConPTY y el terminal coincidan en el ancho de las líneas.
- Pruebas con `ucs-detect` y la pantalla de prueba de emoji, ZWJ y CJK.

## 9. Imágenes (F7)
Ni sixel ni gráficos de kitty en AT. Opciones: interceptar APC (kitty) y DCS (sixel) en INT y pintar en una capa propia, o migrar a libghostty-vt, que trae gráficos de kitty (ADR 0001).

## 10. Box-drawing
Se dibuja por código, sin usar la fuente, como Ghostty, WezTerm y kitty. Punto de partida: `alacritty/src/renderer/text/builtin_font.rs` (Apache-2.0, portar con atribución). Rangos: U+2500–257F, U+2580–259F, U+1FB00–1FBFF, U+2800–28FF (braille) y Powerline (U+E0B0–E0BF).
