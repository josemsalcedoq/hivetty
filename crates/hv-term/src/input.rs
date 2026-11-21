//! Keyboard encoding: turns key presses into the bytes a terminal program expects.
//!
//! This covers the classic xterm encoding. The kitty keyboard protocol encoding is ported from
//! Alacritty's frontend later (task F1-30).

/// A key, independent of the windowing library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    /// Text produced by the key (already includes Shift / layout / dead keys / AltGr).
    Text(String),
    /// Enter / Return.
    Enter,
    /// Backspace.
    Backspace,
    /// Tab.
    Tab,
    /// Escape.
    Escape,
    /// Arrow up.
    Up,
    /// Arrow down.
    Down,
    /// Arrow left.
    Left,
    /// Arrow right.
    Right,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// Insert.
    Insert,
    /// Delete.
    Delete,
    /// Function key F1-F12.
    F(u8),
}

/// Modifier state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Mods {
    /// Shift.
    pub shift: bool,
    /// Control.
    pub ctrl: bool,
    /// Alt / Option-as-Meta.
    pub alt: bool,
}

impl Mods {
    /// xterm modifier parameter (1 + shift + 2*alt + 4*ctrl), or `None` if no modifier.
    fn xterm_param(self) -> Option<u8> {
        let v = u8::from(self.shift) + 2 * u8::from(self.alt) + 4 * u8::from(self.ctrl);
        (v != 0).then_some(v + 1)
    }
}

/// Terminal modes that change key encoding.
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyModes {
    /// DECCKM: cursor keys send `ESC O x` instead of `ESC [ x`.
    pub app_cursor: bool,
}

/// Encodes a key press. Returns `None` for keys that produce nothing.
pub fn encode_key(key: &Key, mods: Mods, modes: KeyModes) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    match key {
        Key::Text(text) => {
            if text.is_empty() {
                return None;
            }
            if mods.ctrl && !mods.alt {
                if let Some(b) = ctrl_byte(text) {
                    return Some(vec![b]);
                }
            }
            if mods.alt {
                out.push(0x1b);
                if mods.ctrl {
                    if let Some(b) = ctrl_byte(text) {
                        out.push(b);
                        return Some(out);
                    }
                }
            }
            out.extend_from_slice(text.as_bytes());
        }
        // Shift+Enter: ESC CR, the newline that coding agents (Claude Code and others) accept.
        Key::Enter if mods.shift => out.extend_from_slice(b"\x1b\r"),
        Key::Enter => alt_prefixed(&mut out, mods, b"\r"),
        Key::Backspace if mods.ctrl => alt_prefixed(&mut out, mods, b"\x08"),
        Key::Backspace => alt_prefixed(&mut out, mods, b"\x7f"),
        Key::Tab if mods.shift => out.extend_from_slice(b"\x1b[Z"),
        Key::Tab => alt_prefixed(&mut out, mods, b"\t"),
        Key::Escape => alt_prefixed(&mut out, mods, b"\x1b"),
        Key::Up => cursor_key(&mut out, b'A', mods, modes),
        Key::Down => cursor_key(&mut out, b'B', mods, modes),
        Key::Right => cursor_key(&mut out, b'C', mods, modes),
        Key::Left => cursor_key(&mut out, b'D', mods, modes),
        Key::Home => cursor_key(&mut out, b'H', mods, modes),
        Key::End => cursor_key(&mut out, b'F', mods, modes),
        Key::Insert => tilde_key(&mut out, 2, mods),
        Key::Delete => tilde_key(&mut out, 3, mods),
        Key::PageUp => tilde_key(&mut out, 5, mods),
        Key::PageDown => tilde_key(&mut out, 6, mods),
        Key::F(n @ 1..=4) => {
            let letter = b"PQRS"[usize::from(*n - 1)];
            match mods.xterm_param() {
                Some(m) => {
                    out.extend_from_slice(format!("\x1b[1;{m}{}", char::from(letter)).as_bytes())
                }
                None => out.extend_from_slice(&[0x1b, b'O', letter]),
            }
        }
        Key::F(n @ 5..=12) => {
            let code = [15, 17, 18, 19, 20, 21, 23, 24][usize::from(*n - 5)];
            tilde_key(&mut out, code, mods);
        }
        Key::F(_) => return None,
    }
    Some(out)
}

fn alt_prefixed(out: &mut Vec<u8>, mods: Mods, bytes: &[u8]) {
    if mods.alt {
        out.push(0x1b);
    }
    out.extend_from_slice(bytes);
}

fn cursor_key(out: &mut Vec<u8>, letter: u8, mods: Mods, modes: KeyModes) {
    match mods.xterm_param() {
        Some(m) => out.extend_from_slice(format!("\x1b[1;{m}{}", char::from(letter)).as_bytes()),
        None if modes.app_cursor => out.extend_from_slice(&[0x1b, b'O', letter]),
        None => out.extend_from_slice(&[0x1b, b'[', letter]),
    }
}

fn tilde_key(out: &mut Vec<u8>, code: u8, mods: Mods) {
    match mods.xterm_param() {
        Some(m) => out.extend_from_slice(format!("\x1b[{code};{m}~").as_bytes()),
        None => out.extend_from_slice(format!("\x1b[{code}~").as_bytes()),
    }
}

/// Control byte for Ctrl+<char>, following xterm.
fn ctrl_byte(text: &str) -> Option<u8> {
    let mut chars = text.chars();
    let c = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    match c {
        'a'..='z' => Some(c as u8 - b'a' + 1),
        'A'..='Z' => Some(c as u8 - b'A' + 1),
        '@' | ' ' | '2' => Some(0),
        '[' | '3' => Some(0x1b),
        '\\' | '4' => Some(0x1c),
        ']' | '5' => Some(0x1d),
        '^' | '6' => Some(0x1e),
        '_' | '-' | '7' => Some(0x1f),
        '?' | '8' => Some(0x7f),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enc(key: Key, mods: Mods) -> Vec<u8> {
        encode_key(&key, mods, KeyModes::default()).unwrap()
    }

    const NONE: Mods = Mods {
        shift: false,
        ctrl: false,
        alt: false,
    };
    const CTRL: Mods = Mods {
        shift: false,
        ctrl: true,
        alt: false,
    };
    const ALT: Mods = Mods {
        shift: false,
        ctrl: false,
        alt: true,
    };
    const SHIFT: Mods = Mods {
        shift: true,
        ctrl: false,
        alt: false,
    };

    #[test]
    fn text_and_control_characters() {
        assert_eq!(enc(Key::Text("ñ".into()), NONE), "ñ".as_bytes());
        assert_eq!(enc(Key::Text("c".into()), CTRL), b"\x03");
        assert_eq!(enc(Key::Text("x".into()), ALT), b"\x1bx");
        assert_eq!(enc(Key::Text("[".into()), CTRL), b"\x1b");
    }

    #[test]
    fn enter_and_shift_enter() {
        assert_eq!(enc(Key::Enter, NONE), b"\r");
        assert_eq!(enc(Key::Enter, SHIFT), b"\x1b\r");
    }

    #[test]
    fn cursor_keys_respect_decckm_and_modifiers() {
        assert_eq!(enc(Key::Up, NONE), b"\x1b[A");
        let app = KeyModes { app_cursor: true };
        assert_eq!(encode_key(&Key::Up, NONE, app).unwrap(), b"\x1bOA");
        assert_eq!(enc(Key::Right, CTRL), b"\x1b[1;5C");
    }

    #[test]
    fn tilde_and_function_keys() {
        assert_eq!(enc(Key::Delete, NONE), b"\x1b[3~");
        assert_eq!(enc(Key::PageUp, SHIFT), b"\x1b[5;2~");
        assert_eq!(enc(Key::F(1), NONE), b"\x1bOP");
        assert_eq!(enc(Key::F(5), NONE), b"\x1b[15~");
        assert_eq!(enc(Key::Tab, SHIFT), b"\x1b[Z");
    }
}
