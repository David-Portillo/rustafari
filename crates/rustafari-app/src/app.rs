use std::collections::HashMap;

use eframe::egui::{self, Color32, ComboBox, Layout, RichText, ScrollArea, TextEdit, TextStyle};
use rustafari_core::{
    all_tools, matches_query, InputMode, OptionSpec, OptionValue, Options, Tool, ToolError,
};

const SELECTED_TOOL_KEY: &str = "selected_tool";
const DARK_MODE_KEY: &str = "dark_mode";

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
    dark_mode: bool,
}

impl Rustafari {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
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

        let (selected, dark_mode) = match cc.storage {
            Some(storage) => {
                // A tool that was removed since the last release just falls back
                // to the first one.
                let selected = storage
                    .get_string(SELECTED_TOOL_KEY)
                    .and_then(|id| tools.iter().position(|t| t.meta().id == id))
                    .unwrap_or(0);
                let dark = storage
                    .get_string(DARK_MODE_KEY)
                    .is_none_or(|v| v == "true");
                (selected, dark)
            }
            None => (0, true),
        };

        cc.egui_ctx.set_visuals(visuals(dark_mode));
        cc.egui_ctx.style_mut(|style| {
            style.spacing.item_spacing = egui::vec2(8.0, 8.0);
            style.spacing.button_padding = egui::vec2(10.0, 5.0);
        });

        Rustafari {
            tools,
            states,
            selected,
            query: String::new(),
            output: Ok(String::new()),
            dirty: true,
            dark_mode,
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
                let icon = if self.dark_mode { "☀" } else { "🌙" };
                if ui.button(icon).on_hover_text("Toggle theme").clicked() {
                    self.dark_mode = !self.dark_mode;
                    ui.ctx().set_visuals(visuals(self.dark_mode));
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
            let input = &mut self.states.get_mut(tool_id).unwrap().input;
            let changed = ScrollArea::vertical()
                .id_salt(("input", tool_id))
                .max_height(pane_height)
                .show(ui, |ui| {
                    ui.add_sized(
                        [ui.available_width(), pane_height],
                        TextEdit::multiline(input)
                            .hint_text(placeholder)
                            .code_editor(),
                    )
                    .changed()
                })
                .inner;

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
                let mut text = output.as_str();
                ScrollArea::vertical()
                    .id_salt(("output", tool_id))
                    .max_height(pane_height)
                    .show(ui, |ui| {
                        // A read-only TextEdit keeps selection and scrolling
                        // while refusing edits.
                        ui.add_sized(
                            [ui.available_width(), pane_height],
                            TextEdit::multiline(&mut text).code_editor(),
                        );
                    });
            }
            Err(error) => {
                ui.add_space(4.0);
                ui.colored_label(Color32::from_rgb(220, 90, 90), format!("⚠ {error}"));
            }
        }
    }
}

impl eframe::App for Rustafari {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(
            SELECTED_TOOL_KEY,
            self.tools[self.selected].meta().id.into(),
        );
        storage.set_string(DARK_MODE_KEY, self.dark_mode.to_string());
    }
}

fn visuals(dark_mode: bool) -> egui::Visuals {
    if dark_mode {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    }
}
