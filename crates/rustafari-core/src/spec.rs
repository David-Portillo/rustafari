//! The UI-agnostic contract every tool implements.
//!
//! A tool declares *what* knobs it has; it never says how they are drawn. The
//! frontend renders `OptionSpec`s generically, so adding a tool requires no UI
//! code at all.

use std::collections::HashMap;
use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Category {
    Formatters,
    Encoders,
    Generators,
    Text,
}

impl Category {
    pub const ALL: &'static [Category] = &[
        Category::Formatters,
        Category::Encoders,
        Category::Generators,
        Category::Text,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Formatters => "Formatters",
            Category::Encoders => "Encoders & Decoders",
            Category::Generators => "Generators",
            Category::Text => "Text",
        }
    }
}

/// Identity and search metadata for a tool.
#[derive(Clone, Copy, Debug)]
pub struct ToolMeta {
    /// Stable id, used for persistence. Never change it once released.
    pub id: &'static str,
    pub name: &'static str,
    pub category: Category,
    pub description: &'static str,
    /// Extra search terms that don't appear in the name.
    pub keywords: &'static [&'static str],
}

/// Whether the tool transforms text or produces it from nothing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputMode {
    /// Runs continuously as the user types.
    Text { placeholder: &'static str },
    /// No input pane; runs when the user asks for it.
    None,
}

#[derive(Clone, Copy, Debug)]
pub enum OptionSpec {
    Toggle {
        id: &'static str,
        label: &'static str,
        default: bool,
    },
    /// A one-of-N picker. Choices are `(value, label)` pairs.
    Choice {
        id: &'static str,
        label: &'static str,
        choices: &'static [(&'static str, &'static str)],
        default: &'static str,
    },
    Number {
        id: &'static str,
        label: &'static str,
        min: i64,
        max: i64,
        default: i64,
    },
}

impl OptionSpec {
    pub fn id(&self) -> &'static str {
        match self {
            OptionSpec::Toggle { id, .. }
            | OptionSpec::Choice { id, .. }
            | OptionSpec::Number { id, .. } => id,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            OptionSpec::Toggle { label, .. }
            | OptionSpec::Choice { label, .. }
            | OptionSpec::Number { label, .. } => label,
        }
    }

    fn default_value(&self) -> OptionValue {
        match self {
            OptionSpec::Toggle { default, .. } => OptionValue::Bool(*default),
            OptionSpec::Choice { default, .. } => OptionValue::Choice((*default).to_string()),
            OptionSpec::Number { default, .. } => OptionValue::Number(*default),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum OptionValue {
    Bool(bool),
    Choice(String),
    Number(i64),
}

/// The current setting of every knob a tool declared.
///
/// Built from the specs, so every lookup has a default and the getters cannot
/// fail — tool implementations never deal with missing options.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Options {
    values: HashMap<&'static str, OptionValue>,
}

impl Options {
    pub fn from_specs(specs: &[OptionSpec]) -> Self {
        Options {
            values: specs.iter().map(|s| (s.id(), s.default_value())).collect(),
        }
    }

    pub fn get(&self, id: &str) -> Option<&OptionValue> {
        self.values.get(id)
    }

    pub fn set(&mut self, id: &'static str, value: OptionValue) {
        self.values.insert(id, value);
    }

    pub fn bool(&self, id: &str) -> bool {
        matches!(self.values.get(id), Some(OptionValue::Bool(true)))
    }

    pub fn choice(&self, id: &str) -> &str {
        match self.values.get(id) {
            Some(OptionValue::Choice(v)) => v,
            _ => "",
        }
    }

    pub fn number(&self, id: &str) -> i64 {
        match self.values.get(id) {
            Some(OptionValue::Number(v)) => *v,
            _ => 0,
        }
    }
}

/// A human-readable failure. Tools surface these to the output pane, so the
/// message is user-facing copy, not a debug dump.
#[derive(Clone, PartialEq, Debug)]
pub struct ToolError(pub String);

impl ToolError {
    pub fn new(msg: impl Into<String>) -> Self {
        ToolError(msg.into())
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ToolError {}

pub type ToolResult = Result<String, ToolError>;

pub trait Tool: Send + Sync {
    fn meta(&self) -> ToolMeta;

    fn input_mode(&self) -> InputMode {
        InputMode::Text {
            placeholder: "Paste your input here",
        }
    }

    fn options(&self) -> &'static [OptionSpec] {
        &[]
    }

    fn run(&self, input: &str, opts: &Options) -> ToolResult;
}
