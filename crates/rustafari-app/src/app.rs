use std::collections::HashMap;

use eframe::egui::{self, Color32, ComboBox, Layout, RichText, ScrollArea, TextEdit, TextStyle};
use rustafari_core::{
    all_tools, matches_query, InputMode, OptionSpec, OptionValue, Options, Tool, ToolError,
};

use crate::settings::{self, Settings, Theme};

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
    settings_open: bool,
}

impl Rustafari {
    /// eframe still handles window geometry through its own persistence; our
    /// settings live in a file of their own.
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
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
            settings_open: false,
        }
    }

    /// Pushes the current settings into egui's style. Cheap to call every
    /// frame: it does nothing unless a setting actually changed, which also
    /// lets the System theme follow the OS without a restart.
    fn apply_settings(&mut self, ctx: &egui::Context) {
        let resolved = self.resolve_theme(ctx);
        let unchanged = self.applied.as_ref() == Some(&self.settings);
        // The second check catches the OS flipping appearance underneath a
        // `System` theme, where our own settings have not changed at all.
        if unchanged && ctx.style().visuals.dark_mode == (resolved == Theme::Dark) {
            return;
        }

        ctx.set_visuals(if resolved == Theme::Dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
        ctx.set_zoom_factor(self.settings.ui_scale);

        let size = self.settings.font_size;
        ctx.style_mut(|style| {
            style.spacing.item_spacing = egui::vec2(8.0, 8.0);
            style.spacing.button_padding = egui::vec2(10.0, 5.0);
            // Only the panes, which are `code_editor()`s and so use Monospace.
            // Chrome text scales with `ui_scale` instead, so the two settings
            // stay independent.
            if let Some(font) = style.text_styles.get_mut(&TextStyle::Monospace) {
                font.size = size;
            }
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

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.heading("rustafari");
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("⚙").on_hover_text("Settings").clicked() {
                    self.settings_open = true;
                }
            });
        });

        ui.add_space(6.0);
        ui.add(
            TextEdit::singleline(&mut self.query)
                .hint_text("Search tools…")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(4.0);

        ScrollArea::vertical().show(ui, |ui| {
            let mut any_shown = false;

            for category in rustafari_core::Category::ALL {
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

                ui.add_space(6.0);
                ui.label(
                    RichText::new(category.label())
                        .small()
                        .weak()
                        .text_style(TextStyle::Button),
                );

                for index in in_category {
                    let meta = self.tools[index].meta();
                    let selected = index == self.selected;
                    if ui
                        .selectable_label(selected, meta.name)
                        .on_hover_text(meta.description)
                        .clicked()
                        && !selected
                    {
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
                ui.add_space(12.0);
                ui.weak("No tools match that search.");
            }
        });
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        let before = self.settings.clone();
        let mut open = self.settings_open;

        egui::Window::new("Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.add_space(4.0);

                egui::Grid::new("settings_grid")
                    .num_columns(2)
                    .spacing([16.0, 12.0])
                    .show(ui, |ui| {
                        ui.label("Theme");
                        ui.horizontal(|ui| {
                            for theme in Theme::ALL {
                                ui.selectable_value(
                                    &mut self.settings.theme,
                                    *theme,
                                    theme.label(),
                                );
                            }
                        });
                        ui.end_row();

                        ui.label("Interface scale");
                        ui.add(
                            egui::Slider::new(
                                &mut self.settings.ui_scale,
                                settings::ui_scale_range(),
                            )
                            .step_by(0.05)
                            .fixed_decimals(2),
                        );
                        ui.end_row();

                        ui.label("Font size");
                        ui.add(
                            egui::Slider::new(
                                &mut self.settings.font_size,
                                settings::font_size_range(),
                            )
                            .step_by(1.0)
                            .fixed_decimals(0)
                            .suffix(" pt"),
                        );
                        ui.end_row();

                        ui.label("Wrap long lines");
                        ui.checkbox(&mut self.settings.wrap, "");
                        ui.end_row();
                    });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                ui.horizontal(|ui| {
                    if ui.button("Reset to defaults").clicked() {
                        // Keep the open tool: it is navigation, not a preference.
                        let tool = self.settings.selected_tool.clone();
                        self.settings = Settings {
                            selected_tool: tool,
                            ..Settings::default()
                        };
                    }

                    ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(path) = Settings::path() {
                            ui.weak(RichText::new(path.to_string_lossy()).small())
                                .on_hover_text("Settings are stored here");
                        }
                    });
                });
            });

        self.settings_open = open;

        if self.settings != before {
            self.settings.save();
        }
    }

    fn pane(&self, ui: &mut egui::Ui, id: &str, height: f32, text: &mut dyn PaneText) -> bool {
        let wrap = self.settings.wrap;
        ScrollArea::vertical()
            .id_salt(id)
            .max_height(height)
            .show(ui, |ui| {
                if wrap {
                    text.show(ui, height, ui.available_width())
                } else {
                    // egui wraps by default, so "no wrap" means giving the
                    // editor unlimited width inside a horizontal scroll area.
                    ScrollArea::horizontal()
                        .id_salt((id, "h"))
                        .show(ui, |ui| text.show(ui, height, f32::INFINITY))
                        .inner
                }
            })
            .inner
    }

    fn panes(&mut self, ui: &mut egui::Ui) {
        let tool_id = self.tools[self.selected].meta().id;
        let input_mode = self.tools[self.selected].input_mode();

        // Split the remaining height between input and output, minus the room
        // the two pane headers take.
        let pane_height = match input_mode {
            InputMode::Text { .. } => (ui.available_height() - 56.0) / 2.0,
            InputMode::None => ui.available_height() - 28.0,
        };

        if let InputMode::Text { placeholder } = input_mode {
            ui.label(RichText::new("Input").strong());

            let mut input = std::mem::take(&mut self.states.get_mut(tool_id).unwrap().input);
            let changed = self.pane(
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
            ui.add_space(4.0);
        }

        ui.horizontal(|ui| {
            ui.label(RichText::new("Output").strong());
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                if let Ok(output) = &self.output {
                    if ui
                        .add_enabled(!output.is_empty(), egui::Button::new("Copy"))
                        .clicked()
                    {
                        ui.output_mut(|o| o.copied_text = output.clone());
                    }
                }
                if input_mode == InputMode::None && ui.button("Generate").clicked() {
                    self.dirty = true;
                }
            });
        });

        match &self.output {
            Ok(output) => {
                let output = output.clone();
                self.pane(ui, "output", pane_height, &mut ReadOnly(&output));
            }
            Err(error) => {
                ui.add_space(4.0);
                ui.colored_label(Color32::from_rgb(220, 90, 90), format!("⚠ {error}"));
            }
        }
    }
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
                .code_editor(),
        );
        false
    }
}

impl Rustafari {
    /// Draws whatever knobs the selected tool declared. Returns true if the
    /// user changed one.
    fn options_row(&mut self, ui: &mut egui::Ui) -> bool {
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
                        if ui.checkbox(&mut value, *label).changed() {
                            options.set(id, OptionValue::Bool(value));
                            changed = true;
                        }
                    }
                    OptionSpec::Choice {
                        id, label, choices, ..
                    } => {
                        let current = options.choice(id).to_string();
                        let current_label = choices
                            .iter()
                            .find(|(value, _)| *value == current)
                            .map_or("", |(_, label)| *label);

                        ui.label(*label);
                        ComboBox::from_id_salt((tool_id, *id))
                            .selected_text(current_label)
                            .show_ui(ui, |ui| {
                                for (value, choice_label) in *choices {
                                    if ui
                                        .selectable_label(*value == current, *choice_label)
                                        .clicked()
                                        && *value != current
                                    {
                                        options.set(id, OptionValue::Choice((*value).to_string()));
                                        changed = true;
                                    }
                                }
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
                        ui.label(*label);
                        if ui
                            .add(egui::DragValue::new(&mut value).range(*min..=*max))
                            .changed()
                        {
                            options.set(id, OptionValue::Number(value));
                            changed = true;
                        }
                    }
                }
                ui.add_space(6.0);
            }
        });

        changed
    }
}

impl eframe::App for Rustafari {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_settings(ctx);

        egui::SidePanel::left("sidebar")
            .exact_width(240.0)
            .resizable(false)
            .show(ctx, |ui| self.sidebar(ui));

        egui::CentralPanel::default().show(ctx, |ui| {
            let meta = self.tools[self.selected].meta();
            ui.add_space(4.0);
            ui.heading(meta.name);
            ui.weak(meta.description);
            ui.add_space(8.0);

            if self.options_row(ui) {
                self.dirty = true;
            }

            ui.separator();

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
