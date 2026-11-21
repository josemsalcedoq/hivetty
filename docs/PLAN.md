# Plan maestro — termhive

Terminal nativo en Rust, acelerado por GPU, para **macOS, Ubuntu y Windows**, diseñado para trabajar con muchos agentes de código en paralelo: barra lateral con el estado en vivo de cada sesión y panel de uso de tokens y horas de agente.

Este documento es el índice y el resumen. El detalle está en:

| Documento | Contenido |
|---|---|
| [requirements.md](requirements.md) | inventario completo de requisitos con IDs (W, S, P, C, A, U, N), fórmulas del panel comprobadas contra la captura y reglas de detección de tecnología |
| [ui-spec.md](ui-spec.md) | tokens de diseño, componentes, diálogos, notificaciones, accesibilidad, idiomas y primer arranque |
| [architecture.md](architecture.md) | crates, traits, hilos, pipeline de render y modelo de datos |
| [vt-compliance.md](vt-compliance.md) | qué secuencias de escape se soportan y quién las implementa |
| [agent-integration.md](agent-integration.md) | estados, fuentes (OSC 7501, hooks…), instalador de hooks, logs de uso y esquema SQLite |
| [platforms.md](platforms.md) | diferencias por SO: ConPTY, IME, notificaciones, WSL, shells… |
| [keybindings.md](keybindings.md) · [config.md](config.md) | atajos por defecto y configuración completa |
| [security.md](security.md) | amenazas, mitigaciones y privacidad |
| [testing.md](testing.md) | estrategia de pruebas, agente simulado y checklist manual |
| [release.md](release.md) | CI, empaquetado, firma, actualizaciones y canales |
| [backlog.md](backlog.md) | todas las tareas con ID, dependencias, tamaño y criterio de aceptación |
| [prior-art.md](prior-art.md) | competidores, qué aprender y problemas a evitar |
| [skills.md](skills.md) | skills, plugins y MCP revisados para desarrollar el proyecto |
| [dev-workflow.md](dev-workflow.md) | ramas, definición de terminado y cómo trabajar con agentes |
| [licensing.md](licensing.md) · [glossary.md](glossary.md) | licencias, marcas, nombre y glosario |
| [adr/](adr/) | 0001 stack · 0002 servidor de PTYs · 0003 egui · 0004 chips · 0005 fuentes de uso |

---

## 1. Visión y propuesta de valor

1. **Saber de un vistazo qué agente te necesita**, sin recorrer pestañas: barra lateral por proyecto, bandeja de atención y "ir al siguiente que me necesita".
2. **Primera clase en Windows**: casi todos los competidores son solo para macOS (cmux, Conductor, Rex) o dependen de tmux o WSL. termhive funciona igual en los tres SO con ConPTY moderno.
3. **Nativo y ligero** (Rust + GPU), frente a las alternativas en Electron.
4. **Uso real y local**: tokens, caché, tareas y horas de agente por modelo y por agente, sin enviar nada fuera.
5. **Sin parpadeo** con las TUIs de agentes (salida sincronizada bien hecha y sin saltos de scroll).

**Idioma:** toda la app (interfaz, CLI, mensajes y documentación de usuario) está **en inglés**. Solo la planificación de `docs/` está en español.

### No-objetivos
- No es un IDE ni un editor de código.
- No llama a las APIs de los modelos: lanza y observa los CLIs oficiales en un PTY (así evita los problemas de facturación de las GUIs que usan los SDKs).
- Sin nube ni cuentas. Todo es local.

## 2. Stack (ADR 0001)

`winit 0.30` + `wgpu 30` · `alacritty_terminal 0.26` con **interceptor propio** de OSC/DCS/APC · `portable-pty` (fork o git) + **ConPTY incluido (NuGet 1.25)** · `egui 0.37` para la interfaz · `cosmic-text 0.19` + `swash` · `rusqlite` · `interprocess` · `accesskit`.

Alternativa en evaluación: **libghostty-vt 1.0** (ABI C estable desde el 6 de noviembre de 2026) en el spike SP-1b.

## 3. Fases y calendario

| Fase | Semanas | Entregable | Hito demostrable |
|---|---|---|---|
| **SP** Spikes | 1–2 | 8 spikes de riesgo con decisiones escritas | informe en `docs/spikes/` |
| **F0** Cimientos | 3 | workspace, CI en 3 SO, licencia, skills | PR vacío en verde |
| **F1** Terminal | 4–8 | terminal usable con agentes, sin parpadeo | Claude Code se usa a diario en termhive |
| **F2** Pestañas y paneles | 9–10 | pestañas, divisiones, atajos, paleta | layout de la captura 1 (sin barra lateral) |
| **F3** Shell y chips | 11–13 | integración de 4 shells, chips | chips como en la captura 1 |
| **F4** Barra de agentes | 14–18 | barra lateral, OSC 7501, hooks, notificaciones, reanudar, worktrees | **captura 1 completa** ★ |
| **F5** Panel de uso | 19–21 | panel completo | **captura 2 completa** ★ |
| **F6** Pulido y distribución | 22–25 | firma, instaladores, actualizaciones, accesibilidad, i18n | **v1.0** en los 3 SO |
| **F7** v1.1+ | — | servidor de PTYs, imágenes, diffs, API… | — |

Unos 6 meses a tiempo parcial con agentes ayudando. Los hitos ★ son los de más valor.

### Reglas
- **No se empieza una fase sin cerrar la anterior** (criterios de aceptación de `backlog.md` y CI verde en los 3 SO).
- Al cerrar cada fase: checklist manual en hardware real (`testing.md` §6, reducida) y medición de rendimiento.
- Cada decisión que se aparte de un ADR → ADR nuevo.

## 4. Spikes (antes de escribir código de producción)

| ID | Riesgo que despeja |
|---|---|
| SP-1 | alacritty_terminal + wgpu + interceptor funcionan y rinden en los 3 SO |
| SP-1b | coste real de libghostty-vt 1.0 (bindings mínimos, Zig, Windows) |
| SP-2 | ConPTY incluido deja pasar OSC 7/8/9/9;4/52/133/777/**7501** y DECSET 2026 en Windows 10 22H2 y 11 |
| SP-3 | egui y la rejilla en la misma pasada; IME en el panel; AccessKit |
| SP-4 | hooks de Claude → IPC → estado en ≤ 300 ms; desde WSL; payloads reales guardados como fixtures |
| SP-5 | parser de Claude Code cuadra con `ccusage` (±1%) con datos reales |
| SP-6 | calidad del texto (mezcla, box-drawing y comparación con los rasterizadores del SO) |
| SP-7 | herramienta de empaquetado (cargo-packager frente a scripts) y prueba de firma |

## 5. Métricas de éxito de la v1.0
- Rendimiento: N-01…N-12 de `requirements.md` cumplidos en el hardware de referencia.
- Estado: con OSC 7501 o hooks, **0 falsos "Done"** y ≤ 300 ms de latencia en la prueba de 6 agentes.
- Uso: totales de Claude Code a ±1% de `ccusage`.
- Estabilidad: ≥ 99.5% de sesiones sin crash; resistencia de 8 h superada.
- Uso propio: el autor usa termhive como terminal principal durante 2 semanas sin volver a otro.

## 6. Riesgos principales

| Riesgo | Prob. | Impacto | Mitigación |
|---|---|---|---|
| alacritty_terminal no expone OSC/DCS/APC | **confirmado** | alto | interceptor propio (diseñado); alternativa libghostty-vt |
| La codificación de teclado de Kitty, el ratón y el box-drawing viven en el frontend de Alacritty | **confirmado** | medio | portarlos con atribución (Apache-2.0) |
| ConPTY del sistema reordena o filtra secuencias | alta en 1809–22H2 | alto | incluir ConPTY 1.25 (deja pasar todo); `--doctor` verifica que se usa |
| `portable-pty` sin releases desde feb 2025 | confirmado | medio | dependencia git fijada o fork en `th-pty` |
| OSC 7501 es un borrador y puede cambiar | media | medio | parser tolerante y versionado; hooks como respaldo |
| Formatos de logs y hooks de agentes cambian | alta | medio | fixtures por versión, parsers tolerantes, avisos en `--doctor` |
| IME en Windows solo con IMM32 (winit 0.30) | media | medio | probar con japonés y chino en SP-3; subir a winit 0.31 cuando egui lo soporte |
| Notificaciones de Windows sin AUMID salen como "PowerShell" | confirmado | bajo | acceso directo e AUMID desde el instalador |
| Firma de Windows: Artifact Signing solo para particulares de EE. UU. y Canadá | confirmado | medio | certificado OV de una CA o crear una organización en un país admitido |
| Cuenta de Apple Developer necesaria para notarizar | confirmado | bajo | 99 $/año; presupuestarlo |
| Sin Mac ni Ubuntu con GPU para probar | alta | medio | runners de GitHub + Mac en la nube + VM con aceleración 3D |
| Alcance enorme para una persona | alta | alto | fases entregables, spikes, agentes en paralelo por crate y recortar F7 sin dudar |
| Choque de nombre o marca | media | bajo | comprobar antes de publicar (`licensing.md`) |

## 7. Presupuesto aproximado
| Concepto | Coste |
|---|---|
| Apple Developer Program | 99 $/año |
| Firma de Windows (Artifact Signing Basic, si se cumple la elegibilidad) o certificado OV | ~10 $/mes o ~200–400 $/año |
| Mac en la nube para pruebas puntuales (si no hay Mac) | variable (por horas) |
| Dominio y web | ~15 $/año |
| GitHub Actions | gratis para repos públicos; en privados hay minutos incluidos, y macOS los gasta 10 veces más rápido |

## 8. Preguntas abiertas (decidir antes o durante F0)
1. Nombre definitivo (¿"termhive"?) y licencia (¿MIT/Apache o GPL?).
2. ¿Repo público desde el principio o privado hasta la v0.5?
3. ¿Hay acceso a un Mac (aunque sea en la nube) y a un equipo Ubuntu con GPU?
4. ¿Organización legal para firmar en Windows, o certificado OV?
5. ¿Se mantiene alacritty_terminal o se pasa a libghostty-vt tras SP-1b?
6. ¿El panel de uso muestra coste estimado por defecto?
