# Backlog

Tamaño: **XS** ≤ 0.5 días · **S** ≈ 1 día · **M** ≈ 2–3 días · **L** ≈ 4–5 días · **XL** > 1 semana (partirla antes de empezar).
"Req" hace referencia a `docs/requirements.md`. Cada fila se convierte en un issue de GitHub con su etiqueta de fase.

---

## SP — Spikes de riesgo (antes de F1, 2 semanas)

**Estado (2026-11-21):** ✅ SP-1 ([nota](spikes/SP-1.md)), ✅ SP-1b (libghostty-vt 1.0 parsea ×2–2.4 más rápido que alacritty_terminal, pero no se cambia ahora: bindings 1.0 sin publicar, tipos `!Send`, Zig en CI; se revisa al empezar F7 o si sale un crate sobre la 1.x; [nota](spikes/SP-1b.md)), 🟡 SP-2 (falta Windows 10; [nota](spikes/SP-2.md)), 🟡 SP-3 (barra lateral de egui en la ventana real; AccessKit conectado; faltan IME en campos y la prueba con lectores de pantalla; [nota](spikes/SP-3.md)), ✅ SP-4 (hook → IPC en ~11 ms; falta WSL; [nota](spikes/SP-4.md)), ✅ SP-5 (coincidencia exacta con ccusage; [nota](spikes/SP-5.md)), ✅ SP-7 (cargo-packager: dmg, deb, AppImage y MSI sin firmar en CI; [nota](spikes/SP-7.md)). ✅ SP-6 (swash sin hinting y mezcla sRGB; [nota](spikes/SP-6.md)). **Spikes cerrados** salvo lo que depende de hardware (Windows 10 en SP-2) o de pruebas manuales (IME y lectores de pantalla en SP-3, WSL en SP-4).

Cada spike es código desechable en la rama `spike/…` y termina con una nota en `docs/spikes/<id>.md`: qué se probó, resultado y decisión.

| ID | Spike | Pregunta que responde | Tam. |
|---|---|---|---|
| SP-1 | Shell real pintada con `alacritty_terminal` + `wgpu` en los 3 SO, con un lector propio + interceptor de OSC (sin el `EventLoop` de serie) | ¿El stack base y el interceptor funcionan y rinden? | L |
| SP-1b | libghostty-vt 1.0 con bindings mínimos propios sobre su ABI C en los 3 SO (incluye el toolchain de Zig en CI) | ¿Compensa cambiar de motor VT? | M |
| SP-2 | ConPTY: ¿llegan OSC 7, 8, 9, 9;4, 52, 133, 777, **7501** y DECSET 2026? Con el ConPTY del sistema y con el incluido (NuGet 1.25); Windows 10 22H2 y 11; respuesta a `CSI 6n` con `INHERIT_CURSOR` | ¿El ConPTY incluido resuelve todo? | M |
| SP-3 | egui y la rejilla propia en la misma pasada; IME en el panel; AccessKit leyendo la barra lateral | ¿Se valida el ADR 0003? | M |
| SP-4 | Hook de Claude Code → `hivetty hook` → IPC → cambio de estado; medir la latencia en los 3 SO; invocarlo desde WSL | ¿Se cumple N-08 (≤ 300 ms)? | M |
| SP-5 | Parser de logs de Claude Code frente a `ccusage` con datos reales | ¿Cuadran los totales (±1%)? | S |
| SP-6 | Calidad de texto: `swash` frente a los rasterizadores del SO; box-drawing por código; mezcla lineal o sRGB | ¿Qué pipeline de texto usamos? | M |
| SP-7 | Paquetes sin firmar (`.dmg`, `.deb`, AppImage y MSI) generados en CI con la herramienta elegida | ¿Qué herramienta de empaquetado? | S |

---

## F0 — Cimientos

**Estado (2026-11-21):**
- ✅ Hechas: F0-01 (Rust 1.100.0 fijado), F0-02, F0-03, F0-04 (CI en verde en los 3 SO), F0-05, F0-06 (MIT OR Apache-2.0), F0-07, F0-08 (repo **público** con etiquetas, milestones, ruleset sin force-push y reporte privado de vulnerabilidades), F0-09 (Dependabot + `audit.yml` diario), F0-10 (skills en `.claude/skills`, ver `SOURCES.md`), F0-11 (esqueleto de `xtask` con `ci` funcional) y F0-12 (versión, SO y arquitectura; la GPU llega con wgpu en F1).
- ✅ **F0 cerrada.** rust-docs-mcp queda opcional (no se instaló para no tocar la configuración global de Claude).

| ID | Tarea | Depende | Tam. | Aceptación |
|---|---|---|---|---|
| F0-01 | Instalar Rust (rustup, MSVC en Windows) y fijar la versión en `rust-toolchain.toml` | — | XS | `cargo --version` en las máquinas de desarrollo |
| F0-02 | Workspace con los crates de `architecture.md` vacíos y un crate `xtask` | F0-01 | S | `cargo build` en los 3 SO |
| F0-03 | `[workspace.lints]`, `rustfmt.toml` y `clippy.toml`; `unsafe_code = "deny"` (no `forbid`, para que `hv-platform` pueda permitirlo) | F0-02 | XS | clippy limpio |
| F0-04 | CI: fmt, clippy, nextest y deny; matriz macOS, Ubuntu y Windows; caché; Actions fijadas por SHA | F0-02 | M | PR vacío en verde |
| F0-05 | `deny.toml` (licencias, advisories, orígenes) | F0-02 | XS | `cargo deny check` en verde |
| F0-06 | Decidir la licencia (`docs/licensing.md`) y añadir `LICENSE-*` | — | XS | archivos presentes |
| F0-07 | `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md` y plantillas de issue y PR | — | S | plantillas visibles en GitHub |
| F0-08 | Repo en GitHub (público), etiquetas, milestones por fase y protección de `main` | — | S | — |
| F0-09 | Renovate o Dependabot (crates y Actions) y `cargo-audit` diario | F0-04 | XS | primer PR automático |
| F0-10 | Revisar e instalar las skills elegidas (`docs/skills.md`) | — | S | revisión anotada en `docs/skills.md` |
| F0-11 | `xtask`: `golden-update`, `fixtures`, `dist`, `bench` | F0-02 | S | `cargo xtask --help` |
| F0-12 | Subcomando `hivetty --doctor` (esqueleto) | F0-02 | XS | imprime la versión, el SO y la GPU |

## F1 — Terminal mínimo usable

**Estado (2026-11-21):**
- ✅ Hechas (primera versión): F1-01, F1-03…F1-08, F1-10, F1-11 (subrayado simple), F1-14, F1-15 (sin pintar el preedit), F1-16…F1-27, F1-28 (throughput al 83 % del emulador solo; ver [perf.md](perf.md)), F1-29, F1-30 (Kitty: flags 1 y 8), F1-31, F1-32, F1-33 (PATHEXT; el fork de portable-pty sigue pendiente), F1-34, F1-35 y F1-36 (preedit del IME subrayado en el cursor; `--screenshot --preedit` para probarlo).
- ⏳ Pendientes: F1-02 (barra de título propia: va con la UI de egui en F2), F1-09 (atlas propio) y F1-12 (damage tracking fino), aplazadas hasta que las mediciones con muchos paneles lo pidan; F1-13 (renderer por software).
- Bug corregido: en Windows, ConPTY no cierra la salida cuando termina el proceso. La salida se detecta con un hilo que espera al hijo.
- Herramientas nuevas: `hivetty --screenshot` (render headless; el CI sube capturas de los 3 SO), `--events`, `--scroll` y `hivetty usage`.

| ID | Tarea | Req | Depende | Tam. | Aceptación |
|---|---|---|---|---|---|
| F1-01 | Ventana `winit`: DPI por monitor, redimensionado, foco, cierre y cambio de monitor | W-01 | F0 | M | ventana estable en los 3 SO |
| F1-02 | Barra de título personalizada: semáforos en macOS, botones en Windows y CSD en Wayland | W-02 | F1-01 | M | arrastrar, doble clic y maximizar funcionan |
| F1-03 | `hv-pty`: abrir, leer y escribir, redimensionar, matar el árbol (grupos de procesos en Unix, *job objects* en Windows) | — | F0 | L | test de integración en los 3 SO |
| F1-04 | Detección de shell y perfiles (pwsh, powershell, cmd, WSL y Git Bash); shell de login en macOS | — | F1-03 | S | test por SO |
| F1-05 | Variables de entorno del PTY (`TERM`, `COLORTERM`, `TERM_PROGRAM`, `HIVETTY_*`) | — | F1-03 | XS | `env` dentro de la shell |
| F1-06 | `hv-term`: envolver `Term`, hilo lector con lotes y *backpressure*, instantáneas sin bloqueo | P-01 | F1-03 | L | `cat` de 1 GB no congela (N-12) |
| F1-07 | Salida sincronizada (DECSET 2026) con timeout | P-02 | F1-06 | M | grabación de Claude Code sin frames rotos |
| F1-08 | `hv-render`: dispositivo wgpu, selección de backend, pérdida de dispositivo y respaldo | N-22 | F1-01 | M | arranca con `HIVETTY_RENDERER=gl/software` |
| F1-09 | Atlas de glifos (`swash` + `etagere`), variantes, emoji en color y fallback (`cosmic-text`/`fontdb`) | — | F1-08 | L | pantalla de prueba con CJK, emoji y Nerd Font |
| F1-10 | Box-drawing y powerline dibujados por código | — | F1-09 | M | dorado pixel-perfect |
| F1-11 | Pasadas de fondos, glifos, cursor y decoraciones (subrayados de cinco estilos y color 58) | — | F1-09 | L | dorados |
| F1-12 | *Damage tracking* y ritmo de frames; 0% de CPU en reposo | N-04 | F1-11 | M | medición en reposo |
| F1-13 | Renderer por software (`tiny-skia`) con el mismo trait | — | F1-11 | M | dorado CPU = GPU |
| F1-14 | Teclado: codificación xterm, protocolo de Kitty, `Option`/`Alt` como Meta, `Shift+Enter` | P-07 | F1-06 | L | tests de codificación por tecla |
| F1-15 | IME (preedit en el cursor, confirmación) en los 3 SO | — | F1-14 | M | checklist de IME |
| F1-16 | Ratón: modos de reporte, SGR 1006, scroll alternativo y arrastre | — | F1-06 | M | `htop` y `vim` con ratón |
| F1-17 | Scrollback: rueda, barra, Shift+PgUp, mantener la posición al llegar salida y **Jump to bottom** | P-03, P-04 | F1-11 | M | test de UI |
| F1-18 | Selección (palabra, línea, bloque), copiar y pegar (`arboard`), selección primaria en Linux | P-12 | F1-11 | M | tests manuales y unitarios |
| F1-19 | Bracketed paste y saneado del pegado | — | F1-18 | S | test de inyección de `ESC[201~` |
| F1-20 | OSC 52 (solo escritura y límite) | P-17 | F1-06 | S | test |
| F1-21 | Búsqueda en el scrollback con regex | P-10 | F1-17 | M | test |
| F1-22 | URLs, rutas `archivo:línea` y OSC 8 con `Cmd/Ctrl+clic` | P-08, P-09 | F1-11 | M | abre el editor en la línea |
| F1-23 | Campana (visual, sonido y atención del SO) | P-15 | F1-11 | S | — |
| F1-24 | Zoom de fuente y recarga de la fuente | P-14 | F1-09 | S | — |
| F1-25 | Arrastrar y soltar archivos → ruta entre comillas según la shell | P-06 | F1-04 | S | rutas con espacios en las 4 shells |
| F1-26 | `hv-config` mínimo: fuente, tema, shell y recarga en caliente | — | F0 | M | cambiar la fuente en caliente |
| F1-27 | Grabación (`--record`, asciicast v2) y reproductor para tests | — | F1-06 | S | reproducir el corpus |
| F1-28 | Medidas de N-01…N-07 y N-12; informe en `docs/perf.md` | N-* | todo F1 | M | objetivos cumplidos o con issue abierto |
| F1-29 | Interceptor de OSC/DCS/APC con partición por límite de OSC y anclaje a filas (`vt-compliance.md` §1) | P-01 | F1-06 | L | tests con bytes de OSC 7/9/133/777/7501 mezclados con texto |
| F1-30 | Portar del frontend de Alacritty (con atribución): codificación de teclado (incl. Kitty), del ratón y `builtin_font.rs` | P-07 | F1-14 | L | tests de codificación iguales que los de Alacritty |
| F1-31 | Windows: `xtask conpty-fetch` descarga y verifica (hash) el NuGet de ConPTY 1.25; se copia junto al exe; flags `INHERIT_CURSOR` + `GLYPH_WIDTH_WCSWIDTH` | — | F1-03 | M | `--doctor` informa "OpenConsole incluido" |
| F1-32 | Responder a consultas (DA, DSR, XTVERSION, OSC 10/11) y a `CSI 6n` de ConPTY sin retraso | — | F1-06 | S | arranque de ConPTY sin bloqueo |
| F1-33 | Fork o dependencia git de `portable-pty` con los arreglos de Windows de WezTerm main | — | F1-03 | S | `kill()` fiable en Windows |
| F1-34 | Windows release (subsistema GUI): conectarse a la consola padre (`AttachConsole`) para que `--help`, `--doctor` y `usage` impriman en la terminal desde la que se lanzan | — | F0-12 | S | `hivetty.exe --doctor` imprime en cmd/PowerShell |
| F1-35 | Box-drawing: esquinas redondeadas reales (`╭╮╯╰`) y uniones correctas de líneas dobles | — | F1-10 | S | dorado de cajas |
| F1-36 | Pintar el texto en preedit del IME en la posición del cursor | — | F1-15 | S | composición visible con japonés |

## F2 — Pestañas, paneles y sesiones

**Estado (2026-11-22):**
- ✅ F2-01: modelo `Tabs → Tab → PaneNode` puro y con tests (`hv-app/src/workspace.rs`). El renderer pinta varios paneles por frame (`PaneView`), con caché de filas por panel, recorte, separadores de 1 px y atenuación del 85 % sin foco. El trait `PaneHost` sigue pendiente: hoy la app habla con `hv_term::Session` directamente, y el renderer ya solo recibe instantáneas.
- ✅ F2-02: dividir a la derecha o abajo, cerrar, mover el foco con flechas (por geometría), redimensionar con teclado o arrastrando el separador (zona de 6 px), maximizar o restaurar el panel.
- ✅ F2-03: barra de pestañas con título, cerrar (× y clic central), `+`, doble clic para renombrar (vacío = título automático) y arrastrar para reordenar. Los iconos de agente llegan con F4.
- ✅ F2-04: spinner (8 puntos a 12 fps) cuando trabaja, si no el punto del estado más urgente; punto azul de "no visto" cuando un panel sin foco termina o pide algo; campana sin ver; barra fina de progreso OSC 9;4 (roja si es de error). Se limpian al enfocar el panel. Falta respetar "reducir movimiento" del sistema (F6).
- 🟡 F2-06: `keymap.rs` con acciones con nombre, acordes en texto (`primary` = Cmd/Ctrl), los atajos de `keybindings.md` por SO, `[[keybindings]]` en `config.toml` (reasignar, `none`, `send_text`) y condiciones `when` (`os`, `search_open`, `terminal_focused`, `agent`, `!`). Recarga en caliente. Falta saber el agente del panel para `agent == …` (F4).
- ✅ F2-07: paleta de comandos (`Ctrl+Shift+P` / `⇧⌘P`) con filtro difuso, flechas, Enter y Esc, y el atajo de cada acción. Mientras está abierta, el teclado y el IME van a egui (cubre el "IME en un campo de egui" de SP-3; falta probarlo a mano con japonés).
- ✅ F2-08: al cerrar la ventana se guardan pestañas, divisiones, la carpeta de cada panel y la geometría de la ventana (`layout.json` en el directorio de datos, escritura atómica, versionado). Al abrir sin `-e` se restauran: carpetas que ya no existen caen a la de por defecto, la posición solo si sigue en un monitor conectado y como mucho 64 paneles. Opciones `[window] restore_layout` y `remember_geometry`.
- ✅ F2-10: 10 paneles con salida continua a 1080p: p50 8,2 ms y p95 12,1 ms por frame (ver [perf.md](perf.md)). Medir en portátil con iGPU queda para la checklist de F6.
- ✅ F2-09: menú ☰ en la barra de pestañas (Windows y Linux) con las mismas acciones y atajos que la paleta; en macOS, barra de menús nativa con `muda` (hivetty, Shell, Edit, View, Window), cuyos atajos salen del keymap y se rehacen al recargar la configuración. El de macOS solo está comprobado con `cargo clippy --target aarch64-apple-darwin`; falta probarlo en un Mac.
- ✅ F2-05: varias ventanas (`Ctrl+Shift+N` / `⌘N`), cada una con su renderer, pestañas y estado de entrada; "Move tab to new window" desde la paleta o el menú (con los procesos vivos). Cerrar la última pestaña cierra su ventana y la app sale con la última ventana; se guarda el layout de la última ventana cerrada. Falta arrastrar una pestaña de una ventana a otra.
- Cada panel recibe su `HIVETTY_SESSION_ID` y los hooks se enrutan a su panel. Los paneles nuevos heredan el cwd del panel con foco (adelanta F3-08 cuando la shell informa el cwd).
- `hivetty --screenshot --ui --split` captura dos paneles con la barra de pestañas.

| ID | Tarea | Req | Depende | Tam. |
|---|---|---|---|---|
| F2-01 | Modelo `Workspace → Tab → PaneNode` y trait `PaneHost` (ADR 0002) | W-08 | F1 | M |
| F2-02 | Dividir, cerrar, redimensionar con arrastre o teclado, maximizar panel | W-08, W-09 | F2-01 | L |
| F2-03 | Barra de pestañas: títulos, iconos, `+`, cerrar, reordenar y renombrar | W-04, W-05, W-06 | F2-01 | L |
| F2-04 | Indicadores en la pestaña (spinner, no visto, progreso OSC 9;4 y campana) | P-16 | F2-03 | M |
| F2-05 | Varias ventanas y mover pestañas entre ellas | W-10 | F2-03 | L |
| F2-06 | Sistema de atajos con contextos `when` y `Primary` | — | F2-02 | M |
| F2-07 | Paleta de comandos | — | F2-06 | M |
| F2-08 | Guardar y restaurar el layout, los directorios y la geometría | W-12 | F2-01 | M |
| F2-09 | Menús nativos (`muda`) en macOS y menú de hamburguesa en Windows/Linux | — | F2-06 | M |
| F2-10 | Rendimiento con 10 pestañas × 2 paneles | N-07 | F2-02 | S |

## F3 — Integración con la shell y chips

**Estado (2026-11-22):**
- ✅ F3-01…F3-04: scripts en `assets/shell/` incrustados en el binario y copiados al directorio de datos (por versión, escritura atómica). zsh por `ZDOTDIR` (restaura el del usuario y carga su `.zshenv`), bash con `--rcfile` que carga antes `/etc/profile` y `~/.bash_profile` o `~/.bashrc` (según sea de login), fish por `XDG_DATA_DIRS` + `vendor_conf.d`, y PowerShell 5.1/7 con `-NoExit -Command . 'hivetty.ps1'` que envuelve `prompt`. Emiten OSC 133 A/B/C/D (con código de salida) y OSC 7 (y OSC 9;9 en PowerShell). Nunca se tocan si la línea de comandos ejecuta un script o un comando. Opción `[shell] integration`. PowerShell probado aquí con el interceptor; zsh, bash y fish faltan por probar en macOS/Linux.
- ✅ F3-07: las marcas OSC 133 se anclan a su línea (el interceptor parte el bloque en el límite de cada secuencia con `advance_until_terminated` y se lee la fila del cursor). Barra en el margen izquierdo verde, roja o gris según el código de salida; saltar al prompt anterior o siguiente (`Ctrl+Shift+↑/↓`, `⌘↑/↓`); "Copy last command output" en la paleta y el menú. Límite conocido: con el scrollback lleno las marcas más antiguas se desplazan; se borran al limpiar el scrollback.
- ✅ F3-09 (sin vigilante de archivos: caché de 3 s por directorio, que basta para prompts), F3-10, F3-11 (solo la rama, leyendo `.git/HEAD` y worktrees; sucio/adelantado/atrasado necesita `gix`, pendiente) y F3-12: chips de tecnología, rama y versiones en la línea que la shell reserva sobre cada prompt, sin ejecutar nada, sin pasar de `$HOME`, y si no caben no se pintan. ADR 0004 aceptado.
- ✅ F3-13: aviso del sistema cuando un comando de shell que duró al menos `[notifications] long_command_seconds` (30 s) termina con la ventana sin foco o en otro panel ("Command finished" / "Command failed" con el código y la duración). `hv_platform::notify`: `notify-rust` en Windows y Linux y `osascript` en macOS hasta tener el `.app` firmado. En Windows sale con el nombre de PowerShell hasta registrar el AUMID (F6-09). No probado a mano todavía.
- ➖ F3-06: no hace falta (SP-2: ConPTY entrega las OSC sin tocar).
- ⏳ F3-14 (iconos SVG con `resvg`) va con los iconos de agentes de F4; F3-11 sucio/adelantado/atrasado queda para cuando entre `gix`.
- ✅ F3-05 y F3-08: el interceptor ya recibía OSC 7/133, y los paneles nuevos heredan el cwd del panel con foco.

| ID | Tarea | Req | Depende | Tam. |
|---|---|---|---|---|
| F3-01 | Scripts de zsh (inyección por ZDOTDIR que respeta el ZDOTDIR del usuario) | C-01 | F1 | M |
| F3-02 | Script de bash (`--rcfile` envolvente que carga `~/.bashrc`) | C-01 | F1 | M |
| F3-03 | Script de fish (`XDG_DATA_DIRS` + `vendor_conf.d`) | C-01 | F1 | S |
| F3-04 | Script de PowerShell 7 y 5.1 (prompt envuelto y PSReadLine) | C-01 | F1 | M |
| F3-05 | Recibir OSC 133 y OSC 7 en `hv-term` (interceptor si `alacritty_terminal` no los expone) | C-02, C-03 | F3-01 | M |
| F3-06 | Plan B por IPC si SP-2 demostró que ConPTY filtra las secuencias | — | SP-2 | M |
| F3-07 | Marcas de prompt en el margen, saltar entre prompts y copiar la última salida | P-13 | F3-05 | M |
| F3-08 | Heredar el cwd en pestañas y paneles nuevos | C-03 | F3-05 | S |
| F3-09 | Detección de tecnología (§8 de los requisitos) con caché y vigilancia de archivos | C-04 | F3-05 | M |
| F3-10 | Decoraciones ancladas a líneas y pintado de chips con regla de "si no cabe, no se pinta" (ADR 0004) | C-05 | F3-09 | L |
| F3-11 | Chips de rama git y estado (`gix`, sin ejecutar `git`) | C-06 | F3-10 | M |
| F3-12 | Chips de versiones leídas de archivos | C-07 | F3-10 | S |
| F3-13 | Notificación de comando largo | C-08 | F3-05 | S |
| F3-14 | Iconos SVG → atlas (`resvg`) | C-05 | F1-09 | S |

## F4 — Barra lateral de agentes

**Estado (2026-11-22):**
- ✅ F4-01: los 11 perfiles en `assets/agents/*.toml` (incrustados). Grok CLI, Junie y Antigravity llevan `verified = false`: falta confirmar el nombre de su comando.
- ✅ F4-02: `hivetty agents list` busca cada comando en el PATH (con PATHEXT en Windows).
- ✅ F4-03: un hilo con `sysinfo` mira cada segundo los procesos bajo la shell de cada panel (nombre del ejecutable, o `node`/`python` con argumentos que lo identifican). El panel sabe qué agente corre, la fila lo usa de título por defecto y el keymap admite `when = "agent == 'claude-code'"`.
- ✅ F4-04/05/06/08/10/26 (parciales de SP-4): estados, prioridad de fuentes, IPC, `hivetty hook`, OSC 7501, notificaciones OSC 9/777 y subtítulos desde los hooks.
- 🟡 F4-11/F4-12: barra lateral con búsqueda (título, subtítulo, proyecto y agente), filtros con contadores (All, Needs you, Working, Done), grupos por proyecto (la raíz del repo git si la hay) con su número, y punto azul de no visto. Faltan el atajo `/`, el indicador "≈" de estado estimado y el ancho redimensionable.
- 🟡 F4-18: notificación del sistema al pasar a Waiting, Done o Error (una vez por estado), con `<Agent> · <project>` / `<título>: <motivo>`, nunca del panel que se está mirando, `[notifications] on` por estado, y más de 3 en 10 s se juntan en "N sessions need you". Falta la acción "Focus" al pulsarla y los sonidos por estado.
- 🟡 F4-19: aviso de atención del SO (parpadeo de la barra de tareas, rebote del dock, urgency hint) cuando una sesión pide algo con la ventana sin foco. Falta el número en el badge (overlay de Windows y dock de macOS).
- 🟡 F4-13/F4-14: botón "New session" con su atajo (`Ctrl+Shift+A` / `⇧⌘T`) y diálogo (agente, carpeta validada, prompt opcional, argumentos, nueva pestaña o dividir). Abre una shell en la carpeta y escribe el comando del agente en el primer prompt (o a los 1,5 s si la shell no tiene integración), así el agente hereda el PATH y el panel sobrevive al salir. El prompt solo se pasa a los agentes con `prompt_arg` (Claude Code y Codex). Dock con los agentes instalados (iniciales hasta tener iconos). Faltan el selector de carpeta nativo, las carpetas recientes, los worktrees (F4-22) y los perfiles.
- ✅ F4-20: "Go to next session that needs you" (`Ctrl+Shift+J` / `⌘J`): primero las que esperan al usuario y después las terminadas sin ver, en todas las ventanas.
- Endurecimiento tras la revisión de seguridad del instalador: la ruta del binario va entre comillas simples (bash no expande `$`), y `settings.json` conserva sus permisos y se escribe a través del enlace simbólico si lo es.
- ✅ F4-07 (Claude Code): `hivetty agents install-hooks|uninstall-hooks [--dry-run]` enseña los cambios y pide `y`; copia `settings.json.hivetty-bak`, escritura atómica, conserva el orden de las claves y las entradas del usuario, actualiza la ruta si el binario se movió y desinstala solo lo suyo. Falta el shim estable de Windows (F6) y el resto de agentes.

| ID | Tarea | Req | Depende | Tam. |
|---|---|---|---|---|
| F4-01 | `hv-agents`: perfiles TOML y perfiles incluidos (los 11 agentes) | A-01 | F2 | M |
| F4-02 | Detección de agentes instalados (PATH y rutas por SO) | A-02 | F4-01 | S |
| F4-03 | Detección por árbol de procesos (`sysinfo`, 1 s) | A-03 | F4-01 | M |
| F4-04 | Máquina de estados con prioridades de fuente y tests exhaustivos | A-04, A-05 | F4-01 | M |
| F4-05 | `hv-ipc`: servidor (socket o pipe), token y protocolo versionado | — | F0 | L |
| F4-06 | Subcomando `hivetty hook`: lee stdin, reenvía, sale en < 1 s y siempre con 0 | A-05 | F4-05 | S |
| F4-07 | Instalador y desinstalador de hooks con diff, copia y marcador (Claude Code primero; después el resto) | A-06 | F4-06 | L |
| F4-08 | Fuente 2: OSC 9/777, título y BEL → eventos de estado | A-05 | F4-04 | S |
| F4-09 | Fuente 3: heurística de actividad | A-05, A-19 | F4-04 | S |
| F4-10 | Título y subtítulo de la sesión (OSC 2 y acciones de los hooks de herramientas) | A-07, A-08 | F4-06 | M |
| F4-11 | UI de la barra lateral: grupos, filas, tiempos e indicador de no visto | S-05…S-08 | F4-04 | L |
| F4-12 | Búsqueda y filtros con contadores | S-03, S-04 | F4-11 | M |
| F4-13 | New session (diálogo) y botón de carpeta | S-01, S-02, A-16 | F4-11 | M |
| F4-14 | Dock de agentes | S-11 | F4-02 | S |
| F4-15 | Archivar y la sección Archived | S-12 | F4-11 | M |
| F4-16 | Menús contextuales de sesión y de proyecto | S-13, S-14 | F4-11 | M |
| F4-17 | Tarjeta al pasar el ratón | S-15 | F5-03 | M |
| F4-18 | Notificaciones del sistema con acción "Focus" y agrupado | A-09 | F4-04 | L |
| F4-19 | Badge en el dock o la barra de tareas y urgency hint | A-10 | F4-04 | M |
| F4-20 | "Ir a la siguiente sesión que te necesita" | A-11 | F4-11 | S |
| F4-21 | Reanudar sesiones tras reiniciar (comando de resume por agente) | A-12 | F4-10 | M |
| F4-22 | Nueva sesión en un worktree y limpieza | A-14 | F4-13 | M |
| F4-23 | Tiempo de agente observado → tabla `agent_interval` | A-21 | F4-04 | S |
| F4-24 | Agentes dentro de tmux, SSH o WSL (degradación y documentación) | A-20 | F4-08 | M |
| F4-25 | Accesibilidad de la barra lateral (AccessKit y anuncios) | §8 UI | F4-11 | M |
| F4-26 | **Receptor OSC 7501** (estados, `kind`, `progress`, `msg` en base64, varios `id`, respuesta a `?`) como fuente nº 1 | A-25 | F1-29 | M |
| F4-27 | Estado agregado panel → pestaña → proyecto → badge; "Done" persiste como no leído hasta verlo | S-07, A-04 | F4-04 | S |
| F4-28 | **Bandeja de atención** enlazada a los paneles y anillo en el panel que necesita atención | S-19 | F4-11 | M |
| F4-29 | **Peek**: ver la pregunta que bloquea y responder sin cambiar de pestaña | S-20 | F4-28 | M |
| F4-30 | Metadatos de fila: rama, diff (+/−), PR y CI (vía `gh` si está), puertos y % de contexto | S-21 | F4-11 | M |
| F4-31 | Worktrees: script de preparación por repo, copia de `.worktreeinclude`/`.env`, puerto por worktree, límite y limpieza con instantánea | A-14 | F4-22 | M |
| F4-32 | Statusline propia que reenvía `rate_limits` y el % de contexto por IPC | U-65 | F4-05 | S |
| F4-33 | Reenvío de notificaciones al móvil (ntfy, Telegram o webhook) | A-26 | F4-18 | S |

## F5 — Panel de uso

**Estado (2026-11-22):**
- ✅ F5-02: el parser de Claude Code también cuenta las tareas (prompts del usuario; no los resultados de herramientas, mensajes meta ni comandos locales).
- ✅ F5-06: `hv_usage::dashboard::build`, pura y con tests de las fórmulas U-*: 30 días contando hoy, deltas frente al periodo anterior (tope visual +999 %), tokens por día con días vacíos, días activos y media, mapa de calor 7×24 en hora local (lunes primero) con 5 niveles por cuartiles, modelos con cuota de proveedor por input, agentes.
- ✅ F5-07…F5-12: panel "Agent usage" (`Ctrl+Alt+U` / `⌥⌘U`, paleta y menú) sobre los paneles: tarjetas KPI con delta y sparkline, tokens por día (input arriba y output abajo con su escala, tooltip, eje primer día / centro / today, "N of M days with agents · X/day"), mapa de calor con leyenda y "most active", modelos (barra por proveedor, 6 filas y "N more") y agentes; selector 7/30/90/All y `esc`; estados de carga y vacío; números compactos y con miles. Probado con los datos reales de este equipo (`--screenshot --ui --usage`).
- ✅ F5-01/F5-04 (con cambio de diseño, [ADR 0006](adr/0006-usage-cache-not-sqlite.md)): caché incremental en `usage.cache` (`postcard`) con el byte leído de cada log; solo se leen las líneas nuevas completas. 110 MB de logs → 650 KB de caché.
- ✅ AGENT TIME (U-13) desde los logs según ADR 0005: tramos de cada sesión con huecos de menos de 5 min, concurrencia máxima ("up to N at once"), tiempo por día (sparkline) y por agente (columna de la tabla). Los intervalos observados en vivo (F4-23) lo afinarán.
- ✅ U-51: pulsar un agente filtra todo el panel; un chip "Claude Code ×" lo quita.
- ✅ F5-14: `hivetty usage [--json|--csv] [--project NAME] [--days N]` (tabla con totales, CSV o JSON; las tareas se filtran por las sesiones del proyecto). Usa la misma caché incremental.
- ⏳ Pendiente: otros agentes (F5-03), coste (F5-13/F5-17: los precios no se inventan; irán en `pricing.toml` editable o desde LiteLLM con caché), límites del plan (F5-19), OTLP (F5-15), WSL (F5-05).

| ID | Tarea | Req | Depende | Tam. |
|---|---|---|---|---|
| F5-01 | `hv-usage`: esquema SQLite, migraciones (`rusqlite_migration`) y cursores de ingesta | — | F0 | M |
| F5-02 | Parser de Claude Code con dedupe y fixtures | ADR 0005 | F5-01 | M |
| F5-03 | Parsers de Codex, Gemini CLI, opencode y los demás con datos disponibles | ADR 0005 | F5-01 | L |
| F5-04 | Vigilancia de archivos e ingesta incremental en segundo plano | N-09 | F5-02 | M |
| F5-05 | Logs de WSL desde Windows | U-64 | F5-04 | S |
| F5-06 | Agregaciones: KPIs, deltas, por día, mapa de calor, modelos y agentes (todas las fórmulas de U-*) | U-10…U-53 | F5-02 | L |
| F5-07 | UI: tarjetas KPI y sparklines | U-10…U-14 | F5-06 | M |
| F5-08 | UI: tokens por día | U-20…U-23 | F5-06 | M |
| F5-09 | UI: mapa de calor | U-30…U-33 | F5-06 | M |
| F5-10 | UI: tablas de modelos y agentes con filtro por agente | U-40…U-53 | F5-06 | M |
| F5-11 | Selector de rango, `Esc`, estados vacío y parcial | U-01…U-03 | F5-07 | S |
| F5-12 | Formato compacto de números y fechas en inglés | §6.7 | F5-07 | S |
| F5-13 | Coste estimado con `pricing.toml` | U-60 | F5-06 | S |
| F5-14 | Exportar a CSV/JSON y filtro por proyecto | U-61, U-62 | F5-06 | S |
| F5-15 | Receptor OTLP local opcional | ADR 0005 | F5-06 | M |
| F5-16 | Validar los totales frente a `ccusage` (±1%) | — | F5-06 | S |
| F5-17 | Precios de LiteLLM con caché offline y sobrescrituras locales | U-60 | F5-13 | S |
| F5-18 | Tokens de razonamiento (Codex, Gemini) en el esquema y en el panel | — | F5-03 | S |
| F5-19 | Límites de 5 h y semanal con cuenta atrás, ritmo de gasto y estimación de agotamiento | U-65 | F4-32 | M |

## F6 — Pulido y distribución

| ID | Tarea | Depende | Tam. |
|---|---|---|---|
| F6-01 | Temas: claro, oscuro, alto contraste; importar temas de iTerm2 y Alacritty; seguir el tema del sistema | F1 | M |
| F6-02 | Configuración completa (`docs/config.md`), esquema JSON y avisos de error | F1-26 | M |
| F6-03 | Asistente de primer arranque | F4, F5 | M |
| F6-04 | Revisión de todos los textos en inglés (UI, errores, `--help`, docs de usuario) y que estén centralizados en `strings.rs` | F4 | S |
| F6-05 | Accesibilidad del texto del terminal (líneas visibles) y alto contraste | F4-25 | L |
| F6-06 | Narrador (TTS por SO) | F4 | M |
| F6-07 | Empaquetado macOS: universal, `.app`, `.dmg`, firma y notarización | SP-7 | L |
| F6-08 | Empaquetado Linux: `.deb`, AppImage, `.desktop`, `x-terminal-emulator` | SP-7 | M |
| F6-09 | Empaquetado Windows: MSI o MSIX, firma, AUMID para las notificaciones, manifiesto winget | SP-7 | L |
| F6-10 | Canales: Homebrew cask, winget, Scoop, repo APT | F6-07…09 | M |
| F6-11 | Actualizaciones automáticas firmadas | F6-07…09 | L |
| F6-12 | `--doctor` completo y modo `--safe` | F4, F5 | M |
| F6-13 | Documentación de usuario (guía, FAQ, solución de problemas) y web | — | M |
| F6-14 | Crash handler local e informe opcional | — | M |
| F6-15 | SBOM, atestaciones y `THIRD_PARTY_NOTICES` | F6-07…09 | S |
| F6-16 | Checklist manual completa en las 8 configuraciones (`testing.md` §6) | todo | L |

## F7 — Extras (v1.1+)

| ID | Tarea | Req |
|---|---|---|
| F7-01 | `hivetty-mux`: servidor de PTYs separado y reconexión | A-13, ADR 0002 |
| F7-02 | Imágenes inline (Kitty, iTerm2, sixel) | P-19 |
| F7-03 | Vista de cambios (diff) por sesión | A-15 |
| F7-04 | Puertos detectados → navegador (globo) | A-24, W-07 |
| F7-05 | Modo copia vi y selección rápida con pistas | P-11 |
| F7-06 | Modo desplegable (quake), siempre encima | W-11 |
| F7-07 | Plugins WASM | — |
| F7-08 | Control remoto por CLI (`hivetty cli send-text`…) con permisos | — |
| F7-09 | Accesibilidad completa del texto (UIA, NSAccessibility, AT-SPI) | §8 UI |
| F7-10 | Compartir o exportar la transcripción de una sesión | A-22 |
| F7-11 | Cola de sesiones y límite de concurrencia | A-23 |
