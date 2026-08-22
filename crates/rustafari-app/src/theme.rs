//! The visual design: a palette and the egui `Visuals` derived from it.
//!
//! egui's stock light and dark themes are deliberately neutral. Everything here
//! exists to replace them with one deliberate look — flat surfaces separated by
//! hairline borders instead of bevels, generous rounding, a single accent
//! colour, and text in three weights (primary, secondary, muted) so hierarchy
//! comes from contrast rather than from boxes.

use eframe::egui::{
    style::{Selection, WidgetVisuals, Widgets},
    Color32, FontFamily, FontId, Rounding, Stroke, TextStyle, Visuals,
};

/// Every colour the interface uses. Defining both themes as the same set of
/// roles keeps them in step: a widget reads `palette.surface`, never a literal.
#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    /// Window background, the furthest-back layer.
    pub base: Color32,
    /// Sidebar and pane backgrounds, one step forward.
    pub surface: Color32,
    /// Menus, hover states and the settings window.
    pub elevated: Color32,
    /// Hairline dividers and widget outlines.
    pub border: Color32,
    pub text: Color32,
    pub text_secondary: Color32,
    pub text_muted: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    /// Accent at low opacity, for the selected row and icon tiles.
    pub accent_soft: Color32,
    pub danger: Color32,
    pub danger_soft: Color32,
}

impl Palette {
    pub const DARK: Palette = Palette {
        dark: true,
        base: Color32::from_rgb(0x0E, 0x10, 0x14),
        surface: Color32::from_rgb(0x15, 0x18, 0x1E),
        elevated: Color32::from_rgb(0x1C, 0x20, 0x29),
        border: Color32::from_rgb(0x27, 0x2C, 0x37),
        text: Color32::from_rgb(0xE8, 0xEB, 0xF2),
        text_secondary: Color32::from_rgb(0xA8, 0xB1, 0xC2),
        text_muted: Color32::from_rgb(0x6B, 0x74, 0x86),
        accent: Color32::from_rgb(0x7C, 0x6B, 0xF5),
        accent_hover: Color32::from_rgb(0x93, 0x85, 0xF7),
        accent_soft: Color32::from_rgb(0x22, 0x22, 0x3D),
        danger: Color32::from_rgb(0xF4, 0x7A, 0x7A),
        danger_soft: Color32::from_rgb(0x2E, 0x1A, 0x1E),
    };

    pub const LIGHT: Palette = Palette {
        dark: false,
        base: Color32::from_rgb(0xF7, 0xF8, 0xFA),
        surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        elevated: Color32::from_rgb(0xFF, 0xFF, 0xFF),
        border: Color32::from_rgb(0xE3, 0xE6, 0xEC),
        text: Color32::from_rgb(0x14, 0x17, 0x1D),
        text_secondary: Color32::from_rgb(0x4B, 0x54, 0x64),
        text_muted: Color32::from_rgb(0x8A, 0x92, 0xA1),
        accent: Color32::from_rgb(0x5B, 0x4B, 0xE0),
        accent_hover: Color32::from_rgb(0x4B, 0x3C, 0xCC),
        accent_soft: Color32::from_rgb(0xEE, 0xEC, 0xFD),
        danger: Color32::from_rgb(0xC5, 0x3B, 0x3B),
        danger_soft: Color32::from_rgb(0xFD, 0xEE, 0xEE),
    };

    pub fn for_dark_mode(dark: bool) -> Self {
        if dark {
            Palette::DARK
        } else {
            Palette::LIGHT
        }
    }
}

pub const ROUNDING: f32 = 8.0;
pub const ROUNDING_SMALL: f32 = 6.0;

/// The one-pixel border used throughout. Also pins the width to `f32`, which
/// `Stroke::new` cannot infer from a bare literal.
pub fn hairline(color: Color32) -> Stroke {
    Stroke::new(1.0_f32, color)
}

/// Builds egui `Visuals` from a palette.
pub fn visuals(p: Palette) -> Visuals {
    let mut v = if p.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };

    v.dark_mode = p.dark;
    v.override_text_color = Some(p.text);
    v.panel_fill = p.base;
    v.window_fill = p.elevated;
    v.faint_bg_color = p.surface;
    // TextEdit and other "sunken" areas.
    v.extreme_bg_color = p.surface;
    v.hyperlink_color = p.accent;

    v.window_rounding = Rounding::same(12.0);
    v.window_stroke = hairline(p.border);
    v.menu_rounding = Rounding::same(ROUNDING);

    // egui's default shadows are heavy enough to muddy a flat design.
    v.window_shadow.color = Color32::from_black_alpha(if p.dark { 96 } else { 24 });
    v.popup_shadow.color = Color32::from_black_alpha(if p.dark { 80 } else { 20 });

    v.selection = Selection {
        bg_fill: p.accent.linear_multiply(0.35),
        stroke: hairline(p.text),
    };

    // Fill the slider track up to the handle, so the value reads at a glance.
    v.slider_trailing_fill = true;

    let rounding = Rounding::same(ROUNDING_SMALL);
    v.widgets = Widgets {
        // Backgrounds and separators that never react to the pointer.
        noninteractive: WidgetVisuals {
            bg_fill: p.surface,
            weak_bg_fill: p.surface,
            bg_stroke: hairline(p.border),
            fg_stroke: hairline(p.text_secondary),
            rounding,
            expansion: 0.0,
        },
        inactive: WidgetVisuals {
            bg_fill: p.elevated,
            weak_bg_fill: p.elevated,
            bg_stroke: hairline(p.border),
            fg_stroke: hairline(p.text_secondary),
            rounding,
            expansion: 0.0,
        },
        hovered: WidgetVisuals {
            bg_fill: p.accent_soft,
            weak_bg_fill: p.accent_soft,
            bg_stroke: hairline(p.accent),
            fg_stroke: hairline(p.text),
            rounding,
            // Stock egui grows widgets on hover, which makes rows jitter.
            expansion: 0.0,
        },
        // White text, because an active widget is filled with the accent.
        //
        // Beware: egui also derives `strong_text_color()` from this stroke, so
        // **`RichText::strong()` without an explicit colour renders white** —
        // fine on dark, invisible on light. Always pair `.strong()` with
        // `.color(...)`; the settings window title was briefly a blank space in
        // the light theme for exactly this reason.
        active: WidgetVisuals {
            bg_fill: p.accent,
            weak_bg_fill: p.accent,
            bg_stroke: hairline(p.accent),
            fg_stroke: hairline(Color32::WHITE),
            rounding,
            expansion: 0.0,
        },
        open: WidgetVisuals {
            bg_fill: p.elevated,
            weak_bg_fill: p.elevated,
            bg_stroke: hairline(p.accent),
            fg_stroke: hairline(p.text),
            rounding,
            expansion: 0.0,
        },
    };

    v
}

/// Text sizes, tuned for Inter. `mono_size` is the only user-adjustable one;
/// the rest scale via the interface zoom factor so the chrome keeps its
/// proportions.
pub fn text_styles(mono_size: f32) -> std::collections::BTreeMap<TextStyle, FontId> {
    use FontFamily::{Monospace, Proportional};
    [
        (TextStyle::Heading, FontId::new(18.0, Proportional)),
        (TextStyle::Body, FontId::new(13.0, Proportional)),
        (TextStyle::Button, FontId::new(13.0, Proportional)),
        (TextStyle::Small, FontId::new(11.0, Proportional)),
        (TextStyle::Monospace, FontId::new(mono_size, Monospace)),
    ]
    .into()
}

/// Spacing that makes the stock widgets sit comfortably with the palette:
/// a little airier than egui's defaults, without becoming a touch UI.
pub fn apply_spacing(style: &mut eframe::egui::Style) {
    let s = &mut style.spacing;
    s.item_spacing = eframe::egui::vec2(8.0, 8.0);
    s.button_padding = eframe::egui::vec2(10.0, 6.0);
    s.menu_margin = eframe::egui::Margin::same(6.0);
    s.slider_width = 160.0;
    // Minimum widget height. 18 is egui's default and reads cramped.
    s.interact_size.y = 24.0;
    s.scroll = eframe::egui::style::ScrollStyle::thin();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sum of per-channel difference. Crude, but enough to catch two roles
    /// that are the same colour or a shade apart.
    fn distance(a: Color32, b: Color32) -> i32 {
        (i32::from(a.r()) - i32::from(b.r())).abs()
            + (i32::from(a.g()) - i32::from(b.g())).abs()
            + (i32::from(a.b()) - i32::from(b.b())).abs()
    }

    /// `border` is what gets drawn *onto* the other surfaces — hairlines,
    /// widget outlines, and the slider track. When it matched the surface
    /// underneath, the track was painted in the background colour and the
    /// slider looked like it stopped at its handle.
    #[test]
    fn the_border_colour_is_visible_on_every_surface() {
        for p in [Palette::DARK, Palette::LIGHT] {
            for (name, surface) in [
                ("base", p.base),
                ("surface", p.surface),
                ("elevated", p.elevated),
            ] {
                assert!(
                    distance(p.border, surface) >= 20,
                    "border is invisible on {name} in the {} theme",
                    if p.dark { "dark" } else { "light" }
                );
            }
        }
    }

    /// Text has to read against the surfaces too, and the muted weight is the
    /// one most likely to drift into invisibility.
    #[test]
    fn muted_text_is_readable_on_every_surface() {
        for p in [Palette::DARK, Palette::LIGHT] {
            for surface in [p.base, p.surface, p.elevated] {
                assert!(distance(p.text_muted, surface) >= 100);
            }
        }
    }
}
