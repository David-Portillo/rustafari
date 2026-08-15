//! Typography. Replaces egui's bundled defaults entirely (the `default_fonts`
//! feature is off), which both drops ~1.4 MB from the binary and swaps the stock
//! Ubuntu-Light look for something deliberate.
//!
//! | Family        | Font                  | Why                                              |
//! | ------------- | --------------------- | ------------------------------------------------ |
//! | Proportional  | Inter Medium          | Medium, not Regular: egui's rasterizer is        |
//! |               |                       | unhinted, and Regular reads thin at 13–14 px.    |
//! | Monospace     | JetBrains Mono        | Built for code panes; clear 0/O and 1/l/I.       |
//! | fallback      | Noto Emoji            | So emoji in pasted JSON render, not tofu.        |
//! | fallback      | Lucide (subset)       | Icons, see `icons.rs`.                           |
//!
//! Inter and JetBrains Mono are subset to Latin, Latin Extended, Greek and
//! Cyrillic — the same coverage egui's defaults had — by
//! `scripts/subset-fonts.sh`. Licenses ship alongside in `assets/`.

use eframe::egui::{Context, FontData, FontDefinitions, FontFamily};

pub fn install(ctx: &Context) {
    let mut fonts = FontDefinitions::empty();

    for (name, bytes) in [
        ("inter", &include_bytes!("../assets/Inter-Medium.ttf")[..]),
        (
            "jetbrains-mono",
            &include_bytes!("../assets/JetBrainsMono-Regular.ttf")[..],
        ),
        (
            "noto-emoji",
            &include_bytes!("../assets/NotoEmoji-Regular.ttf")[..],
        ),
        ("lucide", &include_bytes!("../assets/lucide.ttf")[..]),
    ] {
        fonts
            .font_data
            .insert(name.to_owned(), FontData::from_static(bytes));
    }

    // Order within a family is fallback order: the first font that has the
    // glyph wins. Icons and emoji live in ranges the text fonts don't cover,
    // so they can safely trail every family.
    fonts.families.insert(
        FontFamily::Proportional,
        ["inter", "noto-emoji", "lucide"]
            .map(str::to_owned)
            .to_vec(),
    );
    fonts.families.insert(
        FontFamily::Monospace,
        ["jetbrains-mono", "inter", "noto-emoji", "lucide"]
            .map(str::to_owned)
            .to_vec(),
    );

    ctx.set_fonts(fonts);
}
