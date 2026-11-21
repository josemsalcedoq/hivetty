# Rendimiento (F1-28)

Mediciones con `cargo xtask bench` (build release) y con la app, en el PC de desarrollo: Windows 11, NVIDIA RTX 4060 (DX12). Fecha: 2026-11-21.

| Métrica (`requirements.md` §7.1) | Objetivo | Resultado | Estado |
|---|---|---|---|
| N-03 throughput frente al emulador solo | ≥ 80 % | **100 MB/s frente a 120 MB/s = 83 %** (64 MB de líneas con color, Unicode y OSC) | ✅ |
| Coste de la instantánea por frame (200×60) | parte de un frame de 16 ms | **0.12 ms** | ✅ |
| N-04 CPU en reposo | ~0 % | sin bucle activo (`ControlFlow::Wait`; solo se redibuja con salida, entrada o el vencimiento de un bloque sincronizado). Aún no hay parpadeo de cursor | ✅ (por diseño; medir con el parpadeo cuando se añada) |
| N-11 tamaño del binario | ≤ 30 MB | **13 MB** (release Windows x64, LTO thin) | ✅ |
| N-01 arranque hasta el primer frame | ≤ 200 ms | pendiente de medir con instrumentación | ⏳ |
| N-02 latencia tecla → píxel | p95 ≤ 1 frame + 4 ms | pendiente (Typometer, manual) | ⏳ |
| N-05/N-06 memoria | ≤ 80 MB base, ≤ 25 MB por panel | pendiente (F2, con varios paneles) | ⏳ |
| N-07 10 paneles a 60 fps | 60 fps | pendiente (F2) | ⏳ |
| N-12 `cat` de 1 GB sin congelar | sí | el lector trabaja en bloques de 64 KiB con locks cortos; pendiente de medir con la app | ⏳ |

## Notas
- El interceptor de OSC (un segundo `vte::Parser`) cuesta un 17 %. Optimización posible si hiciera falta: saltarlo cuando el bloque no contiene `ESC` y el parser está en estado base.
- El texto aún se compone con glyphon por filas, cacheadas por contenido. El atlas propio (F1-09) y el damage tracking fino (F1-12) quedan para cuando las mediciones con muchos paneles lo pidan.
