//! Shell prompt marks (OSC 133, F3-07) anchored to lines of the scrollback.
//!
//! Lines are numbered from the top of the scrollback (`history_size + screen row` when the mark
//! arrived). Known limit: once the scrollback is full the history size stops growing, so lines
//! that fall off the top are not counted and every mark recorded after that point drifts up by
//! the number of lines scrolled since. Marks that point past the end are dropped.

use std::collections::VecDeque;

use hv_grid::RowMark;

/// Most marks kept (a few per command; far more than the scrollback holds).
const MAX_MARKS: usize = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Mark {
    line: usize,
    kind: char,
    exit_code: Option<i32>,
}

/// The marks of one terminal.
#[derive(Debug, Default)]
pub(crate) struct Marks {
    list: VecDeque<Mark>,
}

impl Marks {
    /// Records a mark seen at absolute `line`.
    pub(crate) fn record(&mut self, kind: char, exit_code: Option<i32>, line: usize) {
        if !matches!(kind, 'A' | 'B' | 'C' | 'D') {
            return;
        }
        // Marks arrive in order; a line that moved backwards means the screen was cleared.
        while self.list.back().is_some_and(|m| m.line > line) {
            self.list.pop_back();
        }
        self.list.push_back(Mark {
            line,
            kind,
            exit_code,
        });
        if self.list.len() > MAX_MARKS {
            self.list.pop_front();
        }
    }

    /// Forgets everything (e.g. after the scrollback is cleared).
    pub(crate) fn clear(&mut self) {
        self.list.clear();
    }

    /// Prompt lines (`A` marks) with the exit code of the command that followed.
    fn prompts(&self) -> impl Iterator<Item = (usize, Option<i32>)> + '_ {
        self.list
            .iter()
            .enumerate()
            .filter(|(_, m)| m.kind == 'A')
            .map(|(i, m)| {
                let exit = self
                    .list
                    .iter()
                    .skip(i + 1)
                    .take_while(|n| n.kind != 'A')
                    .find(|n| n.kind == 'D')
                    .and_then(|n| n.exit_code);
                (m.line, exit)
            })
    }

    /// Prompts on the screen whose top line is `top` and that has `rows` rows.
    pub(crate) fn visible(&self, top: usize, rows: usize) -> Vec<RowMark> {
        self.prompts()
            .filter(|(line, _)| *line >= top && *line < top + rows)
            .map(|(line, exit_code)| RowMark {
                row: line - top,
                line,
                exit_code,
            })
            .collect()
    }

    /// Line of the newest prompt.
    pub(crate) fn last_prompt(&self) -> Option<usize> {
        self.list
            .iter()
            .rev()
            .find(|m| m.kind == 'A')
            .map(|m| m.line)
    }

    /// The closest prompt line above `line` (or below with `up = false`).
    pub(crate) fn prompt_near(&self, line: usize, up: bool) -> Option<usize> {
        let mut lines = self.prompts().map(|(l, _)| l);
        if up {
            lines.filter(|l| *l < line).last()
        } else {
            lines.find(|l| *l > line)
        }
    }

    /// Lines of the last command's output: from its `C` mark to its `D` mark (exclusive), or to
    /// `end` if it is still running. `None` if no command ran yet.
    pub(crate) fn last_output(&self, end: usize) -> Option<(usize, usize)> {
        let c = self.list.iter().rposition(|m| m.kind == 'C')?;
        let start = self.list[c].line;
        let stop = self
            .list
            .iter()
            .skip(c + 1)
            .find(|m| matches!(m.kind, 'D' | 'A'))
            .map_or(end, |m| m.line);
        (stop > start).then_some((start, stop))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> Marks {
        let mut m = Marks::default();
        // prompt at 0, `ls` output 1..3, exit 0; prompt at 3, `false`, exit 1; prompt at 5.
        for (kind, exit, line) in [
            ('A', None, 0),
            ('B', None, 0),
            ('C', None, 1),
            ('D', Some(0), 3),
            ('A', None, 3),
            ('C', None, 4),
            ('D', Some(1), 5),
            ('A', None, 5),
        ] {
            m.record(kind, exit, line);
        }
        m
    }

    #[test]
    fn visible_prompts_carry_the_exit_code_of_their_command() {
        let m = session();
        assert_eq!(
            m.visible(0, 10),
            vec![
                RowMark {
                    row: 0,
                    line: 0,
                    exit_code: Some(0)
                },
                RowMark {
                    row: 3,
                    line: 3,
                    exit_code: Some(1)
                },
                RowMark {
                    row: 5,
                    line: 5,
                    exit_code: None
                },
            ]
        );
        assert_eq!(m.visible(3, 2).len(), 1);
    }

    #[test]
    fn jumping_and_last_output() {
        let m = session();
        assert_eq!(m.prompt_near(5, true), Some(3));
        assert_eq!(m.prompt_near(3, true), Some(0));
        assert_eq!(m.prompt_near(0, true), None);
        assert_eq!(m.prompt_near(0, false), Some(3));
        assert_eq!(m.last_output(9), Some((4, 5)));
        assert_eq!(m.last_prompt(), Some(5));
    }

    #[test]
    fn a_cleared_screen_drops_later_marks() {
        let mut m = session();
        m.record('A', None, 1);
        assert_eq!(m.prompt_near(10, true), Some(1));
        assert_eq!(m.prompt_near(1, true), Some(0));
    }
}
