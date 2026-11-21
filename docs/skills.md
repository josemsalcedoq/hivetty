# Skills, plugins y MCP para desarrollar hivetty

Revisado el **2026-11-21**: cada `SKILL.md` se leyó en su repo de origen. ★ = estrellas de GitHub en esa fecha.

- Skills del proyecto: `.claude/skills/<nombre>/SKILL.md` (se versionan con el repo).
- Personales: `~/.claude/skills/`.

## Reglas de instalación
1. **Leer siempre el `SKILL.md` y los scripts** antes de instalar. Una skill es un prompt y a veces código que se ejecuta.
2. Preferir **una skill concreta** (`--skill <nombre>`) a un plugin entero: los plugins pueden traer hooks que se ejecutan en cada prompt.
3. Con `npx skills` usar `--copy` (en Windows los symlinks requieren el modo desarrollador) y **commitear la copia**, para que todos usen la misma versión revisada.
4. `npx skills` envía telemetría anónima: usar `DISABLE_TELEMETRY=1`.
5. No apilar varias skills generales de Rust: sus disparadores se pisan.
6. Anotar en la tabla de abajo la fecha de la revisión y el commit instalado.

## Top 10 para instalar primero

| # | Qué | Por qué | Instalación | Notas de confianza |
|---|---|---|---|---|
| 1 | **rust-analyzer-lsp** (oficial) | diagnósticos reales del compilador y "ir a la definición" para Claude | `rustup component add rust-analyzer` y `/plugin install rust-analyzer-lsp@claude-plugins-official` | oficial; solo configuración |
| 2 | **rust-docs-mcp** ([snowmead](https://github.com/snowmead/rust-docs-mcp)) | docs de **las versiones exactas** de crates que usamos (wgpu, winit y egui rompen APIs en cada versión) | `cargo install rust-docs-mcp` y `claude mcp add rust-docs -- rust-docs-mcp` | 142★, activo. Descarga crates de crates.io. **No** usar su instalador `curl \| bash` |
| 3 | **superpowers** ([obra](https://github.com/obra/superpowers)): `test-driven-development`, `systematic-debugging`, `verification-before-completion` | disciplina de TDD, depuración y verificación | escogidas una a una: `npx skills add obra/superpowers --skill test-driven-development -a claude-code --copy` | 323★k. ⚠️ El **plugin** completo tiene un hook de SessionStart que obliga a usar skills en cada sesión y puede pisar las reglas de fases de `CLAUDE.md`: instalar solo las skills sueltas |
| 4 | **alacritty-terminal** y **portable-pty** (guías de API) | las únicas guías encontradas de **nuestros dos crates base**: `Term<T>`, EventListener, grid y modos; PtySystem, ConPTY, hilos lectores y resize | copiar los `SKILL.md` de la [instantánea](https://github.com/majiayu000/claude-skill-registry-data/tree/a21980a5b576646393ed574f158d7dd92fae4c3f/data/) a `.claude/skills/` y **quitar las secciones de script-kit-gpui** | huérfanas (el repo original johnlindquist/script-kit-gpui las borró) y fijadas a alacritty_terminal 0.25: actualizarlas a nuestra versión. Solo markdown. **Convertirlas en skills propias del proyecto** |
| 5 | **terminal-standards** + **input-handling** ([JosiahSiegel/claude-plugin-marketplace](https://github.com/JosiahSiegel/claude-plugin-marketplace), `plugins/tui-master`) | referencia de ECMA-48, DEC, xterm, OSC 8/52, teclado de Kitty, bracketed paste y peculiaridades de ConPTY | `npx skills add JosiahSiegel/claude-plugin-marketplace --skill terminal-standards -a claude-code --copy` | 65★. Escrita desde el lado de la app TUI, pero las tablas de secuencias sirven. Solo markdown |
| 6 | **rust-review** ([trailofbits/skills](https://github.com/trailofbits/skills)) | auditoría de `unsafe`, FFI, concurrencia, pánicos y async (salida REPORT.md + SARIF) | `/plugin marketplace add trailofbits/skills` y `/plugin install rust-review@trailofbits` | 8.3★k, activo. Ejecuta Python incluido con `uv run` y necesita `rg`. Gasta muchos tokens: usarla para `hv-platform` y antes de cada release |
| 7 | **Una** skill general de Rust: [leonardomso/rust-skills](https://github.com/leonardomso/rust-skills) (265 reglas, Rust 2024) **o** [Jeffallan rust-engineer](https://github.com/Jeffallan/claude-skills) (más ligera) | idiomas, errores, APIs, rendimiento y tests | `npx skills add leonardomso/rust-skills -a claude-code --copy` | leonardomso: 737★, solo markdown. Jeffallan: 12.5★k, bastante genérica. **No** instalar el plugin de Jeffallan (carga 70 skills) |
| 8 | **ffi-code-review** ([existential-birds/beagle](https://github.com/existential-birds/beagle)) **o** `unsafe-checker` de [actionbook/rust-skills](https://github.com/actionbook/rust-skills) suelta | checklist de `extern`, `repr(C)`, CString y callbacks, para `hv-platform` (Win32, ConPTY, Cocoa) | `npx skills add existential-birds/beagle --skill ffi-code-review -a claude-code --copy` | beagle: solo markdown. ⚠️ **Nunca el plugin de actionbook**: trae un hook de UserPromptSubmit que se dispara en casi cualquier prompt e inyecta reglas "MANDATORY", y un MCP con `npx …@latest` sin fijar versión |
| 9 | **webgpu-specs** (oficial de [gfx-rs/wgpu](https://github.com/gfx-rs/wgpu), `.claude/skills/webgpu-specs`) | descarga las especificaciones de WebGPU y WGSL y responde citándolas | copiar a mano `SKILL.md` + `download.sh` y ajustar la ruta | oficial. Necesita sh, jq y cargo; usa curl contra raw.githubusercontent.com/gpuweb |
| 10 | **plugin-dev** (oficial, skill `hook-development`) | la barra lateral depende de los hooks de los agentes: escribirlos y probarlos bien | `/plugin install plugin-dev@claude-plugins-official` | oficial; sus scripts corren en local |

## Segundo nivel (cuando toque)

| Cuándo | Skill | Fuente | Notas |
|---|---|---|---|
| F1 (fuzzing de parsers) | `testing-handbook-skills` (cargo-fuzz, cobertura) | trailofbits/skills | cargo-fuzz necesita nightly |
| F1–F2 (reflow, serialización) | `property-based-testing` | trailofbits/skills | solo markdown |
| Revisión de PRs | `differential-review`, `sharp-edges` | trailofbits/skills | solo markdown |
| Revisión de PRs | `pr-review-toolkit` | anthropics/claude-plugins-official | oficial; 6 agentes revisores |
| Dependencias | `supply-chain-risk-auditor` | trailofbits/skills | consulta osv.dev, deps.dev, scorecard.dev y la API de GitHub |
| CI | `gh-fix-ci` | [openai/skills](https://github.com/openai/skills) | necesita `gh` autenticado (**el token de `gh` de este PC no es válido ahora mismo**) |
| CI | `add-zizmor` | [Seldaek/zizmorify](https://github.com/Seldaek/zizmorify) | abre un PR al final: revisarlo |
| ADRs | `architecture-decision-records` | [wshobson/agents](https://github.com/wshobson/agents) | plantillas MADR |
| Docs | `doc-coauthoring` | [anthropics/skills](https://github.com/anthropics/skills) | oficial |
| F5 (referencia) | `session-report` | anthropics/claude-plugins-official | script que parsea `~/.claude/projects`: útil para comparar con nuestro parser |
| Opcional | `rust-best-practices` | [apollographql/skills](https://github.com/apollographql/skills) | pre-aprueba `Bash(cargo:*)` |
| Opcional | context7 (MCP) | upstash/context7 | servidor remoto: **las consultas salen de la máquina** |
| Opcional | Serena (MCP) | oraios/serena | fijar una etiqueta (por defecto ejecuta HEAD sin fijar); se solapa con rust-analyzer-lsp |

## Descartadas o solo como referencia
- **iterm2-rs "Rust Terminal Emulation Patterns"** ([Wolfschanze-Berlin/iterm2-rs](https://github.com/Wolfschanze-Berlin/iterm2-rs)): unas 80 líneas sobre el flujo winit → PTY → alacritty_terminal → wgpu+glyphon, con rutas de ese repo. Sirve **como plantilla**. ⚠️ El `.claude/settings.json` de ese repo activa `bypassPermissions`: **no clonar ni abrir ese repo con Claude Code**; copiar solo el texto del `SKILL.md`.
- **rio-webgpu-tiles** (plurigrid/asi): es un protocolo OSC 1337 para un fork de Rio, no trata texto ni la rejilla.
- **wgpu-shader-specialist**: el original se borró y solo queda una copia en un agregador, ligada a otro proyecto.
- **ghostling-libghostty-terminal** (reason-machines/trending-skills): generada automáticamente por un LLM. Solo si un ADR cambia a libghostty-vt.
- **qntx/skill**: es un CLI, no una skill, y está casi vacío. Usar el `npx skills` original (vercel-labs/skills).
- **zhanghandong/rust-skills = actionbook/rust-skills** (el repo se transfirió): no son dos opciones distintas.
- **martinholovsky/claude-skills-generator** (sqlite, cross-platform-builds): generadas por IA, abandonadas y con nombres que incumplen la especificación. Solo para ideas.
- Sin fuente real o fuera de nuestro stack: wgpu-rendering, slug-font-rendering, microsoft-rust-training, makepad-*, adr-skill de vercel/ai y security-guidance.

## Huecos: skills que hay que escribir nosotros
No hay nada fiable publicado para estos temas. Se crean con `skill-creator` (incluida en Claude Code) a medida que se aprende en cada fase:

| Skill propia | Fase | Contenido |
|---|---|---|
| `hivetty-platform-check` | ✔ ya existe | checklist multiplataforma |
| `hivetty-alacritty-term` | F1 | uso de `alacritty_terminal` en nuestra versión, adaptado de la instantánea (#4) |
| `hivetty-pty` | F1 | `portable-pty`, ConPTY y matar árboles de procesos, adaptado de la instantánea (#4) |
| `hivetty-text-rendering` | F1 | cosmic-text, swash y atlas; box-drawing; mezcla de alfa (de SP-6) |
| `hivetty-egui-integration` | F2–F4 | egui + rejilla propia, AccessKit e IME (de SP-3) |
| `hivetty-winit` | F1 | eventos, DPI, IME y particularidades por SO |
| `hivetty-agent-hooks` | F4 | contrato de `hivetty hook`, OSC 7501 y añadir un agente nuevo |
| `hivetty-usage-parser` | F5 | cómo añadir un parser de logs con fixtures |
| `hivetty-release` | F6 | empaquetado, firma y notarización por SO |

## Proyectos de referencia (código, no skills)
Ver `docs/prior-art.md` para los terminales con agentes. Para el núcleo del terminal: [Alacritty](https://github.com/alacritty/alacritty), [WezTerm](https://github.com/wezterm/wezterm), [Rio](https://github.com/raphamorim/rio), [Ghostty](https://github.com/ghostty-org/ghostty), [Zed](https://github.com/zed-industries/zed) (terminal con alacritty_terminal), [CuTTY](https://docs.rs/crate/cutty/latest/source/README.md), [par-term](https://docs.rs/crate/par-term/latest/source/CONTRIBUTING.md), [Warp AGENTS.md](https://raw.githubusercontent.com/warpdotdev/warp/HEAD/AGENTS.md), [ccusage](https://github.com/ryoppippi/ccusage) y [tokscale](https://github.com/junhoyeo/tokscale).
