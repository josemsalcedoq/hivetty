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

pub mod theme_import;

/// The JSON schema of `config.toml` (F6-02), for editors that validate TOML against one
/// (Taplo / Even Better TOML: `#:schema <url>` at the top of the file).
pub fn schema_json() -> String {
    let schema = schemars::schema_for!(Config);
    serde_json::to_string_pretty(&schema).unwrap_or_default()
        + "
"
}

/// The whole configuration.
#[derive(Debug, Clone, PartialEq, Deserialize, Default, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Terminal font.
    pub font: Font,
    /// Shell started in new terminals.
    pub shell: Shell,
    /// Terminal behaviour.
    pub terminal: Terminal,
    /// Window behaviour.
    pub window: WindowConfig,
    /// Desktop notifications.
    pub notifications: Notifications,
    /// Agent behaviour.
    pub agents: Agents,
    /// Colors.
    pub theme: Theme,
    /// Shortcut overrides, applied on top of the defaults (docs/keybindings.md).
    pub keybindings: Vec<KeyBinding>,
    /// The editor for "Open in editor".
    pub editor: Editor,
    /// Looking for new versions.
    pub updates: Updates,
}

/// One `[[keybindings]]` entry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Deserialize, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Shell {
    /// Program to run; `None` uses the platform default (docs/platforms.md).
    pub program: Option<String>,
    /// Arguments for `program`.
    pub args: Vec<String>,
    /// Load hivetty's shell integration (prompt marks, current directory) into zsh, bash,
    /// fish and PowerShell without editing their startup files.
    pub integration: bool,
    /// Show project chips (technology, git branch, versions) above the prompt. Needs the
    /// integration; the shell then leaves a blank line above each prompt for them.
    pub chips: bool,
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            program: None,
            args: Vec::new(),
            integration: true,
            chips: true,
        }
    }
}

/// `[terminal]`
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
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

/// `[window]`
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct WindowConfig {
    /// Reopen the window with the size and position it had.
    pub remember_geometry: bool,
    /// Reopen the tabs, splits and directories of the last session.
    pub restore_layout: bool,
    /// Title bar: `"auto"` (hivetty's own on Windows, transparent on macOS, the system's on
    /// Linux), `"full"` (always the system's) or `"none"` (always hivetty's).
    pub decorations: String,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            remember_geometry: true,
            restore_layout: true,
            decorations: "auto".to_owned(),
        }
    }
}

/// `[theme]`
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Theme {
    /// `"system"` follows the OS light/dark setting; `"hivetty-dark"` or `"hivetty-light"`
    /// force one.
    pub name: String,
    /// White on black with saturated colors, for low vision (wins over `colors`).
    pub high_contrast: bool,
    /// A custom terminal palette (`hivetty theme import` writes one from iTerm2 or Alacritty).
    pub colors: Option<ThemeColors>,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            name: "system".to_owned(),
            high_contrast: false,
            colors: None,
        }
    }
}

/// `[theme.colors]`: `#rrggbb` strings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThemeColors {
    pub foreground: String,
    pub background: String,
    pub cursor: Option<String>,
    pub selection: Option<String>,
    /// The 16 ANSI colors: black, red, green, yellow, blue, magenta, cyan, white, then the
    /// bright ones.
    pub ansi: Vec<String>,
}

/// A terminal palette as `0xRRGGBB` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub foreground: u32,
    pub background: u32,
    pub cursor: u32,
    pub selection: u32,
    pub ansi: [u32; 16],
}

impl Palette {
    /// The high-contrast palette (F6-01).
    pub const HIGH_CONTRAST: Self = Self {
        foreground: 0xFFFFFF,
        background: 0x000000,
        cursor: 0xFFFF00,
        selection: 0x0050C8,
        ansi: [
            0x000000, 0xFF5555, 0x55FF55, 0xFFFF55, 0x7799FF, 0xFF77FF, 0x55FFFF, 0xFFFFFF, //
            0xBBBBBB, 0xFF8888, 0x99FF99, 0xFFFF99, 0xAABBFF, 0xFFAAFF, 0x99FFFF, 0xFFFFFF,
        ],
    };

    /// Whether the background is light (the chrome then uses its light theme).
    pub fn is_light(&self) -> bool {
        let [_, r, g, b] = self.background.to_be_bytes();
        // Relative luminance, roughly (sRGB weights, no gamma).
        0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b) > 128.0
    }
}

/// `#rrggbb` (or `rrggbb`) to `0xRRGGBB`.
fn parse_hex(text: &str) -> Option<u32> {
    let digits = text.trim().strip_prefix('#').unwrap_or(text.trim());
    (digits.len() == 6)
        .then(|| u32::from_str_radix(digits, 16).ok())
        .flatten()
}

impl ThemeColors {
    /// Checks and converts the colors.
    pub fn palette(&self) -> Result<Palette, String> {
        let color = |name: &str, text: &str| {
            parse_hex(text).ok_or_else(|| format!("theme.colors.{name}: '{text}' is not #rrggbb"))
        };
        let foreground = color("foreground", &self.foreground)?;
        let background = color("background", &self.background)?;
        if self.ansi.len() != 16 {
            return Err(format!(
                "theme.colors.ansi needs 16 colors, it has {}",
                self.ansi.len()
            ));
        }
        let mut ansi = [0; 16];
        for (slot, text) in ansi.iter_mut().zip(&self.ansi) {
            *slot = color("ansi", text)?;
        }
        Ok(Palette {
            foreground,
            background,
            cursor: self
                .cursor
                .as_deref()
                .map_or(Ok(foreground), |c| color("cursor", c))?,
            selection: self
                .selection
                .as_deref()
                .map_or(Ok(ansi[4]), |c| color("selection", c))?,
            ansi,
        })
    }
}

impl Theme {
    /// The terminal palette that replaces hivetty's dark and light ones, if any.
    pub fn palette(&self) -> Option<Palette> {
        if self.high_contrast {
            return Some(Palette::HIGH_CONTRAST);
        }
        self.colors.as_ref().and_then(|c| c.palette().ok())
    }

    /// Whether to use the light variant, given what the OS prefers (`None` = unknown). A custom
    /// palette decides by its background.
    pub fn is_light(&self, system_light: Option<bool>) -> bool {
        if let Some(palette) = self.palette() {
            return palette.is_light();
        }
        match self.name.as_str() {
            "hivetty-light" | "light" => true,
            "hivetty-dark" | "dark" => false,
            _ => system_light.unwrap_or(false),
        }
    }
}

/// `[updates]`: whether hivetty looks for new versions (F6-11). Off unless the user turns it
/// on, since it contacts GitHub.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Updates {
    /// Once a day, ask GitHub for the latest release and say when there is a newer one.
    pub check: bool,
}

/// `[editor]`: what "Open in editor" runs, with the folder as its last argument.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Editor {
    /// The program and its arguments, split on spaces (`code`, `zed`, `subl -n`).
    pub command: String,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            command: "code".to_owned(),
        }
    }
}

/// `[agents]`
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Agents {
    /// What to do with agents that were running when the layout was saved: `"ask"` types the
    /// resume command and waits for Enter, `"always"` runs it, `"never"` leaves a plain shell.
    pub restore_on_launch: String,
    /// Agents that do not report their state (no hooks or OSC 7501): seconds without output
    /// after working before the session counts as done (F4-09).
    pub waiting_idle_seconds: u64,
}

impl Default for Agents {
    fn default() -> Self {
        Self {
            restore_on_launch: "ask".to_owned(),
            waiting_idle_seconds: 5,
        }
    }
}

/// `[notifications]`
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Notifications {
    /// Send desktop notifications at all.
    pub enabled: bool,
    /// Only when the user is not looking at the pane (window unfocused or another pane).
    pub only_when_unfocused: bool,
    /// Notify when a shell command that ran at least this long finishes; 0 disables it.
    pub long_command_seconds: u64,
    /// Agent states that notify: any of "waiting", "done", "error".
    pub on: Vec<String>,
    /// More than three notifications in this many seconds are merged into one.
    pub group_window_seconds: u64,
    /// Agent states whose notification plays a sound: any of "waiting", "done", "error".
    pub sounds: Vec<String>,
    /// Also send them to the phone (F4-33).
    pub forward: Forward,
}

/// `[notifications.forward]`: where notifications are also sent. Off by default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Forward {
    /// "ntfy", "telegram" or "webhook"; empty = off.
    pub service: String,
    /// ntfy topic URL, Telegram `https://api.telegram.org/bot<token>/sendMessage`, or the
    /// webhook URL.
    pub url: String,
    /// Telegram chat id.
    pub chat_id: String,
}

impl Default for Notifications {
    fn default() -> Self {
        Self {
            enabled: true,
            only_when_unfocused: true,
            long_command_seconds: 30,
            on: vec!["waiting".into(), "done".into(), "error".into()],
            group_window_seconds: 10,
            sounds: vec!["waiting".into(), "error".into()],
            forward: Forward::default(),
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

/// Where hivetty keeps its state (layout, later the usage database).
///
/// - Windows: `%LOCALAPPDATA%\hivetty`
/// - macOS: `~/Library/Application Support/hivetty`
/// - Linux: `$XDG_DATA_HOME/hivetty` (default `~/.local/share`)
///
/// `HIVETTY_DATA_DIR` overrides it.
pub fn data_dir() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("HIVETTY_DATA_DIR") {
        return Some(PathBuf::from(path));
    }
    let base = if cfg!(windows) {
        PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support")
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?
    };
    Some(base.join("hivetty"))
}

/// Parses config text. On error returns the defaults plus a readable message.
pub fn parse(text: &str) -> (Config, Option<String>) {
    match toml::from_str::<Config>(text) {
        Ok(mut config) => {
            config.font.size = config.font.size.clamp(6.0, 72.0);
            config.terminal.scrollback_lines = config.terminal.scrollback_lines.min(1_000_000);
            // Bad custom colors: report them and keep the built-in palette.
            let error = config.theme.colors.as_ref().and_then(|c| c.palette().err());
            if error.is_some() {
                config.theme.colors = None;
            }
            (config, error)
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

/// Safe mode (`--safe` or `HIVETTY_SAFE=1`, N-23): default settings, no shell integration, no
/// layout restore and the software renderer, to start even when the config or the GPU is the
/// problem.
static SAFE_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Turns safe mode on for this process.
pub fn set_safe_mode() {
    SAFE_MODE.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// Whether safe mode is on (the flag or `HIVETTY_SAFE=1`).
pub fn safe_mode() -> bool {
    SAFE_MODE.load(std::sync::atomic::Ordering::Relaxed)
        || std::env::var_os("HIVETTY_SAFE").is_some_and(|v| v == "1")
}

/// The settings used in safe mode.
pub fn safe_config() -> Config {
    let mut config = Config::default();
    config.shell.integration = false;
    config.shell.chips = false;
    config.window.restore_layout = false;
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_published_schema_is_up_to_date() {
        let published = include_str!("../../../docs/config.schema.json");
        assert_eq!(
            published.replace("\r\n", "\n"),
            schema_json(),
            "run `hivetty config schema > docs/config.schema.json`"
        );
    }

    #[test]
    fn custom_colors_are_checked_and_decide_light_or_dark() {
        let ansi = vec!["#000000"; 16]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>();
        let colors = ThemeColors {
            foreground: "#1f2328".into(),
            background: "#fdf6e3".into(),
            cursor: None,
            selection: None,
            ansi: ansi.clone(),
        };
        let theme = Theme {
            colors: Some(colors.clone()),
            ..Theme::default()
        };
        assert!(
            theme.is_light(Some(false)),
            "a light background wins over the OS"
        );
        assert_eq!(theme.palette().unwrap().cursor, 0x1F2328);
        let high = Theme {
            high_contrast: true,
            ..theme
        };
        assert_eq!(high.palette(), Some(Palette::HIGH_CONTRAST));
        assert!(!high.is_light(Some(true)));
        let bad = ThemeColors {
            ansi: ansi[..3].to_vec(),
            ..colors
        };
        assert!(bad.palette().unwrap_err().contains("16 colors"));
        let (config, error) = parse(
            "[theme.colors]
foreground = 'red'
background = '#000000'
ansi = []
",
        );
        assert!(error.unwrap().contains("foreground"));
        assert_eq!(
            config.theme.colors, None,
            "bad colors fall back to the built-in palette"
        );
    }

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
