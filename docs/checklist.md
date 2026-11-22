# Checklist de pruebas manuales

Para probar hivetty a mano en macOS, Ubuntu y Windows. Cada prueba dice qué hacer y qué debería verse. Marca `[x]` lo que funcione y anota lo que no (ver [Cómo informar](#cómo-informar)).

Las pruebas que dependen de cosas que aún no existen (paquetes firmados, actualizaciones automáticas) no están aquí.

## 0. Conseguir la versión a probar

Los paquetes salen **sin firmar** del workflow `package` de GitHub Actions (pestaña *Actions* → *package* → la última ejecución → *Artifacts*):

| Sistema | Artefacto | Instalar |
|---|---|---|
| macOS | `hivetty-macOS-unsigned` → `.dmg` | Arrastra la app a Aplicaciones. Al ser sin firmar, la primera vez: clic derecho → *Abrir* → *Abrir*. Si dice que está dañada: `xattr -dr com.apple.quarantine /Applications/hivetty.app` |
| Ubuntu | `hivetty-Linux-unsigned` → `.deb` o `.AppImage` | `sudo apt install ./hivetty_*.deb`, o `chmod +x hivetty*.AppImage && ./hivetty*.AppImage` |
| Windows | `hivetty-Windows-unsigned` → `.msi` | SmartScreen avisará: *Más información* → *Ejecutar de todas formas* |

Para lanzar una ejecución nueva: *Actions* → *package* → *Run workflow*.

Antes de empezar, en una terminal: `hivetty --doctor`. Guarda la salida; sirve para cualquier informe.

## 1. Primer arranque

- [ ] **Asistente.** Con una instalación limpia (sin `config.toml`), al abrir aparece *Welcome to hivetty* con tema, integración de shell y, si tienes Claude Code, sus hooks y su statusline. Bajo cada opción de Claude Code se listan los cambios exactos en `~/.claude/settings.json`.
- [ ] Elegir *Light*, dejar la integración y pulsar *Start*: la interfaz y el terminal pasan a claro en un segundo, y se crea `config.toml`.
- [ ] Cerrar y abrir: el asistente **no** vuelve a salir.
- [ ] Si marcaste los hooks: `~/.claude/settings.json` tiene las entradas de hivetty y hay un `settings.json.hivetty-bak` al lado.

Para repetirlo: borra `config.toml` y el archivo `welcomed` del directorio de datos (`hivetty --doctor` muestra ambas rutas).

## 2. Terminal básico

- [ ] `vim`, `less`, `htop`/`btop`, `git log` con paginador: se ven bien, se puede salir, y al volver el texto anterior sigue ahí.
- [ ] `tmux` y `ssh` a un servidor: funcionan y los colores son correctos.
- [ ] Texto con acentos, `ñ`, emoji y CJK (`echo "ñandú 日本 ✓ 🎉"`).
- [ ] Teclado español: tildes muertas (`´` + `a` → `á`), `@` con AltGr (Windows/Linux) u Option (macOS).
- [ ] IME japonés o chino: el texto en composición aparece subrayado en el cursor y se confirma bien.
- [ ] Seleccionar con el ratón, copiar y pegar (`Ctrl+Shift+C/V` en Windows/Linux, `⌘C/V` en macOS).
- [ ] Buscar (`Ctrl+Shift+F` / `⌘F`): resalta coincidencias, Enter salta a la siguiente.
- [ ] Hacer scroll hacia arriba mientras sale texto: aparece *Jump to bottom* y al pulsarlo vuelve abajo.
- [ ] Enlaces: `Ctrl`/`⌘` + clic en una URL la abre en el navegador.

## 3. Pestañas, paneles y ventanas

- [ ] Pestaña nueva, dividir horizontal y vertical, cerrar paneles. El foco y el tamaño de cada panel son correctos.
- [ ] Arrastrar el divisor entre paneles; arrastrar pestañas para reordenarlas; doble clic para renombrar una pestaña.
- [ ] Mover una pestaña a una ventana nueva; varias ventanas a la vez.
- [ ] Paleta de comandos (`Ctrl+Shift+P` / `⇧⌘P`).
- [ ] **Windows:** la barra de pestañas es la barra de título: arrastrar el hueco mueve la ventana, doble clic maximiza, los botones minimizar/maximizar/cerrar funcionan y los bordes redimensionan.
- [ ] Cerrar hivetty con varias pestañas y abrirlo: vuelven las pestañas, divisiones y carpetas.
- [ ] Dos monitores con DPI distinto: mover la ventana de uno a otro; el texto cambia de tamaño correctamente.
- [ ] Suspender y reanudar el equipo con sesiones abiertas: todo sigue respondiendo.

## 4. Integración con la shell

Probar en las shells que tengas: zsh, bash, fish; en Windows PowerShell 7, **PowerShell 5.1** y Git Bash.

- [ ] Tras ejecutar comandos, en el margen izquierdo hay una marca verde (éxito) o roja (`false`, `exit 1`).
- [ ] Saltar entre prompts (`Ctrl+Shift+↑/↓` / `⌘↑/↓`).
- [ ] Copiar la salida del último comando desde la paleta.
- [ ] Pestaña nueva tras `cd` a otra carpeta: se abre en esa carpeta.
- [ ] En un repositorio git aparecen los chips sobre el prompt: tecnología, rama y versiones (`rust-toolchain.toml`, `.nvmrc`…).
- [ ] Un comando de más de 30 s (`sleep 35`) con la ventana sin foco: llega una notificación al terminar.
- [ ] **PowerShell 5.1** (`powershell.exe`): arranca sin el error "la ejecución de scripts está deshabilitada" y tiene las marcas.

## 5. Agentes en la barra lateral

Instala los hooks de cada agente (cada comando muestra los cambios y pide confirmación):

```sh
hivetty agents install-hooks                     # Claude Code
hivetty agents install-hooks --agent gemini-cli  # Gemini CLI
hivetty agents install-hooks --agent codex       # Codex (al abrirlo, te pide confiar en ellos)
```

- [ ] Tras instalarlos, cada agente sigue funcionando con normalidad (sin errores de hooks), también fuera de hivetty.
- [ ] Tu propia configuración de cada agente sigue intacta (en `~/.codex/config.toml`, tus comentarios también) y hay un `.hivetty-bak` al lado.

Con los tres agentes:

- [ ] Arrancar cada agente en un panel: la fila muestra su nombre y su estado (*Working*, *Needs permission*, *Done*…). En Claude Code, además, lo que está haciendo ("Editing src/main.rs").
- [ ] Un permiso pendiente en otro panel: anillo ámbar alrededor de ese panel, notificación del sistema, y `Ctrl+Shift+J` / `⌘J` te lleva a él.
- [ ] *Peek* en una fila que espera: ver la pregunta y responder sin cambiar de pestaña.
- [ ] **Windows y Linux:** pulsar una notificación de un agente trae la ventana al frente y enfoca su panel. Las de *waiting* y *error* suenan; las de *done*, no.
- [ ] 6 sesiones a la vez: los estados se mantienen correctos; varias notificaciones juntas se agrupan ("4 sessions need you").
- [ ] **macOS:** el icono del dock muestra cuántas sesiones te necesitan.
- [ ] Pasar el ratón por una fila: tarjeta con agente, modelo, rama, carpeta, tokens, inicio y última herramienta.
- [ ] Clic derecho en una fila: *Rename…*, *Archive*, *Copy session id*, *Show folder*, *Close…*
- [ ] Archivar una sesión y restaurarla desde *Archived*: se reabre y reanuda la sesión del agente.
- [ ] Clic derecho en el nombre de un proyecto: *New session here*, *Collapse*, *Archive finished sessions*.
- [ ] Arrastrar el borde derecho de la barra lateral para hacerla más ancha o estrecha.
- [ ] Cerrar hivetty con agentes trabajando y abrirlo: ofrece reanudarlos.

- [ ] **Windows con WSL:** en un panel, `wsl` y luego `echo $HIVETTY_SESSION_ID` muestra un número (hivetty pasa sus variables a WSL).

## 6. Nueva sesión y worktrees

- [ ] *New session* (`Ctrl+Shift+A` / `⇧⌘T`): elegir agente, carpeta y un prompt inicial. El agente arranca con ese prompt.
- [ ] Marcar *create a git worktree* en un repositorio: aparece una carpeta hermana `<repo>-<rama>` y la sesión arranca allí. Si el repo tiene `.worktreeinclude` con `.env`, el `.env` está copiado.
- [ ] Clic derecho en esa sesión → *Remove worktree…*: con cambios sin confirmar git se niega y llega un aviso; sin cambios, la carpeta desaparece.

## 7. Statusline de Claude Code

- [ ] `hivetty agents install-statusline`: muestra el cambio y pide confirmación.
- [ ] En Claude Code, bajo el prompt aparece `Opus · ctx 8% · 5h 24% · 7d 41%` (o similar).
- [ ] La tarjeta de la sesión muestra *Context*. El panel de uso muestra los límites de 5 h y semanal con su cuenta atrás.

## 8. Panel de uso

- [ ] `Ctrl+Alt+U` / `⌥⌘U`: tokens, tareas, tiempo de agente, gráfico diario, mapa de calor, modelos y agentes (Claude Code, Codex, Gemini CLI).
- [ ] Las cifras de Claude Code coinciden más o menos con `npx ccusage`.
- [ ] Clic en un agente: el panel se filtra a ese agente.
- [ ] Con un `pricing.toml` (ver la guía de usuario): aparece "≈ $X API equivalent".
- [ ] `hivetty usage`, `hivetty usage --csv`, `hivetty usage --days 7`.

## 9. Temas y accesibilidad

- [ ] El tema sigue al del sistema al cambiarlo con la app abierta.
- [ ] `[theme] high_contrast = true` en `config.toml`: negro y blanco con colores saturados, al guardar.
- [ ] `hivetty theme import <archivo>` con un tema de iTerm2 (`.itermcolors`) o de Alacritty: imprime el bloque `[theme.colors]`; pegarlo en `config.toml` cambia los colores.
- [ ] Un error a propósito en `config.toml` (por ejemplo `size = "grande"`): aparece una barra de aviso sobre el terminal y hivetty sigue con la configuración anterior.
- [ ] Lector de pantalla (VoiceOver, NVDA u Orca): lee las filas de la barra lateral.

## 10. Notificaciones al móvil (opcional)

- [ ] Configurar `[notifications.forward]` con un tema de ntfy (ver la guía): al pedir permiso un agente con la ventana sin foco, llega al móvil.

## 11. Modo seguro y diagnóstico

- [ ] `hivetty --safe`: abre con la configuración por defecto, sin integración de shell. Al cerrar, tu layout normal **no** se pierde.
- [ ] `hivetty --doctor`: versión, SO, GPU, configuración, integración y hooks.

## Cómo informar

Para cada fallo:
1. Qué hiciste, qué esperabas y qué pasó (una captura ayuda).
2. La salida de `hivetty --doctor`.
3. Si hivetty se cerró de golpe: el último `crash-*.txt` del directorio de datos (`--doctor` lo indica). No contiene el contenido del terminal.
4. Para más detalle: arranca con `HIVETTY_LOG=debug hivetty` y adjunta lo que salga en la consola.
