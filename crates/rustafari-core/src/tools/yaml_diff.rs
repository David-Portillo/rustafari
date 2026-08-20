//! Structural comparison of two YAML documents.
//!
//! YAML carries a lot that is presentation rather than meaning: quoting style,
//! block versus flow, indentation, comments, line folding, and anchors. None of
//! it survives parsing, so none of it can show up as a difference. What is left
//! is mappings, sequences and scalars — the same shape JSON has, which is why
//! both tools share `structural`.
//!
//! Anchors, aliases and merge keys are resolved before comparison, so a
//! document that names a block once and merges it in equals the document that
//! spells it out every time. `<<:` is a merge instruction, not a key, and would
//! otherwise show up as one.

use serde_json::{Map, Number, Value};

use crate::spec::*;
use crate::tools::structural;

pub struct YamlDiff;

impl Tool for YamlDiff {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            id: "yaml-diff",
            name: "YAML Diff",
            category: Category::Formatters,
            description:
                "Compare two YAML documents structurally, ignoring key order, quoting and comments.",
            keywords: &["diff", "compare", "yaml", "yml", "difference", "change"],
        }
    }

    fn input_mode(&self) -> InputMode {
        InputMode::TwoText {
            left_label: "Original",
            right_label: "Changed",
            placeholder: "hello: world",
        }
    }

    fn options(&self) -> &'static [OptionSpec] {
        structural::OPTIONS
    }

    fn run(&self, input: Input<'_>, opts: &Options) -> ToolResult {
        structural::run_diff(input, opts, ("Original", "Changed"), parse)
    }
}

fn parse(text: &str, side: &str) -> Result<Value, ToolError> {
    let mut value: serde_norway::Value = serde_norway::from_str(text)
        .map_err(|e| ToolError::new(format!("{side} is not valid YAML: {e}")))?;
    // The parser leaves `<<` as an ordinary key. It is a merge instruction, so
    // resolve it here or every merged block reads as a difference.
    value.apply_merge().map_err(|e| {
        ToolError::new(format!(
            "{side} has a merge key that cannot be resolved: {e}"
        ))
    })?;
    Ok(convert(value))
}

/// Maps YAML onto the model `structural` compares.
///
/// YAML allows any scalar as a mapping key, which JSON's model does not, so
/// non-string keys become their own text — `1:` and `"1":` therefore compare
/// as the same key. That is a deliberate flattening: it keeps one comparison
/// engine, and a document that distinguishes those two keys is pathological.
fn convert(value: serde_norway::Value) -> Value {
    use serde_norway::Value as Y;
    match value {
        Y::Null => Value::Null,
        Y::Bool(b) => Value::Bool(b),
        Y::Number(n) => number(n),
        Y::String(s) => Value::String(s),
        Y::Sequence(items) => Value::Array(items.into_iter().map(convert).collect()),
        Y::Mapping(map) => {
            let mut out = Map::new();
            for (k, v) in map {
                out.insert(key_text(&k), convert(v));
            }
            Value::Object(out)
        }
        // `!Tag value` — the tag is metadata about how to build a type, not
        // part of the data, so compare what it wraps.
        Y::Tagged(tagged) => convert(tagged.value),
    }
}

fn number(n: serde_norway::Number) -> Value {
    if let Some(i) = n.as_i64() {
        return Value::Number(i.into());
    }
    if let Some(u) = n.as_u64() {
        return Value::Number(u.into());
    }
    match n.as_f64().and_then(Number::from_f64) {
        Some(f) => Value::Number(f),
        // NaN and the infinities have no JSON number; keep them readable.
        None => Value::String(n.to_string()),
    }
}

fn key_text(key: &serde_norway::Value) -> String {
    use serde_norway::Value as Y;
    match key {
        Y::String(s) => s.clone(),
        Y::Bool(b) => b.to_string(),
        Y::Number(n) => n.to_string(),
        Y::Null => "null".to_owned(),
        // A sequence or mapping used as a key: rare enough that a compact
        // rendering is better than inventing a syntax for it.
        other => {
            serde_json::to_string(&convert(other.clone())).unwrap_or_else(|_| "<key>".to_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Options {
        Options::from_specs(structural::OPTIONS)
    }

    fn run(left: &str, right: &str) -> String {
        YamlDiff.run(Input::pair(left, right), &opts()).unwrap()
    }

    #[test]
    fn key_order_is_not_a_difference() {
        assert!(run("a: 1\nb: 2\n", "b: 2\na: 1\n").starts_with("No differences."));
    }

    #[test]
    fn comments_and_quoting_and_indentation_are_not_differences() {
        let left = "# a leading comment\nname:   \"widget\"\ntags:\n  - a\n  - b\n";
        let right = "tags: [a, b]\nname: widget # trailing\n";
        assert!(
            run(left, right).starts_with("No differences."),
            "{}",
            run(left, right)
        );
    }

    #[test]
    fn block_and_flow_styles_agree() {
        assert!(run("a: {b: 1}\n", "a:\n  b: 1\n").starts_with("No differences."));
    }

    #[test]
    fn anchors_are_resolved_before_comparing() {
        let anchored = "base: &b\n  x: 1\nfirst: *b\nsecond: *b\n";
        let spelled_out = "base:\n  x: 1\nfirst:\n  x: 1\nsecond:\n  x: 1\n";
        assert!(
            run(anchored, spelled_out).starts_with("No differences."),
            "{}",
            run(anchored, spelled_out)
        );
    }

    #[test]
    fn merge_keys_are_resolved() {
        let merged = "base: &b\n  LOG: info\nworker:\n  <<: *b\n";
        let spelled_out = "base:\n  LOG: info\nworker:\n  LOG: info\n";
        assert!(
            run(merged, spelled_out).starts_with("No differences."),
            "{}",
            run(merged, spelled_out)
        );
    }

    #[test]
    fn a_merge_that_is_overridden_still_reports_the_override() {
        let left = "base: &b\n  LOG: info\nworker:\n  <<: *b\n";
        let right = "base: &b\n  LOG: info\nworker:\n  <<: *b\n  LOG: debug\n";
        let out = run(left, right);
        assert!(out.contains(r#"~ $.worker.LOG: "info" → "debug""#), "{out}");
    }

    #[test]
    fn reports_a_changed_value_with_its_path() {
        let out = run("meta:\n  version: 3\n", "meta:\n  version: 4\n");
        assert!(out.contains("~ $.meta.version: 3 → 4"), "{out}");
    }

    #[test]
    fn reports_added_and_removed_keys() {
        let out = run("keep: 1\ngone: 2\n", "keep: 1\nfresh: 3\n");
        assert!(out.contains("- $.gone: 2"), "{out}");
        assert!(out.contains("+ $.fresh: 3"), "{out}");
    }

    #[test]
    fn sequence_order_is_a_difference() {
        assert!(run("- 1\n- 2\n", "- 2\n- 1\n").contains("difference"));
    }

    #[test]
    fn yaml_booleans_and_nulls_survive() {
        assert!(run("a: true\nb: null\n", "a: true\nb: ~\n").starts_with("No differences."));
        assert!(run("a: true\n", "a: false\n").contains("~ $.a: true → false"));
    }

    #[test]
    fn non_string_keys_become_their_text() {
        let out = run("1: one\n", "1: uno\n");
        assert!(out.contains(r#"~ $.1: "one" → "uno""#), "{out}");
    }

    #[test]
    fn waits_for_both_sides() {
        assert_eq!(run("", ""), "");
        assert_eq!(run("a: 1\n", ""), "");
    }

    #[test]
    fn invalid_yaml_names_the_side() {
        let err = YamlDiff
            .run(Input::pair("a:\n  - [unclosed\n", "a: 1"), &opts())
            .unwrap_err();
        assert!(err.0.starts_with("Original is not valid YAML"), "{}", err.0);
    }
}
