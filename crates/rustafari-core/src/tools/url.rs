use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, CONTROLS};

use crate::spec::*;

/// Everything outside the RFC 3986 unreserved set, so the result is safe to
/// drop into any part of a URL (a query value, a path segment, a form field).
const COMPONENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'!')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b'\\')
    .add(b']')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}');

pub struct UrlTool;

const OPTIONS: &[OptionSpec] = &[OptionSpec::Choice {
    id: "direction",
    label: "Mode",
    choices: &[("encode", "Encode"), ("decode", "Decode")],
    default: "encode",
}];

impl Tool for UrlTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            id: "url-encode",
            name: "URL Encoder",
            category: Category::Encoders,
            description: "Percent-encode text for use in URLs, or decode it back.",
            keywords: &["url", "uri", "percent", "escape", "querystring"],
        }
    }

    fn options(&self) -> &'static [OptionSpec] {
        OPTIONS
    }

    fn run(&self, input: &str, opts: &Options) -> ToolResult {
        if input.is_empty() {
            return Ok(String::new());
        }

        match opts.choice("direction") {
            "decode" => percent_decode_str(input)
                .decode_utf8()
                .map(|s| s.into_owned())
                .map_err(|_| ToolError::new("Decoded bytes are not valid UTF-8 text")),
            _ => Ok(utf8_percent_encode(input, COMPONENT).to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Options {
        Options::from_specs(OPTIONS)
    }

    #[test]
    fn encodes_reserved_characters() {
        assert_eq!(
            UrlTool.run("a b&c=d/e?f", &opts()).unwrap(),
            "a%20b%26c%3Dd%2Fe%3Ff"
        );
    }

    #[test]
    fn leaves_unreserved_characters_alone() {
        assert_eq!(UrlTool.run("aZ0-_.~", &opts()).unwrap(), "aZ0-_.~");
    }

    #[test]
    fn round_trips_non_ascii() {
        let encoded = UrlTool.run("café 東京", &opts()).unwrap();
        assert!(!encoded.contains('é'));

        let mut o = opts();
        o.set("direction", OptionValue::Choice("decode".into()));
        assert_eq!(UrlTool.run(&encoded, &o).unwrap(), "café 東京");
    }

    #[test]
    fn rejects_sequences_that_are_not_utf8() {
        let mut o = opts();
        o.set("direction", OptionValue::Choice("decode".into()));
        assert!(UrlTool.run("%FF%FE", &o).is_err());
    }
}
