//! Offline developer utilities, free of any UI dependency.
//!
//! Everything here is pure and synchronous: no network, no filesystem, no
//! globals. The GUI is one consumer; a CLI or a test harness could be another.
//!
//! ## Adding a tool
//!
//! 1. Write `src/tools/<name>.rs` with a unit struct that implements [`Tool`].
//! 2. Register it in the `mod tools` list and in [`all_tools`].
//!
//! There is no third step — the frontend renders whatever options the tool
//! declares.

mod spec;
mod tools;

pub use spec::{
    Category, Format, Input, InputMode, OptionPane, OptionSpec, OptionValue, Options, Tool,
    ToolError, ToolMeta, ToolResult,
};

/// Every tool the app ships, in menu order.
pub fn all_tools() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(tools::json::JsonFormatter),
        Box::new(tools::json_diff::JsonDiff),
        Box::new(tools::yaml_diff::YamlDiff),
        Box::new(tools::xml_diff::XmlDiff),
        Box::new(tools::base64::Base64Tool),
        Box::new(tools::url::UrlTool),
        Box::new(tools::hash::HashTool),
        Box::new(tools::uuid::UuidTool),
        Box::new(tools::cron::Cron),
        Box::new(tools::list_compare::ListCompare),
    ]
}

/// Case-insensitive match against a tool's name, description and keywords.
pub fn matches_query(meta: &ToolMeta, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }

    query.split_whitespace().all(|term| {
        meta.name.to_lowercase().contains(term)
            || meta.description.to_lowercase().contains(term)
            || meta.keywords.iter().any(|k| k.contains(term))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn tool_ids_are_unique() {
        let tools = all_tools();
        let ids: HashSet<_> = tools.iter().map(|t| t.meta().id).collect();
        assert_eq!(ids.len(), tools.len(), "duplicate tool id");
    }

    #[test]
    fn every_declared_option_has_a_usable_default() {
        for tool in all_tools() {
            let specs = tool.options();
            let opts = Options::from_specs(specs);
            for spec in specs.iter().filter(|s| s.is_value()) {
                assert!(
                    opts.get(spec.id()).is_some(),
                    "{} option {} has no default",
                    tool.meta().id,
                    spec.id()
                );
            }
            // Empty input under default options is exactly what the user sees
            // when they open a tool, so it must never be an error.
            assert!(
                tool.run(Input::default(), &opts).is_ok(),
                "{}",
                tool.meta().id
            );
        }
    }

    #[test]
    fn choice_defaults_name_a_real_choice() {
        for tool in all_tools() {
            for spec in tool.options() {
                if let OptionSpec::Choice {
                    id,
                    choices,
                    default,
                    ..
                } = spec
                {
                    assert!(
                        choices.iter().any(|(value, _)| value == default),
                        "{}.{id} defaults to {default}, which is not a choice",
                        tool.meta().id
                    );
                }
            }
        }
    }

    /// The chaining menu is only worth having if it refuses links that cannot
    /// work, and only usable if it keeps the ones that can. Both halves have
    /// bitten: an unfiltered menu offered "send this list to the JSON
    /// formatter", and an over-eager filter would drop decode-then-pretty-print,
    /// which is the chain the feature exists for.
    #[test]
    fn chaining_refuses_impossible_links_and_keeps_possible_ones() {
        let tools = all_tools();
        let by_id = |id: &str| {
            tools
                .iter()
                .find(|t| t.meta().id == id)
                .expect("tool exists")
        };

        let json = by_id("json-formatter");
        let list = by_id("list-compare");
        let base64 = by_id("base64");

        // A list of names is not a JSON document.
        let list_opts = Options::from_specs(list.options());
        assert!(!list.produces(&list_opts).flows_into(json.accepts()));

        // ...but asking List Compare for JSON output makes it one.
        let mut as_json = Options::from_specs(list.options());
        as_json.set("format", OptionValue::Choice("json".into()));
        assert!(list.produces(&as_json).flows_into(json.accepts()));

        // Base64 *decode* has no idea what it produced, so it must not be
        // pruned: decode-then-format is the everyday chain.
        let mut decode = Options::from_specs(base64.options());
        decode.set("direction", OptionValue::Choice("decode".into()));
        assert!(base64.produces(&decode).flows_into(json.accepts()));

        // Encoding, though, is known to be Base64 and known not to be JSON.
        let encode = Options::from_specs(base64.options());
        assert!(!base64.produces(&encode).flows_into(json.accepts()));

        // Every tool that takes input must accept something.
        for tool in &tools {
            if tool.input_mode() != InputMode::None {
                assert!(!tool.accepts().is_empty(), "{}", tool.meta().id);
            }
        }
    }

    #[test]
    fn search_matches_name_and_keywords_and_requires_all_terms() {
        let tools = all_tools();
        let json = tools[0].meta();
        assert!(matches_query(&json, ""));
        assert!(matches_query(&json, "JSON"));
        assert!(matches_query(&json, "beautify"));
        assert!(matches_query(&json, "json minify"));
        assert!(!matches_query(&json, "json sha"));
    }
}
