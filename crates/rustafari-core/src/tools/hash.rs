use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};

use crate::spec::*;

pub struct HashTool;

const OPTIONS: &[OptionSpec] = &[
    OptionSpec::Choice {
        id: "algorithm",
        label: "Algorithm",
        choices: &[
            ("md5", "MD5"),
            ("sha1", "SHA-1"),
            ("sha256", "SHA-256"),
            ("sha512", "SHA-512"),
        ],
        default: "sha256",
    },
    OptionSpec::Toggle {
        id: "uppercase",
        label: "Uppercase",
        default: false,
    },
];

impl Tool for HashTool {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            id: "hash",
            name: "Hash Generator",
            category: Category::Generators,
            description: "Compute MD5, SHA-1, SHA-256 or SHA-512 digests of any text.",
            keywords: &["hash", "digest", "md5", "sha", "checksum"],
        }
    }

    fn input_mode(&self) -> InputMode {
        InputMode::Text {
            placeholder: "Text to hash",
        }
    }

    fn options(&self) -> &'static [OptionSpec] {
        OPTIONS
    }

    fn run(&self, input: &str, opts: &Options) -> ToolResult {
        let bytes = input.as_bytes();
        let digest = match opts.choice("algorithm") {
            "md5" => hex::encode(Md5::digest(bytes)),
            "sha1" => hex::encode(Sha1::digest(bytes)),
            "sha512" => hex::encode(Sha512::digest(bytes)),
            _ => hex::encode(Sha256::digest(bytes)),
        };

        Ok(if opts.bool("uppercase") {
            digest.to_uppercase()
        } else {
            digest
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> Options {
        Options::from_specs(OPTIONS)
    }

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            HashTool.run("abc", &opts()).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn md5_matches_known_vector() {
        let mut o = opts();
        o.set("algorithm", OptionValue::Choice("md5".into()));
        assert_eq!(
            HashTool.run("abc", &o).unwrap(),
            "900150983cd24fb0d6963f7d28e17f72"
        );
    }

    #[test]
    fn hashes_empty_input() {
        // Unlike the transform tools, an empty digest is a real answer.
        assert_eq!(
            HashTool.run("", &opts()).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn uppercase_option_applies() {
        let mut o = opts();
        o.set("uppercase", OptionValue::Bool(true));
        assert!(HashTool.run("abc", &o).unwrap().starts_with("BA7816BF"));
    }
}
