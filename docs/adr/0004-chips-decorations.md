# ADR 0004 — Chips del prompt como decoraciones ancladas a la rejilla

- **Estado:** aceptado (implementado en F3, 2026-11-22)
- **Fecha:** 2026-11-21

## Contexto

En la captura, el prompt muestra *chips* con el logo de la tecnología del proyecto, y si no caben en la fila no se muestran. Lograrlo pintando imágenes desde la shell exige un protocolo de imágenes (que `alacritty_terminal` no trae) y lógica de anchos duplicada en cuatro shells.

## Decisión

1. La **shell** solo informa: OSC 133 (marcas de prompt) y OSC 7 (directorio).
2. El **terminal** (Rust) detecta la tecnología (`docs/requirements.md` §8), lee la rama git y las versiones, y decide qué chips caben.
3. Los chips se pintan como **decoraciones ancladas a una línea** de la rejilla, la del inicio del prompt (OSC 133;A). Se dibujan en una franja sobre el prompt reservada por la shell:
   - La integración de shell imprime una **línea vacía reservada** antes del prompt solo si el terminal lo pide (variable `HIVETTY_CHIPS=1`).
   - El terminal pinta los chips sobre esa línea. Al hacer scroll se mueven con ella porque están anclados a la línea, no a la pantalla.
4. Si un chip no cabe en el ancho del panel, **no se pinta** (se evalúan en orden y se para en el primero que no cabe).
5. Los logos son SVG en `assets/icons/tech/`, rasterizados con `resvg` al atlas de iconos según el DPI.

## Consecuencias
- \+ Igual en zsh, bash, fish y PowerShell; un único sitio con la lógica.
- \+ No depende de protocolos de imágenes ni de que el terminal remoto los soporte.
- − Al copiar texto del prompt, los chips no se copian (es lo deseado).
- − Por SSH no hay chips salvo que el servidor tenga la integración instalada (C-09).

## Alternativa descartada
Calcular los chips en la shell (como hace el terminal de referencia con un script de zsh): obliga a reimplementarlo en cuatro lenguajes de shell y a un protocolo de imágenes.

## Notas de implementación (F3)
- La shell imprime la línea en blanco reservada justo antes de OSC 133;A cuando `HIVETTY_CHIPS=1` (opción `[shell] chips`). Los chips se pintan en la fila anterior a cada prompt, y solo si esa fila sigue vacía.
- La detección corre en un hilo (`hv_shell::ChipService`) con caché de 3 s por directorio; los chips de cada prompt se guardan por su línea del scrollback, así que los prompts antiguos conservan los suyos al hacer scroll.
- Aún sin logos SVG (F3-14): cada chip lleva un punto de color.
