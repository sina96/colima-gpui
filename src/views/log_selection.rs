//! Text selection for the log panel, kept apart from rendering so it can be unit-tested.
//!
//! Positions use *absolute* line numbers (lines ever received, counting the ones the panel
//! has since dropped) and byte columns into the line's displayed text, so a selection stays
//! on the same text while new output streams in and old lines fall off the top.

use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

impl Pos {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

/// `anchor` is where the drag started; `head` follows the pointer or Shift-click.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: Pos,
    pub head: Pos,
}

impl Selection {
    pub fn new(anchor: Pos, head: Pos) -> Self {
        Self { anchor, head }
    }

    pub fn start(&self) -> Pos {
        self.anchor.min(self.head)
    }

    pub fn end(&self) -> Pos {
        self.anchor.max(self.head)
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// The selected byte range of one line whose displayed text is `text`.
    pub fn range_in_line(&self, line: usize, text: &str) -> Option<Range<usize>> {
        let (start, end) = (self.start(), self.end());
        if self.is_empty() || line < start.line || line > end.line {
            return None;
        }
        let from = if line == start.line { start.col } else { 0 };
        let to = if line == end.line {
            end.col
        } else {
            text.len()
        };
        let range = floor_char(text, from)..floor_char(text, to);
        (range.start < range.end).then_some(range)
    }

    /// The selected text. `line_text(n)` returns absolute line `n`'s displayed text, or
    /// `None` once it has been dropped. Lines are joined with `\n`.
    pub fn text<'a>(&self, line_text: impl Fn(usize) -> Option<&'a str>) -> String {
        if self.is_empty() {
            return String::new();
        }
        let (start, end) = (self.start(), self.end());
        (start.line..=end.line)
            .filter_map(|line| {
                let text = line_text(line)?;
                Some(match self.range_in_line(line, text) {
                    Some(range) => &text[range],
                    None => "",
                })
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Trims the part above `first_line` (dropped lines). `None` when nothing is left.
    pub fn clamp_to(self, first_line: usize) -> Option<Self> {
        if self.end().line < first_line {
            return None;
        }
        let clamp = |pos: Pos| {
            if pos.line < first_line {
                Pos::new(first_line, 0)
            } else {
                pos
            }
        };
        Some(Self::new(clamp(self.anchor), clamp(self.head)))
    }
}

/// The word around byte `index`, for double-click. Paths, hosts and IDs count as one word.
pub fn word_at(text: &str, index: usize) -> Range<usize> {
    let is_word = |c: char| c.is_alphanumeric() || "_-./:@".contains(c);
    let index = floor_char(text, index);
    let start = text[..index]
        .char_indices()
        .rev()
        .take_while(|&(_, c)| is_word(c))
        .last()
        .map_or(index, |(i, _)| i);
    let end = text[index..]
        .char_indices()
        .find(|&(_, c)| !is_word(c))
        .map_or(text.len(), |(i, _)| index + i);
    start..end
}

fn floor_char(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINES: [&str; 3] = ["first line", "second", "third line"];

    fn text_of(selection: Selection, first: usize) -> String {
        selection.text(|line| line.checked_sub(first).and_then(|i| LINES.get(i).copied()))
    }

    #[test]
    fn selects_across_lines_in_either_direction() {
        let forward = Selection::new(Pos::new(0, 6), Pos::new(2, 5));
        let backward = Selection::new(Pos::new(2, 5), Pos::new(0, 6));
        assert_eq!(text_of(forward, 0), "line\nsecond\nthird");
        assert_eq!(text_of(backward, 0), "line\nsecond\nthird");
        assert_eq!(forward.range_in_line(1, "second"), Some(0..6));
        assert_eq!(forward.range_in_line(3, "x"), None);
    }

    #[test]
    fn empty_and_single_line_selections() {
        let empty = Selection::new(Pos::new(1, 2), Pos::new(1, 2));
        assert!(empty.is_empty());
        assert_eq!(text_of(empty, 0), "");
        let inner = Selection::new(Pos::new(1, 4), Pos::new(1, 1));
        assert_eq!(text_of(inner, 0), "eco");
    }

    #[test]
    fn columns_past_the_end_or_inside_a_char_are_clamped() {
        let selection = Selection::new(Pos::new(0, 0), Pos::new(0, 99));
        assert_eq!(selection.range_in_line(0, "héllo"), Some(0..6));
        let mid_char = Selection::new(Pos::new(0, 0), Pos::new(0, 2));
        assert_eq!(mid_char.range_in_line(0, "héllo"), Some(0..1));
    }

    #[test]
    fn stays_on_its_text_when_old_lines_are_dropped() {
        // Absolute lines 10..13 hold LINES after 10 lines were dropped.
        let selection = Selection::new(Pos::new(11, 0), Pos::new(12, 5));
        assert_eq!(text_of(selection, 10), "second\nthird");
        let trimmed = selection.clamp_to(12).unwrap();
        assert_eq!(trimmed.start(), Pos::new(12, 0));
        assert_eq!(text_of(trimmed, 10), "third");
        assert_eq!(selection.clamp_to(13), None);
    }

    #[test]
    fn double_click_word_includes_paths_and_hosts() {
        let text = "GET /api/v1/health 200 from 10.0.0.1:443";
        assert_eq!(&text[word_at(text, 6)], "/api/v1/health");
        assert_eq!(&text[word_at(text, 30)], "10.0.0.1:443");
        assert_eq!(&text[word_at(text, 0)], "GET");
        assert_eq!(word_at(text, 3), 0..3);
    }
}
