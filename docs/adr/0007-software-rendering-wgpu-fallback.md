# ADR 0007 — Renderizado por software con el adaptador de reserva de wgpu, sin tiny-skia

- **Estado:** aceptado
- **Fecha:** 2026-11-22
- **Cambia:** la tarea F1-13 ("Renderer por software (`tiny-skia`) con el mismo trait") del backlog.

## Contexto

El plan preveía un segundo renderer en CPU con `tiny-skia` para máquinas sin GPU utilizable (escritorios remotos, máquinas virtuales, drivers rotos) y para el modo `--safe` (N-23). Ese renderer tendría que reproducir todo lo que hace `hv-render`: celdas, atlas de glifos, subrayados, selección, chips, anillos, el margen de marcas y la interfaz egui. Serían dos implementaciones que mantener iguales píxel a píxel.

wgpu ya ofrece un adaptador por software (`force_fallback_adapter`): WARP en Windows (incluido en el sistema desde Windows 8) y llvmpipe/lavapipe en Linux (Mesa). Usa los mismos shaders y el mismo código que la GPU.

## Decisión

- No hay renderer `tiny-skia`. El renderizado por software es `hv_render::set_software(true)`, que pide a wgpu el adaptador de reserva.
- `--safe` (o `HIVETTY_SAFE=1`) lo activa junto con la configuración por defecto (F6-12). `--doctor` y el log muestran el adaptador ("Microsoft Basic Render Driver (Dx12, Cpu)").
- Comprobado en Windows: la captura sin cabeza con `--safe` sale igual que con la GPU.

## Consecuencias

- \+ Un solo renderer: lo que se ve con software es exactamente lo de la GPU ("dorado CPU = GPU" por construcción).
- \+ Nada de código nuevo que mantener.
- − macOS no tiene adaptador de reserva. En la práctica todos los Mac compatibles tienen Metal; si falla, `--safe` no lo arregla allí.
- − En Linux depende de que Mesa esté instalado (lo está en casi todas las distribuciones de escritorio; el paquete `.deb` lo puede declarar como recomendado).
- − WARP y llvmpipe son lentos con ventanas grandes; para el modo seguro es suficiente.
