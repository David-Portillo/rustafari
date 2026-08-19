//! Structural comparison of two XML documents.
//!
//! "Semantic" needs defining for XML, because it carries more than JSON does.
//! The mapping used here, and therefore what counts as a difference:
//!
//! | XML | Becomes | So a difference in… |
//! | --- | --- | --- |
//! | element | an object under its tag name | tag names, nesting |
//! | attribute | a member named `@name` | attribute values, not their order |
//! | text | a member named `#text`, whitespace-collapsed | wording, not indentation |
//! | child elements | an array under their tag name, in document order | their order (unless `array_order` is off) |
//! | comment, processing instruction, XML declaration | dropped | nothing |
//!
//! Attribute order is never a difference, and neither is indentation, line
//! wrapping, or how empty elements are spelled — `<a/>` and `<a></a>` are the
//! same element. Namespaced names keep their namespace in Clark notation,
//! `{uri}local`, so two documents that use different prefixes for the same
//! namespace still match, while the same prefix bound to different namespaces
//! does not.
//!
//! Elements with *different* tag names become separate members, so their
//! relative order is not compared; only repeats of the same tag are ordered.

use serde_json::{Map, Value};

use crate::spec::*;
use crate::tools::structural;

pub struct XmlDiff;

impl Tool for XmlDiff {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            id: "xml-diff",
            name: "XML Diff",
            category: Category::Formatters,
            description:
                "Compare two XML documents structurally, ignoring attribute order and formatting.",
            keywords: &["diff", "compare", "xml", "html", "difference", "change"],
        }
    }

    fn input_mode(&self) -> InputMode {
        InputMode::TwoText {
            left_label: "Original",
            right_label: "Changed",
            placeholder: "<hello>world</hello>",
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
    let document = roxmltree::Document::parse(text)
        .map_err(|e| ToolError::new(format!("{side} is not valid XML: {e}")))?;

    let root = document.root_element();
    let mut out = Map::new();
    out.insert(name_of(&root), element(&root));
    Ok(Value::Object(out))
}

/// One element, as attributes, text and grouped children.
fn element(node: &roxmltree::Node<'_, '_>) -> Value {
    let mut object = Map::new();

    for attribute in node.attributes() {
        let name = match attribute.namespace() {
            Some(uri) => format!("@{{{uri}}}{}", attribute.name()),
            None => format!("@{}", attribute.name()),
        };
        object.insert(name, Value::String(attribute.value().to_owned()));
    }

    let text = collapse(
        &node
            .children()
            .filter(|c| c.is_text())
            .filter_map(|c| c.text())
            .collect::<String>(),
    );
    if !text.is_empty() {
        object.insert("#text".to_owned(), Value::String(text));
    }

    // Children are grouped by name, so ordering between *different* tags is
    // ignored while repeats of one tag stay a sequence.
    //
    // Always an array, even for a lone child. The tempting alternative — an
    // object when there is one, an array when there are several — makes adding
    // a second `<item>` report a type change instead of an addition, which is
    // both noisier and wrong about what happened.
    for child in node.children().filter(|c| c.is_element()) {
        let name = name_of(&child);
        let value = element(&child);
        match object.get_mut(&name) {
            Some(Value::Array(existing)) => existing.push(value),
            _ => {
                object.insert(name, Value::Array(vec![value]));
            }
        }
    }

    // An element with nothing in it is empty, not absent.
    if object.is_empty() {
        return Value::String(String::new());
    }
    Value::Object(object)
}

/// Clark notation for namespaced names, so prefixes do not matter but the
/// namespace itself does.
fn name_of(node: &roxmltree::Node<'_, '_>) -> String {
    match node.tag_name().namespace() {
        Some(uri) => format!("{{{uri}}}{}", node.tag_name().name()),
        None => node.tag_name().name().to_owned(),
    }
}

/// Collapses runs of whitespace and trims, so indentation and line wrapping
/// are not differences.
fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Options {
        Options::from_specs(structural::OPTIONS)
    }

    fn run(left: &str, right: &str) -> String {
        XmlDiff.run(Input::pair(left, right), &opts()).unwrap()
    }

    #[test]
    fn attribute_order_is_not_a_difference() {
        assert!(run(r#"<a x="1" y="2"/>"#, r#"<a y="2" x="1"/>"#).starts_with("No differences."));
    }

    #[test]
    fn indentation_and_line_wrapping_are_not_differences() {
        let left = "<root>\n  <item>hello   world</item>\n</root>";
        let right = "<root><item>hello world</item></root>";
        assert!(
            run(left, right).starts_with("No differences."),
            "{}",
            run(left, right)
        );
    }

    #[test]
    fn empty_elements_spelled_either_way_are_the_same() {
        assert!(run("<a/>", "<a></a>").starts_with("No differences."));
    }

    #[test]
    fn comments_and_declarations_are_ignored() {
        let left = r#"<?xml version="1.0"?><!-- note --><a>1</a>"#;
        assert!(run(left, "<a>1</a>").starts_with("No differences."));
    }

    #[test]
    fn reports_a_changed_attribute_with_its_path() {
        let out = run(r#"<a id="1"/>"#, r#"<a id="2"/>"#);
        assert!(out.contains(r#"~ $.a.@id: "1" → "2""#), "{out}");
    }

    #[test]
    fn reports_changed_text() {
        let out = run("<a>one</a>", "<a>two</a>");
        assert!(out.contains(r#"~ $.a.#text: "one" → "two""#), "{out}");
    }

    #[test]
    fn a_lone_child_is_still_a_sequence_of_one() {
        // So that gaining a sibling reads as an addition, not a type change.
        let out = run("<r><i>1</i></r>", "<r><i>2</i></r>");
        assert!(out.contains(r#"~ $.r.i[0].#text: "1" → "2""#), "{out}");
    }

    #[test]
    fn reports_added_and_removed_elements() {
        let out = run("<r><a/></r>", "<r><b/></r>");
        assert!(out.contains("- $.r.a"), "{out}");
        assert!(out.contains("+ $.r.b"), "{out}");
    }

    #[test]
    fn repeated_siblings_are_compared_in_order() {
        let out = run("<r><i>1</i><i>2</i></r>", "<r><i>1</i><i>9</i></r>");
        assert!(out.contains(r#"~ $.r.i[1].#text: "2" → "9""#), "{out}");
    }

    #[test]
    fn an_extra_repeated_sibling_is_reported() {
        let out = run("<r><i>1</i></r>", "<r><i>1</i><i>2</i></r>");
        assert!(out.contains("+ $.r.i"), "{out}");
    }

    #[test]
    fn different_prefixes_for_one_namespace_match() {
        let left = r#"<a:root xmlns:a="urn:x"><a:item/></a:root>"#;
        let right = r#"<b:root xmlns:b="urn:x"><b:item/></b:root>"#;
        assert!(
            run(left, right).starts_with("No differences."),
            "{}",
            run(left, right)
        );
    }

    #[test]
    fn one_prefix_bound_to_different_namespaces_does_not_match() {
        let left = r#"<a:root xmlns:a="urn:x"/>"#;
        let right = r#"<a:root xmlns:a="urn:y"/>"#;
        assert!(
            run(left, right).contains("difference"),
            "{}",
            run(left, right)
        );
    }

    #[test]
    fn order_between_differently_named_children_is_not_compared() {
        assert!(run("<r><a/><b/></r>", "<r><b/><a/></r>").starts_with("No differences."));
    }

    #[test]
    fn waits_for_both_sides() {
        assert_eq!(run("", ""), "");
        assert_eq!(run("<a/>", ""), "");
    }

    #[test]
    fn invalid_xml_names_the_side() {
        let err = XmlDiff
            .run(Input::pair("<a><b></a>", "<a/>"), &opts())
            .unwrap_err();
        assert!(err.0.starts_with("Original is not valid XML"), "{}", err.0);
    }
}
