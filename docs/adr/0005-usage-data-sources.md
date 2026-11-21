# ADR 0005 — De dónde salen las métricas de uso

- **Estado:** propuesto
- **Fecha:** 2026-11-21

## Contexto

El panel de uso necesita tokens por modelo y agente, tareas, turnos y horas de agente, incluido el historial previo a instalar termhive y el uso de agentes que corrieron en otros terminales.

## Decisión

Tres fuentes, cada una para lo que hace mejor:

| Métrica | Fuente principal | Respaldo |
|---|---|---|
| Tokens (input, cache, output), modelo, turnos | **logs locales** de cada agente (transcripciones JSONL/JSON) | OpenTelemetry del agente si lo exporta (receptor OTLP local opcional) |
| Tareas | hooks (`UserPromptSubmit`) | logs: mensajes del usuario que no son resultados de herramientas |
| Agent time y concurrencia | **intervalos `Working` observados por termhive** (hooks o heurística) | logs: sumar los intervalos entre eventos con huecos < 5 min |

Reglas:
- **Dedupe** por (agente, id de mensaje o petición): los agentes pueden escribir el mismo mensaje más de una vez.
- Las sesiones observadas en vivo y las de los logs se unen por `session_id`; si una sesión tiene intervalos observados, se usan esos y no los inferidos.
- Los logs son la fuente de verdad de los tokens aunque haya OTel; OTel solo aporta inmediatez.
- Cada parser es un módulo con versión y fixtures por versión del agente.

## Consecuencias
- \+ Funciona con el historial desde el primer arranque y sin configurar nada.
- \+ Las horas de agente de sesiones que no tienen tokens accesibles también se ven (como opencode "— — 1 min" en la captura).
- − Los formatos de logs no son APIs estables: hacen falta mantenimiento y fixtures. `--doctor` avisa cuando un parser no reconoce un archivo.
