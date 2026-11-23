//! The local OpenTelemetry receiver (F5-15, opt-in with `[usage] otlp`): a tiny HTTP endpoint
//! on 127.0.0.1 that takes OTLP/HTTP JSON metrics from the sessions hivetty starts and keeps
//! lines of code, commits and pull requests (`hv_usage::otel`). A random token in the path
//! keeps other local programs from adding numbers. The panes get the endpoint through
//! signal-specific `OTEL_EXPORTER_OTLP_METRICS_*` variables, so a user's own logs or traces
//! setup is left alone.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

/// Largest request body read.
const MAX_BODY: usize = 1 << 20;
/// Largest header block read.
const MAX_HEADERS: usize = 16 * 1024;

/// A running receiver.
#[derive(Debug)]
pub(crate) struct Receiver {
    /// The full metrics URL, token included.
    endpoint: String,
}

impl Receiver {
    /// Starts listening on a free port; points are appended to `store`.
    pub(crate) fn start(store: PathBuf) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let token = hv_ipc::Token::generate()?.to_hex();
        let path = format!("/{token}/v1/metrics");
        let endpoint = format!("http://127.0.0.1:{port}{path}");
        std::thread::Builder::new()
            .name("hv-otel".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    if let Err(err) = serve(stream, &path, &store) {
                        log::debug!("otel request: {err}");
                    }
                }
            })?;
        Ok(Self { endpoint })
    }

    /// The variables that point a Claude Code session at this receiver.
    pub(crate) fn env(&self) -> Vec<(String, String)> {
        [
            ("CLAUDE_CODE_ENABLE_TELEMETRY", "1"),
            ("OTEL_METRICS_EXPORTER", "otlp"),
            ("OTEL_EXPORTER_OTLP_METRICS_PROTOCOL", "http/json"),
            (
                "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
                self.endpoint.as_str(),
            ),
            ("OTEL_EXPORTER_OTLP_METRICS_TEMPORALITY_PREFERENCE", "delta"),
            ("OTEL_METRIC_EXPORT_INTERVAL", "30000"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
    }
}

/// Whether the user already sends their agents' metrics somewhere (then hivetty does not
/// redirect them).
pub(crate) fn user_has_otel() -> bool {
    [
        "OTEL_EXPORTER_OTLP_ENDPOINT",
        "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT",
    ]
    .iter()
    .any(|k| std::env::var_os(k).is_some_and(|v| !v.is_empty()))
}

/// One request: `POST <path>` with a JSON body. Always answers, then closes.
fn serve(stream: TcpStream, path: &str, store: &std::path::Path) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request = String::new();
    reader.read_line(&mut request)?;
    let mut length = 0usize;
    let mut read = request.len();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        read += line.len();
        if read > MAX_HEADERS {
            return respond(stream, "431 Request Header Fields Too Large");
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap_or(usize::MAX);
        }
    }
    let mut parts = request.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if method != "POST" || target != path {
        return respond(stream, "404 Not Found");
    }
    if length > MAX_BODY {
        return respond(stream, "413 Payload Too Large");
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    let points = hv_usage::otel::parse_metrics(&String::from_utf8_lossy(&body));
    if let Err(err) = hv_usage::otel::append(store, &points) {
        log::warn!("cannot save agent metrics: {err}");
    }
    respond(stream, "200 OK")
}

fn respond(mut stream: TcpStream, status: &str) -> std::io::Result<()> {
    let body = "{}";
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn post(endpoint: &str, path: &str, body: &str) -> String {
        let addr = endpoint
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap()
            .to_owned();
        let mut stream = TcpStream::connect(addr).unwrap();
        write!(
            stream,
            "POST {path} HTTP/1.1\r\nHost: x\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).unwrap();
        answer
    }

    #[test]
    fn metrics_with_the_token_are_kept_others_refused() {
        let store = std::env::temp_dir().join(format!("hv-otel-recv-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&store);
        let receiver = Receiver::start(store.clone()).unwrap();
        let path = receiver
            .endpoint
            .splitn(4, '/')
            .nth(3)
            .map(|p| format!("/{p}"))
            .unwrap();
        let body =
            include_str!("../../hv-usage/tests/fixtures/otel/claude-code/2.1.330/metrics.jsonl")
                .lines()
                .nth(1)
                .unwrap();
        assert!(post(&receiver.endpoint, &path, body).starts_with("HTTP/1.1 200"));
        assert!(post(&receiver.endpoint, "/wrong/v1/metrics", body).starts_with("HTTP/1.1 404"));
        let points = hv_usage::otel::load(&store);
        assert_eq!(points.len(), 1, "only the request with the token");
        assert!(receiver.env().iter().any(|(k, v)| k == "OTEL_EXPORTER_OTLP_METRICS_ENDPOINT" && *v == receiver.endpoint));
        let _ = std::fs::remove_file(&store);
    }
}
