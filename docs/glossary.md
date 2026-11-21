# Glosario

| Término | Significado |
|---|---|
| **Agente** | Programa de IA para programar que corre en el terminal (Claude Code, Codex, Gemini CLI…). |
| **Sesión** | Una ejecución de un agente en un panel, con su id, proyecto, estado y métricas. |
| **Tarea** | Un prompt del usuario que provoca trabajo del agente (de `UserPromptSubmit` a `Stop`). |
| **Turno** | Un mensaje del asistente (respuesta del modelo) dentro de una tarea. |
| **Agent time** | Tiempo que una sesión pasa en estado `Working`. |
| **Proyecto** | La raíz git del directorio de la sesión (o el propio directorio si no hay repo). |
| **Hook** | Comando que el agente ejecuta en ciertos eventos; hivetty lo usa para saber el estado. |
| **PTY** | Pseudo-terminal: el par de extremos que conecta el terminal con la shell o el agente. En Windows, **ConPTY**. |
| **VT / secuencias de escape** | Protocolo de texto (`ESC [ …`) con el que los programas controlan el terminal. |
| **OSC** | *Operating System Command* (`ESC ] … BEL`): secuencias para título, directorio, enlaces, notificaciones… |
| **OSC 7** | Informa del directorio actual. |
| **OSC 8** | Hiperenlaces. |
| **OSC 9 / 777** | Notificaciones de escritorio. **OSC 9;4**: barra de progreso. |
| **OSC 52** | Acceso al portapapeles. |
| **OSC 133** | Marcas semánticas de prompt y comando (FinalTerm). |
| **DECSET 2026** | Salida sincronizada: el programa marca el inicio y el fin de un frame para que no se vea a medias. |
| **Chip** | Píldora con un logo o dato (tecnología, rama, versión) que se muestra sobre el prompt. |
| **Mux / servidor de PTYs** | Proceso que posee los PTYs para que sobrevivan a la interfaz (ADR 0002). |
| **Atlas de glifos** | Textura de la GPU con los caracteres ya rasterizados. |
| **Instancing** | Dibujar miles de quads (celdas) con una sola llamada a la GPU. |
| **Dorado (test)** | Comparar una imagen generada con una de referencia guardada. |
| **Spike** | Prototipo corto para resolver una duda técnica antes de comprometerse. |
| **Worktree** | Copia de trabajo extra de un repo git en otra carpeta y otra rama. |
| **ADR** | *Architecture Decision Record*: documento corto con una decisión y su porqué. |
