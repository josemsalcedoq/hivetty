//! Plain data shared by the emulator (hv-term) and the renderers (hv-render): a copy of the
//! visible screen. No dependencies, so the renderers never depend on the PTY layer.

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

    /// Two thirds of the brightness (SGR 2, "dim").
    #[must_use]
    pub fn dim(self) -> Self {
        let f = |c: u8| (u16::from(c) * 2 / 3) as u8;
        Self {
            r: f(self.r),
            g: f(self.g),
            b: f(self.b),
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
