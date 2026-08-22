//! Set operations on two lists of items.
//!
//! The distinction that makes this useful: **normalisation decides what counts
//! as the same item, and never changes what is printed.** Trimming, case
//! folding and the rest build a comparison key; the text that comes out is the
//! item as it was first written, with the output transforms applied afterwards.
//! So comparing case-insensitively still shows you `Alice`, not `alice`.
//!
//! Items are deduplicated — these are set operations — and keep the order they
//! first appeared in, unless a sort is asked for.

use std::collections::HashSet;
use std::fmt::Write as _;

use crate::spec::*;

pub struct ListCompare;

const OPTIONS: &[OptionSpec] = &[
    OptionSpec::Group { label: "Compare" },
    OptionSpec::Choice {
        id: "result",
        label: "Show",
        choices: &[
            ("a-only", "A only"),
            ("both", "In both"),
            ("b-only", "B only"),
            ("union", "All items"),
            ("summary", "Summary"),
        ],
        default: "a-only",
    },
    OptionSpec::Choice {
        id: "split",
        label: "Split on",
        choices: &[
            ("lines", "Lines"),
            ("comma", "Commas"),
            ("semicolon", "Semicolons"),
            ("tab", "Tabs"),
            ("whitespace", "Whitespace"),
        ],
        default: "lines",
    },
    OptionSpec::Toggle {
        id: "case_sensitive",
        label: "Case",
        default: true,
    },
    OptionSpec::Toggle {
        id: "trim",
        label: "Trim ends",
        default: true,
    },
    OptionSpec::Toggle {
        id: "collapse_spaces",
        label: "Collapse spaces",
        default: false,
    },
    OptionSpec::Toggle {
        id: "ignore_leading_zeros",
        label: "Leading zeros",
        default: false,
    },
    OptionSpec::Group { label: "Output" },
    OptionSpec::Choice {
        id: "sort",
        label: "Sort",
        choices: &[
            ("none", "As written"),
            ("az", "A → Z"),
            ("za", "Z → A"),
            ("numeric", "0 → 9"),
            ("numeric-desc", "9 → 0"),
        ],
        default: "none",
    },
    OptionSpec::Choice {
        id: "case",
        label: "Output case",
        choices: &[
            ("unchanged", "Unchanged"),
            ("upper", "UPPER"),
            ("lower", "lower"),
            ("capitalize", "Capitalize"),
        ],
        default: "unchanged",
    },
    OptionSpec::Choice {
        id: "format",
        label: "Format",
        choices: &[
            ("plain", "Plain"),
            ("numbered", "Numbered"),
            ("bullets", "Markdown bullets"),
            ("html-ul", "HTML <ul>"),
            ("html-ol", "HTML <ol>"),
            ("hashtags", "Hashtags"),
            ("json", "JSON array"),
            ("quoted", "Quoted"),
        ],
        default: "plain",
    },
];

impl Tool for ListCompare {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            id: "list-compare",
            name: "List Compare",
            category: Category::Text,
            description: "Find what is unique to each of two lists and what they share.",
            keywords: &[
                "list",
                "compare",
                "set",
                "union",
                "intersection",
                "difference",
                "dedupe",
                "unique",
                "common",
            ],
        }
    }

    fn input_mode(&self) -> InputMode {
        InputMode::TwoText {
            left_label: "List A",
            right_label: "List B",
            placeholder: "one item per line",
        }
    }

    fn options(&self) -> &'static [OptionSpec] {
        OPTIONS
    }

    fn run(&self, input: Input<'_>, opts: &Options) -> ToolResult {
        // Both sides are needed before any answer means anything; with one
        // empty, every item would read as unique while you are still pasting.
        if input.left.trim().is_empty() || input.right.trim().is_empty() {
            return Ok(String::new());
        }

        let rules = Rules::from(opts);
        let a = List::parse(input.left, &rules);
        let b = List::parse(input.right, &rules);

        let items = match opts.choice("result") {
            "both" => a.matching(&b, true),
            "b-only" => b.matching(&a, false),
            "union" => {
                let mut all = a.items.clone();
                all.extend(b.matching(&a, false));
                all
            }
            "summary" => return Ok(summary(&a, &b, opts)),
            _ => a.matching(&b, false),
        };

        Ok(present(items, opts))
    }
}

#[derive(Clone, Copy)]
struct Rules {
    case_sensitive: bool,
    trim: bool,
    collapse_spaces: bool,
    ignore_leading_zeros: bool,
    split: &'static str,
}

impl Rules {
    fn from(opts: &Options) -> Self {
        Rules {
            case_sensitive: opts.bool("case_sensitive"),
            trim: opts.bool("trim"),
            collapse_spaces: opts.bool("collapse_spaces"),
            ignore_leading_zeros: opts.bool("ignore_leading_zeros"),
            split: match opts.choice("split") {
                "comma" => "comma",
                "semicolon" => "semicolon",
                "tab" => "tab",
                "whitespace" => "whitespace",
                _ => "lines",
            },
        }
    }

    /// What two items have to share to count as the same item. Never printed.
    fn key(&self, item: &str) -> String {
        let mut key = item.to_owned();
        if self.trim {
            key = key.trim().to_owned();
        }
        if self.collapse_spaces {
            key = key.split_whitespace().collect::<Vec<_>>().join(" ");
        }
        if !self.case_sensitive {
            key = key.to_lowercase();
        }
        if self.ignore_leading_zeros {
            let stripped = key.trim_start_matches('0');
            // All zeros is the number zero, not the empty string.
            key = if stripped.is_empty() && !key.is_empty() {
                "0".to_owned()
            } else {
                stripped.to_owned()
            };
        }
        key
    }
}

/// Items in first-seen order, deduplicated, each with its comparison key.
struct List {
    items: Vec<String>,
    keys: Vec<String>,
}

impl List {
    fn parse(text: &str, rules: &Rules) -> Self {
        // Every delimiter mode also breaks on line endings. Pasting a
        // multi-line file and choosing "Commas" otherwise yields one item per
        // line with commas inside it — which looks like sorting and comparison
        // are broken, when really nothing was ever split.
        let raw: Vec<&str> = match rules.split {
            "comma" => text.split([',', '\n', '\r']).collect(),
            "semicolon" => text.split([';', '\n', '\r']).collect(),
            "tab" => text.split(['\t', '\n', '\r']).collect(),
            "whitespace" => text.split_whitespace().collect(),
            _ => text.lines().collect(),
        };

        let mut items = Vec::new();
        let mut keys = Vec::new();
        let mut seen = HashSet::new();

        for item in raw {
            let display = if rules.trim { item.trim() } else { item };
            if display.is_empty() {
                continue;
            }
            let key = rules.key(item);
            if seen.insert(key.clone()) {
                items.push(display.to_owned());
                keys.push(key);
            }
        }
        List { items, keys }
    }

    fn key_set(&self) -> HashSet<&str> {
        self.keys.iter().map(String::as_str).collect()
    }

    /// This list's items whose key is, or is not, present in `other`.
    fn matching(&self, other: &List, present: bool) -> Vec<String> {
        let theirs = other.key_set();
        self.items
            .iter()
            .zip(&self.keys)
            .filter(|(_, key)| theirs.contains(key.as_str()) == present)
            .map(|(item, _)| item.clone())
            .collect()
    }
}

fn summary(a: &List, b: &List, opts: &Options) -> String {
    let a_only = a.matching(b, false);
    let both = a.matching(b, true);
    let b_only = b.matching(a, false);

    let mut out = format!(
        "A: {} unique · B: {} unique\nA only: {} · In both: {} · B only: {} · All: {}\n",
        a.items.len(),
        b.items.len(),
        a_only.len(),
        both.len(),
        b_only.len(),
        a.items.len() + b_only.len(),
    );

    for (title, items) in [("A ONLY", a_only), ("IN BOTH", both), ("B ONLY", b_only)] {
        let _ = write!(out, "\n{title} ({})\n", items.len());
        if items.is_empty() {
            out.push_str("  —\n");
        } else {
            for line in present(items, opts).lines() {
                let _ = writeln!(out, "  {line}");
            }
        }
    }
    out
}

/// Sorts, recases and formats the items for display.
fn present(items: Vec<String>, opts: &Options) -> String {
    let mut items: Vec<String> = items
        .into_iter()
        .map(|item| match opts.choice("case") {
            "upper" => item.to_uppercase(),
            "lower" => item.to_lowercase(),
            "capitalize" => capitalize(&item),
            _ => item,
        })
        .collect();

    match opts.choice("sort") {
        "az" => items.sort(),
        "za" => {
            items.sort();
            items.reverse();
        }
        // Anything unparseable sorts after every number rather than being
        // dropped or treated as zero.
        "numeric" => items.sort_by(|a, b| number(a).partial_cmp(&number(b)).unwrap()),
        "numeric-desc" => {
            items.sort_by(|a, b| number(a).partial_cmp(&number(b)).unwrap());
            items.reverse();
        }
        _ => {}
    }

    format(&items, opts.choice("format"))
}

fn number(item: &str) -> f64 {
    item.trim().parse::<f64>().unwrap_or(f64::INFINITY)
}

fn capitalize(item: &str) -> String {
    let mut chars = item.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
        None => String::new(),
    }
}

fn format(items: &[String], style: &str) -> String {
    match style {
        "numbered" => items
            .iter()
            .enumerate()
            .map(|(i, item)| format!("{}. {item}", i + 1))
            .collect::<Vec<_>>()
            .join("\n"),
        "bullets" => items
            .iter()
            .map(|item| format!("- {item}"))
            .collect::<Vec<_>>()
            .join("\n"),
        "html-ul" | "html-ol" => {
            let tag = if style == "html-ul" { "ul" } else { "ol" };
            let body = items
                .iter()
                .map(|item| format!("  <li>{}</li>", escape_html(item)))
                .collect::<Vec<_>>()
                .join("\n");
            format!("<{tag}>\n{body}\n</{tag}>")
        }
        "hashtags" => items
            .iter()
            .map(|item| format!("#{}", item.split_whitespace().collect::<Vec<_>>().join("")))
            .collect::<Vec<_>>()
            .join(" "),
        "json" => serde_json::to_string_pretty(items).unwrap_or_else(|_| "[]".to_owned()),
        "quoted" => items
            .iter()
            .map(|item| format!("\"{}\"", item.replace('"', "\\\"")))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => items.join("\n"),
    }
}

fn escape_html(item: &str) -> String {
    item.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(pairs: &[(&'static str, &str)]) -> Options {
        let mut o = Options::from_specs(OPTIONS);
        for (id, value) in pairs {
            let spec = OPTIONS
                .iter()
                .find(|s| s.id() == *id)
                .expect("known option");
            o.set(
                id,
                match spec {
                    OptionSpec::Toggle { .. } => OptionValue::Bool(*value == "true"),
                    _ => OptionValue::Choice((*value).to_owned()),
                },
            );
        }
        o
    }

    fn run(a: &str, b: &str, pairs: &[(&'static str, &str)]) -> String {
        ListCompare.run(Input::pair(a, b), &opts(pairs)).unwrap()
    }

    const A: &str = "apple\nbanana\ncherry";
    const B: &str = "banana\ncherry\ndate";

    #[test]
    fn the_four_set_operations() {
        assert_eq!(run(A, B, &[("result", "a-only")]), "apple");
        assert_eq!(run(A, B, &[("result", "both")]), "banana\ncherry");
        assert_eq!(run(A, B, &[("result", "b-only")]), "date");
        assert_eq!(
            run(A, B, &[("result", "union")]),
            "apple\nbanana\ncherry\ndate"
        );
    }

    #[test]
    fn items_keep_the_order_they_were_written_in() {
        assert_eq!(run("c\nb\na", "z", &[("result", "a-only")]), "c\nb\na");
    }

    #[test]
    fn duplicates_collapse_because_these_are_sets() {
        assert_eq!(run("a\na\nb", "b", &[("result", "a-only")]), "a");
        assert_eq!(run("a\na", "a", &[("result", "both")]), "a");
    }

    #[test]
    fn case_sensitivity_decides_matching_but_never_the_output() {
        // Sensitive: different items.
        assert_eq!(run("Alice", "alice", &[("result", "a-only")]), "Alice");
        // Insensitive: the same item — and A's spelling is what prints.
        assert_eq!(
            run(
                "Alice",
                "alice",
                &[("result", "both"), ("case_sensitive", "false")]
            ),
            "Alice",
            "normalisation must not rewrite the output"
        );
    }

    #[test]
    fn ends_are_trimmed_by_default() {
        assert_eq!(run("  apple  ", "apple", &[("result", "both")]), "apple");
        assert_eq!(
            run(
                "  apple  ",
                "apple",
                &[("result", "a-only"), ("trim", "false")]
            ),
            "  apple  ",
            "without trimming they are different items"
        );
    }

    #[test]
    fn repeated_spaces_can_be_ignored() {
        let pairs = &[("result", "both"), ("collapse_spaces", "true")];
        assert_eq!(run("New   York", "New York", pairs), "New   York");
        assert_eq!(
            run("New   York", "New York", &[("result", "a-only")]),
            "New   York"
        );
    }

    #[test]
    fn leading_zeros_can_be_ignored() {
        let pairs = &[("result", "both"), ("ignore_leading_zeros", "true")];
        assert_eq!(run("007", "7", pairs), "007");
        // All zeros is the number zero, not nothing.
        assert_eq!(run("000", "0", pairs), "000");
    }

    #[test]
    fn splitting_on_other_delimiters() {
        assert_eq!(
            run("a,b,c", "b", &[("result", "a-only"), ("split", "comma")]),
            "a\nc"
        );
        assert_eq!(
            run("a;b", "b", &[("result", "a-only"), ("split", "semicolon")]),
            "a"
        );
        assert_eq!(
            run(
                "a b  c",
                "b",
                &[("result", "a-only"), ("split", "whitespace")]
            ),
            "a\nc"
        );
    }

    #[test]
    fn a_delimiter_also_breaks_on_lines() {
        // Choosing "Commas" on multi-line data must not leave one item per
        // line with commas inside it — that looks like nothing works.
        assert_eq!(
            run("a,b\nc,d", "zzz", &[("result", "a-only"), ("split", "comma")]),
            "a\nb\nc\nd"
        );
        // And the case that prompted this: a delimiter that is not in the
        // data at all still yields one item per line, not one giant item.
        assert_eq!(
            run(
                "cherry\nbanana\napple",
                "zzz",
                &[("result", "a-only"), ("split", "tab"), ("sort", "az")]
            ),
            "apple\nbanana\ncherry",
            "sorting must work even when the chosen delimiter is absent"
        );
    }

    #[test]
    fn empty_entries_are_dropped() {
        // A trailing newline or a doubled comma should not become an item.
        assert_eq!(run("a\n\nb\n", "b", &[("result", "a-only")]), "a");
        assert_eq!(
            run("a,,b", "b", &[("result", "a-only"), ("split", "comma")]),
            "a"
        );
    }

    #[test]
    fn sorting() {
        let both = "b\na\nc";
        assert_eq!(
            run(both, "z", &[("result", "a-only"), ("sort", "az")]),
            "a\nb\nc"
        );
        assert_eq!(
            run(both, "z", &[("result", "a-only"), ("sort", "za")]),
            "c\nb\na"
        );
    }

    #[test]
    fn numeric_sorting_orders_by_value_and_parks_the_rest_at_the_end() {
        let list = "10\n2\n100\nx";
        assert_eq!(
            run(list, "z", &[("result", "a-only"), ("sort", "numeric")]),
            "2\n10\n100\nx"
        );
    }

    #[test]
    fn output_case_transforms() {
        assert_eq!(
            run("aB", "z", &[("result", "a-only"), ("case", "upper")]),
            "AB"
        );
        assert_eq!(
            run("aB", "z", &[("result", "a-only"), ("case", "lower")]),
            "ab"
        );
        assert_eq!(
            run(
                "aB cD",
                "z",
                &[("result", "a-only"), ("case", "capitalize")]
            ),
            "Ab cd"
        );
    }

    #[test]
    fn output_formats() {
        let pairs = |f: &'static str| vec![("result", "a-only"), ("format", f)];
        assert_eq!(run("a\nb", "z", &pairs("numbered")), "1. a\n2. b");
        assert_eq!(run("a\nb", "z", &pairs("bullets")), "- a\n- b");
        assert_eq!(
            run("a\nb", "z", &pairs("html-ul")),
            "<ul>\n  <li>a</li>\n  <li>b</li>\n</ul>"
        );
        assert_eq!(
            run("a\nb", "z", &pairs("html-ol")),
            "<ol>\n  <li>a</li>\n  <li>b</li>\n</ol>"
        );
        assert_eq!(run("big cat\nb", "z", &pairs("hashtags")), "#bigcat #b");
        assert_eq!(run("a\nb", "z", &pairs("json")), "[\n  \"a\",\n  \"b\"\n]");
        assert_eq!(run("a\nb", "z", &pairs("quoted")), "\"a\"\n\"b\"");
    }

    #[test]
    fn html_and_json_output_escapes_its_input() {
        assert!(
            run("<b>", "z", &[("result", "a-only"), ("format", "html-ul")]).contains("&lt;b&gt;")
        );
        assert_eq!(
            run(
                "say \"hi\"",
                "z",
                &[("result", "a-only"), ("format", "json")]
            ),
            "[\n  \"say \\\"hi\\\"\"\n]"
        );
    }

    #[test]
    fn the_summary_counts_every_set() {
        let out = run(A, B, &[("result", "summary")]);
        assert!(out.contains("A: 3 unique · B: 3 unique"), "{out}");
        assert!(
            out.contains("A only: 1 · In both: 2 · B only: 1 · All: 4"),
            "{out}"
        );
        assert!(out.contains("A ONLY (1)\n  apple"), "{out}");
        assert!(out.contains("IN BOTH (2)"), "{out}");
        assert!(out.contains("B ONLY (1)\n  date"), "{out}");
    }

    #[test]
    fn a_summary_section_with_nothing_in_it_says_so() {
        let out = run("a", "a", &[("result", "summary")]);
        assert!(out.contains("A ONLY (0)\n  —"), "{out}");
    }

    #[test]
    fn waits_for_both_lists() {
        assert_eq!(run("", "", &[]), "");
        assert_eq!(run("a\nb", "", &[]), "");
        assert_eq!(run("", "a", &[]), "");
    }
}
