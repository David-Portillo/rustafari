use base64::engine::general_purpose::{
    GeneralPurpose, STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD,
};
use base64::Engine;

use crate::spec::*;

pub struct Base64Tool;

const OPTIONS: &[OptionSpec] = &[
    OptionSpec::Choice {
        id: "direction",
        label: "Mode",
        choices: &[("encode", "Encode"), ("decode", "Decode")],
        default: "encode",
    },
    OptionSpec::Toggle {
        id: "url_safe",
        label: "URL-safe alphabet (-_ instead of +/)",
        default: false,
    },
    OptionSpec::Toggle {
        id: "padding",
        label: "Padding (=)",
        default: true,
    },
];

impl Tool for Base64Tool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            id: "base64",
            name: "Base64",
            category: Category::Encoders,
            description:
                "Encode text to Base64 or decode it back, with URL-safe and padding variants.",
            keywords: &["base64", "b64", "encode", "decode"],
        }
    }

    fn options(&self) -> &'static [OptionSpec] {
        OPTIONS
    }

    fn run(&self, input: &str, opts: &Options) -> ToolResult {
        if input.is_empty() {
            return Ok(String::new());
        }

        let engine: &GeneralPurpose = match (opts.bool("url_safe"), opts.bool("padding")) {
            (false, true) => &STANDARD,
            (false, false) => &STANDARD_NO_PAD,
            (true, true) => &URL_SAFE,
            (true, false) => &URL_SAFE_NO_PAD,
        };

        match opts.choice("direction") {
            "decode" => {
                // Decoding is what users paste into, so be forgiving about the
                // whitespace that line-wrapped Base64 arrives with.
                let cleaned: String = input.split_whitespace().collect();
                let bytes = engine
                    .decode(cleaned)
                    .map_err(|e| ToolError::new(format!("Not valid Base64: {e}")))?;
                String::from_utf8(bytes)
                    .map_err(|_| ToolError::new("Decoded bytes are not valid UTF-8 text"))
            }
            _ => Ok(engine.encode(input.as_bytes())),
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
    fn round_trips() {
        let encoded = Base64Tool.run("hello world", &opts()).unwrap();
        assert_eq!(encoded, "aGVsbG8gd29ybGQ=");

        let mut o = opts();
        o.set("direction", OptionValue::Choice("decode".into()));
        assert_eq!(Base64Tool.run(&encoded, &o).unwrap(), "hello world");
    }

    #[test]
    fn url_safe_alphabet_avoids_plus_and_slash() {
        let mut o = opts();
        o.set("url_safe", OptionValue::Bool(true));
        o.set("padding", OptionValue::Bool(false));
        let out = Base64Tool.run("\u{3ff}\u{3ff}\u{3ff}", &o).unwrap();
        assert!(!out.contains('+') && !out.contains('/') && !out.contains('='));
    }

    #[test]
    fn decoding_tolerates_wrapped_lines() {
        let mut o = opts();
        o.set("direction", OptionValue::Choice("decode".into()));
        assert_eq!(
            Base64Tool.run("aGVsbG8g\nd29ybGQ=", &o).unwrap(),
            "hello world"
        );
    }

    #[test]
    fn rejects_garbage() {
        let mut o = opts();
        o.set("direction", OptionValue::Choice("decode".into()));
        assert!(Base64Tool.run("!!!!", &o).is_err());
    }
}
