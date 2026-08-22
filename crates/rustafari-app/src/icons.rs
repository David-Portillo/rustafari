//! Line icons from [Lucide](https://lucide.dev) (ISC, see `assets/LUCIDE-LICENSE`).
//!
//! The bundled font is subset to only the glyphs below — 8 KB rather than the
//! 854 KB full set. To add an icon, add its codepoint here and re-run
//! `scripts/subset-fonts.sh`, which regenerates the font from this file.
//!
//! Codepoints sit in the Private Use Area, so they can never collide with real
//! text. That lets the icon font act as a fallback on every font family (see
//! `fonts.rs`), and an icon is then just a `&str` usable in any label or button.

pub const SEARCH: &str = "\u{E151}";
pub const SETTINGS: &str = "\u{E245}";
pub const COPY: &str = "\u{E09E}";
pub const CHECK: &str = "\u{E06C}";
pub const REFRESH: &str = "\u{E145}";
pub const SUN: &str = "\u{E178}";
pub const MOON: &str = "\u{E11E}";
pub const MONITOR: &str = "\u{E11D}";
pub const WRAP_TEXT: &str = "\u{E248}";
pub const ALERT: &str = "\u{E193}";
pub const BRACES: &str = "\u{E36A}";
pub const BINARY: &str = "\u{E1F2}";
pub const LINK: &str = "\u{E102}";
pub const HASH: &str = "\u{E0EF}";
pub const FINGERPRINT: &str = "\u{E2CB}";
pub const WRENCH: &str = "\u{E1B1}";
pub const ROTATE: &str = "\u{E148}";
pub const TYPE: &str = "\u{E198}";
pub const X: &str = "\u{E1B2}";
pub const DIFF: &str = "\u{E359}";
pub const ARROW_RIGHT: &str = "\u{E049}";
pub const CLOCK: &str = "\u{E304}";
pub const COLUMNS: &str = "\u{E098}";
