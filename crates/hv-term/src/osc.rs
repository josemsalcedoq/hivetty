//! Interceptor for the escape sequences `alacritty_terminal` ignores.
//!
//! `alacritty_terminal` only handles OSC 0/1/2/4/8/10/11/12/50/52/104/110/111/112 and drops
//! everything else. hivetty needs OSC 7 (cwd), 9 (notifications and 9;4 progress),
//! 133 (prompt marks), 777 (notifications) and 7501 (program status for coding agents),
//! so every byte is run through this second parser before it reaches the emulator.
//! See docs/vt-compliance.md section 1.

use std::collections::BTreeMap;

use alacritty_terminal::vte::{Parser, Perform};
use base64::Engine as _;

/// A sequence of interest found in the PTY output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TermEvent {
    /// OSC 7: the shell's working directory.
    Cwd(String),
    /// OSC 133: a semantic prompt mark (`A` prompt start, `B` command start,
    /// `C` output start, `D` command finished, with its exit code).
    PromptMark {
        /// The mark letter.
        kind: char,
        /// Exit code, for `D` marks.
        exit_code: Option<i32>,
    },
    /// OSC 9 / OSC 777 desktop notification.
    Notify {
        /// Title, when the sequence provides one (OSC 777).
        title: Option<String>,
        /// Notification body.
        body: String,
    },
    /// OSC 9;4 progress report (ConEmu / Windows Terminal convention).
    Progress {
        /// 0 = remove, 1 = normal, 2 = error, 3 = indeterminate, 4 = paused.
        state: u8,
        /// Percentage, 0-100.
        value: u8,
    },
    /// OSC 7501 program status (draft protocol for coding agents).
    ProgramStatus(ProgramStatus),
}

/// Parsed OSC 7501 payload, e.g. `state=blocked:kind=permission:msg=<base64>`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProgramStatus {
    /// `idle`, `working`, `blocked`, `done`, `error`, `clear` or `?` (query).
    pub state: String,
    /// For `blocked`: `permission`, `question` or `auth`.
    pub kind: Option<String>,
    /// Decoded human-readable message.
    pub message: Option<String>,
    /// Progress 0-100 for `working`.
    pub progress: Option<u8>,
    /// Status id, when a program reports several statuses in one pane.
    pub id: Option<String>,
    /// All raw key/value pairs, for fields this version does not know yet.
    pub fields: BTreeMap<String, String>,
}

/// Largest OSC payload kept, to bound memory use (docs/security.md 2.1).
const MAX_OSC_BYTES: usize = 64 * 1024;

/// Scans PTY output for [`TermEvent`]s without modifying it.
#[derive(Default)]
pub struct Interceptor {
    parser: Parser,
    collector: Collector,
}

impl std::fmt::Debug for Interceptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Interceptor").finish_non_exhaustive()
    }
}

impl Interceptor {
    /// Creates an interceptor.
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds PTY output and returns the events it completed.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<TermEvent> {
        self.parser.advance(&mut self.collector, bytes);
        std::mem::take(&mut self.collector.events)
    }
}

#[derive(Default)]
struct Collector {
    events: Vec<TermEvent>,
}

impl Perform for Collector {
    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if params.iter().map(|p| p.len()).sum::<usize>() > MAX_OSC_BYTES {
            return;
        }
        if let Some(event) = parse_osc(params) {
            self.events.push(event);
        }
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Joins `params[from..]` back with `;` (OSC payloads may legitimately contain semicolons).
fn rest(params: &[&[u8]], from: usize) -> String {
    params
        .get(from..)
        .unwrap_or_default()
        .iter()
        .map(|p| text(p))
        .collect::<Vec<_>>()
        .join(";")
}

fn parse_osc(params: &[&[u8]]) -> Option<TermEvent> {
    match *params.first()? {
        b"7" => parse_cwd(&rest(params, 1)).map(TermEvent::Cwd),
        b"9" => parse_osc9(params),
        b"133" => {
            let mark = params.get(1)?;
            let kind = char::from(*mark.first()?);
            let exit_code = if kind == 'D' {
                params.get(2).and_then(|p| text(p).parse().ok())
            } else {
                None
            };
            Some(TermEvent::PromptMark { kind, exit_code })
        }
        b"777" if params.get(1) == Some(&&b"notify"[..]) => Some(TermEvent::Notify {
            title: params.get(2).map(|p| text(p)),
            body: rest(params, 3),
        }),
        b"7501" => Some(TermEvent::ProgramStatus(parse_program_status(&rest(
            params, 1,
        )))),
        _ => None,
    }
}

fn parse_osc9(params: &[&[u8]]) -> Option<TermEvent> {
    if params.get(1) == Some(&&b"4"[..]) {
        let state = params
            .get(2)
            .and_then(|p| text(p).parse().ok())
            .unwrap_or(0);
        let value: u8 = params
            .get(3)
            .and_then(|p| text(p).parse().ok())
            .unwrap_or(0);
        return Some(TermEvent::Progress {
            state,
            value: value.min(100),
        });
    }
    // OSC 9;9 is ConEmu's "current directory"; other numeric sub-commands are ignored.
    if params.get(1) == Some(&&b"9"[..]) {
        return Some(TermEvent::Cwd(rest(params, 2).trim_matches('"').to_owned()));
    }
    let body = rest(params, 1);
    if body.is_empty() || body.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(TermEvent::Notify { title: None, body })
}

/// Parses `file://host/path` (percent-encoded) into a local path.
fn parse_cwd(uri: &str) -> Option<String> {
    let after_scheme = uri.strip_prefix("file://")?;
    let path = &after_scheme[after_scheme.find('/')?..];
    let decoded = percent_decode(path);
    // Windows paths arrive as /C:/Users/...
    let bytes = decoded.as_bytes();
    if bytes.len() >= 3 && bytes[0] == b'/' && bytes[2] == b':' && bytes[1].is_ascii_alphabetic() {
        return Some(decoded[1..].to_owned());
    }
    Some(decoded)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(v) = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn parse_program_status(payload: &str) -> ProgramStatus {
    let mut status = ProgramStatus::default();
    if payload.trim() == "?" {
        status.state = "?".to_owned();
        return status;
    }
    for pair in payload.split(':') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        status.fields.insert(key.to_owned(), value.to_owned());
    }
    status.state = status.fields.get("state").cloned().unwrap_or_default();
    status.kind = status.fields.get("kind").cloned();
    status.id = status.fields.get("id").cloned();
    status.progress = status
        .fields
        .get("progress")
        .and_then(|p| p.parse::<u8>().ok())
        .map(|p| p.min(100));
    status.message = status.fields.get("msg").map(|m| {
        base64::engine::general_purpose::STANDARD
            .decode(m)
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .unwrap_or_else(|| m.clone())
    });
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(bytes: &[u8]) -> Vec<TermEvent> {
        Interceptor::new().feed(bytes)
    }

    #[test]
    fn osc7_cwd_unix_and_windows() {
        assert_eq!(
            feed(b"\x1b]7;file://host/home/me/My%20Dir\x07"),
            vec![TermEvent::Cwd("/home/me/My Dir".into())]
        );
        assert_eq!(
            feed(b"\x1b]7;file://pc/C:/Users/Josh\x1b\\"),
            vec![TermEvent::Cwd("C:/Users/Josh".into())]
        );
    }

    #[test]
    fn osc133_marks_with_exit_code() {
        assert_eq!(
            feed(b"\x1b]133;A\x07$ ls\x1b]133;D;2\x07"),
            vec![
                TermEvent::PromptMark {
                    kind: 'A',
                    exit_code: None
                },
                TermEvent::PromptMark {
                    kind: 'D',
                    exit_code: Some(2)
                },
            ]
        );
    }

    #[test]
    fn osc9_notification_and_progress() {
        assert_eq!(
            feed(b"\x1b]9;Build finished\x07"),
            vec![TermEvent::Notify {
                title: None,
                body: "Build finished".into()
            }]
        );
        assert_eq!(
            feed(b"\x1b]9;4;1;42\x07"),
            vec![TermEvent::Progress {
                state: 1,
                value: 42
            }]
        );
        assert_eq!(
            feed(b"\x1b]9;4;1;250\x07"),
            vec![TermEvent::Progress {
                state: 1,
                value: 100
            }]
        );
    }

    #[test]
    fn osc777_notification_keeps_semicolons_in_body() {
        assert_eq!(
            feed(b"\x1b]777;notify;Claude Code;Done; 3 files\x07"),
            vec![TermEvent::Notify {
                title: Some("Claude Code".into()),
                body: "Done; 3 files".into()
            }]
        );
    }

    #[test]
    fn osc7501_blocked_with_base64_message() {
        // "Needs permission" in base64.
        let events =
            feed(b"\x1b]7501;state=blocked:kind=permission:msg=TmVlZHMgcGVybWlzc2lvbg==\x1b\\");
        let [TermEvent::ProgramStatus(status)] = events.as_slice() else {
            panic!("{events:?}")
        };
        assert_eq!(status.state, "blocked");
        assert_eq!(status.kind.as_deref(), Some("permission"));
        assert_eq!(status.message.as_deref(), Some("Needs permission"));
    }

    #[test]
    fn osc7501_working_progress_and_query() {
        let events = feed(b"\x1b]7501;state=working:progress=40:id=build\x07\x1b]7501;?\x07");
        assert_eq!(events.len(), 2);
        let TermEvent::ProgramStatus(working) = &events[0] else {
            panic!()
        };
        assert_eq!(
            (
                working.state.as_str(),
                working.progress,
                working.id.as_deref()
            ),
            ("working", Some(40), Some("build"))
        );
        let TermEvent::ProgramStatus(query) = &events[1] else {
            panic!()
        };
        assert_eq!(query.state, "?");
    }

    #[test]
    fn sequences_split_across_reads() {
        let mut i = Interceptor::new();
        assert!(i.feed(b"text \x1b]9;Hel").is_empty());
        assert_eq!(
            i.feed(b"lo\x07 more"),
            vec![TermEvent::Notify {
                title: None,
                body: "Hello".into()
            }]
        );
    }

    #[test]
    fn ignores_handled_and_unknown_oscs() {
        assert!(feed(b"\x1b]0;title\x07\x1b]8;;https://x\x07\x1b]1234;x\x07plain").is_empty());
    }
}
