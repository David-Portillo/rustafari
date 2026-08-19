use serde::Serialize;
use serde_json::{ser::PrettyFormatter, Serializer, Value};

use crate::spec::*;

pub struct JsonFormatter;

const OPTIONS: &[OptionSpec] = &[
    OptionSpec::Choice {
        id: "indent",
        label: "Indentation",
        choices: &[
            ("2", "2 spaces"),
            ("4", "4 spaces"),
            ("tab", "Tab"),
            ("minify", "Minified"),
        ],
        default: "2",
    },
    OptionSpec::Toggle {
        id: "sort_keys",
        label: "Sort keys alphabetically",
        default: false,
    },
];

impl Tool for JsonFormatter {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            id: "json-formatter",
            name: "JSON Formatter",
            category: Category::Formatters,
            description: "Validate, pretty-print, minify and sort JSON documents.",
            keywords: &["json", "pretty", "beautify", "minify", "validate"],
        }
    }

    fn input_mode(&self) -> InputMode {
        InputMode::Text {
            placeholder: r#"{"hello": "world"}"#,
        }
    }

    fn options(&self) -> &'static [OptionSpec] {
        OPTIONS
    }

    fn run(&self, input: Input<'_>, opts: &Options) -> ToolResult {
        let input = input.left;
        if input.trim().is_empty() {
            return Ok(String::new());
        }

        let mut value: Value = serde_json::from_str(input).map_err(|e| {
            ToolError::new(format!(
                "Invalid JSON at line {}, column {}: {}",
                e.line(),
                e.column(),
                e
            ))
        })?;

        if opts.bool("sort_keys") {
            sort_keys(&mut value);
        }

        let indent = match opts.choice("indent") {
            "4" => "    ",
            "tab" => "\t",
            "minify" => return serde_json::to_string(&value).map_err(render_err),
            _ => "  ",
        };

        let mut buf = Vec::new();
        let formatter = PrettyFormatter::with_indent(indent.as_bytes());
        let mut ser = Serializer::with_formatter(&mut buf, formatter);
        value.serialize(&mut ser).map_err(render_err)?;

        String::from_utf8(buf).map_err(|_| ToolError::new("Output was not valid UTF-8"))
    }
}

fn render_err(e: serde_json::Error) -> ToolError {
    ToolError::new(format!("Could not render JSON: {e}"))
}

/// Recursively sorts object keys. `preserve_order` keeps insertion order by
/// default, so sorting has to be explicit.
fn sort_keys(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = std::mem::take(map).into_iter().collect();
            entries.sort_by(|(a, _), (b, _)| a.cmp(b));
            for (key, mut child) in entries {
                sort_keys(&mut child);
                map.insert(key, child);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(sort_keys),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Options {
        Options::from_specs(OPTIONS)
    }

    #[test]
    fn pretty_prints_with_two_spaces_by_default() {
        let out = JsonFormatter.run(r#"{"a":1}"#.into(), &opts()).unwrap();
        assert_eq!(out, "{\n  \"a\": 1\n}");
    }

    #[test]
    fn minifies() {
        let mut o = opts();
        o.set("indent", OptionValue::Choice("minify".into()));
        let out = JsonFormatter.run("{\n  \"a\": 1\n}".into(), &o).unwrap();
        assert_eq!(out, r#"{"a":1}"#);
    }

    #[test]
    fn preserves_key_order_unless_sorting_is_requested() {
        let input = r#"{"b":1,"a":{"d":2,"c":3}}"#;
        let mut o = opts();
        o.set("indent", OptionValue::Choice("minify".into()));

        assert_eq!(JsonFormatter.run(input.into(), &o).unwrap(), input);

        o.set("sort_keys", OptionValue::Bool(true));
        assert_eq!(
            JsonFormatter.run(input.into(), &o).unwrap(),
            r#"{"a":{"c":3,"d":2},"b":1}"#
        );
    }

    #[test]
    fn empty_input_is_not_an_error() {
        assert_eq!(JsonFormatter.run("   ".into(), &opts()).unwrap(), "");
    }

    #[test]
    fn reports_where_the_syntax_error_is() {
        let err = JsonFormatter.run("{\"a\": }".into(), &opts()).unwrap_err();
        assert!(err.0.contains("line 1"), "{}", err.0);
    }
}
