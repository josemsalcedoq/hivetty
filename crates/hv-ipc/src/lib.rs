//! Local IPC between `hivetty hook` (and later `hivetty cli`) and the running app
//! (docs/security.md 2.3, docs/agent-integration.md 2.4).
//!
//! - Transport: a Unix domain socket in a private directory (`0700`, socket `0600`), or a named
//!   pipe that rejects remote clients and only grants access to its owner on Windows.
//! - Every message carries the per-launch 128-bit token from `HIVETTY_IPC_TOKEN`, compared in
//!   constant time. Messages with a wrong token or protocol version are dropped.
//! - Framing: `u32` little-endian length + `postcard` body, at most [`MAX_FRAME`] bytes. The
//!   server answers one status byte so clients can measure the round trip.

use std::io::{self, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use interprocess::local_socket::traits::{ListenerExt as _, Stream as _};
use interprocess::local_socket::{GenericFilePath, ListenerOptions, Stream, ToFsName};
use serde::{Deserialize, Serialize};

/// Version of the wire protocol. Bump on any incompatible change to [`Message`].
pub const PROTOCOL_VERSION: u16 = 1;
/// Environment variable with the endpoint (socket path or pipe name) of the running app.
pub const ENV_ENDPOINT: &str = "HIVETTY_IPC";
/// Environment variable with the hex token.
pub const ENV_TOKEN: &str = "HIVETTY_IPC_TOKEN";
/// Environment variable with the id of the pane/session a process runs in.
pub const ENV_SESSION: &str = "HIVETTY_SESSION_ID";
/// Largest frame accepted (hook payloads are capped at 1 MiB, plus headers).
pub const MAX_FRAME: usize = (1 << 20) + 4096;

/// Status byte the server answers: message accepted.
const ACK_OK: u8 = 1;
/// Status byte the server answers: rejected (token, version or format).
const ACK_REJECTED: u8 = 0;

/// A random 128-bit secret created at every app launch.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token([u8; 16]);

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Token(..)") // never log the secret
    }
}

impl Token {
    /// A new random token from the OS generator.
    pub fn generate() -> io::Result<Self> {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).map_err(|e| io::Error::other(e.to_string()))?;
        Ok(Self(bytes))
    }

    /// Lowercase hex, for the environment.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Parses the hex form. `None` if it is not exactly 32 hex digits.
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim().as_bytes();
        if hex.len() != 32 {
            return None;
        }
        let mut bytes = [0u8; 16];
        for (i, pair) in hex.chunks(2).enumerate() {
            let digit = |c: u8| (c as char).to_digit(16);
            bytes[i] = (digit(pair[0])? * 16 + digit(pair[1])?) as u8;
        }
        Some(Self(bytes))
    }

    /// Constant-time comparison.
    fn matches(&self, other: &Self) -> bool {
        self.0
            .iter()
            .zip(other.0.iter())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
    }
}

/// What a client sends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Message {
    /// An agent hook fired (`hivetty hook --agent <id> --event <name>`).
    Hook(HookEvent),
}

/// One hook invocation, forwarded as is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookEvent {
    /// `HIVETTY_SESSION_ID` of the pane the agent runs in, if set.
    pub session: Option<String>,
    /// Agent profile id (`claude`, `codex`…).
    pub agent: String,
    /// Event name given on the command line (`Stop`, `PreToolUse`…).
    pub event: String,
    /// The hook's stdin, raw (JSON for Claude Code). Parsed by `hv-agents`.
    pub payload: String,
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    version: u16,
    token: Token,
    message: Message,
}

/// Where the app listens and the token clients must present.
#[derive(Debug, Clone)]
pub struct Endpoint {
    /// Socket path (Unix) or pipe name (Windows).
    pub name: String,
    /// The secret.
    pub token: Token,
}

impl Endpoint {
    /// The endpoint advertised in the environment of the current process, if any.
    pub fn from_env() -> Option<Self> {
        let name = std::env::var(ENV_ENDPOINT).ok().filter(|n| !n.is_empty())?;
        let token = Token::from_hex(&std::env::var(ENV_TOKEN).ok()?)?;
        Some(Self { name, token })
    }

    /// The variables to put in the environment of every pane.
    pub fn env(&self) -> [(String, String); 2] {
        [
            (ENV_ENDPOINT.to_owned(), self.name.clone()),
            (ENV_TOKEN.to_owned(), self.token.to_hex()),
        ]
    }
}

/// The listening side, run by the app. Stops when dropped.
#[derive(Debug)]
pub struct Server {
    endpoint: Endpoint,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    /// Starts listening on a fresh endpoint. `on_message` runs on an IPC thread for every
    /// authenticated message.
    pub fn start(on_message: impl Fn(Message) + Send + Sync + 'static) -> io::Result<Self> {
        let token = Token::generate()?;
        let name = endpoint_name()?;
        let listener = listen(&name)?;
        let endpoint = Endpoint { name, token };
        let stop = Arc::new(AtomicBool::new(false));
        let on_message = Arc::new(on_message);
        let thread = {
            let stop = Arc::clone(&stop);
            std::thread::Builder::new()
                .name("hv-ipc".into())
                .spawn(move || {
                    for conn in listener.incoming() {
                        if stop.load(Ordering::Acquire) {
                            break;
                        }
                        let Ok(conn) = conn else {
                            std::thread::sleep(Duration::from_millis(10));
                            continue;
                        };
                        let on_message = Arc::clone(&on_message);
                        // One short-lived thread per client, so a stalled client cannot block
                        // the accept loop.
                        let _ = std::thread::Builder::new()
                            .name("hv-ipc-conn".into())
                            .spawn(move || serve(conn, &token, &*on_message));
                    }
                })?
        };
        Ok(Self {
            endpoint,
            stop,
            thread: Some(thread),
        })
    }

    /// Where clients connect.
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Wake the blocking accept with a throwaway connection.
        let _ = connect(&self.endpoint.name);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        #[cfg(unix)]
        let _ = std::fs::remove_file(&self.endpoint.name);
    }
}

/// Sends one message and waits for the server's answer. Returns `Ok(true)` if it was accepted.
///
/// Callers that must never hang (like `hivetty hook`) run this on a helper thread with their
/// own deadline: a named pipe connect has no timeout.
pub fn send(endpoint: &Endpoint, message: Message) -> io::Result<bool> {
    let body = postcard::to_stdvec(&Envelope {
        version: PROTOCOL_VERSION,
        token: endpoint.token,
        message,
    })
    .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
    if body.len() > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "message too large",
        ));
    }
    let mut stream = connect(&endpoint.name)?;
    let len = u32::try_from(body.len()).map_err(io::Error::other)?;
    let mut frame = Vec::with_capacity(4 + body.len());
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(&body);
    stream.write_all(&frame)?;
    stream.flush()?;
    let mut ack = [0u8; 1];
    stream.read_exact(&mut ack)?;
    Ok(ack[0] == ACK_OK)
}

/// Reads one frame, checks it and hands the message over.
fn serve(mut conn: Stream, token: &Token, on_message: &(dyn Fn(Message) + Send + Sync)) {
    let accepted = match read_frame(&mut conn) {
        Ok(body) => match postcard::from_bytes::<Envelope>(&body) {
            Ok(env) if env.version == PROTOCOL_VERSION && env.token.matches(token) => {
                on_message(env.message);
                true
            }
            Ok(_) => {
                log::warn!("IPC: rejected a message with a wrong token or protocol version");
                false
            }
            Err(err) => {
                log::warn!("IPC: malformed message: {err}");
                false
            }
        },
        Err(err) => {
            log::debug!("IPC: read failed: {err}");
            return;
        }
    };
    let _ = conn.write_all(&[if accepted { ACK_OK } else { ACK_REJECTED }]);
    let _ = conn.flush();
}

fn read_frame(conn: &mut Stream) -> io::Result<Vec<u8>> {
    let mut len = [0u8; 4];
    conn.read_exact(&mut len)?;
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut body = vec![0u8; len];
    conn.read_exact(&mut body)?;
    Ok(body)
}

fn connect(name: &str) -> io::Result<Stream> {
    Stream::connect(name.to_fs_name::<GenericFilePath>()?)
}

/// A unique pipe name for this launch.
#[cfg(windows)]
fn endpoint_name() -> io::Result<String> {
    let mut nonce = [0u8; 8];
    getrandom::fill(&mut nonce).map_err(|e| io::Error::other(e.to_string()))?;
    let nonce: String = nonce.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!(r"\\.\pipe\hivetty-{}-{nonce}", std::process::id()))
}

/// A socket path in a private directory: `$XDG_RUNTIME_DIR/hivetty/` or `$TMPDIR/hivetty-$USER/`.
#[cfg(unix)]
fn endpoint_name() -> io::Result<String> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    let dir = match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(runtime) if !runtime.is_empty() => std::path::PathBuf::from(runtime).join("hivetty"),
        _ => {
            let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
            std::env::temp_dir().join(format!("hivetty-{user}"))
        }
    };
    match std::fs::DirBuilder::new().mode(0o700).create(&dir) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
            let meta = std::fs::symlink_metadata(&dir)?;
            // Another user could have created it first (in a shared /tmp): it must be ours.
            if !meta.is_dir()
                || meta.permissions().mode() & 0o077 != 0
                || meta.uid() != hv_platform::effective_uid()
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!("{} is not a private directory", dir.display()),
                ));
            }
        }
        Err(err) => return Err(err),
    }
    let path = dir.join(format!("ipc-{}.sock", std::process::id()));
    // A stale socket from a crashed process with the same pid.
    let _ = std::fs::remove_file(&path);
    path.into_os_string()
        .into_string()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "non-UTF-8 runtime directory"))
}

#[cfg(windows)]
fn listen(name: &str) -> io::Result<interprocess::local_socket::Listener> {
    use interprocess::os::windows::local_socket::ListenerOptionsExt;
    use interprocess::os::windows::security_descriptor::SecurityDescriptor;
    // Protected DACL with a single ACE: full access for the owner (the current user).
    // interprocess also sets PIPE_REJECT_REMOTE_CLIENTS unless asked not to.
    let sddl = widestring::U16CString::from_str("D:P(A;;GA;;;OW)")
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e.to_string()))?;
    let sd = SecurityDescriptor::deserialize(&sddl)?;
    ListenerOptions::new()
        .name(name.to_fs_name::<GenericFilePath>()?)
        .security_descriptor(sd)
        .create_sync()
}

#[cfg(unix)]
fn listen(name: &str) -> io::Result<interprocess::local_socket::Listener> {
    use std::os::unix::fs::PermissionsExt;
    let listener = ListenerOptions::new()
        .name(name.to_fs_name::<GenericFilePath>()?)
        .create_sync()?;
    // chmod the path (fchmod on a socket is not supported on macOS). The private 0700 directory
    // already keeps other users out; this narrows it further.
    std::fs::set_permissions(name, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Default deadline for clients that must not hang.
pub const CLIENT_DEADLINE: Duration = Duration::from_millis(500);

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Instant;

    fn hook(event: &str) -> Message {
        Message::Hook(HookEvent {
            session: Some("1".into()),
            agent: "claude".into(),
            event: event.into(),
            payload: r#"{"hook_event_name":"Stop"}"#.into(),
        })
    }

    #[test]
    fn token_hex_round_trip() {
        let t = Token::generate().unwrap();
        assert_eq!(Token::from_hex(&t.to_hex()), Some(t));
        assert_eq!(Token::from_hex("xyz"), None);
        assert_eq!(Token::from_hex(&"g".repeat(32)), None);
        assert_eq!(format!("{t:?}"), "Token(..)");
    }

    #[test]
    fn messages_arrive_and_wrong_tokens_are_rejected() {
        let (tx, rx) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        let server = Server::start(move |m| {
            let _ = tx.lock().unwrap().send(m);
        })
        .unwrap();
        let endpoint = server.endpoint().clone();

        let started = Instant::now();
        assert!(send(&endpoint, hook("Stop")).unwrap());
        let got = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let elapsed = started.elapsed();
        assert_eq!(got, hook("Stop"));
        assert!(
            elapsed < Duration::from_millis(300),
            "round trip {elapsed:?}"
        );

        let forged = Endpoint {
            name: endpoint.name.clone(),
            token: Token::generate().unwrap(),
        };
        assert!(!send(&forged, hook("Forged")).unwrap());
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
    }

    #[test]
    fn oversized_frames_are_refused_by_the_server() {
        let server = Server::start(|_| {}).unwrap();
        let mut stream = connect(&server.endpoint().name).unwrap();
        let len = u32::try_from(MAX_FRAME + 1).unwrap();
        stream.write_all(&len.to_le_bytes()).unwrap();
        stream.flush().unwrap();
        let mut ack = [0u8; 1];
        // The server closes the connection without an answer.
        assert!(stream.read_exact(&mut ack).is_err());
    }
}
