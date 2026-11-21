//! Session recording in asciicast v2 (task F1-27), compatible with asciinema.
//!
//! Records **everything** the program prints, including anything secret shown on screen, so it
//! is only enabled explicitly (`--record`) and says so in the log (docs/security.md 2.9).

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result};

/// Writes PTY output with timestamps.
#[derive(Debug)]
pub struct Recorder {
    out: BufWriter<File>,
    start: Instant,
    /// Bytes of a UTF-8 character split across two PTY reads.
    pending: Vec<u8>,
}

impl Recorder {
    /// Creates `path` and writes the asciicast header.
    pub fn create(path: &Path, cols: u16, rows: u16) -> Result<Self> {
        let file =
            File::create(path).with_context(|| format!("cannot create {}", path.display()))?;
        let mut out = BufWriter::new(file);
        let header = serde_json::json!({
            "version": 2,
            "width": cols,
            "height": rows,
            "env": { "TERM": "xterm-256color" },
        });
        writeln!(out, "{header}")?;
        log::warn!("recording all terminal output to {}", path.display());
        Ok(Self {
            out,
            start: Instant::now(),
            pending: Vec::new(),
        })
    }

    /// Appends an output event.
    pub fn output(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
        let valid = match std::str::from_utf8(&self.pending) {
            Ok(_) => self.pending.len(),
            // An incomplete character at the end: keep it for the next read.
            Err(e) if e.error_len().is_none() => e.valid_up_to(),
            Err(_) => self.pending.len(),
        };
        let chunk: Vec<u8> = self.pending.drain(..valid).collect();
        if !chunk.is_empty() {
            self.event("o", &String::from_utf8_lossy(&chunk));
        }
    }

    /// Appends a resize event.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.event("r", &format!("{cols}x{rows}"));
    }

    fn event(&mut self, kind: &str, data: &str) {
        let t = self.start.elapsed().as_secs_f64();
        let line = serde_json::json!([(t * 1e6).round() / 1e6, kind, data]);
        let _ = writeln!(self.out, "{line}").and_then(|()| self.out.flush());
    }
}

/// Reads an asciicast v2 file back: the output events' data, in order.
pub fn read_output(text: &str) -> Vec<String> {
    text.lines()
        .skip(1)
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|v| v.get(1).and_then(|k| k.as_str()) == Some("o"))
        .filter_map(|v| v.get(2).and_then(|d| d.as_str()).map(str::to_owned))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_valid_asciicast_file() {
        let path = std::env::temp_dir().join(format!("hivetty-rec-{}.cast", std::process::id()));
        {
            let mut rec = Recorder::create(&path, 80, 24).unwrap();
            rec.output(b"hello \x1b[31mred\x1b[0m\r\n");
            rec.resize(100, 30);
            let bytes = "ñ 日本".as_bytes();
            rec.output(&bytes[..4]); // splits 日 in the middle
            rec.output(&bytes[4..]);
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let header: serde_json::Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
        assert_eq!(
            (header["version"].as_u64(), header["width"].as_u64()),
            (Some(2), Some(80))
        );
        assert_eq!(
            read_output(&text),
            ["hello \x1b[31mred\x1b[0m\r\n", "ñ ", "日本"]
        );
        let _ = std::fs::remove_file(path);
    }
}
