//! Built-in box drawing and block elements (task F1-10).
//!
//! Fonts rarely line up box-drawing characters exactly with the cell grid, so lines get gaps
//! between cells (very visible in agent TUIs that frame their prompt with `╭─╮`). Like Ghostty,
//! WezTerm and kitty, these characters are drawn as rectangles that fill the cell edge to edge.
//!
//! Each line character is described by its four arms (left, right, up, down) and their weight.

use hv_grid::Rgb8;

use crate::rects::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum W {
    /// No arm.
    N,
    /// Light line.
    L,
    /// Heavy line.
    H,
    /// Double line.
    D,
}

/// Arms (left, right, up, down) of a box-drawing character.
fn arms(c: char) -> Option<[W; 4]> {
    use W::{D, H, L, N};
    Some(match c {
        // Light / heavy straight lines, including dashed variants drawn solid.
        '─' | '┄' | '┈' | '╌' => [L, L, N, N],
        '━' | '┅' | '┉' | '╍' => [H, H, N, N],
        '│' | '┆' | '┊' | '╎' => [N, N, L, L],
        '┃' | '┇' | '┋' | '╏' => [N, N, H, H],
        // Corners (rounded corners are drawn square for now).
        '┌' | '╭' => [N, L, N, L],
        '┐' | '╮' => [L, N, N, L],
        '└' | '╰' => [N, L, L, N],
        '┘' | '╯' => [L, N, L, N],
        '┏' => [N, H, N, H],
        '┓' => [H, N, N, H],
        '┗' => [N, H, H, N],
        '┛' => [H, N, H, N],
        // Tees and crosses.
        '├' => [N, L, L, L],
        '┤' => [L, N, L, L],
        '┬' => [L, L, N, L],
        '┴' => [L, L, L, N],
        '┼' => [L, L, L, L],
        '┣' => [N, H, H, H],
        '┫' => [H, N, H, H],
        '┳' => [H, H, N, H],
        '┻' => [H, H, H, N],
        '╋' => [H, H, H, H],
        // Double lines.
        '═' => [D, D, N, N],
        '║' => [N, N, D, D],
        '╔' => [N, D, N, D],
        '╗' => [D, N, N, D],
        '╚' => [N, D, D, N],
        '╝' => [D, N, D, N],
        '╠' => [N, D, D, D],
        '╣' => [D, N, D, D],
        '╦' => [D, D, N, D],
        '╩' => [D, D, D, N],
        '╬' => [D, D, D, D],
        // Half lines.
        '╴' => [L, N, N, N],
        '╵' => [N, N, L, N],
        '╶' => [N, L, N, N],
        '╷' => [N, N, N, L],
        _ => return None,
    })
}

/// Whether `c` is drawn by [`rects_for`] instead of the font.
pub(crate) fn is_builtin(c: char) -> bool {
    arms(c).is_some() || block(c).is_some()
}

/// Block elements as fractions of the cell: (x0, y0, x1, y1, alpha).
fn block(c: char) -> Option<(f32, f32, f32, f32, f32)> {
    Some(match c {
        '█' => (0.0, 0.0, 1.0, 1.0, 1.0),
        '▀' => (0.0, 0.0, 1.0, 0.5, 1.0),
        '▄' => (0.0, 0.5, 1.0, 1.0, 1.0),
        '▌' => (0.0, 0.0, 0.5, 1.0, 1.0),
        '▐' => (0.5, 0.0, 1.0, 1.0, 1.0),
        '▁' => (0.0, 7.0 / 8.0, 1.0, 1.0, 1.0),
        '▂' => (0.0, 6.0 / 8.0, 1.0, 1.0, 1.0),
        '▃' => (0.0, 5.0 / 8.0, 1.0, 1.0, 1.0),
        '▅' => (0.0, 3.0 / 8.0, 1.0, 1.0, 1.0),
        '▆' => (0.0, 2.0 / 8.0, 1.0, 1.0, 1.0),
        '▇' => (0.0, 1.0 / 8.0, 1.0, 1.0, 1.0),
        '▏' => (0.0, 0.0, 1.0 / 8.0, 1.0, 1.0),
        '▎' => (0.0, 0.0, 2.0 / 8.0, 1.0, 1.0),
        '▍' => (0.0, 0.0, 3.0 / 8.0, 1.0, 1.0),
        '▋' => (0.0, 0.0, 5.0 / 8.0, 1.0, 1.0),
        '▊' => (0.0, 0.0, 6.0 / 8.0, 1.0, 1.0),
        '▉' => (0.0, 0.0, 7.0 / 8.0, 1.0, 1.0),
        '▔' => (0.0, 0.0, 1.0, 1.0 / 8.0, 1.0),
        '▕' => (7.0 / 8.0, 0.0, 1.0, 1.0, 1.0),
        '░' => (0.0, 0.0, 1.0, 1.0, 0.25),
        '▒' => (0.0, 0.0, 1.0, 1.0, 0.5),
        '▓' => (0.0, 0.0, 1.0, 1.0, 0.75),
        _ => return None,
    })
}

/// Rectangles that draw `c` in the cell at (`x`, `y`) of size `w` x `h`. `scale` is the DPI
/// factor (line thickness grows with it).
pub(crate) fn rects_for(
    c: char,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: Rgb8,
    scale: f32,
) -> Vec<Rect> {
    if let Some((x0, y0, x1, y1, alpha)) = block(c) {
        let px = |v: f32, size: f32| (v * size).round();
        return vec![Rect::with_alpha(
            x + px(x0, w),
            y + px(y0, h),
            px(x1, w) - px(x0, w),
            px(y1, h) - px(y0, h),
            color,
            alpha,
        )];
    }
    let Some([left, right, up, down]) = arms(c) else {
        return Vec::new();
    };
    let light = scale.round().max(1.0);
    let thickness = |weight: W| if weight == W::H { light * 2.0 } else { light };
    // Snap the centre to whole pixels so lines are crisp and line up across cells.
    let cx = x + ((w - light) / 2.0).round();
    let cy = y + ((h - light) / 2.0).round();
    let gap = light * 2.0; // distance between the two strokes of a double line

    let mut out = Vec::new();
    let mut horizontal = |from: f32, to: f32, weight: W| {
        let t = thickness(weight);
        let offsets: &[f32] = if weight == W::D {
            &[-gap / 2.0 - light / 2.0, gap / 2.0 + light / 2.0]
        } else {
            &[0.0]
        };
        for off in offsets {
            let top = (cy + light / 2.0 - t / 2.0 + off).round();
            out.push(Rect::new(from, top, to - from, t, color));
        }
    };
    if left != W::N {
        horizontal(x, cx + thickness(left.max_with(up, down)), left);
    }
    if right != W::N {
        horizontal(cx, x + w, right);
    }
    let mut vertical = |from: f32, to: f32, weight: W| {
        let t = thickness(weight);
        let offsets: &[f32] = if weight == W::D {
            &[-gap / 2.0 - light / 2.0, gap / 2.0 + light / 2.0]
        } else {
            &[0.0]
        };
        for off in offsets {
            let l = (cx + light / 2.0 - t / 2.0 + off).round();
            out.push(Rect::new(l, from, t, to - from, color));
        }
    };
    if up != W::N {
        vertical(y, cy + thickness(up.max_with(left, right)), up);
    }
    if down != W::N {
        vertical(cy, y + h, down);
    }
    out
}

impl W {
    /// The heavier of `self` and the two crossing arms, so joints have no notch.
    fn max_with(self, a: W, b: W) -> W {
        [self, a, b]
            .into_iter()
            .find(|w| *w == W::H)
            .unwrap_or(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: Rgb8 = Rgb8::hex(0xFFFFFF);

    #[test]
    fn horizontal_line_spans_the_whole_cell() {
        let rects = rects_for('─', 10.0, 20.0, 8.0, 16.0, WHITE, 1.0);
        assert_eq!(rects.len(), 2, "left and right arms");
        let (min_x, max_x) = rects.iter().fold((f32::MAX, f32::MIN), |(lo, hi), r| {
            (lo.min(r.x()), hi.max(r.x() + r.w()))
        });
        assert_eq!((min_x, max_x), (10.0, 18.0), "no gap at either edge");
    }

    #[test]
    fn cross_has_four_arms_and_double_line_two_strokes() {
        assert_eq!(rects_for('┼', 0.0, 0.0, 8.0, 16.0, WHITE, 1.0).len(), 4);
        assert_eq!(rects_for('═', 0.0, 0.0, 8.0, 16.0, WHITE, 1.0).len(), 4);
    }

    #[test]
    fn blocks_and_shades() {
        let full = rects_for('█', 0.0, 0.0, 8.0, 16.0, WHITE, 1.0);
        assert_eq!((full[0].w(), full[0].h()), (8.0, 16.0));
        let upper = rects_for('▀', 0.0, 0.0, 8.0, 16.0, WHITE, 1.0);
        assert_eq!(upper[0].h(), 8.0);
        assert!(is_builtin('▒') && is_builtin('╭') && !is_builtin('a'));
    }
}
