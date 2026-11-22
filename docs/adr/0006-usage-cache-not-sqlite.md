# ADR 0006 — Caché incremental de uso en un archivo, sin SQLite por ahora

- **Estado:** aceptado
- **Fecha:** 2026-11-22
- **Cambia:** la fila "Sesiones, layout, uso, intervalos → `state.sqlite`" de `docs/architecture.md` §6 para el uso.

## Contexto

El plan guardaba el uso en SQLite (`rusqlite` con `bundled`). Al implementarlo (F5-01, F5-04):
- `bundled` compila SQLite en C. Eso rompe la comprobación de tipos para macOS y Linux que `cargo xtask ci` hace desde Windows (no hay compilador de C para esos destinos) y añade un toolchain de C a los tres CI.
- Los datos de uso son de **solo añadir**: los agentes escriben sus logs al final y nunca cambian lo anterior. No hacen falta consultas arbitrarias: el panel agrega en memoria (`hv_usage::dashboard::build`) y es rápido.

## Decisión

- `hv_usage::store::Store`: lo ya parseado (registros y tareas), las claves de dedupe y, por cada log, **hasta qué byte se leyó** (siempre en un salto de línea). Cada refresco lee solo las líneas nuevas completas; una línea a medio escribir se deja para la siguiente vez; un archivo que encoge se vuelve a leer (el dedupe evita duplicados).
- Se guarda con `postcard` en `<datos>/usage.cache`, versionado y con escritura atómica. Un archivo corrupto, de otra versión o de más de 512 MB se ignora y se reconstruye.
- La lectura va en un hilo al abrir el panel, con una caché en memoria de 60 s.

## Consecuencias

- \+ Sin C en el árbol de dependencias; la comprobación cruzada sigue cubriendo todo el workspace.
- \+ Con los logs de este equipo (110 MB, 42 archivos) la caché ocupa 650 KB y la segunda apertura no relee nada.
- − Todo el uso se carga en memoria (unos 40 bytes por registro en disco; algo más en memoria). Con historiales muy grandes (millones de turnos) habría que agregar por día al guardar.
- − Si algún día hacen falta consultas (filtros por proyecto sobre años de datos, exportar por rangos), se puede volver a SQLite con un ADR nuevo; el formato de la caché es interno y se puede migrar releyendo los logs.
