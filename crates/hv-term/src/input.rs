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
#[allow(clippy::struct_excessive_bools)]
pub struct KeyModes {
    /// DECCKM: cursor keys send `ESC O x` instead of `ESC [ x`.
    pub app_cursor: bool,
    /// Kitty keyboard protocol flag 1: disambiguate escape codes.
    pub kitty_disambiguate: bool,
    /// Kitty keyboard protocol flag 8: report all keys as escape codes.
    pub kitty_all_keys: bool,
}

/// Encodes a key press. Returns `None` for keys that produce nothing.
pub fn encode_key(key: &Key, mods: Mods, modes: KeyModes) -> Option<Vec<u8>> {
    if (modes.kitty_disambiguate || modes.kitty_all_keys)
        && let Some(seq) = encode_kitty(key, mods, modes)
    {
        return Some(seq);
    }
    let mut out = Vec::new();
    match key {
        Key::Text(text) => {
            if text.is_empty() {
                return None;
            }
            if mods.ctrl
                && !mods.alt
                && let Some(b) = ctrl_byte(text)
            {
                return Some(vec![b]);
            }
            if mods.alt {
                out.push(0x1b);
                if mods.ctrl
                    && let Some(b) = ctrl_byte(text)
                {
                    out.push(b);
                    return Some(out);
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

/// Kitty keyboard protocol (<https://sw.kovidgoyal.net/kitty/keyboard-protocol/>), flags 1 and 8.
/// Returns `None` when the key keeps its legacy encoding.
fn encode_kitty(key: &Key, mods: Mods, modes: KeyModes) -> Option<Vec<u8>> {
    let modifier = mods.xterm_param(); // same 1 + shift + 2*alt + 4*ctrl scheme as kitty
    let csi_u = |code: u32| {
        let seq = match modifier {
            Some(m) => format!("\x1b[{code};{m}u"),
            None => format!("\x1b[{code}u"),
        };
        Some(seq.into_bytes())
    };
    let any_mod = modifier.is_some();
    match key {
        Key::Escape => csi_u(27),
        Key::Enter if any_mod || modes.kitty_all_keys => csi_u(13),
        Key::Tab if any_mod || modes.kitty_all_keys => csi_u(9),
        Key::Backspace if any_mod || modes.kitty_all_keys => csi_u(127),
        Key::Text(text) => {
            let mut chars = text.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None; // composed text (IME, dead keys): send as is
            }
            // With only Shift the produced text is sent as is (flag 1); with Ctrl/Alt, or when
            // all keys are reported, the key is encoded by its unshifted code point.
            let only_shift = mods.shift && !mods.ctrl && !mods.alt;
            if !modes.kitty_all_keys && (!any_mod || only_shift) {
                return None;
            }
            let base = c.to_lowercase().next().unwrap_or(c);
            csi_u(u32::from(base))
        }
        // Arrows, Home/End, F-keys and the tilde keys keep their xterm encodings, which already
        // carry the modifier parameter in the same format.
        _ => None,
    }
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

/// How to quote a dropped file path for the program running in the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quoting {
    /// POSIX shells and most programs: single quotes, `'` written as `'\''`.
    Posix,
    /// PowerShell: single quotes, `'` doubled.
    PowerShell,
    /// cmd.exe: double quotes (paths cannot contain `"`).
    Cmd,
}

impl Quoting {
    /// Picks the quoting style from the program name (`pwsh`, `powershell.exe`, `cmd`…).
    pub fn for_program(program: &str) -> Self {
        // Split on both separators: a Windows path must work the same on every OS.
        let file = program.rsplit(['/', '\\']).next().unwrap_or(program);
        let name = file
            .strip_suffix(".exe")
            .or_else(|| file.strip_suffix(".EXE"))
            .unwrap_or(file)
            .to_ascii_lowercase();
        match name.as_str() {
            "pwsh" | "powershell" => Self::PowerShell,
            "cmd" => Self::Cmd,
            _ => Self::Posix,
        }
    }
}

/// Quotes a path so it can be inserted on a command line. Paths with only safe characters are
/// left as they are.
pub fn quote_path(path: &str, quoting: Quoting) -> String {
    let safe = path
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '/' | '\\' | '.' | '-' | '_' | ':' | '~'));
    if safe && !path.is_empty() {
        return path.to_owned();
    }
    match quoting {
        Quoting::Posix => format!("'{}'", path.replace('\'', r"'\''")),
        Quoting::PowerShell => format!("'{}'", path.replace('\'', "''")),
        Quoting::Cmd => format!("\"{path}\""),
    }
}

/// Mouse button for reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    /// Left button.
    Left,
    /// Middle button.
    Middle,
    /// Right button.
    Right,
    /// Wheel up one notch.
    WheelUp,
    /// Wheel down one notch.
    WheelDown,
    /// Pointer moved with no button held.
    None,
}

/// What happened to the mouse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseAction {
    /// Button pressed (or wheel notch).
    Press,
    /// Button released.
    Release,
    /// Pointer moved (with `button` held, or `MouseButton::None`).
    Motion,
}

/// Mouse reporting modes requested by the program (DECSET 1000/1002/1003/1006).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct MouseModes {
    /// 1000: report presses and releases.
    pub click: bool,
    /// 1002: also report motion while a button is held.
    pub drag: bool,
    /// 1003: report all motion.
    pub motion: bool,
    /// 1006: SGR extended coordinates.
    pub sgr: bool,
}

impl MouseModes {
    /// Whether the program wants any mouse events at all.
    pub fn any(self) -> bool {
        self.click || self.drag || self.motion
    }
}

/// Encodes a mouse event at a 0-based cell for a program that asked for mouse reporting.
/// Returns `None` when the current modes do not report this kind of event.
pub fn encode_mouse(
    button: MouseButton,
    action: MouseAction,
    row: usize,
    col: usize,
    mods: Mods,
    modes: MouseModes,
) -> Option<Vec<u8>> {
    if !modes.any() {
        return None;
    }
    if action == MouseAction::Motion {
        let held = button != MouseButton::None;
        if !(modes.motion || (modes.drag && held)) {
            return None;
        }
    }
    let mut code: u32 = match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
        MouseButton::None => 3,
        MouseButton::WheelUp => 64,
        MouseButton::WheelDown => 65,
    };
    if action == MouseAction::Motion {
        code += 32;
    }
    code += 4 * u32::from(mods.shift) + 8 * u32::from(mods.alt) + 16 * u32::from(mods.ctrl);
    let (x, y) = (col + 1, row + 1);
    if modes.sgr {
        let suffix = if action == MouseAction::Release {
            'm'
        } else {
            'M'
        };
        return Some(format!("\x1b[<{code};{x};{y}{suffix}").into_bytes());
    }
    // Classic X10 encoding: releases carry no button, coordinates are limited to 223.
    if action == MouseAction::Release {
        code = (code & !0b11) | 3;
    }
    let byte = |v: usize| u8::try_from(32 + v.min(223)).unwrap_or(255);
    let code = u8::try_from(32 + code).ok()?;
    Some(vec![0x1b, b'[', b'M', code, byte(x), byte(y)])
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
        let app = KeyModes {
            app_cursor: true,
            ..KeyModes::default()
        };
        assert_eq!(encode_key(&Key::Up, NONE, app).unwrap(), b"\x1bOA");
        assert_eq!(enc(Key::Right, CTRL), b"\x1b[1;5C");
    }

    #[test]
    fn dropped_paths_are_quoted_per_shell() {
        assert_eq!(
            quote_path(r"C:\dev\shot.png", Quoting::PowerShell),
            r"C:\dev\shot.png"
        );
        assert_eq!(
            quote_path(r"C:\My Files\it's.png", Quoting::PowerShell),
            r"'C:\My Files\it''s.png'"
        );
        assert_eq!(
            quote_path("/tmp/a b/it's", Quoting::Posix),
            r"'/tmp/a b/it'\''s'"
        );
        assert_eq!(
            quote_path(r"C:\a b\c.txt", Quoting::Cmd),
            "\"C:\\a b\\c.txt\""
        );
        assert_eq!(
            Quoting::for_program(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe"),
            Quoting::PowerShell
        );
        assert_eq!(Quoting::for_program("cmd.exe"), Quoting::Cmd);
        assert_eq!(Quoting::for_program("/bin/zsh"), Quoting::Posix);
    }

    #[test]
    fn mouse_sgr_and_x10() {
        let sgr = MouseModes {
            click: true,
            sgr: true,
            ..MouseModes::default()
        };
        let press = encode_mouse(MouseButton::Left, MouseAction::Press, 4, 9, NONE, sgr).unwrap();
        assert_eq!(press, b"\x1b[<0;10;5M");
        let release =
            encode_mouse(MouseButton::Left, MouseAction::Release, 4, 9, NONE, sgr).unwrap();
        assert_eq!(release, b"\x1b[<0;10;5m");
        let wheel =
            encode_mouse(MouseButton::WheelDown, MouseAction::Press, 0, 0, CTRL, sgr).unwrap();
        assert_eq!(wheel, b"\x1b[<81;1;1M");

        let x10 = MouseModes {
            click: true,
            ..MouseModes::default()
        };
        let press = encode_mouse(MouseButton::Right, MouseAction::Press, 0, 0, NONE, x10).unwrap();
        assert_eq!(press, [0x1b, b'[', b'M', 32 + 2, 33, 33]);
        let release =
            encode_mouse(MouseButton::Right, MouseAction::Release, 0, 0, NONE, x10).unwrap();
        assert_eq!(release[3], 32 + 3);
    }

    #[test]
    fn mouse_motion_only_when_requested() {
        let click = MouseModes {
            click: true,
            sgr: true,
            ..MouseModes::default()
        };
        assert!(encode_mouse(MouseButton::Left, MouseAction::Motion, 0, 0, NONE, click).is_none());
        let drag = MouseModes {
            drag: true,
            ..click
        };
        assert!(encode_mouse(MouseButton::Left, MouseAction::Motion, 0, 0, NONE, drag).is_some());
        assert!(encode_mouse(MouseButton::None, MouseAction::Motion, 0, 0, NONE, drag).is_none());
        let all = MouseModes {
            motion: true,
            ..click
        };
        assert_eq!(
            encode_mouse(MouseButton::None, MouseAction::Motion, 1, 1, NONE, all).unwrap(),
            b"\x1b[<35;2;2M"
        );
        assert!(
            encode_mouse(
                MouseButton::Left,
                MouseAction::Press,
                0,
                0,
                NONE,
                MouseModes::default()
            )
            .is_none()
        );
    }

    #[test]
    fn kitty_disambiguate_mode() {
        let kitty = KeyModes {
            kitty_disambiguate: true,
            ..KeyModes::default()
        };
        let enc = |key: Key, mods: Mods| encode_key(&key, mods, kitty).unwrap();
        assert_eq!(enc(Key::Escape, NONE), b"\x1b[27u");
        assert_eq!(enc(Key::Enter, SHIFT), b"\x1b[13;2u");
        assert_eq!(
            enc(Key::Enter, NONE),
            b"\r",
            "unmodified Enter stays legacy"
        );
        assert_eq!(
            enc(Key::Text("i".into()), CTRL),
            b"\x1b[105;5u",
            "Ctrl+I is not Tab"
        );
        assert_eq!(
            enc(Key::Text("A".into()), SHIFT),
            b"A",
            "shifted text is plain text"
        );
        assert_eq!(enc(Key::Text("x".into()), ALT), b"\x1b[120;3u");
        assert_eq!(enc(Key::Up, CTRL), b"\x1b[1;5A");
    }

    #[test]
    fn kitty_report_all_keys() {
        let all = KeyModes {
            kitty_all_keys: true,
            ..KeyModes::default()
        };
        let enc = |key: Key, mods: Mods| encode_key(&key, mods, all).unwrap();
        assert_eq!(enc(Key::Text("a".into()), NONE), b"\x1b[97u");
        assert_eq!(enc(Key::Enter, NONE), b"\x1b[13u");
        assert_eq!(enc(Key::Text("ñ".into()), NONE), "\x1b[241u".as_bytes());
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
