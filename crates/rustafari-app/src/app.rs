use std::collections::HashMap;

use eframe::egui::{
    self, Align, Frame, Label, Layout, Margin, RichText, Rounding, ScrollArea, Sense, TextEdit,
    TextStyle, Vec2,
};
use rustafari_core::{
    all_tools, matches_query, Category, InputMode, OptionSpec, OptionValue, Options, Tool,
    ToolError,
};

use crate::icons;
use crate::settings::{self, Settings, Theme};
use crate::theme::{self, Palette};

const SIDEBAR_WIDTH: f32 = 232.0;

/// Per-tool editing state, so switching tools and coming back keeps your work.
struct ToolState {
    input: String,
    options: Options,
}

pub struct Rustafari {
    tools: Vec<Box<dyn Tool>>,
    states: HashMap<&'static str, ToolState>,
    selected: usize,
    query: String,
    /// Cached result of the selected tool; recomputed only when inputs change.
    output: Result<String, ToolError>,
    dirty: bool,
    settings: Settings,
    /// What was last pushed to egui, so we only restyle when something moved.
    applied: Option<Settings>,
    palette: Palette,
    settings_open: bool,
    /// Frame count remaining on the "Copied" confirmation.
    copied_ticks: u8,
}

impl Rustafari {
    /// eframe still handles window geometry through its own persistence; our
    /// settings live in a file of their own.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        icons::install(&cc.egui_ctx);

        let tools = all_tools();

        let states = tools
            .iter()
            .map(|tool| {
                (
                    tool.meta().id,
                    ToolState {
                        input: String::new(),
                        options: Options::from_specs(tool.options()),
                    },
                )
            })
            .collect();

        let settings = Settings::load();

        // Write the file on first run so it is there to be found and edited;
        // the settings window shows its path.
        if Settings::path().is_some_and(|path| !path.exists()) {
            settings.save();
        }

        // A tool removed since the settings were written falls back to the first.
        let selected = settings
            .selected_tool
            .as_deref()
            .and_then(|id| tools.iter().position(|t| t.meta().id == id))
            .unwrap_or(0);

        Rustafari {
            tools,
            states,
            selected,
            query: String::new(),
            output: Ok(String::new()),
            dirty: true,
            settings,
            applied: None,
            palette: Palette::DARK,
            settings_open: false,
            copied_ticks: 0,
        }
    }

    /// Pushes the current settings into egui's style. Cheap to call every
    /// frame: it does nothing unless a setting actually changed, which also
    /// lets the System theme follow the OS without a restart.
    fn apply_settings(&mut self, ctx: &egui::Context) {
        let dark = self.resolve_theme(ctx) == Theme::Dark;
        let unchanged = self.applied.as_ref() == Some(&self.settings);
        // The second check catches the OS flipping appearance underneath a
        // `System` theme, where our own settings have not changed at all.
        if unchanged && self.palette.dark == dark && self.applied.is_some() {
            return;
        }

        self.palette = Palette::for_dark_mode(dark);
        ctx.set_visuals(theme::visuals(self.palette));
        ctx.set_zoom_factor(self.settings.ui_scale);

        let mono_size = self.settings.font_size;
        ctx.style_mut(|style| {
            style.text_styles = theme::text_styles(mono_size);
            style.spacing.item_spacing = Vec2::new(8.0, 8.0);
            style.spacing.button_padding = Vec2::new(10.0, 6.0);
            style.spacing.menu_margin = Margin::same(6.0);
            style.spacing.slider_width = 160.0;
            style.visuals.widgets.noninteractive.bg_stroke.width = 1.0;
        });

        self.applied = Some(self.settings.clone());
    }

    /// `Theme::System` needs the OS preference, which egui only knows on
    /// platforms that report it; dark is the fallback.
    fn resolve_theme(&self, ctx: &egui::Context) -> Theme {
        match self.settings.theme {
            Theme::Light => Theme::Light,
            Theme::Dark => Theme::Dark,
            Theme::System => match ctx.input(|i| i.raw.system_theme) {
                Some(egui::Theme::Light) => Theme::Light,
                _ => Theme::Dark,
            },
        }
    }

    fn recompute(&mut self) {
        let tool = &self.tools[self.selected];
        let state = &self.states[tool.meta().id];
        self.output = tool.run(&state.input, &state.options);
        self.dirty = false;
    }

    // ---------------------------------------------------------------- sidebar

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new(icons::WRENCH).size(17.0).color(p.accent));
            ui.label(RichText::new("rustafari").size(17.0).strong().color(p.text));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, icons::SETTINGS, p, self.settings_open)
                    .on_hover_text("Settings")
                    .clicked()
                {
                    self.settings_open = !self.settings_open;
                }
            });
        });

        ui.add_space(12.0);

        // Search field: a framed row so the icon sits inside the input.
        Frame::none()
            .fill(p.surface)
            .stroke(theme::hairline(p.border))
            .rounding(Rounding::same(theme::ROUNDING_SMALL))
            .inner_margin(Margin::symmetric(9.0, 6.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(icons::SEARCH).color(p.text_muted));
                    ui.add(
                        TextEdit::singleline(&mut self.query)
                            .hint_text(RichText::new("Search tools").color(p.text_muted))
                            .desired_width(f32::INFINITY)
                            .frame(false),
                    );
                });
            });

        ui.add_space(10.0);

        ScrollArea::vertical().show(ui, |ui| {
            let mut any_shown = false;

            for category in Category::ALL {
                let in_category: Vec<usize> = self
                    .tools
                    .iter()
                    .enumerate()
                    .filter(|(_, tool)| {
                        let meta = tool.meta();
                        meta.category == *category && matches_query(&meta, &self.query)
                    })
                    .map(|(i, _)| i)
                    .collect();

                if in_category.is_empty() {
                    continue;
                }
                any_shown = true;

                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(category.label().to_uppercase())
                            .size(10.0)
                            .color(p.text_muted)
                            .strong(),
                    );
                });
                ui.add_space(4.0);

                for index in in_category {
                    let meta = self.tools[index].meta();
                    if self.tool_row(ui, index, meta.name, tool_icon(&meta)) {
                        self.selected = index;
                        self.settings.selected_tool = Some(meta.id.to_string());
                        // Saved here rather than only on exit, because a crash
                        // or a kill signal never reaches `on_exit`.
                        self.settings.save();
                        self.dirty = true;
                    }
                }
            }

            if !any_shown {
                ui.add_space(16.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("No tools match").color(p.text_muted));
                });
            }
        });
    }

    /// One tool in the sidebar: icon, name, and an accent-tinted background
    /// when selected. Returns true when clicked.
    fn tool_row(&self, ui: &mut egui::Ui, index: usize, name: &str, icon: &str) -> bool {
        let p = self.palette;
        let selected = index == self.selected;

        let height = ui.text_style_height(&TextStyle::Body) + 14.0;
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());

        let hovered = response.hovered();
        if selected || hovered {
            ui.painter().rect_filled(
                rect,
                Rounding::same(theme::ROUNDING_SMALL),
                if selected { p.accent_soft } else { p.surface },
            );
        }
        if selected {
            // A short accent bar reads as "current" without shouting.
            let bar = egui::Rect::from_min_size(
                rect.left_top() + Vec2::new(0.0, height * 0.25),
                Vec2::new(2.5, height * 0.5),
            );
            ui.painter().rect_filled(bar, Rounding::same(2.0), p.accent);
        }

        let color = if selected { p.accent } else { p.text_secondary };
        let text_color = if selected { p.text } else { p.text_secondary };

        let mut content = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(Vec2::new(12.0, 0.0)))
                .layout(Layout::left_to_right(Align::Center)),
        );
        content.label(RichText::new(icon).color(color));
        content.add_space(2.0);
        content.label(RichText::new(name).color(text_color));

        response.clicked()
    }

    // ------------------------------------------------------------------ main

    fn header(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let meta = self.tools[self.selected].meta();

        ui.horizontal(|ui| {
            // Icon tile: accent glyph on an accent-tinted rounded square.
            let size = 38.0;
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
            ui.painter()
                .rect_filled(rect, Rounding::same(theme::ROUNDING), p.accent_soft);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                tool_icon(&meta),
                egui::FontId::proportional(19.0),
                p.accent,
            );

            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.add_space(1.0);
                ui.label(RichText::new(meta.name).size(17.0).strong().color(p.text));
                ui.label(
                    RichText::new(meta.description)
                        .size(12.0)
                        .color(p.text_muted),
                );
            });
        });
    }

    /// Draws whatever knobs the selected tool declared. Returns true if the
    /// user changed one.
    fn options_row(&mut self, ui: &mut egui::Ui) -> bool {
        let p = self.palette;
        let tool_id = self.tools[self.selected].meta().id;
        let specs = self.tools[self.selected].options();
        if specs.is_empty() {
            return false;
        }

        let options = &mut self.states.get_mut(tool_id).unwrap().options;
        let mut changed = false;

        ui.horizontal_wrapped(|ui| {
            for spec in specs {
                match spec {
                    OptionSpec::Toggle { id, label, .. } => {
                        let mut value = options.bool(id);
                        if ui
                            .checkbox(&mut value, RichText::new(*label).color(p.text_secondary))
                            .changed()
                        {
                            options.set(id, OptionValue::Bool(value));
                            changed = true;
                        }
                    }
                    OptionSpec::Choice {
                        id, label, choices, ..
                    } => {
                        let current = options.choice(id).to_string();
                        ui.label(RichText::new(*label).color(p.text_muted));

                        // A segmented control reads faster than a dropdown for
                        // the handful of choices tools actually declare.
                        Frame::none()
                            .fill(p.surface)
                            .rounding(Rounding::same(theme::ROUNDING_SMALL))
                            .inner_margin(Margin::same(2.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 2.0;
                                    for (value, choice_label) in *choices {
                                        let active = *value == current;
                                        if segment(ui, choice_label, p, active).clicked() && !active
                                        {
                                            options
                                                .set(id, OptionValue::Choice((*value).to_string()));
                                            changed = true;
                                        }
                                    }
                                });
                            });
                    }
                    OptionSpec::Number {
                        id,
                        label,
                        min,
                        max,
                        ..
                    } => {
                        let mut value = options.number(id);
                        ui.label(RichText::new(*label).color(p.text_muted));
                        if ui
                            .add(egui::DragValue::new(&mut value).range(*min..=*max))
                            .changed()
                        {
                            options.set(id, OptionValue::Number(value));
                            changed = true;
                        }
                    }
                }
                ui.add_space(10.0);
            }
        });

        changed
    }

    // ----------------------------------------------------------------- panes

    fn panes(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let tool_id = self.tools[self.selected].meta().id;
        let input_mode = self.tools[self.selected].input_mode();

        let has_input = matches!(input_mode, InputMode::Text { .. });
        // Each pane carries a header row and frame padding; the rest is split.
        let chrome = if has_input { 88.0 } else { 44.0 };
        let panes = if has_input { 2.0 } else { 1.0 };
        let pane_height = ((ui.available_height() - chrome) / panes).max(80.0);

        if let InputMode::Text { placeholder } = input_mode {
            let mut input = std::mem::take(&mut self.states.get_mut(tool_id).unwrap().input);
            let cleared = self.pane_header(ui, "Input", |ui| {
                !input.is_empty()
                    && icon_button(ui, icons::ROTATE, p, false)
                        .on_hover_text("Clear")
                        .clicked()
            });

            if cleared {
                input.clear();
                self.dirty = true;
            }

            let changed = self.pane_body(
                ui,
                "input",
                pane_height,
                &mut Editable {
                    text: &mut input,
                    placeholder,
                },
            );
            self.states.get_mut(tool_id).unwrap().input = input;

            if changed {
                self.dirty = true;
            }
            ui.add_space(10.0);
        }

        let output = self.output.clone();
        let copied = self.copied_ticks > 0;

        let action = self.pane_header(ui, "Output", |ui| {
            let mut clicked = None;
            if input_mode == InputMode::None
                && ui
                    .button(RichText::new(format!("{}  Generate", icons::REFRESH)))
                    .clicked()
            {
                clicked = Some(PaneAction::Generate);
            }
            if let Ok(text) = &output {
                let label = if copied {
                    format!("{}  Copied", icons::CHECK)
                } else {
                    format!("{}  Copy", icons::COPY)
                };
                if ui
                    .add_enabled(!text.is_empty(), egui::Button::new(RichText::new(label)))
                    .clicked()
                {
                    clicked = Some(PaneAction::Copy);
                }
            }
            clicked
        });

        match action {
            Some(PaneAction::Generate) => self.dirty = true,
            Some(PaneAction::Copy) => {
                if let Ok(text) = &self.output {
                    ui.output_mut(|o| o.copied_text = text.clone());
                    self.copied_ticks = 90;
                }
            }
            None => {}
        }

        match &self.output {
            Ok(text) => {
                let text = text.clone();
                self.pane_body(ui, "output", pane_height, &mut ReadOnly(&text));
            }
            Err(error) => {
                Frame::none()
                    .fill(p.danger_soft)
                    .stroke(theme::hairline(p.danger.linear_multiply(0.4)))
                    .rounding(Rounding::same(theme::ROUNDING))
                    .inner_margin(Margin::symmetric(12.0, 10.0))
                    .show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            ui.label(RichText::new(icons::ALERT).color(p.danger));
                            ui.add(
                                Label::new(RichText::new(error.to_string()).color(p.danger)).wrap(),
                            );
                        });
                    });
            }
        }
    }

    /// A pane's title row, with room on the right for its actions.
    fn pane_header<R>(
        &self,
        ui: &mut egui::Ui,
        title: &str,
        actions: impl FnOnce(&mut egui::Ui) -> R,
    ) -> R {
        let p = self.palette;
        let mut result = None;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(title.to_uppercase())
                    .size(10.0)
                    .strong()
                    .color(p.text_muted),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                result = Some(actions(ui));
            });
        });
        ui.add_space(5.0);
        result.expect("actions always run")
    }

    fn pane_body(&self, ui: &mut egui::Ui, id: &str, height: f32, text: &mut dyn PaneText) -> bool {
        let p = self.palette;
        let wrap = self.settings.wrap;

        Frame::none()
            .fill(p.surface)
            .stroke(theme::hairline(p.border))
            .rounding(Rounding::same(theme::ROUNDING))
            .inner_margin(Margin::same(10.0))
            .show(ui, |ui| {
                ScrollArea::vertical()
                    .id_salt(id)
                    .max_height(height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if wrap {
                            text.show(ui, height, ui.available_width())
                        } else {
                            // egui wraps by default, so "no wrap" means giving
                            // the editor unlimited width inside a horizontal
                            // scroll area.
                            ScrollArea::horizontal()
                                .id_salt((id, "h"))
                                .show(ui, |ui| text.show(ui, height, f32::INFINITY))
                                .inner
                        }
                    })
                    .inner
            })
            .inner
    }

    // -------------------------------------------------------------- settings

    fn settings_window(&mut self, ctx: &egui::Context) {
        let p = self.palette;
        let before = self.settings.clone();
        let mut open = self.settings_open;

        egui::Window::new(RichText::new("Settings").size(15.0).strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .frame(
                Frame::window(&ctx.style())
                    .fill(p.elevated)
                    .stroke(theme::hairline(p.border))
                    .inner_margin(Margin::same(18.0)),
            )
            .show(ctx, |ui| {
                ui.set_min_width(390.0);
                ui.add_space(6.0);

                setting_row(ui, p, icons::MONITOR, "Theme", |ui| {
                    Frame::none()
                        .fill(p.surface)
                        .rounding(Rounding::same(theme::ROUNDING_SMALL))
                        .inner_margin(Margin::same(2.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                for theme_choice in Theme::ALL {
                                    let icon = match theme_choice {
                                        Theme::System => icons::MONITOR,
                                        Theme::Light => icons::SUN,
                                        Theme::Dark => icons::MOON,
                                    };
                                    let active = self.settings.theme == *theme_choice;
                                    let label = format!("{icon}  {}", theme_choice.label());
                                    if segment(ui, &label, p, active).clicked() {
                                        self.settings.theme = *theme_choice;
                                    }
                                }
                            });
                        });
                });

                setting_row(ui, p, icons::SEARCH, "Interface scale", |ui| {
                    ui.add(
                        egui::Slider::new(&mut self.settings.ui_scale, settings::ui_scale_range())
                            .step_by(0.05)
                            .fixed_decimals(2)
                            .suffix("×"),
                    );
                });

                setting_row(ui, p, icons::TYPE, "Editor font size", |ui| {
                    ui.add(
                        egui::Slider::new(
                            &mut self.settings.font_size,
                            settings::font_size_range(),
                        )
                        .step_by(1.0)
                        .fixed_decimals(0)
                        .suffix(" pt"),
                    );
                });

                setting_row(ui, p, icons::WRAP_TEXT, "Wrap long lines", |ui| {
                    ui.checkbox(&mut self.settings.wrap, "");
                });

                ui.add_space(14.0);
                ui.separator();
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    if ui
                        .button(RichText::new(format!(
                            "{}  Reset to defaults",
                            icons::ROTATE
                        )))
                        .clicked()
                    {
                        // Keep the open tool: it is navigation, not a preference.
                        let tool = self.settings.selected_tool.clone();
                        self.settings = Settings {
                            selected_tool: tool,
                            ..Settings::default()
                        };
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if let Some(path) = Settings::path() {
                            ui.add(
                                Label::new(
                                    RichText::new(path.to_string_lossy())
                                        .size(10.0)
                                        .color(p.text_muted),
                                )
                                .truncate(),
                            )
                            .on_hover_text(path.to_string_lossy());
                        }
                    });
                });
            });

        self.settings_open = open;

        if self.settings != before {
            self.settings.save();
        }
    }
}

enum PaneAction {
    Generate,
    Copy,
}

// ------------------------------------------------------------------ widgets

/// A square, borderless icon button that tints on hover.
fn icon_button(ui: &mut egui::Ui, icon: &str, p: Palette, active: bool) -> egui::Response {
    let size = Vec2::splat(ui.text_style_height(&TextStyle::Body) + 12.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if active || response.hovered() {
        ui.painter().rect_filled(
            rect,
            Rounding::same(theme::ROUNDING_SMALL),
            if active { p.accent_soft } else { p.surface },
        );
    }

    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::proportional(15.0),
        if active || response.hovered() {
            p.accent
        } else {
            p.text_secondary
        },
    );

    response
}

/// One button of a segmented control.
fn segment(ui: &mut egui::Ui, label: &str, p: Palette, active: bool) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        TextStyle::Body.resolve(ui.style()),
        p.text,
    );
    let size = Vec2::new(galley.size().x + 18.0, galley.size().y + 10.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if active {
        ui.painter().rect_filled(
            rect,
            Rounding::same(theme::ROUNDING_SMALL - 1.0),
            if response.hovered() {
                p.accent_hover
            } else {
                p.accent
            },
        );
    } else if response.hovered() {
        ui.painter().rect_filled(
            rect,
            Rounding::same(theme::ROUNDING_SMALL - 1.0),
            p.elevated,
        );
    }

    let color = if active {
        egui::Color32::WHITE
    } else if response.hovered() {
        p.text
    } else {
        p.text_secondary
    };
    ui.painter().galley(
        rect.center() - galley.size() / 2.0,
        ui.painter()
            .layout_no_wrap(label.to_owned(), TextStyle::Body.resolve(ui.style()), color),
        color,
    );

    response
}

/// A labelled row in the settings window: icon, name, control on the right.
fn setting_row(
    ui: &mut egui::Ui,
    p: Palette,
    icon: &str,
    label: &str,
    control: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(icon).color(p.text_muted));
        ui.add_space(2.0);
        ui.label(RichText::new(label).color(p.text));
        ui.with_layout(Layout::right_to_left(Align::Center), control);
    });
    ui.add_space(12.0);
}

/// Lets the input and output panes share wrapping and sizing logic despite one
/// being editable and the other not.
trait PaneText {
    fn show(&mut self, ui: &mut egui::Ui, height: f32, width: f32) -> bool;
}

struct Editable<'a> {
    text: &'a mut String,
    placeholder: &'static str,
}

impl PaneText for Editable<'_> {
    fn show(&mut self, ui: &mut egui::Ui, height: f32, width: f32) -> bool {
        ui.add_sized(
            [width, height],
            TextEdit::multiline(self.text)
                .hint_text(self.placeholder)
                .desired_width(width)
                .frame(false)
                .code_editor(),
        )
        .changed()
    }
}

struct ReadOnly<'a>(&'a str);

impl PaneText for ReadOnly<'_> {
    fn show(&mut self, ui: &mut egui::Ui, height: f32, width: f32) -> bool {
        // A read-only TextEdit keeps selection and scrolling while refusing edits.
        let mut text = self.0;
        ui.add_sized(
            [width, height],
            TextEdit::multiline(&mut text)
                .desired_width(width)
                .frame(false)
                .code_editor(),
        );
        false
    }
}

/// Tools declare no icon — that is UI vocabulary, and keeping it here means a
/// new tool still needs no UI changes. Unmapped tools fall back to something
/// sensible for their category.
fn tool_icon(meta: &rustafari_core::ToolMeta) -> &'static str {
    match meta.id {
        "json-formatter" => icons::BRACES,
        "base64" => icons::BINARY,
        "url-encode" => icons::LINK,
        "hash" => icons::HASH,
        "uuid" => icons::FINGERPRINT,
        _ => match meta.category {
            Category::Formatters => icons::BRACES,
            Category::Encoders => icons::BINARY,
            Category::Generators => icons::REFRESH,
            Category::Text => icons::TYPE,
        },
    }
}

impl eframe::App for Rustafari {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_settings(ctx);
        let p = self.palette;

        if self.copied_ticks > 0 {
            self.copied_ticks -= 1;
            ctx.request_repaint();
        }

        egui::SidePanel::left("sidebar")
            .exact_width(SIDEBAR_WIDTH)
            .resizable(false)
            .frame(
                Frame::none()
                    .fill(p.surface)
                    .inner_margin(Margin::symmetric(10.0, 0.0)),
            )
            .show(ctx, |ui| {
                // Hairline separating sidebar from content.
                let rect = ui.max_rect();
                ui.painter().line_segment(
                    [rect.right_top(), rect.right_bottom()],
                    theme::hairline(p.border),
                );
                self.sidebar(ui);
            });

        egui::CentralPanel::default()
            .frame(
                Frame::none()
                    .fill(p.base)
                    .inner_margin(Margin::symmetric(20.0, 16.0)),
            )
            .show(ctx, |ui| {
                self.header(ui);
                ui.add_space(14.0);

                if self.options_row(ui) {
                    self.dirty = true;
                }
                ui.add_space(12.0);

                if self.dirty {
                    self.recompute();
                }

                self.panes(ui);
            });

        self.settings_window(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // The selected tool changes without going through the settings window,
        // so make sure the last one is recorded.
        self.settings.save();
    }
}
