//! Small custom widgets that egui's stock set doesn't cover in the shape we
//! want. Each one is a plain function taking the palette, so they carry no
//! state of their own and can be dropped anywhere.

use eframe::egui::{
    self, Align2, Color32, CursorIcon, FontId, Rect, Response, Rounding, Sense, TextStyle, Ui, Vec2,
};

use crate::theme::{self, Palette};

/// A square, borderless icon button that tints on hover.
pub fn icon_button(ui: &mut Ui, icon: &str, p: Palette, active: bool) -> Response {
    let size = Vec2::splat(ui.text_style_height(&TextStyle::Body) + 12.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let lit = active || response.hovered();

    if lit {
        ui.painter().rect_filled(
            rect,
            Rounding::same(theme::ROUNDING_SMALL),
            if active { p.accent_soft } else { p.surface },
        );
    }

    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon,
        FontId::proportional(15.0),
        if lit { p.accent } else { p.text_secondary },
    );

    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// One button of a segmented control.
///
/// The label is laid out once with a placeholder colour and recoloured at
/// paint time, rather than laid out twice (once to measure, once to draw).
pub fn segment(ui: &mut Ui, label: &str, p: Palette, active: bool) -> Response {
    let font = TextStyle::Body.resolve(ui.style());
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font, Color32::PLACEHOLDER);

    let size = Vec2::new(galley.size().x + 18.0, galley.size().y + 10.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    let rounding = Rounding::same(theme::ROUNDING_SMALL - 1.0);

    let (fill, text) = match (active, response.hovered()) {
        (true, true) => (Some(p.accent_hover), Color32::WHITE),
        (true, false) => (Some(p.accent), Color32::WHITE),
        (false, true) => (Some(p.elevated), p.text),
        (false, false) => (None, p.text_secondary),
    };

    if let Some(fill) = fill {
        ui.painter().rect_filled(rect, rounding, fill);
    }
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, text);

    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// An animated on/off switch. Reads as a preference where a checkbox reads as
/// a form field, and unlike egui's checkbox it can show its "on" state in the
/// accent colour.
pub fn toggle(ui: &mut Ui, on: &mut bool, p: Palette) -> Response {
    let height = ui.text_style_height(&TextStyle::Body) + 4.0;
    let size = Vec2::new(height * 1.8, height);
    let (rect, mut response) = ui.allocate_exact_size(size, Sense::click());

    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }

    // 0.0 = off, 1.0 = on, eased between the two over ~0.1 s.
    let t = ui.ctx().animate_bool(response.id, *on);
    let radius = rect.height() / 2.0;

    let track = lerp_color(
        if response.hovered() {
            p.elevated
        } else {
            p.surface
        },
        if response.hovered() {
            p.accent_hover
        } else {
            p.accent
        },
        t,
    );
    let painter = ui.painter();
    painter.rect(
        rect,
        Rounding::same(radius),
        track,
        theme::hairline(lerp_color(p.border, p.accent, t)),
    );

    let knob_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), t);
    painter.circle_filled(
        egui::pos2(knob_x, rect.center().y),
        radius - 3.0,
        Color32::WHITE,
    );

    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// The draggable divider between two panes. Returns the drag delta along the
/// split axis, in points, if the user moved it this frame.
///
/// `rect` is the strip the handle occupies; it should be wider than the line
/// it draws so it is easy to grab. `vertical_line` is true when the panes sit
/// side by side.
pub fn splitter(ui: &mut Ui, rect: Rect, vertical_line: bool, p: Palette) -> Option<f32> {
    let response = ui.interact(rect, ui.id().with("splitter"), Sense::drag());
    let lit = response.hovered() || response.dragged();

    // Draw a hairline through the middle, thickening to the accent when live.
    let thickness = if lit { 2.0 } else { 1.0 };
    let color = if lit { p.accent } else { p.border };
    let line = if vertical_line {
        Rect::from_center_size(rect.center(), Vec2::new(thickness, rect.height()))
    } else {
        Rect::from_center_size(rect.center(), Vec2::new(rect.width(), thickness))
    };
    ui.painter().rect_filled(line, Rounding::same(1.0), color);

    if lit {
        ui.ctx().set_cursor_icon(if vertical_line {
            CursorIcon::ResizeHorizontal
        } else {
            CursorIcon::ResizeVertical
        });
    }

    if response.dragged() {
        let delta = response.drag_delta();
        Some(if vertical_line { delta.x } else { delta.y })
    } else {
        None
    }
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let lerp = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(
        lerp(a.r(), b.r()),
        lerp(a.g(), b.g()),
        lerp(a.b(), b.b()),
        lerp(a.a(), b.a()),
    )
}
