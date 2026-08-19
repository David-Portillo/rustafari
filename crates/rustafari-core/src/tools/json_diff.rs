//! Structural JSON comparison.
//!
//! Compares two documents as *data*, not as text: objects match by key, so
//! reordering keys or reformatting whitespace produces no difference. That is
//! the whole point — a line-based diff of the same two documents is mostly
//! noise.
//!
//! Arrays are ordered by default, because JSON arrays are ordered and moving
//! an element usually is a real change. `array_order` turns that off for the
//! cases where a list is really a set.

use std::fmt::Write as _;

use serde_json::{Map, Number, Value};

use crate::spec::*;

pub struct JsonDiff;

const OPTIONS: &[OptionSpec] = &[
    OptionSpec::Toggle {
        id: "array_order",
        label: "Array order matters",
        default: true,
    },
    OptionSpec::Toggle {
        id: "strict_numbers",
        label: "Distinguish 1 from 1.0",
        default: false,
    },
    OptionSpec::Toggle {
        id: "show_unchanged",
        label: "List unchanged paths",
        default: false,
    },
];

/// How the two documents differ at one path.
#[derive(Debug, PartialEq)]
enum Change {
    /// Present on the right only.
    Added(String, Value),
    /// Present on the left only.
    Removed(String, Value),
    /// Present on both, with different values.
    Changed(String, Value, Value),
    Unchanged(String),
}

#[derive(Clone, Copy)]
struct Rules {
    array_order: bool,
    strict_numbers: bool,
    show_unchanged: bool,
}

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
        OPTIONS
    }

    fn run(&self, input: Input<'_>, opts: &Options) -> ToolResult {
        // Nothing to say until there is something on both sides. Reporting
        // "everything was removed" while the user is still pasting would be
        // technically true and completely useless.
        if input.left.trim().is_empty() || input.right.trim().is_empty() {
            return Ok(String::new());
        }

        let left = parse(input.left, "Original")?;
        let right = parse(input.right, "Changed")?;

        let rules = Rules {
            array_order: opts.bool("array_order"),
            strict_numbers: opts.bool("strict_numbers"),
            show_unchanged: opts.bool("show_unchanged"),
        };

        let mut changes = Vec::new();
        diff(&left, &right, "$", rules, &mut changes);
        Ok(render(&changes, rules))
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

/// Walks both documents together, recording differences by path.
fn diff(left: &Value, right: &Value, path: &str, rules: Rules, out: &mut Vec<Change>) {
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => diff_objects(a, b, path, rules, out),
        (Value::Array(a), Value::Array(b)) if rules.array_order => {
            diff_arrays_ordered(a, b, path, rules, out)
        }
        (Value::Array(a), Value::Array(b)) => diff_arrays_unordered(a, b, path, rules, out),
        _ if equal(left, right, rules) => {
            if rules.show_unchanged {
                out.push(Change::Unchanged(path.to_owned()));
            }
        }
        _ => out.push(Change::Changed(
            path.to_owned(),
            left.clone(),
            right.clone(),
        )),
    }
}

fn diff_objects(
    a: &Map<String, Value>,
    b: &Map<String, Value>,
    path: &str,
    rules: Rules,
    out: &mut Vec<Change>,
) {
    // The union of both key sets, sorted, so key order in the source cannot
    // affect either the result or the order it is reported in.
    let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
    keys.sort_unstable();
    keys.dedup();

    for key in keys {
        let child = format!("{path}.{key}");
        match (a.get(key), b.get(key)) {
            (Some(l), Some(r)) => diff(l, r, &child, rules, out),
            (Some(l), None) => out.push(Change::Removed(child, l.clone())),
            (None, Some(r)) => out.push(Change::Added(child, r.clone())),
            (None, None) => unreachable!("key came from one of the two maps"),
        }
    }
}

fn diff_arrays_ordered(a: &[Value], b: &[Value], path: &str, rules: Rules, out: &mut Vec<Change>) {
    for (index, pair) in a
        .iter()
        .map(Some)
        .chain(std::iter::repeat(None))
        .zip(b.iter().map(Some).chain(std::iter::repeat(None)))
        .take(a.len().max(b.len()))
        .enumerate()
    {
        let child = format!("{path}[{index}]");
        match pair {
            (Some(l), Some(r)) => diff(l, r, &child, rules, out),
            (Some(l), None) => out.push(Change::Removed(child, l.clone())),
            (None, Some(r)) => out.push(Change::Added(child, r.clone())),
            (None, None) => unreachable!("index is below the longer length"),
        }
    }
}

/// Treats both arrays as multisets: pair off equal elements, and report
/// whatever is left over. Quadratic, which is fine for the sizes a person
/// pastes into a diff.
fn diff_arrays_unordered(
    a: &[Value],
    b: &[Value],
    path: &str,
    rules: Rules,
    out: &mut Vec<Change>,
) {
    let mut matched = vec![false; b.len()];

    for left in a {
        let found = b
            .iter()
            .enumerate()
            .find(|(i, right)| !matched[*i] && equal(left, right, rules));
        match found {
            Some((i, _)) => matched[i] = true,
            None => out.push(Change::Removed(path.to_owned(), left.clone())),
        }
    }

    for (i, right) in b.iter().enumerate() {
        if !matched[i] {
            out.push(Change::Added(path.to_owned(), right.clone()));
        }
    }

    if rules.show_unchanged && out.is_empty() {
        out.push(Change::Unchanged(path.to_owned()));
    }
}

/// Deep equality under the current rules — key order never matters, array
/// order and number types matter only if asked.
fn equal(a: &Value, b: &Value, rules: Rules) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => numbers_equal(x, y, rules.strict_numbers),
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|other| equal(v, other, rules)))
        }
        (Value::Array(x), Value::Array(y)) if rules.array_order => {
            x.len() == y.len() && x.iter().zip(y).all(|(v, o)| equal(v, o, rules))
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return false;
            }
            let mut matched = vec![false; y.len()];
            x.iter().all(|v| {
                match y
                    .iter()
                    .enumerate()
                    .find(|(i, o)| !matched[*i] && equal(v, o, rules))
                {
                    Some((i, _)) => {
                        matched[i] = true;
                        true
                    }
                    None => false,
                }
            })
        }
        _ => a == b,
    }
}

/// `1` and `1.0` are different `serde_json` numbers but the same quantity.
/// Most people diffing JSON mean the quantity.
fn numbers_equal(a: &Number, b: &Number, strict: bool) -> bool {
    if strict {
        return a == b;
    }
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => x == y,
        // Integers too large for f64 keep their exact comparison.
        _ => a == b,
    }
}

/// Renders one line per change, plus a closing count.
fn render(changes: &[Change], rules: Rules) -> String {
    let differences = changes
        .iter()
        .filter(|c| !matches!(c, Change::Unchanged(_)))
        .count();

    if differences == 0 && !rules.show_unchanged {
        return "No differences.\n\nThe two documents are structurally identical.".to_owned();
    }

    let mut out = String::new();
    for change in changes {
        match change {
            Change::Added(path, value) => {
                let _ = writeln!(out, "+ {path}: {}", brief(value));
            }
            Change::Removed(path, value) => {
                let _ = writeln!(out, "- {path}: {}", brief(value));
            }
            Change::Changed(path, from, to) => {
                let _ = writeln!(out, "~ {path}: {} → {}", brief(from), brief(to));
            }
            Change::Unchanged(path) => {
                let _ = writeln!(out, "  {path}");
            }
        }
    }

    let _ = write!(
        out,
        "\n{differences} difference{}",
        if differences == 1 { "" } else { "s" }
    );
    out
}

/// A value on one line, shortened if it is too big to read at a glance. The
/// full value is always available in the pane the user pasted it into.
fn brief(value: &Value) -> String {
    const LIMIT: usize = 120;
    let text = serde_json::to_string(value).unwrap_or_else(|_| "<unprintable>".to_owned());
    if text.chars().count() <= LIMIT {
        return text;
    }
    let head: String = text.chars().take(LIMIT).collect();
    format!("{head}… ({} chars)", text.chars().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Options {
        Options::from_specs(OPTIONS)
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
