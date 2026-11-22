# SP-1b — libghostty-vt 1.0 frente a alacritty_terminal: ¿compensa cambiar de motor VT?

- **Fecha:** 2026-11-21
- **Estado:** ✅ terminado. **Decisión: no cambiar ahora; revisar antes de F7 (imágenes) o si aparece un crate publicado sobre la 1.x** (ver "Decisión").
- **Rama:** `spike/sp-1b-libghostty` (no se fusiona). Código desechable en `spikes/ghostty-vt-bench` (proyecto Cargo independiente con su propio `[workspace]`, fuera del workspace de hivetty). Workflow: `.github/workflows/spike-sp1b.yml`.

## Qué se probó
- **Versiones.** libghostty-vt `vt-1.0.0` salió el 2026-11-06 y `vt-1.0.1` el 2026-11-11. El crate de crates.io `libghostty-vt` **0.2.2** (2026-09-28) fija un commit de ghostty **anterior a la 1.0** (2026-07-11, Zig 0.15.2). La rama `main` de [uzaaft/libghostty-rs](https://github.com/uzaaft/libghostty-rs) ya fija `vt-1.0.1` (commit `108cacb`) con una API bastante distinta (efectos, búsqueda, `vt_write_until`…), pero **no está publicada**. El spike usa esa rama fijada por commit (`rev = "483ea3d"`), porque es lo que habría que usar para estar sobre la ABI 1.0.
- **Zig 0.17.1** (ghostty 1.0.1 exige `minimum_zig_version = 0.17.0`), descargado del zip oficial a `spikes/ghostty-vt-bench/target/zig-dl/` y añadido al `PATH` solo para los comandos del spike. Comprobado su SHA-256 con `index.json`.
- El `build.rs` de `libghostty-vt-sys` **clona ghostty con git** en `OUT_DIR`, ejecuta `zig build -Demit-lib-vt=true -Doptimize=ReleaseFast -Dcpu=baseline` (Zig a su vez **descarga sus paquetes**: uucode, simdutf, highway) y enlaza `ghostty-vt-static.lib` de forma estática.
- **Benchmark en un solo binario release** (mismo perfil que el workspace: LTO thin, `codegen-units = 1`, `strip`), con el **mismo `snapshot.rs` y `osc.rs` de `hv-term`** compilados con `#[path]`:
  - Carga 1: el generador de `bench_throughput_and_snapshot` (líneas de log con color, Unicode, OSC 133 y 7).
  - Carga 2 (pesada): SGR 256 y truecolor, cursiva, subrayado, inverso, CJK y emoji, `CUP`, `DECSTBM`, `EL`, `DL` y OSC 8.
  - 32 MB por carga, 200×60, 10 000 líneas de historial, bloques de 64 KiB. Cada ejecución dura ~3 s.
  - Throughput: AT solo, AT + nuestro interceptor (lo que hace hivetty hoy), ghostty solo, y ghostty con los callbacks que sustituirían al interceptor (`on_pwd_changed`, `on_semantic_prompt` y `on_unknown_sequence` para OSC 7501).
  - Instantánea: copia de la pantalla completa (carácter, fg y bg resueltos, negrita, cursiva, subrayado, inverso, tenue, oculto, ancho y selección) al mismo `hv_grid::Snapshot`. En ghostty, `RenderState::update` + iteradores de filas y celdas, en dos variantes: "ingenua" (~6 llamadas FFI por celda) y "rápida" (una sola lectura del `Cell` en bruto para las celdas sin estilo; solo las celdas con estilo piden estilo y colores).
  - Comprobación: las 60 líneas visibles finales coinciden entre los dos motores en las dos cargas, y la variante rápida da exactamente las mismas celdas que la ingenua.

## Resultados (Windows 11, PC compartido)
Cinco ejecuciones (la variante rápida de la instantánea, solo en las dos últimas); el PC estaba en uso, así que los valores absolutos varían bastante entre ejecuciones (AT dio 54–79 MB/s aquí frente a 120 MB/s en `perf.md`). **La proporción entre motores, medida en el mismo proceso y en la misma ejecución, es estable.** Medianas:

| Métrica | alacritty_terminal 0.26 | libghostty-vt 1.0.1 | Relación |
|---|---|---|---|
| Throughput, carga 1, motor solo | 76 MB/s | 159 MB/s | **×2.1** |
| Throughput, carga 1, como en hivetty (AT + interceptor / ghostty + callbacks) | 67 MB/s | 159 MB/s | **×2.4** |
| Throughput, carga 2, motor solo | 75 MB/s | 144 MB/s | ×1.9 |
| Throughput, carga 2, como en hivetty | 61 MB/s | 139 MB/s | ×2.3 |
| Instantánea 200×60 sin cambios | 0.18 ms | 0.40 ms (ingenua) / **0.28 ms** (rápida) | ×1.5 más lenta |
| Instantánea 200×60 tras 16 KiB de salida | 0.17 ms | 0.37 ms (ingenua) / **0.24 ms** (rápida) | ×1.4 más lenta |
| Tamaño del binario (sondas mínimas: alimentar y leer la pantalla) | 0.30 MB | 2.47 MB | **+2.2 MB** |
| Compilación en frío del motor (Windows, clon + Zig) | — | ~4 min 20 s | |

- El interceptor de OSC desaparece: ghostty informa de todo con callbacks y el coste es casi nulo (con callbacks fue igual o más rápido que sin ellos, dentro del ruido).
- La instantánea es más lenta por las llamadas FFI por celda, pero 0.24–0.28 ms sigue siendo ~1.5 % de un frame de 16 ms. Con el seguimiento de filas sucias (`RowIteration::dirty`) y la caché de filas por `RowId` se copiaría solo lo que cambia, cosa que AT no ofrece.
- +2.2 MB sobre los 13 MB actuales deja el binario lejos del límite N-11 (30 MB). La DLL equivalente (`ghostty-vt.dll`) pesa 2.35 MB.

### CI en los 3 SO
Ejecución [44973142256](https://github.com/josemsalcedoq/hivetty/actions/runs/44973142256): **verde en los 3 SO**, con `mlugg/setup-zig` v2.2.1 (fijado por SHA) y Zig 0.17.1, sin caché de Cargo. Al final de cada job se ejecuta el benchmark con 16 MB por carga.

| Runner | `cargo build --release` en frío (clon + Zig + Rust) | Binario del benchmark | Throughput carga 1 como en hivetty (AT / ghostty) | Instantánea sin cambios (AT / ghostty rápida) |
|---|---|---|---|---|
| macOS 26 arm64 | 146 s | 1.7 MB | 75 / 248 MB/s (×3.3) | 0.14 / 0.29 ms |
| Ubuntu 24.04 | 279 s | 3.1 MB | 68 / 149 MB/s (×2.2) | 0.19 / 0.32 ms |
| Windows 2025 (MSVC) | 181 s | 2.8 MB | 61 / 153 MB/s (×2.5) | 0.20 / 0.30 ms |

En los 3 SO las 60 líneas visibles coinciden entre motores y la variante rápida de la instantánea da las mismas celdas que la ingenua. El primer intento falló solo en el paso de ejecución (`cargo run` sin `default-run`), no en la compilación.

`workflow_dispatch` no se puede lanzar desde la rama mientras el workflow no esté en `main` (HTTP 404). Por eso el workflow también se dispara con pushes a `spike/sp-1b-libghostty` que toquen el spike.

## Cobertura de la API frente a lo que necesita hv-term
| Necesidad de hv-term | libghostty-vt 1.0 (bindings de `main`) | Estado |
|---|---|---|
| Parseo, pantalla alternativa, modos DEC, consultas (DA, DSR/CSI 6n para ConPTY) | `vt_write`, `on_pty_write`, `on_device_attributes`, `on_xtversion`, `on_size`, `on_color_scheme` | ✅ |
| Cuadrícula y celdas (carácter, grafemas, colores resueltos, estilos, ancho) | `RenderState` + iteradores; `Cell` en bruto; `GridRef` | ✅ |
| Historial y desplazamiento | `set_scrollback_max_lines`/`max_bytes`, `scroll_viewport`, `scrollbar` | ✅ |
| Reflow al redimensionar | `resize` (reflow en la pantalla principal) | ✅ (AT también) |
| Selección (palabra, línea, bloque, salida de un comando) y copiar | `selection` (`select_word`, `select_line`, `select_output`, rectangular, `format_selection`) | ✅ (más que AT) |
| Búsqueda en el historial | `search` (incremental, coincidencias en el viewport) | ✅, pero **solo texto literal**: AT tiene regex (`RegexSearch`), que usamos hoy |
| Teclado de Kitty y xterm | `key::Encoder` con `set_options_from_terminal` | ✅ (sustituye el código portado de F1-30) |
| Ratón, foco, pegado seguro | `mouse`, `focus`, `paste` | ✅ |
| OSC 7 (cwd) | `on_pwd_changed` + `pwd()` | ✅ |
| OSC 8 (hiperenlaces) | `Cell::has_hyperlink` + `GridRef::hyperlink_uri` | ✅ (hay que leerlo por celda, no viene en el iterador de render) |
| OSC 9 / 777 (notificaciones), 9;4 (progreso) | `on_desktop_notification`, `on_progress_report` | ✅ |
| OSC 52 (portapapeles, escritura limitada y lectura desactivada) | `on_clipboard_write`, `set_clipboard_write_max_bytes`; `on_clipboard_read` se deja sin instalar | ✅ |
| OSC 133 (marcas de prompt) | `on_semantic_prompt`, `Row::semantic_prompt`, `is_cursor_at_prompt` | ✅ (más que nuestro interceptor) |
| OSC 7501 (estado para agentes) y otras propias | `on_unknown_sequence` + `set_unknown_max_bytes` | ✅ |
| Anclar marcas a la fila donde llegan | `vt_write_until` con límites `OSC_START`/`OSC_END` | ✅ (sustituye el troceado del interceptor) |
| Salida sincronizada (2026) | modo `SYNC_OUTPUT` + `on_render_hold` | ✅, pero **el temporizador lo pone el embebedor**, no hay un `sync_timeout()` como en `StdSyncHandler` |
| Modo 2027 (anchura por grafemas) | incluido | ✅ (AT no lo tiene) |
| Gráficos de kitty (F7) | `kitty::graphics` + `ImageIterator` | ✅ (AT no lo tiene) |
| Sixel | no | ❌ (tampoco en AT) |

**Huecos:** búsqueda por regex (habría que hacerla sobre el texto que da el formateador, o aceptar solo texto literal), el temporizador de la salida sincronizada, y los hiperenlaces, que se leen por celda con `GridRef`. Ninguno bloquea.

## Riesgos y costes
- **Bindings sin publicar.** En crates.io solo está 0.2.2, anterior a la 1.0. La 1.0 exige depender de un commit de git de un proyecto comunitario (un mantenedor principal) cuya API ha cambiado mucho entre 0.2.2 y `main` (p. ej. "vt/alloc!: use the standard library allocator API"). Escribir **bindings propios mínimos** sobre la ABI C (varias decenas de funciones para lo de la tabla) es viable, pero estimamos 1–2 semanas más el mantenimiento.
- **`!Send`/`!Sync`.** Todos los tipos envuelven `NonNull`. Hoy `Session` comparte `Arc<Mutex<TermState>>` entre el hilo lector y la UI, y eso exige `Send`. Opciones:
  1. El `Terminal` vive solo en el hilo lector. Tras cada bloque, ese hilo hace `RenderState::begin_update`/`end` y publica un `hv_grid::Snapshot` en un `Arc<Mutex<Snapshot>>` (o un doble búfer). La UI nunca toca el terminal. Selección, búsqueda, scroll y redimensionado pasan a ser **mensajes** al hilo lector, con una ida y vuelta por canal: más latencia y más código que la llamada directa con lock de hoy.
  2. Bindings propios con `unsafe impl Send` sobre un `Mutex` (la ABI C permite usar un terminal desde cualquier hilo si no hay concurrencia; ghostty lo hace así con su mutex de IO y renderer).
- **`unsafe` solo en `hv-platform`.** Un crate `-sys` y su envoltorio seguro son `unsafe` por naturaleza. Encajaría como dependencia externa (como `windows-sys` o `wgpu`), pero si escribimos los bindings nosotros, hay que crear un crate nuevo (p. ej. `hv-ghostty-sys`) con la misma excepción que `hv-platform`, y eso requiere un **ADR**.
- **Build.** Zig 0.17 en CI en los 3 SO (`mlugg/setup-zig`), red durante el build (clon de ghostty y paquetes de Zig; para builds sin red hay `GHOSTTY_SOURCE_DIR` y `GHOSTTY_ZIG_SYSTEM_DIR`), y de 2.5 a 4.5 min de compilación en frío por SO en CI (con la caché de Cargo, solo cuando cambia el commit fijado). En Windows (MSVC) la lib estática enlazó a la primera, sin flags extra. Compilar para otra arquitectura pasa por `-Dtarget` y está mapeado en `build.rs` (x86_64/aarch64 en los 3 SO), pero no se probó.
- **Licencias.** ghostty es **MIT**; los bindings, MIT OR Apache-2.0; los paquetes que Zig mete en la lib estática (uucode MIT, simdutf MIT/Apache-2.0, highway Apache-2.0/BSD-3) **no los ve `cargo-deny`**, así que habría que listarlos a mano en `docs/licensing.md` y en los avisos de terceros.
- **Conformidad:** a favor de ghostty (referencia actual: modo 2027, gráficos de kitty, OSC tipadas). AT funciona bien para lo que ya hacemos.

## Decisión
**No cambiar ahora.** libghostty-vt rinde más (×2–2.4 en throughput) y cubre de serie lo que hoy hace el interceptor, el teclado portado y lo que harán las imágenes de F7. Pero:
1. El throughput no es un cuello de botella: N-03 ya se cumple con AT (83 %), y la instantánea es algo más lenta con ghostty.
2. Para usar la 1.0 hoy hay que depender de un commit de git de unos bindings comunitarios sin publicar, o escribir los nuestros, más `unsafe` fuera de `hv-platform` (ADR), Zig en CI y un rediseño del hilo dueño por `!Send`.
3. Lo ya hecho en F1 (interceptor, teclado de Kitty portado, selección, búsqueda por regex) funciona.

**Revisar cuando ocurra cualquiera de estas cosas** (lo que llegue antes):
- empiece **F7 (imágenes)**: los gráficos de kitty vienen hechos en ghostty y en AT habría que construirlos;
- se publique en crates.io un `libghostty-vt` sobre la ABI 1.x (o los bindings tengan `Send` opcional);
- el interceptor, el modo 2027 o la conformidad empiecen a costar más que el cambio.

**Para que el cambio futuro sea barato:** crear el trait `VtEngine` que menciona el ADR 0001 y que todavía no existe en `hv-term`, con la forma de ghostty (callbacks de efectos, instantáneas desde el hilo dueño), y no exponer tipos de `alacritty_terminal` fuera de `hv-term`.

## Cómo reproducirlo
```sh
# Zig 0.17.x en el PATH solo para esta sesión (zip oficial, sin instalar)
cd spikes/ghostty-vt-bench
cargo run --release -- 32      # MB por carga; ~3 s
```
