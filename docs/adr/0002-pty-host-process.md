# ADR 0002 — Servidor de PTYs separado de la interfaz

- **Estado:** aceptado (implementación en dos pasos)
- **Fecha:** 2026-11-21

## Contexto

Los agentes trabajan durante decenas de minutos. Si la interfaz se cuelga, se cierra por error o se actualiza, hoy todos los agentes morirían con ella. Además, el cuelgue de un panel no debe bloquear los demás.

## Decisión

Separar dos papeles detrás de un trait desde el primer día:

```rust
trait PaneHost {
    fn spawn(&self, spec: SpawnSpec) -> Result<PaneId>;
    fn write(&self, pane: PaneId, bytes: &[u8]);
    fn resize(&self, pane: PaneId, size: TermSize);
    fn kill(&self, pane: PaneId, signal: KillMode);
    fn subscribe(&self, pane: PaneId) -> Receiver<PaneUpdate>; // líneas cambiadas, cursor, título, eventos
    fn snapshot(&self, pane: PaneId) -> ScreenSnapshot;
}
```

1. **Paso 1 (F1–F5):** `InProcessHost`, con los PTYs y los `Term` en hilos dentro de la app.
2. **Paso 2 (F7, v1.1):** `termhive-mux`, un proceso servidor por usuario que posee los PTYs, el estado VT de cada panel, la máquina de estados de agentes y el servidor IPC de hooks. La UI se conecta con `RemoteHost` por el mismo transporte local (socket Unix o named pipe).
   - Protocolo: mensajes con prefijo de longitud serializados con `postcard`, versionados.
   - El servidor envía **diferencias de líneas** (no bytes crudos), como el multiplexor de WezTerm, para que reconectar sea instantáneo.
   - El servidor se cierra solo si no hay paneles ni clientes durante 30 s.
   - Windows: proceso separado (`DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`). Unix: `setsid`.

## Consecuencias

- \+ Los agentes sobreviven a crashes y actualizaciones de la UI.
- \+ Se pueden tener varias ventanas o clientes, y en el futuro una conexión remota.
- − Más complejidad: protocolo, versiones, reconexión. Por eso se hace en dos pasos.
- − El estado VT vive en el servidor: el renderer trabaja con instantáneas, nunca con `&Term` directo. **Esto se impone desde el paso 1** para que el cambio no sea una reescritura.

## Alternativas descartadas
- Depender de tmux o zellij: no existe en Windows nativo y añade otra capa de escape.
- Reanudar con el comando *resume* del agente: sirve tras un reinicio del equipo (y se implementa igualmente, A-12), pero pierde el estado del terminal y la ejecución en curso.
