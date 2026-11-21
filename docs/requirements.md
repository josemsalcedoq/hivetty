# Requisitos

Inventario **exhaustivo** de lo que se ve en las dos capturas de referencia y de lo que hace falta para que funcione en serio. Cada requisito tiene un ID que se usa en `docs/backlog.md`.

Prioridad: **M** = imprescindible para la v1.0 · **S** = debería estar en la v1.0 · **C** = puede llegar después.

---

## 1. Ventana y estructura (W)

| ID | Requisito | Prio | Notas de la captura |
|---|---|---|---|
| W-01 | Ventana con tres zonas: barra lateral, barra de pestañas y área de paneles | M | |
| W-02 | Barra de título integrada: en macOS los botones semáforo dentro de la barra lateral; en Windows minimizar/maximizar/cerrar a la derecha; en Linux decoraciones del cliente en Wayland y nativas en X11 | M | captura de macOS con barra transparente |
| W-03 | Botón para ocultar/mostrar la barra lateral junto a los semáforos | M | icono de panel junto al semáforo |
| W-04 | Barra de pestañas: icono del agente o de la shell, título (de OSC 2) truncado con fundido, pestaña activa resaltada, cerrar al pasar el ratón, `+` para nueva pestaña | M | "✳ Logos que no se muestran…", "✳ Claude Code", icono de OpenAI en otra pestaña |
| W-05 | Reordenar pestañas arrastrando; arrastrar una pestaña a un panel para dividir; arrastrar fuera para nueva ventana | S | |
| W-06 | Doble clic en una pestaña para renombrarla; clic central para cerrar | S | |
| W-07 | Iconos a la derecha de la barra de pestañas: **actividad** (abre el panel de uso) y **globo** (puertos/servidores de desarrollo detectados, ver A-24) | M / C | iconos de pulso y globo arriba a la derecha |
| W-08 | Paneles divididos en horizontal y en vertical, anidados, con separador arrastrable | M | dos paneles lado a lado |
| W-09 | Cada panel tiene foco propio; el panel con foco se distingue (borde o atenuación de los demás) | M | |
| W-10 | Varias ventanas | S | |
| W-11 | Pantalla completa, "siempre encima" y modo desplegable tipo *quake* | C | |
| W-12 | Recordar el tamaño y la posición de la ventana por monitor | S | |

## 2. Barra lateral de agentes (S)

| ID | Requisito | Prio | Notas de la captura |
|---|---|---|---|
| S-01 | Botón **New session** con su atajo visible (`⇧⌘T` en macOS) | M | "New session ⇧⌘T" |
| S-02 | Botón de carpeta junto a New session: elegir el directorio o proyecto de la nueva sesión | M | icono de carpeta |
| S-03 | Campo **Search agents** con el atajo `/` visible; busca por título, subtítulo, proyecto, agente y rama | M | "Search agents /" |
| S-04 | Filtro segmentado **All N · Waiting N · Working N** con contadores en vivo; Waiting con punto y Working con spinner | M | "All 6 · Waiting · Working 5" |
| S-05 | Agrupar por **proyecto** (nombre de la raíz git del directorio de la sesión); los grupos se ordenan por actividad reciente | M | cuatro grupos de proyecto con sus sesiones |
| S-06 | Fila de sesión con icono del agente, **título**, **subtítulo** (estado o acción actual) y **tiempo** relativo | M | "Claude Code / Converting screenshot to PNG / 21m" |
| S-07 | Indicador de **no visto** (punto) cuando una sesión termina o pide algo y el usuario no la ha mirado | M | punto verde sobre el icono de la primera sesión |
| S-08 | Colores de estado: `Done` en verde, `Working` en gris con animación, `Waiting` en ámbar, `Error` en rojo. **Nunca solo color**: también icono o texto | M | "Done" en verde |
| S-09 | Clic en una fila → enfoca su pestaña/panel (y su ventana) | M | |
| S-10 | Navegación con teclado (↑ ↓ Enter, Esc vuelve al terminal) | M | |
| S-11 | **Dock** de agentes instalados abajo: un icono por agente, tooltip con el nombre, clic = nueva sesión de ese agente en el proyecto actual | M | fila de 8 iconos |
| S-12 | Sección **Archived** con contador; archivar o desarchivar sesiones; las archivadas no ocupan pestaña | M | "Archived 19" |
| S-13 | Menú contextual de sesión: enfocar, renombrar, archivar, reanudar, reiniciar, terminar (con confirmación), copiar id, abrir transcripción, mostrar carpeta, abrir en editor, ver uso de esta sesión | M | |
| S-14 | Menú contextual de proyecto: nueva sesión aquí, colapsar, abrir carpeta, archivar todas las terminadas | S | |
| S-15 | Tarjeta al pasar el ratón: modelo, rama, directorio, tokens de la sesión, coste estimado, inicio y última herramienta usada | S | |
| S-16 | Barra lateral redimensionable (200–480 px) y colapsable; recuerda el ancho | M | |
| S-17 | Arrastrar sesiones entre ventanas o a un panel | C | |
| S-18 | Sesiones que no son de agente (shells) también listadas, opcionalmente | S | |
| S-19 | **Bandeja de atención**: lista de sesiones que piden algo o han terminado sin verse, enlazada a su panel; anillo de color en el panel que necesita atención | M | (de cmux, Zellij 0.46 e iTerm2 3.7) |
| S-20 | **Peek**: ver la pregunta o el permiso pendiente de una sesión y contestar sin cambiar de pestaña | S | |
| S-21 | Metadatos de fila: rama, diff (+/−), número de PR y estado de CI, puertos abiertos y % de contexto | S | |

### Formato del tiempo relativo (S-06)

- `Working`: tiempo desde que empezó la tarea actual.
- `Waiting`: tiempo esperando.
- `Done`/`Error`/`Exited`: tiempo desde que terminó.
- Formato: `<60 s` → `27s` · `<60 min` → `21m` · `<24 h` → `3h` · resto → `2d`. Se actualiza cada segundo solo si la barra lateral es visible.

## 3. Paneles de terminal (P)

| ID | Requisito | Prio |
|---|---|---|
| P-01 | Emulación VT completa para TUIs de agentes (Ink/React), editores y monitores: ver `docs/vt-compliance.md` | M |
| P-02 | **Salida sincronizada** (DECSET 2026) para que los agentes no parpadeen al redibujar | M |
| P-03 | Indicador flotante **"Jump to bottom ↓"** cuando el usuario ha subido en el scrollback; clic o tecla para volver | M |
| P-04 | No saltar al final cuando llega salida si el usuario está leyendo arriba (configurable) | M |
| P-05 | Pegar imágenes en agentes: no interceptar la combinación que usa el agente para leer el portapapeles (`Ctrl+V` en macOS/Linux, `Alt+V` en Windows para Claude Code, verificar por agente) | M |
| P-06 | Arrastrar archivos al panel → insertar la ruta entre comillas según la shell; con imágenes, igual | M |
| P-07 | `Shift+Enter` envía un salto de línea que entienden los agentes (modo teclado de Kitty o `ESC CR`), sin configuración manual | M |
| P-08 | Detectar rutas `archivo:línea:col` y URLs en la salida; `Cmd/Ctrl+clic` abre en el editor configurado o en el navegador | M |
| P-09 | Hiperenlaces OSC 8 | M |
| P-10 | Búsqueda en scrollback con regex, resaltando coincidencias | M |
| P-11 | Modo copia por teclado (estilo vi) y selección rápida con pistas (hashes, rutas, URLs) | S |
| P-12 | Selección por palabra, línea, bloque rectangular (Alt+arrastrar); copiar al soltar (opcional); selección primaria en Linux (clic central) | M |
| P-13 | Copiar la salida del último comando y saltar entre prompts (requiere OSC 133) | S |
| P-14 | Zoom de fuente por panel y global | M |
| P-15 | Campana: visual, sonido, rebote en el dock o parpadeo en la barra de tareas | M |
| P-16 | Barra de progreso de OSC 9;4 en la pestaña y en la barra de tareas o dock | S |
| P-17 | Leer el portapapeles (OSC 52) desactivado; escribir permitido con límite | M |
| P-18 | Guardar el scrollback en un archivo; limpiar el scrollback | S |
| P-19 | Imágenes inline (protocolo de gráficos de Kitty, iTerm2, sixel) | C |

## 4. Integración con la shell y chips (C)

| ID | Requisito | Prio | Notas de la captura |
|---|---|---|---|
| C-01 | Inyección automática de integración en zsh, bash, fish y PowerShell, con opción para desactivarla | M | |
| C-02 | Marcas de prompt y de comando (OSC 133) con código de salida; marca visual en el margen | M | |
| C-03 | Directorio actual (OSC 7) para heredarlo al abrir pestañas o paneles | M | |
| C-04 | **Detección de la tecnología** del proyecto con el orden de la captura y sus reglas (ver §8) | M | "Rust → Python → Bun → TypeScript → Node → JavaScript" |
| C-05 | **Chips** con logo y texto en una fila sobre el prompt; **si un chip no cabe en la fila, no se muestra** (no salta de línea) | M | "si no cabe… no se muestra en vez de bajar a otra línea" |
| C-06 | Chip de rama git y estado (sucio, adelantado o atrasado) | S | |
| C-07 | Versiones en los chips (rust-toolchain, `.nvmrc`, `.python-version`, `engines`) leídas de archivos, **sin ejecutar** nada | S | |
| C-08 | Notificación al terminar un comando largo con la ventana sin foco | S | |
| C-09 | Integración en WSL y por SSH (opcional, explícita) | C | |

## 5. Agentes (A)

| ID | Requisito | Prio |
|---|---|---|
| A-01 | Perfiles de agente declarativos (TOML): Claude Code, Codex, Gemini CLI, opencode, Grok CLI, Junie, Antigravity, Copilot CLI, Cursor CLI, Amp y Aider | M |
| A-02 | Detectar los agentes instalados (PATH + rutas típicas por SO) para el dock y el asistente inicial | M |
| A-03 | Detectar que un panel está ejecutando un agente aunque se lance a mano (árbol de procesos) | M |
| A-04 | Máquina de estados por sesión: `Starting, Working, Waiting, Done, Error, Exited` | M |
| A-05 | Fuentes de estado: hooks → secuencias del terminal → heurística (`docs/agent-integration.md`) | M |
| A-06 | Instalador y desinstalador de hooks con diff, copia de seguridad y confirmación | M |
| A-07 | Título de sesión: el título del agente (OSC 2, por ejemplo "✳ Logos que…") o el primer prompt | M |
| A-08 | Subtítulo con la acción actual a partir de los hooks de herramientas ("Writing crates/…/vision.rs", "Converting screenshot to PNG") | M |
| A-09 | Notificaciones del sistema al pasar a `Waiting`/`Done` sin foco, con acción "Enfocar" | M |
| A-10 | Badge con el número de sesiones esperando (dock en macOS, overlay de la barra de tareas en Windows, *urgency hint* en Linux) | S |
| A-11 | Atajo "ir a la siguiente sesión que me necesita" | M |
| A-12 | **Reanudar** sesiones tras reiniciar (comando de *resume* de cada agente con su id de sesión) | M |
| A-13 | Las sesiones sobreviven a un cierre o crash de la UI y a una actualización (servidor de PTYs separado, ADR 0002) | S (v1.1) |
| A-14 | Nueva sesión en un **git worktree** propio (rama `agent/<slug>`), con limpieza al archivar (con confirmación) | S |
| A-15 | Vista de cambios de la sesión (git diff del worktree o directorio) | C |
| A-16 | Lanzar con un prompt inicial, un modelo o argumentos desde el diálogo New session | S |
| A-17 | Agentes personalizados (cualquier comando) con detección por nombre de proceso | M |
| A-18 | Narrador opcional (TTS) de eventos de agentes | C |
| A-19 | Estado aproximado visible cuando no hay hooks (icono "≈") | M |
| A-20 | Sesiones de agentes dentro de tmux, SSH o WSL: degradar con elegancia (ver `docs/agent-integration.md`) | S |
| A-21 | Tiempo de agente medido por el propio terminal (intervalos `Working` observados) | M |
| A-22 | Exportar o compartir la transcripción de una sesión | C |
| A-23 | Límite de sesiones concurrentes configurable y cola de inicio | C |
| A-24 | Detectar puertos abiertos por procesos de la sesión (servidores de desarrollo) y abrirlos en el navegador (icono del globo) | C |
| A-25 | **Receptor de OSC 7501** (Program Status Protocol) como fuente de estado principal, con estado `Blocked` y su motivo (permiso, pregunta o autenticación) | M |
| A-26 | Reenviar notificaciones al móvil (ntfy, Telegram o webhook), opcional | C |

## 6. Panel de uso (U)

Reconstrucción exacta de la segunda captura. Las fórmulas están **comprobadas con los números de la captura**.

### 6.1 Cabecera
| ID | Requisito |
|---|---|
| U-01 | Título "Agent usage" y rango "Oct 23 – today" (30 días contados **incluyendo** hoy: del 23 oct al 21 nov son 30 días) |
| U-02 | Selector `7 days · 30 days · 90 days · All`; `Esc` cierra el panel |
| U-03 | El panel es una vista superpuesta sobre los paneles o una pestaña especial (configurable) |

### 6.2 Tarjetas KPI (con delta frente al periodo anterior de igual duración y sparkline diario)
| ID | Tarjeta | Valor | Subtítulo | Comprobación con la captura |
|---|---|---|---|---|
| U-10 | INPUT TOKENS | Σ(input + cache_read + cache_write) | "N% from cache" = Σcache_read / input total | 11.6B; 98% de caché |
| U-11 | OUTPUT TOKENS | Σoutput | "X per turn" = output / turnos | 61.3M / 2,674 = **22.9K** ✔ |
| U-12 | TASKS | nº de tareas | "N turns" | 1,110 tareas, 2,674 turnos |
| U-13 | AGENT TIME | Σ duración en `Working` | "up to N at once" = concurrencia máxima | 347 h ≈ suma por agente (302+22+12+9.2+0.4) ✔ |
| U-14 | Delta | `(actual − anterior) / anterior`, en verde si sube; **tope de visualización +999%** | | "+999%" es un tope |

### 6.3 Tokens por día
| ID | Requisito | Comprobación |
|---|---|---|
| U-20 | Barras por día: input hacia arriba (gris) y output hacia abajo (azul), **cada uno con su propia escala** | ejes "1.3B input" y "6.2M output" = máximos diarios |
| U-21 | Etiquetas del eje X: primer día, día central y "today" | "Oct 23 · Nov 7 · today" |
| U-22 | Resumen "A of N days with agents · X/day", donde X = (input + output) / días activos | (11.6B + 61.3M) / 29 = **402M** ✔ |
| U-23 | Tooltip por barra con la fecha y los valores exactos | — |

### 6.4 Cuándo trabajan
| ID | Requisito |
|---|---|
| U-30 | Mapa de calor 7 filas (L–D) × 24 columnas (horas locales), intensidad = tiempo de agente (o tokens, configurable) |
| U-31 | 5 niveles por cuantiles y leyenda "less ▪▪▪▪ more" |
| U-32 | "most active <día> <hora> h" |
| U-33 | La semana empieza en lunes (configurable) y los cambios de horario de verano se tratan bien |

### 6.5 Modelos
| ID | Requisito | Comprobación |
|---|---|---|
| U-40 | "N models"; barra apilada por proveedor y leyenda con % | el % se calcula **por tokens de entrada**: Anthropic (10B + 592M) / 11.58B = 91.5% → **92%** ✔ |
| U-41 | Tabla: icono del proveedor, modelo, barra proporcional (lineal, mínimo un punto), input, output y tareas | |
| U-42 | Se muestran las 6 primeras filas y el resto se agrupa en "N more" (desplegable) | 16 modelos = 6 + "10 more" |
| U-43 | La suma de las filas cuadra con el total | output: 50.5+4.5+2.7+0.963+0.327+0.927+1.4 = **61.3M** ✔ |
| U-44 | Las tareas sin modelo cuentan en el total pero no en la tabla | 1,084 en la tabla frente a 1,110 en total |

### 6.6 Agentes
| ID | Requisito | Comprobación |
|---|---|---|
| U-50 | Tabla: icono, nombre, barra, input, output y **tiempo** | |
| U-51 | "click one to see only its usage": pulsar un agente filtra **todo** el panel | |
| U-52 | Un agente sin fuente de tokens muestra "—" pero sí su tiempo (medido por el terminal) | opencode: "— — 1 min" |
| U-53 | La suma de input de los agentes ≈ el total | 10.6B + 447M + 419M + 74M + 5.9M ≈ 11.55B ✔ |

### 6.7 Formato de números
- Compacto: `11.6B`, `61.3M`, `22.9K`, `963K`, `70.2K`; enteros con coma de miles: `1,110`.
- Tiempo: `347 h`, `9.2 h`, `23 min`, `1 min`.
- Siempre en formato inglés, independiente del idioma del sistema (la app es solo en inglés).

### 6.8 Extras del panel
| ID | Requisito | Prio |
|---|---|---|
| U-60 | Coste estimado con tabla de precios editable; "coste equivalente API" para quien usa suscripción | S |
| U-61 | Filtro por proyecto | S |
| U-62 | Exportar a CSV/JSON | S |
| U-63 | Uso de la sesión actual en la tarjeta al pasar el ratón de la barra lateral | S |
| U-64 | Incluir los logs de distribuciones WSL en Windows | S |
| U-65 | Límites del plan: ventana de 5 h y semanal con cuenta atrás ("week ▬ 15% ↻ 5d 11h"), ritmo de gasto y estimación de agotamiento | S |

## 7. Requisitos no funcionales (N)

### 7.1 Rendimiento (medido en un MacBook Air M1, un portátil Windows con iGPU Intel de 11.ª gen y Ubuntu con iGPU AMD)

| ID | Métrica | Objetivo |
|---|---|---|
| N-01 | Arranque en frío hasta el primer frame | ≤ 200 ms |
| N-02 | Latencia tecla → píxel (eco local) | p95 ≤ 1 frame + 4 ms |
| N-03 | Throughput de parseo (`vtebench`) | ≥ 80% del de Alacritty |
| N-04 | CPU en reposo (nada cambia) | ~0% (sin bucles activos; el parpadeo del cursor ≤ 0.5%) |
| N-05 | Memoria base | ≤ 80 MB |
| N-06 | Memoria por panel con 10 000 líneas de scrollback | ≤ 25 MB |
| N-07 | Fluidez con 10 paneles visibles con salida continua | 60 fps sostenidos |
| N-08 | Estado en la barra lateral desde un hook | ≤ 300 ms |
| N-09 | Ingesta inicial de 90 días de logs (~2 GB) | ≤ 60 s en segundo plano, sin bloquear la UI |
| N-10 | Abrir el panel de uso (90 días) | ≤ 300 ms |
| N-11 | Tamaño del binario | ≤ 30 MB |
| N-12 | `cat` de 1 GB | sin congelar la UI; `Ctrl+C` responde en ≤ 100 ms |

### 7.2 Fiabilidad
| ID | Requisito |
|---|---|
| N-20 | ≥ 99.5% de sesiones sin crash |
| N-21 | Un panel colgado o con salida infinita no bloquea los demás |
| N-22 | Si la GPU falla o se pierde el dispositivo (cambio de GPU, suspensión), recrear la superficie o pasar al renderer por software |
| N-23 | Modo seguro `--safe`: renderer por software, configuración por defecto, sin integración |
| N-24 | Prueba de resistencia: 10 agentes simulados durante 8 h con crecimiento de memoria < 5% |

### 7.3 Plataformas soportadas
| SO | Versiones | Arquitecturas |
|---|---|---|
| macOS | 13 Ventura o posterior | arm64, x86_64 (binario universal) |
| Ubuntu | 22.04, 24.04 y 26.04 LTS (X11 y Wayland; GNOME y KDE) | x86_64, aarch64 |
| Windows | 10 22H2 (mínimo técnico 1809, por ConPTY) y 11 | x86_64, aarch64 |

### 7.4 Idioma, accesibilidad y privacidad
- **N-30: toda la app en inglés** (UI, notificaciones, errores, CLI, configuración y documentación de usuario). Sin traducciones en la v1.0. Ver `docs/ui-spec.md` §9.
- Accesibilidad: `docs/ui-spec.md` §8. Privacidad y seguridad: `docs/security.md`.

## 8. Reglas de detección de tecnología (C-04)

1. Directorio de búsqueda: el directorio actual (OSC 7); si no contiene ningún marcador, la raíz del repo git (subiendo hasta `.git`, como máximo 20 niveles y sin cruzar `$HOME`).
2. **Gana el primero** que encaje en este orden (el orden de la captura, ampliado):

| # | Tecnología | Marcadores |
|---|---|---|
| 1 | Rust | `Cargo.toml` |
| 2 | Python | `pyproject.toml`, `requirements.txt`, `setup.py`, `Pipfile`, `uv.lock` |
| 3 | Bun | `bun.lock`, `bun.lockb`, `bunfig.toml` |
| 4 | TypeScript | `tsconfig.json`, o `"typescript"` en `dependencies`/`devDependencies` de `package.json` |
| 5 | Node | `package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`, `.nvmrc`, `.node-version` |
| 6 | JavaScript | `package.json` |
| 7 | Go | `go.mod` |
| 8 | Dart / Flutter | `pubspec.yaml` (Flutter si contiene `flutter:`) |
| 9 | Deno | `deno.json`, `deno.jsonc` |
| 10 | Java / Kotlin | `pom.xml`, `build.gradle(.kts)` |
| 11 | .NET | `*.csproj`, `*.sln` |
| 12 | Ruby, PHP, Elixir, Swift, Zig, C/C++ | `Gemfile`, `composer.json`, `mix.exs`, `Package.swift`, `build.zig`, `CMakeLists.txt` |

3. **Un solo logo** de tecnología (el ganador) y, como chips aparte, rama git y versiones.
4. **Nunca se ejecutan** herramientas del proyecto para detectar (ejecutar `rustc` dentro de un repo con `rust-toolchain.toml` podría descargar toolchains; ver `docs/security.md`). Solo se leen archivos, con un límite de 1 MB por archivo.
5. Caché por directorio, invalidada con un vigilante de archivos (`notify`) sobre los marcadores.
6. Los usuarios pueden añadir o reordenar reglas en la configuración.
