//! The comparison engine shared by every structural diff tool.
//!
//! JSON, YAML and XML each parse into `serde_json::Value` — chosen as the
//! common model because it already carries exactly what a structural diff
//! needs: named members, ordered sequences, and scalars. What "structural"
//! means is decided here once, so the three tools cannot drift apart:
//!
//! - Members are matched by name. Their order in the source is never a
//!   difference, and never affects the order of the report.
//! - Sequences are ordered by default, since sequence order usually means
//!   something. `array_order` compares them as multisets instead.
//! - `1` and `1.0` are the same quantity unless `strict_numbers` says
//!   otherwise.
//!
//! Each tool supplies only a parser; see `run_diff`.

use std::fmt::Write as _;

use serde_json::{Map, Number, Value};

use crate::spec::*;

/// Every diff tool offers the same three knobs.
pub const OPTIONS: &[OptionSpec] = &[
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

/// The shape every diff tool shares: two documents in, a change report out.
///
/// `parse` turns one side's text into a value, and is told which side it is
/// looking at so the error can say so.
pub fn run_diff(
    input: Input<'_>,
    opts: &Options,
    labels: (&str, &str),
    parse: impl Fn(&str, &str) -> Result<Value, ToolError>,
) -> ToolResult {
    // Nothing to say until there is something on both sides. Reporting
    // "everything was removed" while the user is still pasting would be
    // technically true and completely useless.
    if input.left.trim().is_empty() || input.right.trim().is_empty() {
        return Ok(String::new());
    }

    let left = parse(input.left, labels.0)?;
    let right = parse(input.right, labels.1)?;

    let rules = Rules {
        array_order: opts.bool("array_order"),
        strict_numbers: opts.bool("strict_numbers"),
        show_unchanged: opts.bool("show_unchanged"),
    };

    let mut changes = Vec::new();
    diff(&left, &right, "$", rules, &mut changes);
    Ok(render(&changes, rules))
}
