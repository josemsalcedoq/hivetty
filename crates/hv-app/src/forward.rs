//! Forwarding notifications to the phone (F4-33, A-26): ntfy, Telegram or any webhook, opt-in
//! in `[notifications.forward]`. Sent with the system's `curl` (Windows 10+, macOS and most
//! Linux systems ship it) on a background thread, so hivetty needs no TLS stack of its own. The
//! URL can hold a token: it is never logged.

use std::io::Write;
use std::process::{Command, Stdio};

use hv_config::Forward;

/// What to send, as `(content type, body)`, or `None` when forwarding is off or misconfigured.
pub(crate) fn payload(config: &Forward, title: &str, body: &str) -> Option<(&'static str, String)> {
    if !(config.url.starts_with("https://") || config.url.starts_with("http://")) {
        return None;
    }
    let text = format!("{title}\n{body}");
    match config.service.as_str() {
        "ntfy" => Some(("text/plain; charset=utf-8", text)),
        "telegram" if !config.chat_id.is_empty() => Some((
            "application/json",
            serde_json::json!({ "chat_id": config.chat_id, "text": text }).to_string(),
        )),
        "webhook" => Some((
            "application/json",
            serde_json::json!({ "source": "hivetty", "title": title, "body": body }).to_string(),
        )),
        _ => None,
    }
}

/// Sends a notification in the background if forwarding is on.
pub(crate) fn send(config: &Forward, title: &str, body: &str) {
    let Some((content_type, data)) = payload(config, title, body) else {
        return;
    };
    let url = config.url.clone();
    let _ = std::thread::Builder::new()
        .name("hv-forward".into())
        .spawn(move || {
            // Not the URL in the message: it may contain a token.
            if let Err(err) = post(&url, content_type, &data) {
                log::warn!("forwarding the notification failed: {err}");
            }
        });
}

/// POSTs `data` with curl. Blocking.
fn post(url: &str, content_type: &str, data: &str) -> Result<(), String> {
    let mut child = Command::new(hv_pty::resolve_program("curl"))
        .args([
            "-fsS",
            "-m",
            "10",
            "--proto",
            "=https,http",
            "-X",
            "POST",
            "-H",
        ])
        .arg(format!("Content-Type: {content_type}"))
        .args(["--data-binary", "@-", "--"])
        .arg(url)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot run curl: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(data.as_bytes())
            .map_err(|e| e.to_string())?;
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(status.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forward(service: &str, url: &str, chat_id: &str) -> Forward {
        Forward {
            service: service.into(),
            url: url.into(),
            chat_id: chat_id.into(),
        }
    }

    #[test]
    fn curl_posts_the_body() {
        use std::io::{BufRead, BufReader, Read};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/topic", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut head = Vec::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line.trim().is_empty() {
                    break;
                }
                head.push(line.trim().to_owned());
            }
            let length: usize = head
                .iter()
                .find_map(|h| {
                    h.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .map(str::to_owned)
                })
                .unwrap()
                .parse()
                .unwrap();
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let mut stream = stream;
            stream
                .write_all(
                    b"HTTP/1.1 200 OK
Content-Length: 0
Connection: close

",
                )
                .unwrap();
            (head[0].clone(), String::from_utf8(body).unwrap())
        });
        post(
            &url,
            "text/plain; charset=utf-8",
            "Done
Claude Code · hivetty",
        )
        .unwrap();
        let (request, body) = server.join().unwrap();
        assert_eq!(request, "POST /topic HTTP/1.1");
        assert_eq!(
            body,
            "Done
Claude Code · hivetty"
        );
    }

    #[test]
    fn payloads_per_service() {
        let (kind, body) = payload(&forward("ntfy", "https://ntfy.sh/t", ""), "T", "B").unwrap();
        assert_eq!((kind, body.as_str()), ("text/plain; charset=utf-8", "T\nB"));
        let (_, body) = payload(
            &forward("telegram", "https://api.example/bot/send", "42"),
            "T",
            "B",
        )
        .unwrap();
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            (json["chat_id"].as_str(), json["text"].as_str()),
            (Some("42"), Some("T\nB"))
        );
        let (_, body) = payload(&forward("webhook", "http://localhost:9/x", ""), "T", "B").unwrap();
        assert!(body.contains("\"title\":\"T\""));
        // Off, unknown, missing chat id, or not a web URL.
        assert!(payload(&Forward::default(), "T", "B").is_none());
        assert!(payload(&forward("telegram", "https://x", ""), "T", "B").is_none());
        assert!(payload(&forward("ntfy", "file:///etc/passwd", ""), "T", "B").is_none());
        assert!(payload(&forward("ntfy", "-K/etc/x", ""), "T", "B").is_none());
    }
}
