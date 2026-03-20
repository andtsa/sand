//! internal types for program debugability

use std::fmt::Display;

use pest::RuleType;
use pest::Span;
use pest::iterators::Pair;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Range {
    pub start: Pos,
    pub end: Pos,
}

impl Pos {
    pub fn line_col(&self) -> (usize, usize) {
        (self.line, self.col)
    }

    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

impl From<(usize, usize)> for Pos {
    fn from(value: (usize, usize)) -> Self {
        Pos {
            line: value.0,
            col: value.1,
        }
    }
}

impl Range {
    pub fn new(start_line: usize, start_col: usize, end_line: usize, end_col: usize) -> Self {
        Self {
            start: Pos::new(start_line, start_col),
            end: Pos::new(end_line, end_col),
        }
    }

    pub fn destruct(&self) -> ((usize, usize), (usize, usize)) {
        (self.start.line_col(), self.end.line_col())
    }
}

impl From<(Pos, Pos)> for Range {
    fn from(value: (Pos, Pos)) -> Self {
        Range {
            start: value.0,
            end: value.1,
        }
    }
}

impl From<Range> for (Pos, Pos) {
    fn from(value: Range) -> Self {
        (value.start, value.end)
    }
}

impl From<&Span<'_>> for Range {
    fn from(value: &Span) -> Self {
        let start = value.start_pos();
        let end = value.end_pos();
        Range {
            start: Pos::from(start.line_col()),
            end: Pos::from(end.line_col()),
        }
    }
}

impl From<Span<'_>> for Range {
    fn from(value: Span) -> Self {
        (&value).into()
    }
}

impl<T: RuleType> From<&Pair<'_, T>> for Range {
    fn from(value: &Pair<T>) -> Self {
        value.as_span().into()
    }
}

impl<T: RuleType> From<Pair<'_, T>> for Range {
    fn from(value: Pair<T>) -> Self {
        (&value).into()
    }
}

impl Display for Range {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Range({}:{} – {}:{})",
            self.start.line, self.start.col, self.end.line, self.end.col
        )
    }
}

impl Display for Pos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}
