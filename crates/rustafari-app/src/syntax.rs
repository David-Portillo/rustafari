//! Syntax highlighting for the panes: which byte of the text is what.
//!
//! Pure and testable, like `folding`. It knows nothing about colour — a
//! [`Token`] names a *role* and `theme` decides what that role looks like, so
//! the two themes cannot drift apart and no literal colour appears here.
//!
//! The spans a lexer returns **tile the text exactly**: contiguous, in order,
//! covering every byte. egui builds a `LayoutJob` from them, and a gap would
//! silently drop that text from the display. `tiles_exactly` checks it.
//!
//! These are lexers, not parsers. They never fail: invalid input still
//! produces spans, because the input is highlighted *as it is typed* and is
//! therefore invalid most of the time.

use std::ops::Range;

/// What a run of bytes is, for colouring.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Token {
    /// Anything with no special meaning, and whitespace.
    Plain,
    /// An object key, an XML attribute name, a YAML mapping key.
    Key,
    Str,
    Number,
    /// `true`, `false`, `null`, YAML anchors and aliases.
    Keyword,
    Comment,
    /// An XML element name.
    Tag,
    /// Separators that are not brackets: `,` `:` `=` `-`.
    Punct,
    /// A bracket, carrying its nesting depth so it can be rainbow-coloured.
    /// The depth is of the bracket itself, so a pair matches in colour.
    Bracket(usize),
}

/// The languages worth highlighting: exactly the structured formats the tools
/// read and write. Anything else is left plain, which is the honest answer —
/// colouring a Base64 blob or a hash would only invent structure.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Language {
    Json,
    Yaml,
    Xml,
}

/// A run of bytes and what it is.
pub type Span = (Range<usize>, Token);

/// Lexes `text`, returning spans that tile it exactly.
pub fn spans(text: &str, language: Language) -> Vec<Span> {
    let mut out = Spans::new(text.len());
    match language {
        Language::Json => json(text, &mut out),
        Language::Yaml => yaml(text, &mut out),
        Language::Xml => xml(text, &mut out),
    }
    out.finish()
}

/// Collects spans while guaranteeing they tile the text.
///
/// A lexer only reports the runs it finds interesting; everything it skips is
/// filled in as `Plain` here. That is what keeps "cover every byte" from being
/// each lexer's problem, and it is why they can be written as a scan that only
/// handles the cases it recognises.
struct Spans {
    out: Vec<Span>,
    covered: usize,
    len: usize,
}

impl Spans {
    fn new(len: usize) -> Self {
        Self {
            out: Vec::new(),
            covered: 0,
            len,
        }
    }

    fn push(&mut self, range: Range<usize>, token: Token) {
        if range.start > self.covered {
            let gap = self.covered..range.start;
            self.merge(gap, Token::Plain);
        }
        self.covered = range.end;
        self.merge(range, token);
    }

    /// Extends the previous span when it has the same token, so a document of
    /// mostly plain text does not become one span per character.
    fn merge(&mut self, range: Range<usize>, token: Token) {
        if range.is_empty() {
            return;
        }
        match self.out.last_mut() {
            Some((last, kind)) if *kind == token && last.end == range.start => {
                last.end = range.end;
            }
            _ => self.out.push((range, token)),
        }
    }

    fn finish(mut self) -> Vec<Span> {
        if self.covered < self.len {
            let tail = self.covered..self.len;
            self.merge(tail, Token::Plain);
        }
        self.out
    }
}

/// Scans a quoted string from `start`, returning the byte after its closing
/// quote. Handles backslash escapes; an unterminated string runs to the end,
/// which is what a half-typed one should do.
fn quoted(bytes: &[u8], start: usize) -> usize {
    let quote = bytes[start];
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b if b == quote => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// The byte after a run of `-+.0-9eE` starting at `start`.
fn number(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < bytes.len() && matches!(bytes[i], b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E') {
        i += 1;
    }
    i
}

/// The byte after a run of identifier characters starting at `start`.
fn word(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'_' | b'-')) {
        i += 1;
    }
    i
}

/// The next non-space byte at or after `i`.
fn skip_spaces(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn json(text: &str, out: &mut Spans) {
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut depth: usize = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let end = quoted(bytes, i);
                // A string is a key when a colon follows it. Cheaper and more
                // robust than tracking whether we are inside an object: a
                // half-typed document has no reliable structure to track.
                let token = if bytes.get(skip_spaces(bytes, end)) == Some(&b':') {
                    Token::Key
                } else {
                    Token::Str
                };
                out.push(i..end, token);
                i = end;
            }
            b'{' | b'[' => {
                out.push(i..i + 1, Token::Bracket(depth));
                depth += 1;
                i += 1;
            }
            b'}' | b']' => {
                // Saturating, so a stray closer in half-typed input colours
                // as depth 0 rather than wrapping to a huge number.
                depth = depth.saturating_sub(1);
                out.push(i..i + 1, Token::Bracket(depth));
                i += 1;
            }
            b',' | b':' => {
                out.push(i..i + 1, Token::Punct);
                i += 1;
            }
            b'-' | b'0'..=b'9' => {
                let end = number(bytes, i);
                out.push(i..end, Token::Number);
                i = end;
            }
            b if b.is_ascii_alphabetic() => {
                let end = word(bytes, i);
                // true / false / null, and anything else a user has typed
                // that is not yet valid JSON.
                out.push(i..end, Token::Keyword);
                i = end;
            }
            _ => i += 1,
        }
    }
}

fn yaml(text: &str, out: &mut Spans) {
    let bytes = text.as_bytes();
    let mut line_start = 0;

    while line_start < bytes.len() {
        let line_end = text[line_start..]
            .find('\n')
            .map_or(bytes.len(), |n| line_start + n);
        yaml_line(bytes, line_start, line_end, out);
        line_start = line_end + 1;
    }
}

/// YAML is line-oriented, and that is what makes a lexer enough: a key is a
/// word before a colon at the start of a line, and everything after the colon
/// is a value.
fn yaml_line(bytes: &[u8], start: usize, end: usize, out: &mut Spans) {
    let mut i = skip_spaces_in_line(bytes, start, end);

    // A comment runs to the end of the line wherever it starts.
    if i < end && bytes[i] == b'#' {
        out.push(i..end, Token::Comment);
        return;
    }

    // Sequence markers, which may stack: "- - value".
    while i < end && bytes[i] == b'-' && bytes.get(i + 1).is_none_or(|b| *b == b' ') {
        out.push(i..i + 1, Token::Punct);
        i = skip_spaces_in_line(bytes, i + 1, end);
    }

    // A key: a run up to a colon that ends the word.
    if i < end && bytes[i] != b'#' {
        let key_end = yaml_key_end(bytes, i, end);
        if let Some(colon) = key_end {
            out.push(i..colon, Token::Key);
            out.push(colon..colon + 1, Token::Punct);
            i = colon + 1;
        }
    }

    yaml_value(bytes, i, end, out);
}

/// The offset of the colon ending a mapping key on this line, if there is one.
/// The colon must be followed by a space or end the line — `http://x` is not a
/// key, and treating it as one is the obvious way to get this wrong.
fn yaml_key_end(bytes: &[u8], start: usize, end: usize) -> Option<usize> {
    let mut i = start;
    // A quoted key is a string up to its closing quote.
    if matches!(bytes[i], b'"' | b'\'') {
        i = quoted(bytes, i).min(end);
    }
    while i < end {
        match bytes[i] {
            b':' if bytes.get(i + 1).is_none_or(|b| *b == b' ' || *b == b'\n') || i + 1 >= end => {
                return Some(i);
            }
            b'#' => return None,
            _ => i += 1,
        }
    }
    None
}

fn yaml_value(bytes: &[u8], start: usize, end: usize, out: &mut Spans) {
    let mut i = skip_spaces_in_line(bytes, start, end);
    while i < end {
        match bytes[i] {
            b'#' => {
                out.push(i..end, Token::Comment);
                return;
            }
            b'"' | b'\'' => {
                let stop = quoted(bytes, i).min(end);
                out.push(i..stop, Token::Str);
                i = stop;
            }
            // Anchors, aliases and merge keys — the things that make YAML
            // more than indented JSON, so worth being visible.
            b'&' | b'*' => {
                let stop = word(bytes, i + 1).min(end);
                out.push(i..stop, Token::Keyword);
                i = stop;
            }
            b'{' | b'}' | b'[' | b']' => {
                out.push(i..i + 1, Token::Bracket(0));
                i += 1;
            }
            b',' => {
                out.push(i..i + 1, Token::Punct);
                i += 1;
            }
            b'0'..=b'9' => {
                let stop = number(bytes, i).min(end);
                out.push(i..stop, Token::Number);
                i = stop;
            }
            b if b.is_ascii_alphabetic() => {
                let stop = word(bytes, i).min(end);
                let text = &bytes[i..stop];
                let token = match text {
                    b"true" | b"false" | b"null" | b"yes" | b"no" | b"on" | b"off" | b"~" => {
                        Token::Keyword
                    }
                    _ => Token::Plain,
                };
                out.push(i..stop, token);
                i = stop;
            }
            _ => i += 1,
        }
    }
}

fn skip_spaces_in_line(bytes: &[u8], mut i: usize, end: usize) -> usize {
    while i < end && (bytes[i] == b' ' || bytes[i] == b'\t') {
        i += 1;
    }
    i
}

fn xml(text: &str, out: &mut Spans) {
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut depth: usize = 0;

    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }

        // Comments and declarations, which run to their own terminator and
        // may contain anything at all — including '>'.
        if text[i..].starts_with("<!--") {
            let end = text[i..].find("-->").map_or(bytes.len(), |n| i + n + 3);
            out.push(i..end, Token::Comment);
            i = end;
            continue;
        }
        if text[i..].starts_with("<?") || text[i..].starts_with("<!") {
            let end = text[i..].find('>').map_or(bytes.len(), |n| i + n + 1);
            out.push(i..end, Token::Comment);
            i = end;
            continue;
        }

        let closing = bytes.get(i + 1) == Some(&b'/');
        if closing {
            depth = depth.saturating_sub(1);
        }
        out.push(i..i + 1, Token::Bracket(depth));
        i += 1;
        if closing {
            out.push(i..i + 1, Token::Punct);
            i += 1;
        }

        // The element name, then attributes until the tag closes.
        let name_end = xml_name_end(bytes, i);
        out.push(i..name_end, Token::Tag);
        i = name_end;

        let mut self_closing = false;
        while i < bytes.len() && bytes[i] != b'>' {
            match bytes[i] {
                b'"' | b'\'' => {
                    let end = quoted(bytes, i);
                    out.push(i..end, Token::Str);
                    i = end;
                }
                b'=' => {
                    out.push(i..i + 1, Token::Punct);
                    i += 1;
                }
                b'/' => {
                    self_closing = true;
                    out.push(i..i + 1, Token::Punct);
                    i += 1;
                }
                b if b.is_ascii_alphabetic() || b == b'_' => {
                    let end = xml_name_end(bytes, i);
                    out.push(i..end, Token::Key);
                    i = end;
                }
                _ => i += 1,
            }
        }

        if i < bytes.len() {
            out.push(i..i + 1, Token::Bracket(depth));
            i += 1;
        }
        if !closing && !self_closing {
            depth += 1;
        }
    }
}

/// The byte after an element or attribute name, which may be namespaced.
fn xml_name_end(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < bytes.len()
        && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'_' | b'-' | b'.' | b':'))
    {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The invariant everything else depends on. A gap would drop text from
    /// the display entirely, and an overlap makes egui panic building the job.
    fn tiles_exactly(text: &str, language: Language) {
        let spans = spans(text, language);
        let mut at = 0;
        for (range, _) in &spans {
            assert_eq!(range.start, at, "gap or overlap in {language:?}: {spans:?}");
            assert!(range.end > range.start, "empty span in {language:?}");
            at = range.end;
        }
        assert_eq!(at, text.len(), "spans stop short in {language:?}");
    }

    #[test]
    fn spans_tile_every_language_including_broken_input() {
        let samples: &[(Language, &str)] = &[
            (Language::Json, r#"{"a": [1, 2.5e3, true, null], "b": "x"}"#),
            // Half-typed, which is what the editor sees on most keystrokes.
            (Language::Json, r#"{"a": [1, "unterminated"#),
            (Language::Json, "}}}]"),
            (Language::Json, ""),
            (Language::Yaml, "# note\na: 1\nb:\n  - &x one\n  - *x\n"),
            (Language::Yaml, "url: http://example.com/a:b\n"),
            (Language::Yaml, "  'quoted key': \"value\" # trailing\n"),
            (
                Language::Xml,
                r#"<?xml version="1.0"?><a x="1"><b/>text</a>"#,
            ),
            (Language::Xml, "<!-- a > b --><x"),
            (Language::Xml, "plain text with no tags"),
        ];
        for (language, text) in samples {
            tiles_exactly(text, *language);
        }
    }

    /// Non-ASCII must not split a character: egui slices the text by these
    /// ranges and a boundary inside a code point panics.
    #[test]
    fn spans_land_on_character_boundaries() {
        for (language, text) in [
            (Language::Json, r#"{"k": "héllo → wörld 😀"}"#),
            (Language::Yaml, "clé: vàleur 😀\n"),
            (Language::Xml, "<p>héllo 😀</p>"),
        ] {
            tiles_exactly(text, language);
            for (range, _) in spans(text, language) {
                assert!(text.is_char_boundary(range.start));
                assert!(text.is_char_boundary(range.end));
            }
        }
    }

    fn tokens_for(text: &str, language: Language, want: &str) -> Vec<Token> {
        spans(text, language)
            .into_iter()
            .filter(|(r, _)| text[r.clone()] == *want)
            .map(|(_, t)| t)
            .collect()
    }

    #[test]
    fn json_tells_a_key_from_a_string() {
        let text = r#"{"a": "a"}"#;
        // Same bytes, different roles: the one before the colon is the key.
        let all: Vec<_> = spans(text, Language::Json)
            .into_iter()
            .filter(|(r, t)| matches!(t, Token::Key | Token::Str) && text[r.clone()] == *"\"a\"")
            .map(|(_, t)| t)
            .collect();
        assert_eq!(all, vec![Token::Key, Token::Str]);
    }

    #[test]
    fn json_brackets_carry_matching_depths() {
        let text = "[[]]";
        // Per character, since adjacent brackets of the same depth merge into
        // one span — they are the same colour, which is the point.
        let mut depths = vec![None; text.len()];
        for (range, token) in spans(text, Language::Json) {
            if let Token::Bracket(d) = token {
                for slot in &mut depths[range] {
                    *slot = Some(d);
                }
            }
        }
        // Outer pair 0, inner pair 1 — a pair shares a colour.
        assert_eq!(depths, vec![Some(0), Some(1), Some(1), Some(0)]);
    }

    #[test]
    fn yaml_does_not_mistake_a_url_for_a_key() {
        // `http:` is followed by `/`, not a space, so the line's only key is
        // `url`. Getting this wrong colours half of every config file.
        assert_eq!(
            tokens_for("url: http://example.com\n", Language::Yaml, "url"),
            vec![Token::Key]
        );
        assert!(tokens_for("url: http://example.com\n", Language::Yaml, "http").is_empty());
    }

    #[test]
    fn yaml_comments_win_over_everything_after_them() {
        let text = "a: 1 # b: 2\n";
        let comment: Vec<_> = spans(text, Language::Yaml)
            .into_iter()
            .filter(|(_, t)| *t == Token::Comment)
            .map(|(r, _)| text[r].to_owned())
            .collect();
        assert_eq!(comment, vec!["# b: 2".to_owned()]);
    }

    #[test]
    fn xml_comment_may_contain_a_closing_angle_bracket() {
        let text = "<!-- a > b --><x/>";
        let comment: Vec<_> = spans(text, Language::Xml)
            .into_iter()
            .filter(|(_, t)| *t == Token::Comment)
            .map(|(r, _)| text[r].to_owned())
            .collect();
        assert_eq!(comment, vec!["<!-- a > b -->".to_owned()]);
    }

    #[test]
    fn xml_separates_tag_names_from_attribute_names() {
        let text = r#"<item id="1"/>"#;
        assert_eq!(tokens_for(text, Language::Xml, "item"), vec![Token::Tag]);
        assert_eq!(tokens_for(text, Language::Xml, "id"), vec![Token::Key]);
    }
}
