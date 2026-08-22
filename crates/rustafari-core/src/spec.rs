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

/// Which pane a group of options belongs beside.
///
/// Options live in the header of the pane they act on: the ones that decide
/// the answer sit with the input, the ones that decide how it reads sit with
/// the output. Keeping them apart from the panes made a tall block that
/// squeezed everything else at small window sizes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum OptionPane {
    #[default]
    Input,
    Output,
}

/// Whether the tool transforms text or produces it from nothing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputMode {
    /// Runs continuously as the user types.
    Text { placeholder: &'static str },
    /// Two documents, for tools that compare rather than transform. The
    /// frontend shows a pane per side; the labels name them.
    TwoText {
        left_label: &'static str,
        right_label: &'static str,
        placeholder: &'static str,
    },
    /// No input pane; runs when the user asks for it.
    None,
}

/// What the user typed.
///
/// `right` is empty for every mode except [`InputMode::TwoText`], so a tool
/// that only declared one input can read `left` and ignore the rest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Input<'a> {
    pub left: &'a str,
    pub right: &'a str,
}

impl<'a> Input<'a> {
    pub fn pair(left: &'a str, right: &'a str) -> Self {
        Input { left, right }
    }
}

impl<'a> From<&'a str> for Input<'a> {
    fn from(left: &'a str) -> Self {
        Input { left, right: "" }
    }
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
    /// Marks where the options that follow belong. Carries no value of its
    /// own; it exists so a tool with many knobs can say which of them decide
    /// the answer and which decide how it is presented.
    ///
    /// Options declared before any group default to [`OptionPane::Input`].
    Group {
        label: &'static str,
        pane: OptionPane,
    },
    /// A short free-text field, for values no picker can enumerate — a cron
    /// field, a delimiter, a format string. Rendered inline with the other
    /// options, so keep it genuinely short.
    Text {
        id: &'static str,
        label: &'static str,
        placeholder: &'static str,
        default: &'static str,
    },
}

impl OptionSpec {
    pub fn id(&self) -> &'static str {
        match self {
            OptionSpec::Toggle { id, .. }
            | OptionSpec::Choice { id, .. }
            | OptionSpec::Number { id, .. }
            | OptionSpec::Text { id, .. } => id,
            // A heading holds no value, so it answers to no id.
            OptionSpec::Group { .. } => "",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            OptionSpec::Toggle { label, .. }
            | OptionSpec::Choice { label, .. }
            | OptionSpec::Number { label, .. }
            | OptionSpec::Text { label, .. }
            | OptionSpec::Group { label, .. } => label,
        }
    }

    /// Whether this spec carries a value the user can set.
    pub fn is_value(&self) -> bool {
        !matches!(self, OptionSpec::Group { .. })
    }

    fn default_value(&self) -> OptionValue {
        match self {
            OptionSpec::Toggle { default, .. } => OptionValue::Bool(*default),
            OptionSpec::Choice { default, .. } => OptionValue::Choice((*default).to_string()),
            OptionSpec::Number { default, .. } => OptionValue::Number(*default),
            OptionSpec::Text { default, .. } => OptionValue::Text((*default).to_string()),
            OptionSpec::Group { .. } => OptionValue::Bool(false),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum OptionValue {
    Bool(bool),
    Choice(String),
    Number(i64),
    Text(String),
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
            values: specs
                .iter()
                .filter(|s| s.is_value())
                .map(|s| (s.id(), s.default_value()))
                .collect(),
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

    pub fn text(&self, id: &str) -> &str {
        match self.values.get(id) {
            Some(OptionValue::Text(v)) => v,
            _ => "",
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

/// What a tool's text *is*, so a chain can refuse a link that cannot work.
///
/// Deliberately coarse. This exists to keep "send my list of names to the JSON
/// formatter" out of a menu, not to type-check a pipeline; anything finer would
/// tax every future tool for no gain.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Format {
    /// No claim made — the text could be anything. What a decoder produces,
    /// and what a tool that takes arbitrary text accepts.
    Any,
    /// Human-readable text that is not structured data: a digest, a report,
    /// a list of names. Nothing parses it, so it flows nowhere in particular.
    Plain,
    Json,
    Yaml,
    Xml,
    Base64,
}

impl Format {
    /// Whether text of this format can be fed to a tool accepting `accepted`.
    ///
    /// `Any` on either side waves the check through, and that is the point:
    /// Base64-decode declares its output `Any` because it genuinely does not
    /// know, which keeps the everyday chain — decode this, now pretty-print
    /// it — in the menu. Only a format that is *known* and *wrong* is refused.
    pub fn flows_into(self, accepted: &[Format]) -> bool {
        self == Format::Any || accepted.contains(&Format::Any) || accepted.contains(&self)
    }
}

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

    /// Formats this tool can be handed as input. The default accepts anything.
    fn accepts(&self) -> &'static [Format] {
        &[Format::Any]
    }

    /// What `run` will produce. Takes the options because for several tools
    /// the answer depends on them — Base64 encodes to Base64 but decodes to
    /// anything at all.
    fn produces(&self, _opts: &Options) -> Format {
        Format::Any
    }

    fn run(&self, input: Input<'_>, opts: &Options) -> ToolResult;
}
