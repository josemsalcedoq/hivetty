# Skills de agentes para desarrollar termhive

Las skills son carpetas con un `SKILL.md` que Claude Code (y otros agentes compatibles con el estándar Agent Skills) cargan cuando la tarea encaja.

- Las skills del proyecto van en `.claude/skills/<nombre>/SKILL.md`.
- Las personales van en `~/.claude/skills/`.

> ⚠️ **Todas las de terceros son código y prompts de desconocidos.** Leer el `SKILL.md` y los scripts que incluyan antes de instalarlas. Ninguna de esta lista se ha revisado ni instalado todavía.

## Recomendadas por fase

| Skill | Fuente | Para qué | Fase |
|---|---|---|---|
| rust-engineer | [jeffallan/claude-skills](https://jeffallan.github.io/claude-skills/skills/language/rust-engineer) | ownership, diseño de APIs seguras, concurrencia | todas |
| rust-skills | [zhanghandong/rust-skills](https://claudemarketplaces.com/skills/zhanghandong/rust-skills/rust-learner) | conjunto de skills de Rust (`npx skills add`) | todas |
| rust-skills (actionbook) | [actionbook/rust-skills](https://claudemarketplaces.com/skills/actionbook/rust-skills/core-dynamic-skills) | otra colección de Rust; elegir **una** de las dos para no duplicar | todas |
| Rust Terminal Emulation Patterns (iterm2-rs) | [killer-skills](https://killer-skills.pages.dev/en/skills/Wolfschanze-Berlin/iterm2-rs) | VT100/VT220, gestión de PTY, `alacritty_terminal` + `wgpu` + `glyphon`: **es casi nuestro stack** | F1–F3 |
| Rio WebGPU Tiles | [agent-skills.md](https://agent-skills.md/skills/plurigrid/asi/rio-webgpu-tiles) | render por tiles con wgpu al estilo del terminal Rio | F1 |
| wgpu Shader Specialist | [eliteai.tools](https://eliteai.tools/agent-skills/wgpu-shader-specialist) | pipelines wgpu y shaders WGSL | F1 |
| ghostling-libghostty-terminal | [skills.sh](https://www.skills.sh/aradotso/trending-skills/ghostling-libghostty-terminal) | solo si se cambia a `libghostty-vt` (ver ADR 0001) | — |
| skill-creator | oficial de Anthropic (incluida en Claude Code) | crear nuestras propias skills de proyecto | F0 |

Origen dudoso: iterm2-rs y Rio WebGPU Tiles salen de directorios de terceros. Comprobar el repo original antes de instalar.

## Directorios para buscar más

- [skills.sh](https://skills.sh) — ecosistema abierto de skills (CLI `npx skills`). [Anuncio de Vercel](https://vercel.com/changelog/introducing-skills-the-open-agent-skills-ecosystem).
- [heilcheng/awesome-agent-skills](https://github.com/heilcheng/awesome-agent-skills)
- [kodustech/awesome-agent-skills](https://github.com/kodustech/awesome-agent-skills)
- [JackyST0/awesome-agent-skills](https://github.com/JackyST0/awesome-agent-skills)
- [gmh5225/awesome-skills](https://github.com/gmh5225/awesome-skills)
- [Documentación oficial de skills en Claude Code](https://code.claude.com/docs/en/skills)

## Gestores de skills multiplataforma (opcional)

- [qntx/skill](https://github.com/qntx/skill) — port en Rust del CLI de skills.sh. Funciona en Linux, macOS y Windows (usa *junctions* en Windows).
- [chrlsio/agent-skills](https://awesome.ecosyste.ms/projects/github.com%2Fchrlsio%2Fagent-skills) — app de escritorio (Tauri 2) para gestionar skills.

## Proyectos de referencia (código, no skills)

| Proyecto | Por qué mirarlo |
|---|---|
| [Alacritty](https://github.com/alacritty/alacritty) | `alacritty_terminal`, ConPTY, rendimiento |
| [CuTTY](https://docs.rs/crate/cutty/latest/source/README.md) | fork de Alacritty con wgpu/vello + parley, ConPTY |
| [par-term](https://docs.rs/crate/par-term/latest/source/CONTRIBUTING.md) | terminal Rust multiplataforma con wgpu y shaders WGSL |
| [WezTerm](https://github.com/wezterm/wezterm) | `portable-pty`, multiplexor, imágenes, Windows |
| [Rio](https://github.com/raphamorim/rio) | terminal con WebGPU (`sugarloaf`) |
| [Ghostty](https://github.com/ghostty-org/ghostty) / libghostty | conformidad VT de referencia |
| [Zed](https://github.com/zed-industries/zed) | terminal integrado con `alacritty_terminal` sobre GPUI |
| [Warp `AGENTS.md`](https://raw.githubusercontent.com/warpdotdev/warp/HEAD/AGENTS.md) | cómo organiza un terminal con agentes sus instrucciones para agentes |
| [ccusage](https://github.com/ryoppippi/ccusage) | parseo de los logs de uso de Claude Code |

## Skills propias del repo

- `.claude/skills/termhive-platform-check/`: lista de comprobación multiplataforma antes de cerrar cualquier cambio.
