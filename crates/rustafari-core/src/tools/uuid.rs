use uuid::Uuid;

use crate::spec::*;

pub struct UuidTool;

const OPTIONS: &[OptionSpec] = &[
    OptionSpec::Choice {
        id: "version",
        label: "Version",
        choices: &[("v4", "v4 (random)"), ("v7", "v7 (time-ordered)")],
        default: "v4",
    },
    OptionSpec::Number {
        id: "count",
        label: "How many",
        min: 1,
        max: 1000,
        default: 5,
    },
    OptionSpec::Toggle {
        id: "uppercase",
        label: "Uppercase",
        default: false,
    },
    OptionSpec::Toggle {
        id: "hyphens",
        label: "Hyphens",
        default: true,
    },
];

impl Tool for UuidTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            id: "uuid",
            name: "UUID Generator",
            category: Category::Generators,
            description: "Generate random (v4) or time-ordered (v7) UUIDs in bulk.",
            keywords: &["uuid", "guid", "identifier", "random"],
        }
    }

    fn input_mode(&self) -> InputMode {
        InputMode::None
    }

    fn options(&self) -> &'static [OptionSpec] {
        OPTIONS
    }

    fn produces(&self, _opts: &Options) -> Format {
        Format::Plain
    }

    fn run(&self, _input: Input<'_>, opts: &Options) -> ToolResult {
        let count = opts.number("count").clamp(1, 1000) as usize;
        let time_ordered = opts.choice("version") == "v7";
        let hyphens = opts.bool("hyphens");
        let uppercase = opts.bool("uppercase");

        let mut out = String::with_capacity(count * 37);
        for _ in 0..count {
            let id = if time_ordered {
                Uuid::now_v7()
            } else {
                Uuid::new_v4()
            };

            let text = if hyphens {
                id.hyphenated().to_string()
            } else {
                id.simple().to_string()
            };

            out.push_str(&if uppercase { text.to_uppercase() } else { text });
            out.push('\n');
        }
        out.pop();
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Options {
        Options::from_specs(OPTIONS)
    }

    #[test]
    fn generates_the_requested_count_without_a_trailing_blank_line() {
        let out = UuidTool.run("".into(), &opts()).unwrap();
        let lines: Vec<_> = out.lines().collect();
        assert_eq!(lines.len(), 5);
        assert!(!out.ends_with('\n'));
        assert!(lines.iter().all(|l| l.len() == 36));
    }

    #[test]
    fn values_are_unique() {
        let out = UuidTool.run("".into(), &opts()).unwrap();
        let mut lines: Vec<_> = out.lines().collect();
        lines.sort_unstable();
        lines.dedup();
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn v7_is_time_ordered() {
        let mut o = opts();
        o.set("version", OptionValue::Choice("v7".into()));
        o.set("count", OptionValue::Number(20));
        let out = UuidTool.run("".into(), &o).unwrap();
        let generated: Vec<&str> = out.lines().collect();
        let mut sorted = generated.clone();
        sorted.sort_unstable();
        assert_eq!(generated, sorted);
    }

    #[test]
    fn formatting_options_apply() {
        let mut o = opts();
        o.set("count", OptionValue::Number(1));
        o.set("hyphens", OptionValue::Bool(false));
        o.set("uppercase", OptionValue::Bool(true));
        let out = UuidTool.run("".into(), &o).unwrap();
        assert_eq!(out.len(), 32);
        assert!(!out.contains('-'));
        assert_eq!(out, out.to_uppercase());
    }

    #[test]
    fn count_is_clamped_to_the_declared_range() {
        let mut o = opts();
        o.set("count", OptionValue::Number(-3));
        assert_eq!(UuidTool.run("".into(), &o).unwrap().lines().count(), 1);
    }
}
