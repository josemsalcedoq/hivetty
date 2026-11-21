# Especificación de interfaz

Diseño **original** inspirado en la funcionalidad de las capturas: no se copian logos, nombres ni recursos gráficos de ningún producto. Los iconos de agentes y tecnologías se usan según la política de marca de cada uno (ver `docs/licensing.md`).

## 1. Tokens de diseño

Se definen en `assets/themes/*.toml` y la UI no usa colores sueltos fuera de ellos.

### Color (tema oscuro por defecto)
| Token | Valor | Uso |
|---|---|---|
| `bg.window` | `#0A0A0B` | fondo de la ventana y de los paneles |
| `bg.sidebar` | `#111113` | barra lateral |
| `bg.elevated` | `#18181B` | tarjetas, menús, tooltips |
| `bg.selected` | `#232327` | fila o pestaña seleccionada |
| `bg.hover` | `#1C1C20` | hover |
| `border.subtle` | `#26262B` | separadores |
| `text.primary` | `#EDEDEF` | títulos |
| `text.secondary` | `#9A9AA2` | subtítulos, cabeceras de grupo |
| `text.muted` | `#6B6B73` | tiempos, ayudas de atajos |
| `state.working` | `#9A9AA2` + spinner | Working |
| `state.waiting` | `#E3A008` | Waiting |
| `state.done` | `#3FB950` | Done y deltas positivos |
| `state.error` | `#F85149` | Error |
| `chart.input` | `#5A5A62` | barras de input |
| `chart.output` | `#4EA1FF` | barras de output |
| `heat.0…4` | rampa de grises `#1A1A1D → #E6E6E6` | mapa de calor |
| `provider.anthropic` / `google` / `openai` / `xai` / `other` | colores de categoría | barra de proveedores (validar contraste y daltonismo) |

El tema claro y el de alto contraste definen los mismos tokens. Contraste mínimo de 4.5:1 en texto y 3:1 en elementos gráficos.

### Tipografía
- **UI:** fuente del sistema (SF Pro en macOS, Segoe UI Variable en Windows, Cantarell/Ubuntu en Linux), con Inter incluida como respaldo.
- **Terminal:** configurable; por defecto se incluye JetBrains Mono (OFL) y Symbols Nerd Font para los iconos de prompts.
- Escala: 11 / 12 / 13 / 15 / 20 / 32 px (los valores de las KPI van a 32 px con cifras tabulares).

### Espaciado y tamaños
- Rejilla de 4 px.
- Barra lateral: 300 px por defecto (rango 200–480).
- Fila de sesión: 44 px (dos líneas) con 12 px de relleno horizontal; cabecera de grupo: 28 px.
- Pestaña: 36 px de alto, entre 120 y 240 px de ancho.
- Radio de esquinas: 6 px en controles y 10 px en tarjetas.

### Movimiento
- Spinner de Working: 8 frames a 12 fps (se detiene con "reducir movimiento").
- Transiciones de hover y selección: 80 ms. Panel de uso: aparece con fundido de 120 ms.

## 2. Ventana principal

```
┌─[● ● ●] [▤]──────────┬─[✳ Título A…][✳ Título B][◎ codex][+]──────────[∿][🌐]─┐
│ [✎ New session ⇧⌘T][📁]│                                                      │
│ [🔍 Search agents   / ]│                                                      │
│ [All 6][• Waiting][◌ Working 5]                                               │
│                        │        panel 1           ┃        panel 2           │
│ PROYECTO-A             │                          ┃                          │
│ ✳• Título sesión   3m  │                          ┃                          │
│    Done                │                          ┃                          │
│ PROYECTO-B             │                          ┃                          │
│ ✳  Claude Code    21m  │                    [Jump to bottom ↓]               │
│    Converting scr…     │                          ┃                          │
│ ...                    │ ❯ ▍                      ┃ ❯ ▍                      │
│ [✳][◎][◌][Λ][▣][□][◆]  │                          ┃                          │
│ [🗄 Archived       19] │                          ┃                          │
└────────────────────────┴──────────────────────────────────────────────────────┘
```

## 3. Componentes de la barra lateral

### 3.1 Fila de sesión
- **Icono** (20 px) del agente. Superpuesto: punto de *no visto* (8 px, `state.done` o `state.waiting`) abajo a la derecha.
- **Título**, una línea, con elipsis. En negrita si la sesión está enfocada.
- **Subtítulo**: si `Done`/`Error`/`Waiting`/`Exited`, el nombre del estado en su color; si `Working`, la acción actual ("Writing crates/…/vision.rs") o "Working".
- **Tiempo** alineado a la derecha, `text.muted`, cifras tabulares.
- Estados: normal, hover, seleccionada y enfocada (anillo de foco de teclado de 2 px).
- Estado aproximado (sin hooks): icono "≈" pequeño junto al tiempo, con un tooltip que explica cómo activar los hooks.

### 3.2 Filtro segmentado
`All N` · `• Waiting N` · `◌ Working N`. Si N = 0 se oculta el número (como en la captura "Waiting" sin número). Atajos: `1`, `2` y `3` con la barra lateral enfocada.

### 3.3 Grupos de proyecto
Cabecera en `text.secondary` a 12 px, colapsable con clic. Orden: el grupo con la actividad más reciente arriba; dentro de cada grupo, las sesiones `Waiting` primero, después `Working` y después el resto por recencia (configurable).

### 3.4 Dock de agentes
Iconos de 18 px en una fila. Solo aparecen los agentes instalados; el resto está en "…". Clic → nueva sesión en el proyecto de la sesión seleccionada (o en el directorio actual del panel con foco). Clic derecho → elegir carpeta, worktree o argumentos.

### 3.5 Archived
Fila fija abajo con contador. Al pulsarla se muestra la lista de archivadas, con búsqueda y botón "restaurar" o "reanudar".

## 4. Diálogo "New session"

```
┌ New session ───────────────────────────────────────────┐
│ Agente:    [✳ Claude Code ▾]                           │
│ Carpeta:   [~/Dev/midterm                    ] [📁]    │
│ Worktree:  [ ] crear worktree  rama: agent/<slug>      │
│ Prompt:    [opcional…                              ]   │
│ Argumentos:[--model … ]          Perfil: [default ▾]   │
│ Abrir en:  (•) pestaña nueva  ( ) dividir derecha       │
│                                   [Cancelar] [Crear ⏎] │
└────────────────────────────────────────────────────────┘
```
- Por defecto: el último agente usado y la carpeta del panel con foco.
- Carpetas recientes con autocompletado; validar que existe.

## 5. Pestañas y paneles
- Título: OSC 2 del programa o, si no hay, el nombre del proceso y el directorio. Se puede fijar un título manual.
- Icono: el del agente si el panel tiene sesión; si no, el de la shell.
- Indicadores en la pestaña: spinner (Working), punto (no visto), barra de progreso fina abajo (OSC 9;4), campanilla (campana sin ver).
- Panel sin foco: atenuado al 85% (configurable). Separador de 1 px, con zona de arrastre de 6 px.
- **Jump to bottom**: píldora flotante centrada abajo, `bg.elevated`, que aparece cuando el panel no está al final. Muestra "↓ N líneas nuevas" si llega salida.

## 6. Panel de uso
Rejilla de 12 columnas, que pasa a 4 en ventanas de menos de 900 px de ancho.

- Fila 1: 4 tarjetas KPI (3 columnas cada una). Etiqueta en mayúsculas a 12 px, delta en una píldora verde o roja a la derecha, valor a 32 px, subtítulo a 13 px y sparkline de 24 px de alto.
- Fila 2: "TOKENS BY DAY" (7 columnas) y "WHEN THEY WORK" (5 columnas).
- Fila 3: "MODELS" (6 columnas) y "AGENTS" (6 columnas).
- Selector de rango arriba a la derecha y botón `esc`.
- Al pasar el ratón por cualquier punto: tooltip con el valor exacto y la fecha.
- Estado vacío: "Aún no hay datos. termhive lee el uso de: Claude Code (~/.claude/projects), Codex…" con un botón "Abrir doctor".
- Datos parciales (un parser falló): aviso discreto con enlace al detalle.

## 7. Notificaciones
- Texto: `<Agente> · <proyecto>` / `<título de la sesión>: <motivo>` (por ejemplo "Necesita permiso: Bash(cargo test)").
- Acción: "Enfocar". Se agrupan si hay más de 3 en 10 s ("3 sesiones te necesitan").
- Respetan el modo No molestar del sistema. Sonido configurable por estado.
- Nunca se notifica la sesión que ya está enfocada en una ventana con foco.

## 8. Accesibilidad
- Toda la interfaz (barra lateral, pestañas, diálogos y panel) expone su árbol de accesibilidad con **AccessKit** (VoiceOver, NVDA/Narrador y Orca).
- Los cambios de estado de una sesión se anuncian como *live region* (cortesía) y los de Waiting como asertivos (configurable).
- Texto del terminal accesible para lectores de pantalla:
  - v1.0: las líneas visibles del panel enfocado se exponen como texto de solo lectura.
  - v1.x: navegación completa (UIA TextPattern en Windows, NSAccessibility en macOS y AT-SPI en Linux).
- Todo es operable con teclado y el foco siempre es visible.
- Alto contraste: tema dedicado y respeto de la preferencia del sistema.
- Reducir movimiento: sin spinner animado ni transiciones.
- Escalado de la UI independiente del de la fuente del terminal (de 80% a 200%).
- El estado nunca se indica solo con color.

## 9. Idiomas
- Textos de la UI con **Fluent** (`fluent-rs`): español e inglés en la v1.0. La UI sigue el idioma del sistema y se puede cambiar en la configuración.
- Fechas y números con ICU4X según el idioma.
- El contenido del terminal es Unicode completo: ver la anchura de caracteres en `docs/vt-compliance.md`.

## 10. Asistente de primer arranque
1. Bienvenida y tema (claro, oscuro o sistema) con vista previa.
2. Fuente del terminal (vista previa con ligaduras y Nerd Font).
3. Shell por defecto (detectadas) e integración de shell (activada por defecto).
4. Agentes detectados → "Instalar hooks" para cada uno (muestra el diff y pide confirmación) o "Ahora no".
5. Historial de uso: "Importar los últimos 90 días" (en segundo plano).
6. Notificaciones: pedir permiso al sistema (en macOS es obligatorio).
7. Listo: se abre la primera sesión.

Se puede repetir desde el menú Ayuda.

## 11. Menús nativos
- macOS: barra de menús completa (termhive, Archivo, Editar, Ver, Sesión, Ventana, Ayuda) con `muda`.
- Windows y Linux: menú de hamburguesa en la barra de pestañas con los mismos elementos y la paleta de comandos.
