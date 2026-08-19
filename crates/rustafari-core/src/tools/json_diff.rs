//! Structural comparison of two JSON documents.
//!
//! Compares them as *data*, not as text: objects match by key, so reordering
//! keys or reformatting whitespace produces no difference. That is the whole
//! point — a line-based diff of the same two documents is mostly noise.
//!
//! The comparison itself lives in `structural`; this file only parses.

use serde_json::Value;

use crate::spec::*;
use crate::tools::structural;

pub struct JsonDiff;

impl Tool for JsonDiff {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            id: "json-diff",
            name: "JSON Diff",
            category: Category::Formatters,
            description:
                "Compare two JSON documents structurally, ignoring key order and formatting.",
            keywords: &["diff", "compare", "json", "difference", "change", "delta"],
        }
    }

    fn input_mode(&self) -> InputMode {
        InputMode::TwoText {
            left_label: "Original",
            right_label: "Changed",
            placeholder: r#"{"hello": "world"}"#,
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
    serde_json::from_str(text).map_err(|e| {
        ToolError::new(format!(
            "{side} is not valid JSON at line {}, column {}: {e}",
            e.line(),
            e.column()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Options {
        Options::from_specs(structural::OPTIONS)
    }

    fn run(left: &str, right: &str) -> String {
        JsonDiff.run(Input::pair(left, right), &opts()).unwrap()
    }

    fn run_with(left: &str, right: &str, set: &[(&'static str, bool)]) -> String {
        let mut o = opts();
        for (id, value) in set {
            o.set(id, OptionValue::Bool(*value));
        }
        JsonDiff.run(Input::pair(left, right), &o).unwrap()
    }

    #[test]
    fn key_order_is_not_a_difference() {
        let out = run(r#"{"a":1,"b":2}"#, r#"{"b":2,"a":1}"#);
        assert!(out.starts_with("No differences."), "{out}");
    }

    #[test]
    fn whitespace_and_formatting_are_not_differences() {
        let out = run("{\n  \"a\" : [1,  2]\n}", r#"{"a":[1,2]}"#);
        assert!(out.starts_with("No differences."), "{out}");
    }

    #[test]
    fn nested_key_order_is_not_a_difference() {
        let out = run(
            r#"{"outer":{"x":{"p":1,"q":2},"y":[{"m":1,"n":2}]}}"#,
            r#"{"outer":{"y":[{"n":2,"m":1}],"x":{"q":2,"p":1}}}"#,
        );
        assert!(out.starts_with("No differences."), "{out}");
    }

    #[test]
    fn reports_a_changed_scalar_with_its_path() {
        let out = run(
            r#"{"user":{"name":"alice"}}"#,
            r#"{"user":{"name":"alicia"}}"#,
        );
        assert!(
            out.contains(r#"~ $.user.name: "alice" → "alicia""#),
            "{out}"
        );
        assert!(out.contains("1 difference"), "{out}");
    }

    #[test]
    fn reports_added_and_removed_keys_separately() {
        let out = run(r#"{"keep":1,"gone":2}"#, r#"{"keep":1,"fresh":3}"#);
        assert!(out.contains("- $.gone: 2"), "{out}");
        assert!(out.contains("+ $.fresh: 3"), "{out}");
        assert!(out.contains("2 differences"), "{out}");
    }

    #[test]
    fn reports_array_elements_by_index() {
        let out = run("[1,2,3]", "[1,9,3]");
        assert!(out.contains("~ $[1]: 2 → 9"), "{out}");
    }

    #[test]
    fn a_longer_array_reports_the_extra_elements() {
        let out = run("[1]", "[1,2]");
        assert!(out.contains("+ $[1]: 2"), "{out}");

        let out = run("[1,2]", "[1]");
        assert!(out.contains("- $[1]: 2"), "{out}");
    }

    #[test]
    fn array_order_matters_by_default_but_can_be_turned_off() {
        assert!(run("[1,2]", "[2,1]").contains("difference"));

        let out = run_with("[1,2]", "[2,1]", &[("array_order", false)]);
        assert!(out.starts_with("No differences."), "{out}");
    }

    #[test]
    fn unordered_arrays_still_notice_a_genuinely_missing_element() {
        let out = run_with("[1,2,3]", "[3,1]", &[("array_order", false)]);
        assert!(out.contains("- $: 2"), "{out}");
        assert!(out.contains("1 difference"), "{out}");
    }

    #[test]
    fn unordered_arrays_count_duplicates() {
        // A multiset, not a set: the second 1 has nothing to pair with.
        let out = run_with("[1,1]", "[1]", &[("array_order", false)]);
        assert!(out.contains("- $: 1"), "{out}");
    }

    #[test]
    fn integers_and_floats_of_equal_value_match_unless_strict() {
        assert!(run(r#"{"n":1}"#, r#"{"n":1.0}"#).starts_with("No differences."));

        let out = run_with(r#"{"n":1}"#, r#"{"n":1.0}"#, &[("strict_numbers", true)]);
        assert!(out.contains("~ $.n: 1 → 1.0"), "{out}");
    }

    #[test]
    fn a_changed_type_is_reported_as_a_change() {
        let out = run(r#"{"a":1}"#, r#"{"a":"1"}"#);
        assert!(out.contains(r#"~ $.a: 1 → "1""#), "{out}");
    }

    #[test]
    fn null_is_distinct_from_a_missing_key() {
        let missing = run(r#"{"a":1}"#, r#"{"a":1,"b":null}"#);
        assert!(missing.contains("+ $.b: null"), "{missing}");

        let changed = run(r#"{"b":1}"#, r#"{"b":null}"#);
        assert!(changed.contains("~ $.b: 1 → null"), "{changed}");
    }

    #[test]
    fn root_scalars_and_root_type_changes_are_reported() {
        assert!(run("1", "2").contains("~ $: 1 → 2"));
        assert!(run("{}", "[]").contains("~ $: {} → []"));
    }

    #[test]
    fn output_is_ordered_by_path_regardless_of_source_order() {
        let out = run(r#"{"z":1,"a":1}"#, r#"{"z":2,"a":2}"#);
        let a = out.find("$.a").expect("a listed");
        let z = out.find("$.z").expect("z listed");
        assert!(a < z, "paths should be sorted:\n{out}");
    }

    #[test]
    fn long_values_are_shortened_but_counted() {
        let long = "x".repeat(500);
        let out = run(r#"{"a":1}"#, &format!(r#"{{"a":"{long}"}}"#));
        assert!(out.contains('…'), "{out}");
        assert!(out.contains("502 chars"), "{out}");
    }

    #[test]
    fn unchanged_paths_are_listed_only_when_asked() {
        assert!(!run(r#"{"a":1}"#, r#"{"a":1,"b":2}"#).contains("  $.a"));

        let out = run_with(
            r#"{"a":1}"#,
            r#"{"a":1,"b":2}"#,
            &[("show_unchanged", true)],
        );
        assert!(out.contains("  $.a"), "{out}");
        assert!(out.contains("+ $.b: 2"), "{out}");
        assert!(
            out.contains("1 difference"),
            "unchanged must not be counted:\n{out}"
        );
    }

    #[test]
    fn waits_for_both_sides_before_saying_anything() {
        assert_eq!(run("", ""), "");
        assert_eq!(run(r#"{"a":1}"#, ""), "");
        assert_eq!(run("", r#"{"a":1}"#), "");
        assert_eq!(run("   ", "\n"), "");
    }

    #[test]
    fn invalid_json_names_the_side_and_the_position() {
        let err = JsonDiff
            .run(Input::pair("{oops}", r#"{"a":1}"#), &opts())
            .unwrap_err();
        assert!(err.0.starts_with("Original is not valid JSON"), "{}", err.0);

        let err = JsonDiff
            .run(Input::pair(r#"{"a":1}"#, "{oops}"), &opts())
            .unwrap_err();
        assert!(err.0.starts_with("Changed is not valid JSON"), "{}", err.0);
    }

    #[test]
    fn a_realistic_document_reports_only_what_moved() {
        let before = r#"{
            "id": 7, "name": "widget", "tags": ["a", "b"],
            "meta": {"created": "2024-01-01", "version": 3}
        }"#;
        let after = r#"{
            "meta": {"version": 4, "created": "2024-01-01"},
            "tags": ["a", "b"], "name": "widget", "id": 7,
            "price": 9.99
        }"#;
        let out = run(before, after);
        assert!(out.contains("~ $.meta.version: 3 → 4"), "{out}");
        assert!(out.contains("+ $.price: 9.99"), "{out}");
        assert!(out.contains("2 differences"), "{out}");
    }
}
