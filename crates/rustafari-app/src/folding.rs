//! Which lines of an output can be folded away, and what is left showing.
//!
//! Folding is decided by **indentation**, not by braces. Every format this app
//! produces indents its nesting — pretty-printed JSON and XML, YAML, the diff
//! reports, the cron preview — so one rule covers all of them, and a tool added
//! later gets folding without anyone thinking about it.
//!
//! A line opens a block when the line after it is indented further. Folding it
//! hides everything more-indented that follows, up to the next line at its own
//! depth or shallower.

/// A line of output, and what folding can do with it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Line {
    /// Index into the original text's lines.
    pub number: usize,
    /// Last line of the block this one opens, if it opens one.
    pub block_end: Option<usize>,
}

impl Line {
    pub fn is_foldable(&self) -> bool {
        self.block_end.is_some()
    }
}

/// Indentation width in columns, counting a tab as one level.
fn depth(line: &str) -> usize {
    line.chars().take_while(|c| *c == ' ' || *c == '\t').count()
}

/// The block each line opens, indexed by line.
///
/// Blank lines belong to whatever block surrounds them: a blank line between
/// two nested entries should not end the fold.
pub fn blocks(text: &str) -> Vec<Option<usize>> {
    let lines: Vec<&str> = text.lines().collect();
    let depths: Vec<Option<usize>> = lines
        .iter()
        .map(|l| (!l.trim().is_empty()).then(|| depth(l)))
        .collect();

    lines
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let own = depths[i]?;
            // Walk forward while lines are deeper, remembering the last one
            // that actually had content.
            let mut end = None;
            for (j, deeper) in depths.iter().enumerate().skip(i + 1) {
                match deeper {
                    Some(d) if *d > own => end = Some(j),
                    Some(_) => break,
                    // Blank: might be inside the block, might be after it.
                    None => continue,
                }
            }
            end
        })
        .collect()
}

/// The lines to display, given which blocks are folded.
///
/// Folded lines are dropped from the result; the line that opens a fold stays,
/// so there is always something to click to unfold it.
pub fn visible(text: &str, folded: &[usize]) -> Vec<Line> {
    let blocks = blocks(text);
    let mut out = Vec::with_capacity(blocks.len());
    let mut skip_until: Option<usize> = None;

    for (number, block_end) in blocks.iter().enumerate() {
        if let Some(end) = skip_until {
            if number <= end {
                continue;
            }
            skip_until = None;
        }

        out.push(Line {
            number,
            block_end: *block_end,
        });

        if let Some(end) = block_end {
            if folded.contains(&number) {
                skip_until = Some(*end);
            }
        }
    }
    out
}

/// The text actually rendered, with folded blocks replaced by a marker on the
/// line that opens them.
pub fn render(text: &str, visible: &[Line], folded: &[usize]) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = String::with_capacity(text.len());

    for (i, line) in visible.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let source = lines.get(line.number).copied().unwrap_or_default();
        out.push_str(source);
        if line.is_foldable() && folded.contains(&line.number) {
            let hidden = line.block_end.unwrap_or(line.number) - line.number;
            out.push_str(&format!(" … {hidden} lines"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = "{\n  \"a\": [\n    1,\n    2\n  ],\n  \"b\": 3\n}";

    fn folded_numbers(text: &str, folded: &[usize]) -> Vec<usize> {
        visible(text, folded).iter().map(|l| l.number).collect()
    }

    #[test]
    fn a_line_followed_by_a_deeper_one_opens_a_block() {
        let b = blocks(JSON);
        assert_eq!(b[0], Some(5), "the opening brace covers everything inside");
        assert_eq!(b[1], Some(3), "the array covers its two entries");
        assert_eq!(b[2], None, "a leaf opens nothing");
        assert_eq!(b[5], None);
    }

    #[test]
    fn nothing_is_hidden_until_something_is_folded() {
        assert_eq!(folded_numbers(JSON, &[]), vec![0, 1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn folding_hides_the_block_but_keeps_its_opening_line() {
        // Fold the array at line 1: lines 2 and 3 go, the rest stays.
        assert_eq!(folded_numbers(JSON, &[1]), vec![0, 1, 4, 5, 6]);
    }

    #[test]
    fn folding_the_outermost_block_hides_everything_within() {
        assert_eq!(folded_numbers(JSON, &[0]), vec![0, 6]);
    }

    #[test]
    fn a_fold_inside_a_folded_block_changes_nothing_visible() {
        // The outer fold already hides it; the inner one must not double-skip.
        assert_eq!(folded_numbers(JSON, &[0, 1]), vec![0, 6]);
    }

    #[test]
    fn the_folded_line_says_how_much_it_is_hiding() {
        let v = visible(JSON, &[1]);
        let text = render(JSON, &v, &[1]);
        assert!(text.contains("\"a\": [ … 2 lines"), "{text}");
        assert!(
            !text.contains('1'),
            "the hidden entries must be gone:\n{text}"
        );
    }

    #[test]
    fn unfolded_text_renders_back_exactly() {
        let v = visible(JSON, &[]);
        assert_eq!(render(JSON, &v, &[]), JSON);
    }

    #[test]
    fn a_blank_line_does_not_end_a_block() {
        // A diff report has blank lines between sections.
        let text = "root:\n  a: 1\n\n  b: 2\nnext: 3";
        assert_eq!(
            blocks(text)[0],
            Some(3),
            "the blank line is inside the block"
        );
        assert_eq!(folded_numbers(text, &[0]), vec![0, 4]);
    }

    #[test]
    fn tabs_count_as_indentation() {
        let text = "a\n\tb\nc";
        assert_eq!(blocks(text)[0], Some(1));
    }

    #[test]
    fn flat_text_has_nothing_to_fold() {
        let text = "one\ntwo\nthree";
        assert!(blocks(text).iter().all(Option::is_none));
        assert_eq!(folded_numbers(text, &[0, 1, 2]), vec![0, 1, 2]);
    }
}
