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
    Category, Input, InputMode, OptionSpec, OptionValue, Options, Tool, ToolError, ToolMeta,
    ToolResult,
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
            for spec in specs {
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
