# Seguridad y privacidad

## 1. Qué protegemos
- Lo que el usuario escribe y lo que se muestra (puede incluir contraseñas, tokens y código privado).
- El portapapeles.
- La configuración de los agentes del usuario (`~/.claude/settings.json`, etc.).
- El canal IPC (quien lo controle puede falsear estados o, si se permite, escribir en los PTYs).
- La máquina del usuario frente a actualizaciones o dependencias comprometidas.

## 2. Amenazas y mitigaciones

### 2.1 Salida maliciosa en el terminal
Cualquier programa (o un archivo con `cat`, o un servidor por SSH) puede emitir secuencias de escape.

| Amenaza | Mitigación |
|---|---|
| Reporte de título (`CSI 21t`) que reinyecta texto como entrada | desactivado (`title_reporting = false`) |
| Leer el portapapeles con OSC 52 | **nunca** se permite |
| Escribir el portapapeles con OSC 52 | límite de tamaño y modo "confirmar" opcional |
| Secuencias enormes (DoS de memoria) | límite por OSC/DCS/APC (64 KiB; imágenes: límite propio y cuota por panel) |
| OSC 8 con esquemas peligrosos (`file:` remoto, `javascript:`, protocolos de apps) | solo se abren `http`, `https`, `mailto` y `file` local, con la URL visible al pasar el ratón; otros esquemas piden confirmación |
| Notificaciones OSC 9/777 falsas ("introduce tu contraseña") | se muestra el origen (pestaña) y se limita la longitud; no se pueden ocultar como si fueran del sistema |
| Respuestas a consultas (DA, DECRQSS, XTGETTCAP) que contienen datos controlados por el atacante | las respuestas nunca contienen texto arbitrario del programa |
| Hiperenlaces ocultos en texto legítimo | al pasar el ratón se ve la URL real |

### 2.2 Pegado
- *Bracketed paste* siempre que el programa lo pida.
- Se elimina `ESC[201~` (y cualquier ESC si `paste_strip_escape = true`) del texto pegado para que no pueda cerrar el modo y ejecutar comandos.
- Aviso al pegar varias líneas en una shell sin bracketed paste.

### 2.3 IPC local
- Socket Unix en un directorio privado (`$XDG_RUNTIME_DIR/hivetty/` o `$TMPDIR`, permisos `0700`) y archivo `0600`.
- En Windows, named pipe con una DACL solo para el usuario actual y `PIPE_REJECT_REMOTE_CLIENTS`.
- **Token** aleatorio de 128 bits por arranque en `HIVETTY_IPC_TOKEN`, que cada mensaje debe incluir. Un proceso de otro usuario no puede hablar con el IPC, y uno del mismo usuario sin el entorno de un panel tampoco.
- Los hooks solo pueden **informar** de eventos. Escribir en un PTY (`send-text`) o abrir paneles por IPC está **desactivado** por defecto (`remote_control = false`).
- Mensajes con tamaño máximo (1 MiB) y esquema estricto; la versión del protocolo se negocia.

### 2.4 Hooks en la configuración de los agentes
- Nunca se escribe sin mostrar el diff y pedir confirmación. Se crea una copia (`settings.json.hivetty-bak`).
- Las entradas llevan un marcador (`"hivetty": true` o un comentario según el formato) para poder desinstalarlas exactamente.
- El comando del hook usa la ruta absoluta del binario entre comillas y no interpola datos.
- `hivetty hook` **termina siempre en < 1 s con código 0**: si el IPC no responde, descarta el evento. Nunca bloquea ni rompe el agente.
- El JSON de entrada se lee con un límite de tamaño.

### 2.5 Detección de proyecto
- **Nunca se ejecuta** nada del proyecto (ni `node --version` ni `rustc --version`, que podría descargar un toolchain por `rust-toolchain.toml`; ni scripts de `package.json`). Solo se leen archivos, con límite de tamaño y sin seguir enlaces simbólicos fuera del repo.

### 2.6 Logs de agentes y datos de uso
- Se parsean como datos no confiables: límites de tamaño por línea (16 MiB) y errores tolerados.
- La base de datos guarda **números, modelos, ids, títulos y rutas de proyecto**, nunca el contenido de los prompts ni del código.
- Opciones: excluir proyectos, borrar todo el historial ("Purge usage data") y retención en días.

### 2.7 Actualizaciones
- Solo HTTPS. Paquetes firmados por el sistema operativo (Developer ID + notarización en macOS, Authenticode en Windows) y, además, firma propia ed25519 (minisign) del feed de actualización.
- La versión instalada nunca baja (anti-*rollback*) salvo petición explícita.
- Las actualizaciones no matan las sesiones si el servidor de PTYs está separado (ADR 0002); si no, se pide confirmación cuando hay agentes trabajando.

### 2.8 Cadena de suministro
- `cargo-deny` en CI: advisories (RustSec), licencias permitidas, crates prohibidos y orígenes solo de crates.io.
- `cargo-audit` programado a diario; Dependabot o Renovate para dependencias y Actions.
- GitHub Actions fijadas por SHA, `permissions:` mínimos por workflow, secretos de firma solo en el entorno `release` con aprobación manual.
- SBOM (`cargo-cyclonedx`) y atestaciones de procedencia de artefactos (GitHub artifact attestations) en cada release.
- `Cargo.lock` versionado y `--locked` en CI.

### 2.9 Diagnóstico y crashes
- Los crashes se guardan **localmente** (backtrace y versión, sin entorno ni contenido del terminal).
- Enviar un informe es opcional y siempre muestra qué se envía.
- `record_pty` y `--record` avisan de forma visible de que capturan todo.

### 2.10 Skills, plugins y extensiones
- Las skills de terceros para desarrollar el proyecto se revisan antes de instalarlas (ver `docs/skills.md`).
- Plugins futuros (WASM): modelo de capacidades, sin acceso a red ni a archivos salvo permiso explícito.

## 3. Privacidad
- Sin telemetría. Ninguna petición de red salvo comprobar actualizaciones (desactivable) y lo que pida el usuario.
- Todo se guarda en local, en los directorios estándar de cada SO.
- Política de privacidad en una página, en el README y en la web.

## 4. Proceso
- `SECURITY.md` con cómo informar de vulnerabilidades (GitHub Security Advisories privadas).
- Revisión de seguridad (`/security-review`) antes de cada release mayor y en cualquier PR que toque IPC, parsers, hooks o actualizaciones.
- Fuzzing continuo de parsers (ver `docs/testing.md`).
