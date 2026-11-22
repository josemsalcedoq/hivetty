//! `hivetty statusline`: Claude Code's `statusLine` command (F4-32). Prints a short line
//! (`Opus · ctx 8% · 5h 24% · 7d 41%`) and, inside hivetty, forwards the JSON to the app so it
//! can show the context use and the plan limits.
//!
//! Like `hivetty hook`: always exits 0, and gives up on stdin or the app after the client
//! deadline instead of blocking Claude Code.

use std::io::Read;
use std::sync::mpsc;

use hv_ipc::{CLIENT_DEADLINE, Endpoint, HookEvent, Message};

/// Largest stdin payload read.
const MAX_PAYLOAD: u64 = 1 << 20;

/// Runs the statusline and returns the line to print.
pub(crate) fn run() -> String {
    let (tx, rx) = mpsc::channel();
    let _ = std::thread::Builder::new()
        .name("statusline".into())
        .spawn(move || {
            let mut payload = String::new();
            let _ = std::io::stdin()
                .take(MAX_PAYLOAD)
                .read_to_string(&mut payload);
            let line = hv_agents::statusline::parse(&payload)
                .map(|s| s.render())
                .unwrap_or_default();
            // The line goes out first: forwarding must not delay it.
            let _ = tx.send(Some(line));
            if let Some(endpoint) = Endpoint::from_env() {
                let message = Message::Hook(HookEvent {
                    session: std::env::var(hv_ipc::ENV_SESSION).ok(),
                    agent: "claude-code".into(),
                    event: "StatusLine".into(),
                    payload,
                });
                let ok = hv_ipc::send(&endpoint, message).is_ok();
                let _ = tx.send(ok.then_some(String::new()));
            }
            let _ = tx.send(None);
        });
    let deadline = std::time::Instant::now() + CLIENT_DEADLINE;
    let left = || deadline.saturating_duration_since(std::time::Instant::now());
    let line = rx.recv_timeout(left()).ok().flatten().unwrap_or_default();
    // Give the forward the rest of the deadline.
    let _ = rx.recv_timeout(left());
    line
}
