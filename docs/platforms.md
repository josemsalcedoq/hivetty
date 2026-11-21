# Plataformas

Soportadas: macOS 13 o posterior (arm64 + x86_64) · Ubuntu 22.04, 24.04 y 26.04 LTS (x86_64 + aarch64; X11 y Wayland) · Windows 10 22H2 y 11 (x64 + arm64; mínimo técnico 1809).

## Tabla general

| Tema | macOS | Ubuntu | Windows |
|---|---|---|---|
| Backend GPU | Metal | Vulkan (GL de respaldo) | DX12 (Vulkan/GL de respaldo) |
| PTY | `openpty` | `openpty` | **ConPTY incluido** (`conpty.dll` + `OpenConsole.exe`, NuGet 1.25.x); si falta, el del sistema |
| Shell por defecto | `$SHELL` (zsh), **como shell de login** | `$SHELL` (bash) | `pwsh` → `powershell` → `cmd`; perfiles de WSL (`wsl -l -q`) y Git Bash |
| Tecla principal | `Cmd` | `Ctrl+Shift` | `Ctrl+Shift` |
| Meta | Option (izquierda por defecto; `set_option_as_alt`) | Alt | Alt |
| Decoraciones | barra transparente con semáforos | CSD de sctk-adwaita en Wayland (GNOME no ofrece SSD), nativas en X11 | personalizadas; Mica opcional en Windows 11 |
| IME (winit 0.30) | NSTextInputClient (activar IME también para las teclas muertas) | XIM en X11 (desactiva el reporte de teclas muertas durante la composición), `text-input-v3` en Wayland | **solo IMM32** (sin TSF) |
| Escalado | Retina | fraccional (`wp-fractional-scale`) en Wayland | DPI por monitor v2 (manifiesto) |
| Fuentes del sistema | CoreText (vía `fontdb`) | fontconfig | `C:\Windows\Fonts` + fuentes de usuario |
| Mezcla de texto por defecto | nativa | lineal con corrección | lineal con corrección |
| IPC | socket Unix | socket Unix | named pipe |
| Notificaciones | `mac-notification-sys` (bundle firmado) | D-Bus (`zbus`) | toast con **AUMID** (instalador) |
| Badge | contador en el icono del dock | urgency hint, contador en Unity/launcher si existe | overlay en la barra de tareas (`ITaskbarList3`) y progreso |
| Menús | barra nativa (`muda`) | **pintados con egui** (muda necesita GTK) | `muda` (con `TranslateAcceleratorW` en el bucle) o egui |
| TTS (narrador) | `AVSpeechSynthesizer` / `say` | `speech-dispatcher` | SAPI / WinRT `SpeechSynthesizer` |
| Rutas de datos | `~/Library/Application Support/hivetty` | `$XDG_CONFIG_HOME` / `$XDG_DATA_HOME` / `$XDG_STATE_HOME` | `%APPDATA%\hivetty`, `%LOCALAPPDATA%\hivetty` |
| Paquete | dmg universal firmado y notarizado | deb, AppImage | MSI / Setup, winget, Scoop |

## Windows en detalle

### ConPTY
- El ConPTY del sistema (v1) **reinterpreta** la salida: repinta su búfer, guarda OSC 8 y la vuelve a emitir, gestiona el título y reenvía las OSC desconocidas aparte. En versiones antiguas (1809) pueden llegar **desordenadas** respecto al texto.
- El **ConPTY nuevo** (Windows Terminal 1.22+, incluido en el NuGet 1.25) entrega la salida VT de las apps **sin modificarla**: llegan OSC 7, 8, 9, 52, 133, 777 y 7501, títulos, DCS y APC.
- Por eso se **incluye** `conpty.dll` + `OpenConsole.exe` (x64 y arm64) junto al exe, como hace Zed:
  - `conpty.dll` busca `OpenConsole.exe` a su lado o en `<arch>\`. **Si no lo encuentra, vuelve en silencio al `conhost.exe` del sistema.** `--doctor` debe comprobar qué host se está usando.
  - `portable-pty` ya prefiere `conpty.dll` del directorio del exe si existe.
- Flags: el antiguo `PSEUDOCONSOLE_PASSTHROUGH_MODE` **ya no existe** (eliminado en 2024; ahora 0x8 significa `GLYPH_WIDTH_GRAPHEMES`). Usar `INHERIT_CURSOR` (0x1) y el `GLYPH_WIDTH_*` que coincida con nuestro modelo de anchura (`WCSWIDTH` 0x10). Revisar qué flags pasa `portable-pty` (hoy `INHERIT_CURSOR|RESIZE_QUIRK|WIN32_INPUT_MODE`, estos dos últimos obsoletos) y ajustarlos en nuestro fork.
- Con `INHERIT_CURSOR`, ConPTY envía `CSI 6n` y **no procesa la entrada hasta recibir la respuesta**: el emulador debe responder enseguida.
- Redimensionado: ConPTY solo tiene la zona visible y repinta al cambiar el tamaño. El *reflow* del scrollback es cosa nuestra. Redimensionar ConPTY y la rejilla a la vez, con *debounce* mientras se arrastra.
- El ConPTY nuevo emite `\r\n` literales en algunos saltos de línea de apps de consola, lo que rompe el reflow al redimensionar: aceptarlo como limitación y documentarlo.
- `ReleasePseudoConsole` (24H2+ o el incluido) es necesario para recibir EOF cuando el hijo termina: detectarlo con `GetProcAddress`.
- Sin `DISABLE_NEWLINE_AUTO_RETURN`, la traducción LF→CRLF puede corromper DCS.
- Matar el árbol de procesos: *job object* con `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.

### Shells y WSL
- PowerShell 7 y 5.1: la integración envuelve `prompt` y respeta PSReadLine.
- `cmd`: solo título y cwd mediante `PROMPT` con secuencias `$e]`; sin marcas completas.
- WSL: perfil por distro. Los hooks de agentes que corren dentro de WSL llaman a `hivetty.exe hook` por la interoperabilidad de WSL (llega al named pipe). Los logs de uso de WSL se leen desde `\\wsl.localhost\<distro>\home\<usuario>\…` (U-64).
- Git Bash (MSYS2): se lanza con `--login -i`. Sus rutas `/c/...` se convierten al soltar archivos.

### Otros
- Rutas largas: `longPathAware` en el manifiesto. DPI v2 en el manifiesto.
- Teclado: `AltGr` = `Ctrl+Alt`. **No** tratar `AltGr+Q` (`@` en teclado español) como atajo.
- Windows on ARM: compilar `aarch64-pc-windows-msvc`. ConPTY arm64 viene en el NuGet.
- SmartScreen: necesita la firma (ver `docs/release.md`).

## Linux en detalle
- Wayland: no hay posición absoluta de ventana; el portapapeles necesita foco; la selección primaria tiene protocolo propio (winit 0.31 añade una API). Probar en GNOME y KDE.
- X11: XIM para IME; probar con ibus y fcitx5.
- GPU: VMs sin aceleración o drivers viejos → GL o renderer por software (lavapipe/llvmpipe).
- AppImage: incluir fontconfig si hace falta y depender del `libvulkan` del sistema.
- `.desktop` con `Categories=System;TerminalEmulator;` y registro como `x-terminal-emulator`.
- Abrir aquí desde Nautilus: documentar cómo hacerlo (extensión o script).

## macOS en detalle
- **Shell de login** (`zsh -l`): las apps abiertas desde Finder tienen un PATH mínimo; sin eso, `claude`, `node` y similares no se encuentran.
- Firma y notarización obligatorias para las notificaciones (`UNUserNotificationCenter`) y para evitar Gatekeeper.
- Secure Keyboard Entry: opción en el menú. Respetar cuando otra app lo activa.
- ProMotion: hasta 120 fps al hacer scroll o animar y 60 o menos en el resto, para ahorrar batería.
- Teclados españoles: Option derecha para `@ # € ~ [ ] { } \ |`. Por eso `option_as_alt = "left"` por defecto.
- Binario universal: compilar `aarch64-apple-darwin` y `x86_64-apple-darwin` y unirlos con `lipo`.
