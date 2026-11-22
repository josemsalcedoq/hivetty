//! App shortcuts (docs/keybindings.md): named actions, chords written as text
//! (`"primary+shift+d"`, `"alt+left"`) and the per-OS defaults. User overrides from
//! `config.toml` are layered on top (F2-06).

use std::fmt;

use crate::workspace::Direction;

/// Something the app can do from a shortcut (or, later, the command palette).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Action {
    NewTab,
    NewWindow,
    MoveTabToNewWindow,
    ClosePane,
    SplitRight,
    SplitDown,
    FocusPane(Direction),
    ResizePane(Direction),
    ToggleZoom,
    NextTab,
    PreviousTab,
    /// 0-based.
    GoToTab(usize),
    ToggleSidebar,
    Find,
    Copy,
    Paste,
    FontBigger,
    FontSmaller,
    FontReset,
    CommandPalette,
    /// Write this text to the focused terminal (`action = { send_text = "…" }`).
    SendText(String),
    /// Explicitly unbound (`action = "none"` in the config).
    None,
}

impl Action {
    /// Parses the config name of an action (`"split_right"`, `"go_to_tab_3"`…).
    pub(crate) fn parse(name: &str) -> Option<Self> {
        use Direction::{Down, Left, Right, Up};
        let action = match name {
            "new_tab" => Self::NewTab,
            "new_window" => Self::NewWindow,
            "move_tab_to_new_window" => Self::MoveTabToNewWindow,
            "close_pane" => Self::ClosePane,
            "split_right" => Self::SplitRight,
            "split_down" => Self::SplitDown,
            "focus_left" => Self::FocusPane(Left),
            "focus_right" => Self::FocusPane(Right),
            "focus_up" => Self::FocusPane(Up),
            "focus_down" => Self::FocusPane(Down),
            "resize_left" => Self::ResizePane(Left),
            "resize_right" => Self::ResizePane(Right),
            "resize_up" => Self::ResizePane(Up),
            "resize_down" => Self::ResizePane(Down),
            "toggle_zoom" => Self::ToggleZoom,
            "next_tab" => Self::NextTab,
            "previous_tab" => Self::PreviousTab,
            "toggle_sidebar" => Self::ToggleSidebar,
            "find" => Self::Find,
            "copy" => Self::Copy,
            "paste" => Self::Paste,
            "font_bigger" => Self::FontBigger,
            "font_smaller" => Self::FontSmaller,
            "font_reset" => Self::FontReset,
            "command_palette" => Self::CommandPalette,
            "none" => Self::None,
            other => {
                let n: usize = other.strip_prefix("go_to_tab_")?.parse().ok()?;
                return (1..=9).contains(&n).then_some(Self::GoToTab(n - 1));
            }
        };
        Some(action)
    }
}

/// A key plus modifiers. `key` is lowercase: a character (`"t"`, `"="`, `"1"`) or a key name
/// (`"left"`, `"enter"`, `"tab"`, `"pageup"`, `"f11"`…).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Chord {
    pub(crate) key: String,
    pub(crate) ctrl: bool,
    pub(crate) shift: bool,
    pub(crate) alt: bool,
    /// Cmd on macOS, the Windows key elsewhere.
    pub(crate) super_key: bool,
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [
            (self.ctrl, "ctrl+"),
            (self.alt, "alt+"),
            (self.shift, "shift+"),
            (self.super_key, "cmd+"),
        ] {
            if on {
                f.write_str(name)?;
            }
        }
        f.write_str(&self.key)
    }
}

impl Chord {
    /// How the chord is shown to people: "Ctrl+Shift+D", or "⇧⌘D" on macOS.
    pub(crate) fn pretty(&self) -> String {
        let key = match self.key.as_str() {
            // Words, not arrows: the UI font has no arrow glyphs.
            "left" => "Left".to_owned(),
            "right" => "Right".to_owned(),
            "up" => "Up".to_owned(),
            "down" => "Down".to_owned(),
            "enter" => if cfg!(target_os = "macos") {
                "↩"
            } else {
                "Enter"
            }
            .to_owned(),
            "pageup" => "PgUp".to_owned(),
            "pagedown" => "PgDn".to_owned(),
            k if k.chars().count() == 1 => k.to_uppercase(),
            k => {
                let mut chars = k.chars();
                chars
                    .next()
                    .map(|c| c.to_uppercase().chain(chars).collect())
                    .unwrap_or_default()
            }
        };
        if cfg!(target_os = "macos") {
            let mut out = String::new();
            for (on, sym) in [
                (self.ctrl, "⌃"),
                (self.alt, "⌥"),
                (self.shift, "⇧"),
                (self.super_key, "⌘"),
            ] {
                if on {
                    out.push_str(sym);
                }
            }
            out + &key
        } else {
            let mut parts = Vec::new();
            for (on, name) in [
                (self.ctrl, "Ctrl"),
                (self.shift, "Shift"),
                (self.alt, "Alt"),
                (self.super_key, "Win"),
            ] {
                if on {
                    parts.push(name.to_owned());
                }
            }
            parts.push(key);
            parts.join("+")
        }
    }
}

/// Why a chord could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChordError(pub(crate) String);

impl Chord {
    /// Parses `"primary+shift+d"`. `primary` is Cmd on macOS and Ctrl elsewhere
    /// (docs/keybindings.md, config format).
    pub(crate) fn parse(text: &str) -> Result<Self, ChordError> {
        let mut chord = Chord {
            key: String::new(),
            ctrl: false,
            shift: false,
            alt: false,
            super_key: false,
        };
        let parts: Vec<&str> = text.split('+').map(str::trim).collect();
        // "ctrl++" means Ctrl and the plus key.
        let (mods, key) = match parts.as_slice() {
            [.., "", ""] => (&parts[..parts.len() - 2], "+"),
            [mods @ .., key] => (mods, *key),
            [] => return Err(ChordError(format!("empty shortcut '{text}'"))),
        };
        for m in mods {
            match m.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => chord.ctrl = true,
                "shift" => chord.shift = true,
                "alt" | "option" | "opt" => chord.alt = true,
                "cmd" | "super" | "meta" | "win" => chord.super_key = true,
                "primary" if cfg!(target_os = "macos") => chord.super_key = true,
                "primary" => chord.ctrl = true,
                other => {
                    return Err(ChordError(format!(
                        "unknown modifier '{other}' in '{text}'"
                    )));
                }
            }
        }
        let key = key.to_lowercase();
        let named = [
            "left",
            "right",
            "up",
            "down",
            "enter",
            "tab",
            "escape",
            "space",
            "backspace",
            "delete",
            "insert",
            "home",
            "end",
            "pageup",
            "pagedown",
        ];
        let is_fn = key
            .strip_prefix('f')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=24).contains(&n));
        if key.chars().count() != 1 && !named.contains(&key.as_str()) && !is_fn {
            return Err(ChordError(format!("unknown key '{key}' in '{text}'")));
        }
        chord.key = key;
        Ok(chord)
    }
}

/// A `when` condition (docs/keybindings.md): `os == 'windows'`, `search_open`,
/// `terminal_focused`, `agent == 'claude'`, optionally negated with `!`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum When {
    Os(String),
    SearchOpen,
    TerminalFocused,
    Agent(String),
    Not(Box<When>),
}

impl When {
    pub(crate) fn parse(text: &str) -> Result<Self, ChordError> {
        let text = text.trim();
        if let Some(rest) = text.strip_prefix('!') {
            return Ok(Self::Not(Box::new(Self::parse(rest)?)));
        }
        let quoted = |v: &str| {
            v.trim()
                .trim_matches(|c| c == '\'' || c == '"')
                .to_ascii_lowercase()
        };
        if let Some((lhs, rhs)) = text.split_once("==") {
            return match lhs.trim() {
                "os" => Ok(Self::Os(quoted(rhs))),
                "agent" => Ok(Self::Agent(quoted(rhs))),
                other => Err(ChordError(format!("unknown condition '{other}'"))),
            };
        }
        match text {
            "search_open" => Ok(Self::SearchOpen),
            "terminal_focused" => Ok(Self::TerminalFocused),
            other => Err(ChordError(format!("unknown condition '{other}'"))),
        }
    }

    fn holds(&self, ctx: &Context<'_>) -> bool {
        match self {
            Self::Os(os) => os == std::env::consts::OS,
            Self::SearchOpen => ctx.search_open,
            Self::TerminalFocused => !ctx.search_open,
            Self::Agent(id) => ctx.agent == Some(id.as_str()),
            Self::Not(inner) => !inner.holds(ctx),
        }
    }
}

/// What `when` conditions are checked against.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Context<'a> {
    pub(crate) search_open: bool,
    /// Agent running in the focused pane, if known.
    pub(crate) agent: Option<&'a str>,
}

#[derive(Debug, Clone)]
struct Binding {
    chord: Chord,
    action: Action,
    when: Option<When>,
}

/// Actions listed in the command palette, by config name, in display order.
pub(crate) const PALETTE_ACTIONS: &[&str] = &[
    "new_tab",
    "new_window",
    "move_tab_to_new_window",
    "split_right",
    "split_down",
    "close_pane",
    "toggle_zoom",
    "next_tab",
    "previous_tab",
    "focus_left",
    "focus_right",
    "focus_up",
    "focus_down",
    "resize_left",
    "resize_right",
    "resize_up",
    "resize_down",
    "toggle_sidebar",
    "find",
    "copy",
    "paste",
    "font_bigger",
    "font_smaller",
    "font_reset",
    "command_palette",
];

/// Sections of the menu (the ☰ menu, and the menu bar on macOS), by action name.
pub(crate) const MENU_SECTIONS: &[&[&str]] = &[
    &[
        "new_tab",
        "new_window",
        "split_right",
        "split_down",
        "close_pane",
    ],
    &["move_tab_to_new_window"],
    &["find", "copy", "paste"],
    &[
        "toggle_sidebar",
        "toggle_zoom",
        "font_bigger",
        "font_smaller",
        "font_reset",
    ],
    &["next_tab", "previous_tab"],
    &["command_palette"],
];

/// The palette / menu entry for an action name: its id is the index in [`PALETTE_ACTIONS`].
pub(crate) fn menu_item(keymap: &Keymap, name: &str) -> Option<hv_ui::PaletteItem> {
    let id = PALETTE_ACTIONS.iter().position(|n| *n == name)?;
    let action = Action::parse(name)?;
    Some(hv_ui::PaletteItem {
        id,
        label: hv_ui::strings::action_label(name)?.to_owned(),
        hint: keymap
            .chord_for(&action)
            .map(Chord::pretty)
            .unwrap_or_default(),
    })
}

/// Shortcuts in effect: the defaults with the user's overrides applied.
#[derive(Debug, Clone)]
pub(crate) struct Keymap {
    bindings: Vec<Binding>,
}

impl Keymap {
    /// The defaults for this OS.
    pub(crate) fn defaults() -> Self {
        let table: &[(&str, &str)] = if cfg!(target_os = "macos") {
            MAC
        } else {
            OTHERS
        };
        let mut bindings: Vec<Binding> = table
            .iter()
            .map(|(chord, action)| Binding {
                chord: Chord::parse(chord).expect("valid default chord"),
                action: Action::parse(action).expect("valid default action"),
                when: None,
            })
            .collect();
        for n in 1..=9 {
            let chord = if cfg!(target_os = "macos") {
                format!("cmd+{n}")
            } else {
                format!("alt+{n}")
            };
            bindings.push(Binding {
                chord: Chord::parse(&chord).expect("valid chord"),
                action: Action::GoToTab(n - 1),
                when: None,
            });
        }
        Self { bindings }
    }

    /// Applies the user's `[[keybindings]]`. Returns one message per bad entry; good entries
    /// still apply. An unconditional override replaces the default for that chord; a
    /// conditional one wins only while its condition holds.
    pub(crate) fn with_overrides(
        mut self,
        overrides: &[hv_config::KeyBinding],
    ) -> (Self, Vec<String>) {
        let mut errors = Vec::new();
        for entry in overrides {
            let chord = match Chord::parse(&entry.key) {
                Ok(c) => c,
                Err(ChordError(e)) => {
                    errors.push(e);
                    continue;
                }
            };
            let action = match &entry.action {
                hv_config::BindingAction::SendText { send_text } => {
                    Action::SendText(send_text.clone())
                }
                hv_config::BindingAction::Named(name) => match Action::parse(name) {
                    Some(a) => a,
                    None => {
                        errors.push(format!("unknown action '{name}' for '{chord}'"));
                        continue;
                    }
                },
            };
            let when = match entry.when.as_deref().map(When::parse).transpose() {
                Ok(w) => w,
                Err(ChordError(e)) => {
                    errors.push(format!("{e} in the 'when' of '{chord}'"));
                    continue;
                }
            };
            if when.is_none() {
                self.bindings.retain(|b| b.chord != chord);
            }
            self.bindings.push(Binding {
                chord,
                action,
                when,
            });
        }
        (self, errors)
    }

    /// The first unconditional chord bound to `action`, for display ("Ctrl+Shift+D").
    pub(crate) fn chord_for(&self, action: &Action) -> Option<&Chord> {
        self.bindings
            .iter()
            .filter(|b| b.when.is_none() && b.action == *action)
            .find(|b| self.lookup_unconditional(&b.chord) == Some(action))
            .map(|b| &b.chord)
    }

    fn lookup_unconditional(&self, chord: &Chord) -> Option<&Action> {
        self.bindings
            .iter()
            .rev()
            .find(|b| b.when.is_none() && b.chord == *chord)
            .map(|b| &b.action)
    }

    /// The action bound to a key press in this context, if any. Later bindings (the user's)
    /// win over earlier ones (the defaults).
    pub(crate) fn lookup(&self, pressed: &Chord, ctx: &Context<'_>) -> Option<Action> {
        self.bindings
            .iter()
            .rev()
            .find(|b| b.chord == *pressed && b.when.as_ref().is_none_or(|w| w.holds(ctx)))
            .map(|b| b.action.clone())
            .filter(|a| *a != Action::None)
    }
}

const MAC: &[(&str, &str)] = &[
    ("cmd+t", "new_tab"),
    ("cmd+n", "new_window"),
    ("cmd+w", "close_pane"),
    ("cmd+d", "split_right"),
    ("shift+cmd+d", "split_down"),
    ("alt+cmd+left", "focus_left"),
    ("alt+cmd+right", "focus_right"),
    ("alt+cmd+up", "focus_up"),
    ("alt+cmd+down", "focus_down"),
    ("ctrl+cmd+left", "resize_left"),
    ("ctrl+cmd+right", "resize_right"),
    ("ctrl+cmd+up", "resize_up"),
    ("ctrl+cmd+down", "resize_down"),
    ("shift+cmd+enter", "toggle_zoom"),
    ("shift+cmd+]", "next_tab"),
    ("shift+cmd+[", "previous_tab"),
    ("cmd+b", "toggle_sidebar"),
    ("cmd+f", "find"),
    ("cmd+c", "copy"),
    ("cmd+v", "paste"),
    ("cmd+=", "font_bigger"),
    ("cmd+-", "font_smaller"),
    ("cmd+0", "font_reset"),
    ("shift+cmd+p", "command_palette"),
];

const OTHERS: &[(&str, &str)] = &[
    ("ctrl+shift+t", "new_tab"),
    ("ctrl+shift+n", "new_window"),
    ("ctrl+shift+w", "close_pane"),
    ("ctrl+shift+d", "split_right"),
    ("ctrl+shift+e", "split_down"),
    ("alt+left", "focus_left"),
    ("alt+right", "focus_right"),
    ("alt+up", "focus_up"),
    ("alt+down", "focus_down"),
    ("ctrl+shift+alt+left", "resize_left"),
    ("ctrl+shift+alt+right", "resize_right"),
    ("ctrl+shift+alt+up", "resize_up"),
    ("ctrl+shift+alt+down", "resize_down"),
    ("ctrl+shift+enter", "toggle_zoom"),
    ("ctrl+tab", "next_tab"),
    ("ctrl+shift+tab", "previous_tab"),
    ("ctrl+pagedown", "next_tab"),
    ("ctrl+pageup", "previous_tab"),
    ("ctrl+shift+b", "toggle_sidebar"),
    ("ctrl+shift+f", "find"),
    ("ctrl+shift+c", "copy"),
    ("ctrl+shift+v", "paste"),
    ("ctrl+=", "font_bigger"),
    ("ctrl+-", "font_smaller"),
    ("ctrl+0", "font_reset"),
    ("ctrl+shift+p", "command_palette"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(text: &str) -> Chord {
        Chord::parse(text).unwrap()
    }

    #[test]
    fn chords_parse_modifiers_keys_and_primary() {
        let c = chord("Ctrl+Shift+Alt+Left");
        assert!(c.ctrl && c.shift && c.alt && !c.super_key);
        assert_eq!(c.key, "left");
        assert_eq!(chord("ctrl++").key, "+");
        let primary = chord("primary+t");
        if cfg!(target_os = "macos") {
            assert!(primary.super_key && !primary.ctrl);
        } else {
            assert!(primary.ctrl && !primary.shift && !primary.super_key);
        }
        assert!(Chord::parse("hyper+t").is_err());
        assert!(Chord::parse("ctrl+banana").is_err());
        assert_eq!(chord("f11").key, "f11");
    }

    #[test]
    fn defaults_cover_the_documented_shortcuts() {
        let keymap = Keymap::defaults();
        let ctx = Context::default();
        let (split, tab3) = if cfg!(target_os = "macos") {
            ("cmd+d", "cmd+3")
        } else {
            ("ctrl+shift+d", "alt+3")
        };
        assert_eq!(keymap.lookup(&chord(split), &ctx), Some(Action::SplitRight));
        assert_eq!(keymap.lookup(&chord(tab3), &ctx), Some(Action::GoToTab(2)));
        assert_eq!(keymap.lookup(&chord("ctrl+shift+q"), &ctx), None);
    }

    #[test]
    fn overrides_replace_and_unbind_and_report_errors() {
        let bind = |key: &str, action: &str, when: Option<&str>| hv_config::KeyBinding {
            key: key.into(),
            action: hv_config::BindingAction::Named(action.into()),
            when: when.map(str::to_owned),
        };
        let (keymap, errors) = Keymap::defaults().with_overrides(&[
            bind("ctrl+shift+d", "none", None),
            bind("cmd+d", "none", None),
            bind("alt+backspace", "close_pane", None),
            bind("ctrl+nope", "close_pane", None),
            bind("alt+x", "fly", None),
            bind("alt+y", "find", Some("weather == 'sunny'")),
            hv_config::KeyBinding {
                key: "shift+enter".into(),
                action: hv_config::BindingAction::SendText {
                    send_text: "\u{1b}\r".into(),
                },
                when: Some("agent == 'claude'".into()),
            },
        ]);
        let ctx = Context::default();
        assert_eq!(keymap.lookup(&chord("ctrl+shift+d"), &ctx), None);
        assert_eq!(keymap.lookup(&chord("cmd+d"), &ctx), None);
        assert_eq!(
            keymap.lookup(&chord("alt+backspace"), &ctx),
            Some(Action::ClosePane)
        );
        assert_eq!(errors.len(), 3, "{errors:?}");
        // Conditional bindings apply only while their condition holds.
        assert_eq!(keymap.lookup(&chord("shift+enter"), &ctx), None);
        let claude = Context {
            agent: Some("claude"),
            ..Context::default()
        };
        assert_eq!(
            keymap.lookup(&chord("shift+enter"), &claude),
            Some(Action::SendText("\u{1b}\r".into()))
        );
    }

    #[test]
    fn conditions_parse_and_evaluate() {
        let ctx = Context {
            search_open: true,
            agent: None,
        };
        assert!(When::parse("search_open").unwrap().holds(&ctx));
        assert!(!When::parse("!search_open").unwrap().holds(&ctx));
        let here = When::parse(&format!("os == '{}'", std::env::consts::OS)).unwrap();
        assert!(here.holds(&ctx));
        assert!(When::parse("cpu == 'x'").is_err());
    }

    #[test]
    fn action_names_round_trip() {
        assert_eq!(Action::parse("go_to_tab_9"), Some(Action::GoToTab(8)));
        assert_eq!(Action::parse("go_to_tab_10"), None);
        assert_eq!(
            Action::parse("resize_up"),
            Some(Action::ResizePane(Direction::Up))
        );
    }
}
