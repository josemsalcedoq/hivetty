# SP-6: calidad del texto

**Estado (2026-11-21):** ✅ decidido. Seguimos con `swash` (vía cosmic-text/glyphon), sin hinting y con la mezcla tipo web (sRGB). Box-drawing por código (F1-10, F1-35).

## Qué se probó
1. **Hinting de métricas** (`cosmic_text::Hinting::Enabled`, ajusta los glifos a píxeles enteros en X) frente a sin hinting. Captura sin ventana de un texto con letras, números, ligaduras habituales y símbolos, a 14 px y escala 1 en Windows 11 (RTX 4060).
   - Medida: porcentaje de píxeles de tinta que son parciales (bordes difusos) frente a sólidos.
   - Sin hinting: **70,8 %** parciales. Con hinting: **71,7 %**. Cambian 6.420 píxeles, pero solo de posición: los glifos se mueven, no salen más nítidos.
   - Con avance monoespaciado fijo (`set_monospace_width`) el ajuste en X además separa de forma irregular los caracteres cuando el ancho de celda no es entero.
2. **Mezcla en sRGB (modo `Web` de glyphon, superficie no sRGB) o en lineal (`Accurate`, superficie sRGB).** No se cambió el pipeline: los rectángulos, egui y los colores del tema están en bytes sRGB. Pasar a lineal obliga a convertir todos los colores en los shaders y egui-wgpu.
   - Mezclar en sRGB es lo que hacen navegadores, VS Code y la mayoría de terminales. El texto claro sobre fondo oscuro (el tema por defecto) se ve con más cuerpo.
   - Mezclar en lineal adelgaza el texto claro sobre oscuro y engorda el oscuro sobre claro.
3. **Rasterizadores del SO** (DirectWrite, Core Text, FreeType) frente a `swash`. No se implementó: supone FFI por plataforma (código `unsafe` fuera de `hv-platform`) y tres resultados distintos.
   - `swash` da el mismo píxel en los tres SO, y eso permite tests dorados comunes.
   - Lo que se pierde es el antialiasing por subpíxel (ClearType) en Windows con pantallas de baja densidad. macOS ya no lo usa desde 10.14.

## Decisión
- Mantener `swash`, sin hinting, mezcla sRGB (`ColorMode::Web`) y escala de grises.
- Box-drawing, bloques y esquinas redondeadas se dibujan por código (hecho en F1-10/F1-35), no con la fuente.

## Pendiente o a vigilar
- Si usuarios con monitores de 96 ppp en Windows se quejan de nitidez: probar antialiasing por subpíxel en el atlas propio (F1-09). Requiere conocer el orden RGB/BGR del monitor y desactivarlo con fondos transparentes.
- Opción de configuración `text.gamma` / contraste (como `alpha-blending` en Ghostty) cuando exista el atlas propio.
- Repetir la medida en macOS (Retina, escala 2) y Linux en el CI cuando haya capturas comparables.
