//! SP-4: `hivetty hook` end to end — process start, stdin, IPC, delivery — and its contract
//! (exit code 0, under a second, nothing on stdout).

use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_hivetty");

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/../../tests/fixtures/hooks/claude/2.1.330/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// Runs the binary once so the first timed run does not pay for a cold start (antivirus scans
/// of a freshly built executable take ~0.5 s on Windows; real hooks run a warm binary).
fn warm_up() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = Command::new(BIN).arg("--version").output();
    });
}

fn hook_command() -> Command {
    let mut cmd = Command::new(BIN);
    cmd.args(["hook", "--agent", "claude", "--event", "PreToolUse"])
        .env_remove(hv_ipc::ENV_ENDPOINT)
        .env_remove(hv_ipc::ENV_TOKEN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

#[test]
fn hook_reaches_the_app_quickly() {
    warm_up();
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let server = hv_ipc::Server::start(move |m| {
        let _ = tx.lock().unwrap().send((m, Instant::now()));
    })
    .unwrap();
    let payload = fixture("PreToolUse");

    let mut samples = Vec::new();
    for _ in 0..5 {
        let started = Instant::now();
        let mut child = hook_command()
            .envs(server.endpoint().env())
            .env(hv_ipc::ENV_SESSION, "1")
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(payload.as_bytes())
            .unwrap();
        let (message, arrived) = rx.recv_timeout(Duration::from_secs(2)).expect("delivered");
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success());
        assert!(out.stdout.is_empty(), "hook printed to stdout");
        let hv_ipc::Message::Hook(hook) = message;
        assert_eq!(hook.session.as_deref(), Some("1"));
        assert_eq!(hook.event, "PreToolUse");
        assert_eq!(hook.payload, payload);
        samples.push(arrived - started);
    }
    samples.sort();
    let median = samples[samples.len() / 2];
    println!("hook latency (process start -> delivered): median {median:?}, all {samples:?}");
    // N-08: hook to state change within 300 ms.
    assert!(median < Duration::from_millis(300), "median {median:?}");
}

#[test]
fn hook_without_hivetty_is_a_silent_no_op() {
    warm_up();
    let started = Instant::now();
    let out = hook_command().spawn().unwrap().wait_with_output().unwrap();
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn hook_never_hangs_on_an_open_stdin_or_a_dead_endpoint() {
    warm_up();
    let endpoint = hv_ipc::Endpoint {
        name: if cfg!(windows) {
            r"\\.\pipe\hivetty-test-nobody-listens".into()
        } else {
            std::env::temp_dir()
                .join("hivetty-test-nobody-listens.sock")
                .to_string_lossy()
                .into_owned()
        },
        token: hv_ipc::Token::generate().unwrap(),
    };
    // stdin stays open (we never close the pipe) and nothing listens.
    let started = Instant::now();
    let mut child = hook_command().envs(endpoint.env()).spawn().unwrap();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "hook still running after 2 s"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(status.success());
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
}
