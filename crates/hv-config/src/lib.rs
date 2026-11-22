//! Configuration: `config.toml` in the platform config directory, with hot reload.
//!
//! A broken config never stops hivetty from starting: the defaults are used and the error is
//! reported (docs/config.md). Only the keys implemented so far are listed here; the full
//! schema in docs/config.md is filled in during phase F6.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;

/// The whole configuration.
#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Terminal font.
    pub font: Font,
    /// Shell started in new terminals.
    pub shell: Shell,
    /// Terminal behaviour.
    pub terminal: Terminal,
    /// Shortcut overrides, applied on top of the defaults (docs/keybindings.md).
    pub keybindings: Vec<KeyBinding>,
}

/// One `[[keybindings]]` entry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyBinding {
    /// Chord, e.g. `"primary+shift+t"` (primary = Cmd on macOS, Ctrl elsewhere).
    pub key: String,
    /// What it does.
    pub action: BindingAction,
    /// Optional condition, e.g. `"os == 'windows'"` or `"search_open"`.
    #[serde(default)]
    pub when: Option<String>,
}

/// The action of a key binding: a named action or text to send to the program.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum BindingAction {
    /// `action = "split_right"` (or `"none"` to unbind).
    Named(String),
    /// `action = { send_text = "\u001b\r" }` (ESC + CR).
    SendText {
        /// Bytes to write to the focused terminal.
        send_text: String,
    },
}

/// `[font]`
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Font {
    /// Font family; `None` picks the first installed monospace font from a built-in list.
    pub family: Option<String>,
    /// Size in points (before DPI scaling).
    pub size: f32,
}

impl Default for Font {
    fn default() -> Self {
        Self {
            family: None,
            size: 14.0,
        }
    }
}

/// `[shell]`
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Shell {
    /// Program to run; `None` uses the platform default (docs/platforms.md).
    pub program: Option<String>,
    /// Arguments for `program`.
    pub args: Vec<String>,
}

/// `[terminal]`
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Terminal {
    /// Lines of scrollback history.
    pub scrollback_lines: usize,
}

impl Default for Terminal {
    fn default() -> Self {
        Self {
            scrollback_lines: 10_000,
        }
    }
}

/// Where the config file lives on this platform.
///
/// - Windows: `%APPDATA%\hivetty\config.toml`
/// - macOS: `~/Library/Application Support/hivetty/config.toml`
/// - Linux: `$XDG_CONFIG_HOME/hivetty/config.toml` (default `~/.config`)
///
/// `HIVETTY_CONFIG` overrides it.
pub fn default_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("HIVETTY_CONFIG") {
        return Some(PathBuf::from(path));
    }
    let base = if cfg!(windows) {
        PathBuf::from(std::env::var_os("APPDATA")?)
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support")
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?
    };
    Some(base.join("hivetty").join("config.toml"))
}

/// Parses config text. On error returns the defaults plus a readable message.
pub fn parse(text: &str) -> (Config, Option<String>) {
    match toml::from_str::<Config>(text) {
        Ok(mut config) => {
            config.font.size = config.font.size.clamp(6.0, 72.0);
            config.terminal.scrollback_lines = config.terminal.scrollback_lines.min(1_000_000);
            (config, None)
        }
        Err(err) => (
            Config::default(),
            Some(format!("invalid config, using defaults: {err}")),
        ),
    }
}

/// Loads the config file. A missing file is not an error.
pub fn load(path: &Path) -> (Config, Option<String>) {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => (Config::default(), None),
        Err(err) => (
            Config::default(),
            Some(format!("cannot read {}: {err}", path.display())),
        ),
    }
}

/// Watches the config file and calls `on_change` (debounced) when it is modified.
/// Keep the returned watcher alive for as long as reloading should work.
pub fn watch(path: &Path, on_change: impl Fn() + Send + 'static) -> Option<RecommendedWatcher> {
    // Watch the directory: editors often replace the file instead of writing it in place.
    let dir = path.parent()?.to_path_buf();
    std::fs::create_dir_all(&dir).ok()?;
    let file_name = path.file_name()?.to_owned();
    let (tx, rx) = channel::<()>();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Ok(event) = event
            && event
                .paths
                .iter()
                .any(|p| p.file_name() == Some(&file_name))
        {
            let _ = tx.send(());
        }
    })
    .ok()?;
    watcher.watch(&dir, RecursiveMode::NonRecursive).ok()?;
    std::thread::Builder::new()
        .name("hivetty-config-watch".into())
        .spawn(move || debounce(&rx, &on_change))
        .ok()?;
    Some(watcher)
}

fn debounce(rx: &Receiver<()>, on_change: &impl Fn()) {
    while rx.recv().is_ok() {
        // Collapse the burst of events a single save produces.
        while rx.recv_timeout(Duration::from_millis(150)).is_ok() {}
        on_change();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_gives_defaults() {
        assert_eq!(parse(""), (Config::default(), None));
    }

    #[test]
    fn values_are_read_and_clamped() {
        let (config, err) = parse(
            r#"
            [font]
            family = "JetBrains Mono"
            size = 200

            [shell]
            program = "pwsh"
            args = ["-NoLogo"]

            [terminal]
            scrollback_lines = 5000
            "#,
        );
        assert!(err.is_none());
        assert_eq!(config.font.family.as_deref(), Some("JetBrains Mono"));
        assert_eq!(config.font.size, 72.0);
        assert_eq!(config.shell.program.as_deref(), Some("pwsh"));
        assert_eq!(config.shell.args, ["-NoLogo"]);
        assert_eq!(config.terminal.scrollback_lines, 5000);
    }

    #[test]
    fn errors_fall_back_to_defaults_with_a_message() {
        let (config, err) = parse("[font]\nsize = \"big\"\n");
        assert_eq!(config, Config::default());
        assert!(err.unwrap().contains("invalid config"));
        let (_, err) = parse("[nope]\nx = 1\n");
        assert!(err.is_some(), "unknown sections are reported");
    }

    #[test]
    fn missing_file_is_fine() {
        let (config, err) = load(Path::new("definitely/not/here/config.toml"));
        assert_eq!((config, err), (Config::default(), None));
    }
}
