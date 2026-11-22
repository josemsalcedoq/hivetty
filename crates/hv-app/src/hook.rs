//! `hivetty hook --agent <id> --event <name>`: forwards an agent hook to the running app
//! (docs/agent-integration.md 2.4).
//!
//! Contract: always exits with code 0 in under a second and never prints to stdout, whatever
//! happens (no app running, bad arguments, stdin never closed, IPC stalled).

use std::io::Read;
use std::sync::mpsc;

use hv_ipc::{CLIENT_DEADLINE, Endpoint, HookEvent, Message};

/// Largest stdin payload forwarded.
const MAX_PAYLOAD: u64 = 1 << 20;

/// `--agent` and `--event` from the arguments after `hook`. Unknown flags are ignored.
pub(crate) fn parse(args: &[String]) -> (String, String) {
    let mut agent = "unknown".to_owned();
    let mut event = "unknown".to_owned();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--agent" => agent = it.next().cloned().unwrap_or(agent),
            "--event" => event = it.next().cloned().unwrap_or(event),
            _ => {}
        }
    }
    (agent, event)
}

/// Runs the hook. Returns when the event was delivered or the deadline passed.
pub(crate) fn run(args: &[String]) {
    let Some(endpoint) = Endpoint::from_env() else {
        return; // not running inside hivetty
    };
    let (agent, event) = parse(args);
    let session = std::env::var(hv_ipc::ENV_SESSION).ok();
    let (tx, rx) = mpsc::channel();
    // Everything that can block (stdin, connect, write) runs on a helper thread; the main thread
    // only waits for the deadline. Returning from main ends the process even if it is stuck.
    let _ = std::thread::Builder::new()
        .name("hook".into())
        .spawn(move || {
            let mut payload = String::new();
            let _ = std::io::stdin()
                .take(MAX_PAYLOAD)
                .read_to_string(&mut payload);
            let message = Message::Hook(HookEvent {
                session,
                agent,
                event,
                payload,
            });
            let _ = tx.send(hv_ipc::send(&endpoint, message).is_ok());
        });
    let _ = rx.recv_timeout(CLIENT_DEADLINE);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_default_and_ignore_unknown_flags() {
        let args = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
        assert_eq!(
            parse(&args("--agent claude --event Stop")),
            ("claude".into(), "Stop".into())
        );
        assert_eq!(
            parse(&args("--weird --event")),
            ("unknown".into(), "unknown".into())
        );
    }
}
