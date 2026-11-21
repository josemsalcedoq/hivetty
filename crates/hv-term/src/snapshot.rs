//! A copy of the visible screen, taken under a short lock so painting never holds it.

use alacritty_terminal::Term;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Rgb};

pub use hv_grid::{Cursor, CursorKind, Rgb8, SnapCell, Snapshot};

fn rgb8(c: Rgb) -> Rgb8 {
    Rgb8 {
        r: c.r,
        g: c.g,
        b: c.b,
    }
}

/// Default colors (dark theme). Replaced by the theme system in phase F6.
pub(crate) mod palette {
    use super::Rgb8;

    pub(crate) const FOREGROUND: Rgb8 = Rgb8::hex(0xE6E6E6);
    pub(crate) const BACKGROUND: Rgb8 = Rgb8::hex(0x0A0A0B);
    pub(crate) const CURSOR: Rgb8 = Rgb8::hex(0xE6E6E6);

    const ANSI: [u32; 16] = [
        0x1A1A1D, 0xF85149, 0x3FB950, 0xD29922, 0x4EA1FF, 0xBC8CFF, 0x39C5CF, 0xC9C9CC, //
        0x6B6B73, 0xFF7B72, 0x56D364, 0xE3B341, 0x79C0FF, 0xD2A8FF, 0x56D4DD, 0xFFFFFF,
    ];

    /// xterm 256-color palette with our 16 ANSI colors.
    pub(crate) fn indexed(i: u8) -> Rgb8 {
        match i {
            0..=15 => Rgb8::hex(ANSI[usize::from(i)]),
            16..=231 => {
                let i = i - 16;
                let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
                Rgb8 {
                    r: level(i / 36),
                    g: level((i / 6) % 6),
                    b: level(i % 6),
                }
            }
            232..=255 => {
                let v = 8 + (i - 232) * 10;
                Rgb8 { r: v, g: v, b: v }
            }
        }
    }
}

fn resolve(color: Color, colors: &Colors) -> Rgb8 {
    match color {
        Color::Spec(rgb) => rgb8(rgb),
        Color::Indexed(i) => colors[usize::from(i)].map_or_else(|| palette::indexed(i), rgb8),
        Color::Named(named) => resolve_named(named, colors),
    }
}

fn resolve_named(named: NamedColor, colors: &Colors) -> Rgb8 {
    if let Some(rgb) = colors[named as usize] {
        return rgb8(rgb);
    }
    match named {
        NamedColor::Foreground | NamedColor::BrightForeground => palette::FOREGROUND,
        NamedColor::Background => palette::BACKGROUND,
        NamedColor::Cursor => palette::CURSOR,
        NamedColor::DimForeground => palette::FOREGROUND.dim(),
        NamedColor::DimBlack
        | NamedColor::DimRed
        | NamedColor::DimGreen
        | NamedColor::DimYellow
        | NamedColor::DimBlue
        | NamedColor::DimMagenta
        | NamedColor::DimCyan
        | NamedColor::DimWhite => {
            let base = named as usize - NamedColor::DimBlack as usize;
            palette::indexed(u8::try_from(base).unwrap_or(0)).dim()
        }
        other => palette::indexed(u8::try_from(other as usize).unwrap_or(7)),
    }
}

/// Copies the visible screen of `term`.
pub(crate) fn take<T: alacritty_terminal::event::EventListener>(term: &Term<T>) -> Snapshot {
    let content = term.renderable_content();
    let colors = content.colors;
    let grid = term.grid();
    let cols = grid.columns();
    let rows = grid.screen_lines();
    let display_offset = content.display_offset;
    let foreground = resolve_named(NamedColor::Foreground, colors);
    let background = resolve_named(NamedColor::Background, colors);

    let blank = SnapCell {
        ch: ' ',
        fg: foreground,
        bg: background,
        bold: false,
        italic: false,
        underline: false,
        wide_spacer: false,
    };
    let mut cells = vec![blank; rows * cols];

    for indexed in content.display_iter {
        let Ok(row) = usize::try_from(indexed.point.line.0 + display_offset as i32) else {
            continue;
        };
        let col = indexed.point.column.0;
        if row >= rows || col >= cols {
            continue;
        }
        let cell = indexed.cell;
        let flags = cell.flags;
        let mut fg = resolve(cell.fg, colors);
        let mut bg = resolve(cell.bg, colors);
        if flags.contains(Flags::DIM) {
            fg = fg.dim();
        }
        if flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        if flags.contains(Flags::HIDDEN) {
            fg = bg;
        }
        cells[row * cols + col] = SnapCell {
            ch: cell.c,
            fg,
            bg,
            bold: flags.contains(Flags::BOLD),
            italic: flags.contains(Flags::ITALIC),
            underline: flags.intersects(Flags::ALL_UNDERLINES),
            wide_spacer: flags.contains(Flags::WIDE_CHAR_SPACER),
        };
    }

    let cursor_point = content.cursor.point;
    let cursor_row = usize::try_from(cursor_point.line.0 + display_offset as i32).ok();
    let cursor = match (content.cursor.shape, cursor_row) {
        (CursorShape::Hidden, _) | (_, None) => None,
        _ if !content.mode.contains(TermMode::SHOW_CURSOR) => None,
        (shape, Some(row)) if row < rows => Some(Cursor {
            row,
            col: cursor_point.column.0.min(cols.saturating_sub(1)),
            shape: match shape {
                CursorShape::Beam => CursorKind::Beam,
                CursorShape::Underline => CursorKind::Underline,
                _ => CursorKind::Block,
            },
        }),
        _ => None,
    };

    Snapshot {
        cols,
        rows,
        cells,
        cursor,
        background,
        foreground,
        display_offset,
    }
}
