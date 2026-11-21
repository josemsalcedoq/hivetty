# Plataformas

| Tema | macOS | Ubuntu (24.04+) | Windows (10 1809+ / 11) |
|---|---|---|---|
| Backend GPU | Metal | Vulkan (GL de respaldo) | DX12 (Vulkan/GL de respaldo) |
| PTY | `openpty` | `openpty` | ConPTY |
| Shell por defecto | `$SHELL` (zsh) | `$SHELL` (bash) | `pwsh` → `powershell` → `cmd`; perfiles para WSL y Git Bash |
| Tecla principal | `Cmd` | `Ctrl+Shift` | `Ctrl+Shift` |
| Alt / Option | Option como Meta (configurable por lado) | Alt = Meta | Alt = Meta |
| Ventana | `winit`; barra de título transparente con los botones semáforo | X11 y Wayland (`winit`); decoraciones del cliente en Wayland | `winit`; barra oscura con DWM (Mica opcional) |
| Fuentes del sistema | CoreText vía `cosmic-text`/`fontdb` | fontconfig | DirectWrite / `C:\Windows\Fonts` |
| IME | NSTextInputClient (winit) | ibus/fcitx (winit) | TSF/IMM (winit) |
| IPC local | Unix socket | Unix socket | named pipe (`interprocess`) |
| Notificaciones | `mac-notification-sys` (requiere bundle firmado) | `notify-rust` (D-Bus) | `notify-rust` (toast; requiere AUMID) |
| TTS (narrador) | `say` / AVSpeechSynthesizer | `speech-dispatcher` (`spd-say`) | SAPI (`windows` crate) |
| Rutas | `~/Library/Application Support/termhive` | `$XDG_CONFIG_HOME/termhive`, `$XDG_DATA_HOME/termhive` | `%APPDATA%\termhive` |
| Paquete | `.dmg` universal, firmado y notarizado | `.deb`, AppImage | MSI/MSIX, winget |

## Cosas a vigilar

### Windows
- ConPTY reinterpreta la salida: algunas secuencias se filtran o se reordenan. Probar OSC 7, 133, 9 y 777 en la versión mínima soportada. Valorar distribuir un `conpty.dll`/`OpenConsole.exe` reciente (como hace Windows Terminal y WezTerm) para tener *passthrough*.
- El redimensionado provoca un repintado completo desde ConPTY: evitar parpadeo con doble buffer.
- Codificación: forzar UTF-8 (`chcp 65001` no hace falta con ConPTY, pero `cmd` antiguo sí puede necesitarlo).
- Rutas largas: activar `longPathAware` en el manifiesto.
- DPI por monitor v2 en el manifiesto.

### Linux
- Wayland: no hay posición absoluta de ventana; el portapapeles necesita foco; probar en GNOME y KDE.
- Algunos drivers Vulkan viejos o VMs sin GPU → respaldo GL o renderer por software.
- AppImage necesita incluir o encontrar `libvulkan` y fontconfig.

### macOS
- Firmar y notarizar es obligatorio para que las notificaciones y el arranque funcionen sin avisos.
- Secure Input (contraseñas) bloquea la entrada a otras apps: respetarlo.
- ProMotion: limitar a 60 fps salvo que haya animación, para ahorrar batería.
