# ADR 0003 — Interfaz (barra lateral, pestañas, panel de uso) con egui

- **Estado:** propuesto, a validar en el spike S-3
- **Fecha:** 2026-11-21

## Contexto

Hace falta una interfaz rica (listas, búsqueda, menús, tooltips, gráficos, accesibilidad) alrededor de la rejilla del terminal, que se pinta con un renderer propio. Hacer toda esa UI a mano lleva meses.

## Decisión

- La **rejilla del terminal** la pinta `hv-render` (instancing propio en wgpu).
- **Todo lo demás** se hace con `egui` sobre `egui-wgpu`, en el mismo dispositivo y la misma pasada de render. Los paneles de terminal son "huecos" en el layout de egui donde `hv-render` dibuja.
- Gráficos del panel de uso: dibujados con el `Painter` de egui (barras, sparklines y mapa de calor son formas simples). No hace falta `egui_plot`.
- Accesibilidad: AccessKit integrado en egui.

## Criterios de salida (para revisarlo)

Si al final de la F4 se cumple alguno de estos, se valora migrar la interfaz a un renderer propio o a otro framework:
- No se logra el aspecto de las capturas (tipografía, fundidos o animaciones) con un esfuerzo razonable.
- El coste de egui por frame es > 2 ms en hardware medio con la barra lateral llena (200 sesiones).
- Problemas de IME o accesibilidad sin solución.

## Alternativas
- **UI propia con `taffy` (layout) + `cosmic-text`**: control total, mucho más trabajo.
- **GPUI (Zed)**: excelente, pero obliga a adoptar todo su framework y su ritmo de cambios.
- **Iced / Slint / Xilem**: válidas. Iced tiene un modelo Elm que encaja peor con la integración del renderer propio; Slint tiene licencia dual (GPL/comercial); Xilem aún no está maduro.
