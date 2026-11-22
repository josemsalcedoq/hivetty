# Estrategia de pruebas

## 1. Pirámide

| Nivel | Herramienta | Qué cubre | Dónde corre |
|---|---|---|---|
| Unitarias | `cargo nextest` | lógica pura: máquina de estados, detección de tecnología, keymaps, formateo de números, fórmulas de uso | CI en las 3 plataformas |
| Snapshot de la rejilla | `insta` | alimentar bytes → instantánea textual de la rejilla (celdas, atributos, cursor) | CI |
| Propiedades | `proptest` | invariantes de la rejilla: redimensionar y *reflow* no pierden texto, la selección siempre es válida | CI |
| Dorados de render | captura sin cabeza (`--screenshot`), con GPU o con el adaptador por software (ADR 0007) | pantalla de referencia → PNG; comparación con la imagen guardada | CI |
| CPU frente a GPU | renderer `wgpu` en CI (ver §3) | misma escena por los dos caminos; objetivo **0 píxeles distintos** (tolerancia máxima: 1 de diferencia por canal en ≤ 0.1% de píxeles) | CI |
| Integración de PTY | por SO | lanzar la shell, escribir y leer, redimensionar, matar el árbol de procesos, ConPTY | CI en las 3 plataformas |
| Integración de shell | zsh, bash, fish y pwsh instalados en CI | OSC 133/7 emitidos correctamente, inyección sin romper la configuración del usuario | CI |
| UI | `egui_kittest` | barra lateral, filtros, panel de uso; aserciones sobre el árbol de accesibilidad | CI |
| End-to-end | app real + `hv-fake-agent` + CLI por IPC | flujos completos (ver §4) | CI (Linux con Xvfb, Windows y macOS) |
| Conformidad VT | `vttest` (manual), `esctest2` (automatizado), `ucs-detect` (anchura Unicode) | secuencias de escape y anchura de caracteres | semanal en CI y antes de cada release |
| Fuzzing | `cargo-fuzz` (libFuzzer) | alimentación del parser, OSC propios, parsers de logs y decodificador IPC | nocturno, 30 min por objetivo |
| Rendimiento | `criterion`, `vtebench`, `hyperfine` | throughput, coste por frame, arranque | CI con presupuesto; aviso si empeora > 10% |
| Latencia | Typometer (manual) | tecla → píxel | antes de cada release, en hardware real |
| Memoria | `dhat` / heaptrack | fugas y picos | semanal |
| Resistencia | 10 agentes simulados × 8 h | crecimiento de memoria < 5%, sin crashes | antes de cada release |
| Manual | checklist (§6) | lo que no se puede automatizar | antes de cada release |

## 2. Grabaciones de sesiones reales

- `hivetty --record <archivo>` guarda la salida del PTY con marcas de tiempo en formato **asciicast v2** (compatible con asciinema).
- Corpus en `tests/fixtures/recordings/` con sesiones reales anonimizadas de Claude Code, Codex, Gemini CLI, `vim`, `htop`, `tmux`, `npm install` y `cargo build`.
- Usos:
  1. Regresión del parser (snapshot de la rejilla al final de la reproducción).
  2. Que la salida sincronizada no deje frames rotos.
  3. Banco de rendimiento con salida realista de agentes.
  4. Probar la heurística de estado sin hooks.

## 3. GPU en CI

| Runner | Adaptador |
|---|---|
| `ubuntu-24.04` | Vulkan por software (lavapipe, de Mesa) |
| `windows-latest` | DX12 por WARP (adaptador por software de Windows) |
| `macos-latest` | Metal (GPU paravirtualizada del runner) |

Si un runner no tiene adaptador, esas pruebas se marcan como omitidas (no fallidas) y se registra el aviso.

## 4. Agente simulado (`hv-fake-agent`)

Binario de prueba que imita a un agente de forma determinista a partir de un guion YAML:

```yaml
title: "Arreglar logos"
steps:
  - hook: SessionStart
  - title: "✳ Arreglar logos"
  - hook: UserPromptSubmit
  - output: { file: claude-diff.cast }     # reproduce una grabación
  - hook: { PreToolUse: { tool: Bash, description: "Converting screenshot to PNG" } }
  - sleep: 2s
  - hook: Notification                    # → Waiting
  - wait_input: "y\r"
  - hook: Stop                            # → Done
usage_log: claude-jsonl                    # escribe un log de uso falso compatible
```

Los tests end-to-end lanzan `hivetty` con una configuración temporal, crean sesiones del agente simulado con `hivetty cli` y comprueban por IPC (`hivetty cli list --json`):
- transiciones de estado y tiempos (≤ 300 ms con hooks),
- contadores de filtros, agrupación por proyecto y archivado,
- notificaciones (capturadas por un backend de prueba),
- que el panel de uso cuadra con el log falso.

## 5. Parsers de logs de agentes

- `tests/fixtures/usage/<agente>/<versión>/` con logs reales anonimizados de cada versión soportada.
- Para cada fixture, un `expected.json` con los totales. Para Claude Code, los totales se generan con `ccusage` y deben cuadrar (±1%).
- Prueba de tolerancia: líneas corruptas, campos desconocidos, archivos truncados a medio escribir y archivos que crecen mientras se leen.

## 6. Checklist manual antes de cada release

La versión detallada, paso a paso y con lo que debería verse, está en [checklist.md](checklist.md).

Para **cada** plataforma (macOS arm64, macOS x86_64, Ubuntu 24.04 GNOME Wayland, Ubuntu 24.04 X11, Ubuntu 26.04 KDE Wayland, Windows 10 22H2, Windows 11, Windows 11 ARM):

- [ ] Instalar desde el paquete firmado; arranca sin avisos de seguridad.
- [ ] Asistente de primer arranque completo.
- [ ] `vim`, `htop`/`btop`, `less`, `tmux`, `ssh` a un servidor y `git log` con paginador.
- [ ] Claude Code: prompt largo, `Shift+Enter`, pegar imagen, permisos, diff largo sin parpadeo y scroll arriba mientras trabaja (Jump to bottom).
- [ ] Codex y Gemini CLI: estado en la barra lateral.
- [ ] 6 sesiones simultáneas: estados correctos, notificaciones y `⌘J`/`Ctrl+Shift+J`.
- [ ] Reiniciar la app y reanudar las sesiones.
- [ ] IME: japonés y chino; tildes muertas con teclado español (`´`+`a` → `á`), `ñ`, `@` con AltGr/Option.
- [ ] Dos monitores con DPI distinto: mover la ventana entre ellos.
- [ ] Suspender y reanudar el equipo con sesiones abiertas.
- [ ] Panel de uso: cifras coherentes con `ccusage`.
- [ ] Lector de pantalla: VoiceOver / NVDA / Orca leen la barra lateral y anuncian los cambios de estado.
- [ ] Tema claro, oscuro y alto contraste.
- [ ] Actualización desde la versión anterior.
- [ ] Windows: PowerShell 7, PowerShell 5.1, cmd, WSL y Git Bash.

## 7. Hardware de pruebas
- **macOS:** un Mac con Apple Silicon (local o en la nube: MacStadium, Scaleway o AWS EC2 Mac) además de los runners de GitHub.
- **Ubuntu:** VM con aceleración 3D (VirtualBox, VMware o virt-manager con virgl) y, si es posible, un equipo real con GPU AMD o Intel.
- **Windows:** el PC de desarrollo y una VM de Windows 10; Windows on ARM opcional (VM en Mac con Parallels, o una VM ARM en la nube).
