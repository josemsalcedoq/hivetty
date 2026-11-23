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
    /// Part of the mouse selection (its background is already the selection color).
    pub selected: bool,
    /// OSC 8 hyperlink: 0 = none, otherwise index + 1 into [`Snapshot::hyperlinks`].
    pub link: u16,
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
    /// OSC 8 hyperlink targets referenced by [`SnapCell::link`].
    pub hyperlinks: Vec<String>,
    /// Shell prompts (OSC 133) on visible rows, for the margin marks.
    pub marks: Vec<RowMark>,
}

/// A shell prompt on a visible row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowMark {
    /// Visible row of the prompt.
    pub row: usize,
    /// Line of the prompt counted from the top of the scrollback (stable while scrolling).
    pub line: usize,
    /// Exit code of the command run from this prompt; `None` while it runs or if unknown.
    pub exit_code: Option<i32>,
}

impl Snapshot {
    /// Cell at `(row, col)`.
    pub fn cell(&self, row: usize, col: usize) -> &SnapCell {
        &self.cells[row * self.cols + col]
    }

    /// OSC 8 hyperlink of a cell, if any.
    pub fn hyperlink(&self, row: usize, col: usize) -> Option<&str> {
        let link = self.cell(row, col).link;
        (link > 0)
            .then(|| {
                self.hyperlinks
                    .get(usize::from(link) - 1)
                    .map(String::as_str)
            })
            .flatten()
    }

    /// One character per cell for a row (wide-character spacers become `'\0'`), for link
    /// detection.
    pub fn row_chars(&self, row: usize) -> Vec<char> {
        self.cells[row * self.cols..(row + 1) * self.cols]
            .iter()
            .map(|c| if c.wide_spacer { '\0' } else { c.ch })
            .collect()
    }

    /// The visible text, one line per row, without trailing spaces or trailing blank lines
    /// (what screen readers get, F6-05).
    pub fn text(&self) -> String {
        let mut lines: Vec<String> = (0..self.rows).map(|r| self.row_text(r)).collect();
        while lines.last().is_some_and(String::is_empty) {
            lines.pop();
        }
        lines.join("\n")
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
