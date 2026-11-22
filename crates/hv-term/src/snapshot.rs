//! A copy of the visible screen, taken under a short lock so painting never holds it.

use alacritty_terminal::Term;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Rgb};

pub use hv_grid::{Cursor, CursorKind, Rgb8, RowMark, SnapCell, Snapshot};

fn rgb8(c: Rgb) -> Rgb8 {
    Rgb8 {
        r: c.r,
        g: c.g,
        b: c.b,
    }
}

/// Default colors, dark or light (F6-01). Programs can still override them (OSC 4/10/11).
pub(crate) mod palette {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::Rgb8;

    static LIGHT: AtomicBool = AtomicBool::new(false);

    /// Switches every terminal to the light or dark palette.
    pub fn set_light(on: bool) {
        LIGHT.store(on, Ordering::Relaxed);
    }

    fn light() -> bool {
        LIGHT.load(Ordering::Relaxed)
    }

    fn pick(dark: u32, light_hex: u32) -> Rgb8 {
        Rgb8::hex(if light() { light_hex } else { dark })
    }

    pub(crate) fn foreground() -> Rgb8 {
        pick(0xE6E6E6, 0x1F2328)
    }

    pub(crate) fn background() -> Rgb8 {
        pick(0x0A0A0B, 0xFFFFFF)
    }

    pub(crate) fn cursor() -> Rgb8 {
        pick(0xE6E6E6, 0x1F2328)
    }

    pub(crate) fn selection() -> Rgb8 {
        pick(0x264F78, 0xB6D7FF)
    }

    const ANSI: [u32; 16] = [
        0x1A1A1D, 0xF85149, 0x3FB950, 0xD29922, 0x4EA1FF, 0xBC8CFF, 0x39C5CF, 0xC9C9CC, //
        0x6B6B73, 0xFF7B72, 0x56D364, 0xE3B341, 0x79C0FF, 0xD2A8FF, 0x56D4DD, 0xFFFFFF,
    ];

    /// The 16 colors on a white background: darker, for contrast.
    const ANSI_LIGHT: [u32; 16] = [
        0x24292F, 0xCF222E, 0x116329, 0x9A6700, 0x0969DA, 0x8250DF, 0x1B7C83, 0x6E7781, //
        0x57606A, 0xA40E26, 0x1A7F37, 0x633C01, 0x218BFF, 0xA475F9, 0x3192AA, 0x8C959F,
    ];

    /// xterm 256-color palette with our 16 ANSI colors.
    pub(crate) fn indexed(i: u8) -> Rgb8 {
        match i {
            0..=15 => {
                let table = if light() { &ANSI_LIGHT } else { &ANSI };
                Rgb8::hex(table[usize::from(i)])
            }
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
        NamedColor::Foreground | NamedColor::BrightForeground => palette::foreground(),
        NamedColor::Background => palette::background(),
        NamedColor::Cursor => palette::cursor(),
        NamedColor::DimForeground => palette::foreground().dim(),
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
    let selection = content.selection;
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
        selected: false,
        link: 0,
    };
    let mut cells = vec![blank; rows * cols];
    let mut hyperlinks: Vec<String> = Vec::new();

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
        let selected = selection.is_some_and(|range| range.contains(indexed.point));
        let link = cell.hyperlink().map_or(0, |h| {
            let uri = h.uri();
            let index = hyperlinks.iter().position(|u| u == uri).unwrap_or_else(|| {
                hyperlinks.push(uri.to_owned());
                hyperlinks.len() - 1
            });
            u16::try_from(index + 1).unwrap_or(0)
        });
        if selected {
            bg = palette::selection();
        }
        cells[row * cols + col] = SnapCell {
            ch: cell.c,
            fg,
            bg,
            bold: flags.contains(Flags::BOLD),
            italic: flags.contains(Flags::ITALIC),
            underline: flags.intersects(Flags::ALL_UNDERLINES),
            wide_spacer: flags.contains(Flags::WIDE_CHAR_SPACER),
            selected,
            link,
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
        hyperlinks,
        cols,
        rows,
        cells,
        cursor,
        background,
        foreground,
        display_offset,
        marks: Vec::new(),
    }
}
