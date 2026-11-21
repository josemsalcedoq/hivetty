//! A copy of the visible screen, taken under a short lock so painting never holds it.

use alacritty_terminal::Term;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::TermMode;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Rgb};

/// An sRGB color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb8 {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
}

impl Rgb8 {
    /// Builds a color from a `0xRRGGBB` value.
    pub const fn hex(v: u32) -> Self {
        Self {
            r: (v >> 16) as u8,
            g: (v >> 8) as u8,
            b: v as u8,
        }
    }

    fn dim(self) -> Self {
        let f = |c: u8| (u16::from(c) * 2 / 3) as u8;
        Self {
            r: f(self.r),
            g: f(self.g),
            b: f(self.b),
        }
    }
}

impl From<Rgb> for Rgb8 {
    fn from(c: Rgb) -> Self {
        Self {
            r: c.r,
            g: c.g,
            b: c.b,
        }
    }
}

/// One cell of the snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct SnapCell {
    /// Character (space for empty cells).
    pub ch: char,
    /// Resolved foreground.
    pub fg: Rgb8,
    /// Resolved background.
    pub bg: Rgb8,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Any underline style.
    pub underline: bool,
    /// Placeholder cell to the right of a wide character (draw nothing).
    pub wide_spacer: bool,
}

/// Cursor position and shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    /// Row from the top of the screen.
    pub row: usize,
    /// Column.
    pub col: usize,
    /// Block, beam or underline.
    pub shape: CursorKind,
}

/// Cursor shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorKind {
    /// Filled block.
    Block,
    /// Vertical bar.
    Beam,
    /// Underline.
    Underline,
}

/// The visible screen.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    /// Columns.
    pub cols: usize,
    /// Rows.
    pub rows: usize,
    /// `rows * cols` cells, row-major.
    pub cells: Vec<SnapCell>,
    /// Cursor, if visible.
    pub cursor: Option<Cursor>,
    /// Default background (for padding and clearing).
    pub background: Rgb8,
    /// Default foreground.
    pub foreground: Rgb8,
    /// How many lines the view is scrolled up into the scrollback (0 = at the bottom).
    pub display_offset: usize,
}

impl Snapshot {
    /// Cell at `(row, col)`.
    pub fn cell(&self, row: usize, col: usize) -> &SnapCell {
        &self.cells[row * self.cols + col]
    }

    /// The text of one row, without trailing spaces (handy for tests).
    pub fn row_text(&self, row: usize) -> String {
        let line: String = self.cells[row * self.cols..(row + 1) * self.cols]
            .iter()
            .filter(|c| !c.wide_spacer)
            .map(|c| c.ch)
            .collect();
        line.trim_end().to_owned()
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
        Color::Spec(rgb) => rgb.into(),
        Color::Indexed(i) => colors[usize::from(i)].map_or_else(|| palette::indexed(i), Into::into),
        Color::Named(named) => resolve_named(named, colors),
    }
}

fn resolve_named(named: NamedColor, colors: &Colors) -> Rgb8 {
    if let Some(rgb) = colors[named as usize] {
        return rgb.into();
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
