# SP-2 — ¿Qué secuencias pasan por ConPTY?

- **Fecha:** 2026-11-21
- **Estado:** 🟡 casi cerrado. Windows 11 verificado con el ConPTY del sistema **y** con el incluido (NuGet 1.25); falta Windows 10 22H2.

## Método
`hivetty --screenshot out.png --events -e powershell.exe -File osc.ps1`, donde `osc.ps1` emite cada secuencia con `Write-Host -NoNewline`. Así no se abre ninguna ventana y se imprimen los eventos que ve el interceptor.

## Resultado: Windows 11 Pro 25H2 (build 26200), ConPTY del sistema

| Secuencia | ¿Llega? | Orden respecto al texto |
|---|---|---|
| OSC 7 (cwd) | ✅ | correcto |
| OSC 133;A / 133;D;0 | ✅ | correcto |
| OSC 9 (notificación) | ✅ | correcto |
| OSC 9;4 (progreso) | ✅ | correcto |
| OSC 777;notify | ✅ | correcto |
| **OSC 7501** (estado de agente, terminada en ST) | ✅ | correcto, con `msg` en base64 decodificado |
| OSC 2 (título) | ✅ | ConPTY además envía su propio título inicial (la ruta de `powershell.exe`) |

## Resultado: mismo equipo con el ConPTY incluido (`cargo xtask conpty-fetch`, NuGet 1.25.261109001)

- Se comprobó que el proceso hijo es `target\debug\OpenConsole.exe` y no `conhost.exe`.
- Llegan **las mismas secuencias, en orden**.
- **Desaparece el título falso** que ponía el ConPTY del sistema (la ruta de `powershell.exe`).

## Conclusiones
- En Windows 11 reciente **no hace falta** el plan B por IPC (F3-06) para estas secuencias.
- Se mantiene la decisión de **incluir ConPTY 1.25** (F1-31) por Windows 10 y por las mejoras de reflow y velocidad. Antes hay que medir si en Windows 10 22H2 el ConPTY del sistema las desordena.
- El título inicial que pone ConPTY (la ruta del ejecutable) debe ignorarse para los títulos de sesión de agentes (F4-10).

## Pendiente
- Windows 10 22H2 (VM).
- DECSET 2026 a través de ConPTY con una TUI real (Claude Code).
