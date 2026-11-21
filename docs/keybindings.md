# Atajos de teclado

Principios:
1. En macOS se usa `Cmd` (la tecla de la app) y nunca se roba `Ctrl` a los programas.
2. En Windows y Linux se usa `Ctrl+Shift` para no robar `Ctrl+letra` a la shell ni a los agentes.
3. No se pisan atajos globales del sistema: `Ctrl+Alt+T` y `Ctrl+Alt+flechas` en Ubuntu, `Ctrl+Shift+U` (IBus), `Win+…` en Windows y `Cmd+Espacio`/`Cmd+Tab` en macOS.
4. Todo se puede reasignar en `config.toml` (`[keybindings]`) y desactivar con `action = "none"`.

## Por defecto

| Acción | macOS | Windows / Linux |
|---|---|---|
| Nueva pestaña (shell) | `⌘T` | `Ctrl+Shift+T` |
| **Nueva sesión de agente** | `⇧⌘T` | `Ctrl+Shift+A` |
| Nueva ventana | `⌘N` | `Ctrl+Shift+N` |
| Cerrar panel (o la pestaña si es el último) | `⌘W` | `Ctrl+Shift+W` |
| Dividir a la derecha | `⌘D` | `Ctrl+Shift+D` |
| Dividir abajo | `⇧⌘D` | `Ctrl+Shift+E` |
| Mover el foco entre paneles | `⌥⌘←↑→↓` | `Alt+←↑→↓` |
| Redimensionar panel | `⌃⌘←↑→↓` | `Ctrl+Shift+Alt+←↑→↓` |
| Maximizar o restaurar el panel | `⇧⌘↩` | `Ctrl+Shift+Enter` |
| Pestaña siguiente / anterior | `⇧⌘]` / `⇧⌘[` | `Ctrl+Tab` / `Ctrl+Shift+Tab` (también `Ctrl+PgDn`/`PgUp`) |
| Ir a la pestaña 1–9 | `⌘1…9` | `Alt+1…9` |
| Mostrar u ocultar la barra lateral | `⌘B` | `Ctrl+Shift+B` |
| Enfocar la búsqueda de agentes | `/` (con la barra lateral enfocada) | `/` |
| **Ir a la siguiente sesión que te necesita** | `⌘J` | `Ctrl+Shift+J` |
| Ir a una sesión por nombre | `⌘O` | `Ctrl+Shift+O` |
| Paleta de comandos | `⇧⌘P` | `Ctrl+Shift+P` |
| Panel de uso | `⌥⌘U` | `Ctrl+Alt+U` |
| Buscar en el scrollback | `⌘F` | `Ctrl+Shift+F` |
| Copiar | `⌘C` | `Ctrl+Shift+C` (en Windows, también `Ctrl+C` si hay selección) |
| Pegar | `⌘V` | `Ctrl+Shift+V` (en Windows, también `Ctrl+V`, configurable) |
| Pegar la selección primaria | — | `Shift+Insert` y clic central (Linux) |
| Seleccionar todo el scrollback | `⌘A` | sin atajo (`Ctrl+Shift+A` es Nueva sesión); desde la paleta |
| Limpiar pantalla y scrollback | `⌘K` | `Ctrl+Shift+K` |
| Saltar al prompt anterior / siguiente | `⌘↑` / `⌘↓` | `Ctrl+Shift+↑` / `↓` |
| Subir y bajar página del scrollback | `⇧PgUp` / `⇧PgDn` | `Shift+PgUp` / `Shift+PgDn` |
| Ir al principio y al final | `⌘Home` / `⌘End` | `Shift+Home` / `Shift+End` |
| Modo copia (vi) | `⇧⌘Space` | `Ctrl+Shift+Space` |
| Selección rápida (pistas) | `⇧⌘H` | `Ctrl+Shift+H` |
| Aumentar / reducir / restablecer la fuente | `⌘+` / `⌘-` / `⌘0` | `Ctrl+=` / `Ctrl+-` / `Ctrl+0` |
| Pantalla completa | `⌃⌘F` | `F11` |
| Configuración | `⌘,` | `Ctrl+,` |
| Archivar la sesión enfocada | `⇧⌘⌫` | `Ctrl+Shift+Backspace` |

## Teclas que se envían al programa

| Tecla | Por defecto |
|---|---|
| `Shift+Enter` | Si el programa activó el modo teclado de Kitty: `CSI 13;2 u`. Si no: `ESC CR` (el salto de línea que entienden Claude Code y otros agentes). Configurable por perfil de agente. |
| `Option` (macOS) | `option_as_alt = "left"` por defecto (Option izquierda = Meta y la derecha para caracteres como `@`, `€`, `~` en teclados españoles). Valores: `none`, `left`, `right` y `both`. |
| `Alt` (Windows/Linux) | Prefijo ESC (Meta). |
| `Ctrl+V` en macOS/Linux | Se envía al programa (los agentes lo usan para pegar imágenes). |
| `Alt+V` en Windows | Se envía al programa (Claude Code lo usa para pegar imágenes en Windows). |
| `Ctrl+Backspace` | `^H` o `^W` según el perfil de la shell (PowerShell espera `^H`). |
| `Ctrl+Space` | `NUL`. |

## Formato en la configuración

```toml
[[keybindings]]
key = "primary+shift+t"   # primary = cmd en macOS y ctrl en Windows/Linux
action = "new_agent_session"

[[keybindings]]
key = "ctrl+shift+a"
action = "none"           # libera el atajo

[[keybindings]]
key = "shift+enter"
action = { send_text = "\u001b\r" }
when = "agent == 'claude-code'"
```

Contextos `when`: `sidebar_focused`, `terminal_focused`, `search_open`, `copy_mode`, `agent == '<id>'`, `os == 'macos' | 'windows' | 'linux'`.
