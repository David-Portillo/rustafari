use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, text::LayoutJob, Align, Frame, Id, Key, Label, Layout, Margin, Rect, RichText, Rounding,
    ScrollArea, Sense, TextEdit, TextStyle, Ui, UiBuilder, Vec2,
};
use rustafari_core::{
    all_tools, matches_query, Category, InputMode, OptionSpec, OptionValue, Options, Tool,
    ToolResult,
};

use crate::icons;
use crate::settings::{self, PaneLayout, Settings, Theme};
use crate::theme::{self, Palette};
use crate::widgets::{icon_button, segment, slider, splitter, toggle};
use crate::worker::Worker;

/// Below this width the panes stack even in `PaneLayout::Auto`; side by side
/// would leave each editor too narrow to read code in.
const SIDE_BY_SIDE_MIN_WIDTH: f32 = 860.0;
/// Width of the grab strip between panes. Wider than the line it draws so the
/// divider is easy to catch.
const SPLITTER_GRIP: f32 = 12.0;
/// How long "Copied" stays on the copy button.
const COPIED_FOR: Duration = Duration::from_millis(1500);
/// Only show "Working…" if a run outlives this; short runs must not flicker.
const SHOW_WORKING_AFTER: Duration = Duration::from_millis(150);
/// On macOS the content extends under a transparent title bar; this keeps our
/// controls clear of the traffic lights.
const TITLEBAR_INSET: f32 = if cfg!(target_os = "macos") { 28.0 } else { 0.0 };

const SEARCH_ID: &str = "sidebar-search";

/// Character and line counts, computed when text changes rather than every
/// frame — both are O(n) scans, and inputs can be megabytes.
#[derive(Clone, Copy, Default)]
struct TextStats {
    chars: usize,
    lines: usize,
}

impl TextStats {
    fn of(text: &str) -> Self {
        if text.is_empty() {
            return TextStats::default();
        }
        TextStats {
            chars: text.chars().count(),
            lines: text.lines().count(),
        }
    }
}

/// Per-tool editing state, so switching tools and coming back keeps your work.
struct ToolState {
    input: String,
    input_stats: TextStats,
    /// Only used by `InputMode::TwoText` tools; empty for everything else.
    right: String,
    right_stats: TextStats,
    options: Options,
}

/// Which of a tool's inputs a pane is editing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}

pub struct Rustafari {
    tools: Vec<Arc<dyn Tool>>,
    states: HashMap<&'static str, ToolState>,
    selected: usize,

    query: String,
    /// Tool indices that match the query, grouped in `Category::ALL` order.
    /// Recomputed only when the query changes.
    filtered: Vec<Vec<usize>>,

    worker: Worker,
    /// Result of the last completed run for the selected tool.
    output: ToolResult,
    output_stats: TextStats,
    /// Something changed this frame that warrants a re-run. Coalesced into one
    /// submission at the end of `update`.
    dirty: bool,
    submitted_at: Option<Instant>,

    settings: Settings,
    /// What was last pushed to egui, so we only restyle when something moved.
    applied: Option<StyleInputs>,
    palette: Palette,
    settings_open: bool,
    /// The divider is being dragged; its position is saved on release.
    split_dragging: bool,
    /// The interface-scale slider is being dragged. Zooming is held off until
    /// it is released, because re-zooming mid-drag moves the slider itself.
    scale_dragging: bool,
    copied_at: Option<Instant>,
}

/// The subset of settings that affects egui's style. Layout preferences like
/// the pane split are deliberately excluded: dragging the divider must not
/// rebuild the whole style every frame.
#[derive(Clone, Copy, PartialEq)]
struct StyleInputs {
    dark: bool,
    ui_scale: f32,
    font_size: f32,
}

impl Rustafari {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        crate::fonts::install(&cc.egui_ctx);

        let tools: Vec<Arc<dyn Tool>> = all_tools().into_iter().map(Arc::from).collect();

        let states = tools
            .iter()
            .map(|tool| {
                (
                    tool.meta().id,
                    ToolState {
                        input: String::new(),
                        input_stats: TextStats::default(),
                        right: String::new(),
                        right_stats: TextStats::default(),
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

        // A finished run has to wake the renderer, which otherwise only
        // repaints on input.
        let ctx = cc.egui_ctx.clone();
        let worker = Worker::spawn(move || ctx.request_repaint());

        let mut app = Rustafari {
            filtered: Vec::new(),
            tools,
            states,
            selected,
            query: String::new(),
            worker,
            output: Ok(String::new()),
            output_stats: TextStats::default(),
            dirty: true,
            submitted_at: None,
            settings,
            applied: None,
            palette: Palette::DARK,
            settings_open: false,
            split_dragging: false,
            scale_dragging: false,
            copied_at: None,
        };
        app.refilter();
        app
    }

    fn tool(&self) -> &Arc<dyn Tool> {
        &self.tools[self.selected]
    }

    fn state(&self) -> &ToolState {
        &self.states[self.tool().meta().id]
    }

    fn state_mut(&mut self) -> &mut ToolState {
        let id = self.tools[self.selected].meta().id;
        self.states.get_mut(id).expect("every tool has state")
    }

    fn refilter(&mut self) {
        self.filtered = Category::ALL
            .iter()
            .map(|category| {
                self.tools
                    .iter()
                    .enumerate()
                    .filter(|(_, tool)| {
                        let meta = tool.meta();
                        meta.category == *category && matches_query(&meta, &self.query)
                    })
                    .map(|(i, _)| i)
                    .collect()
            })
            .collect();
    }

    fn select(&mut self, index: usize) {
        if index == self.selected {
            return;
        }
        self.selected = index;
        self.settings.selected_tool = Some(self.tools[index].meta().id.to_string());
        // Saved here rather than only on exit, because a crash or a kill
        // signal never reaches `on_exit`.
        self.settings.save();
        // Show nothing stale from the previous tool while the new run is out.
        self.output = Ok(String::new());
        self.output_stats = TextStats::default();
        self.dirty = true;
    }

    /// Hands this output to another tool as its input, and goes there.
    ///
    /// A comparison tool receives it as the left-hand document, which is the
    /// only side a single output can sensibly fill.
    fn send_to(&mut self, index: usize, text: String) {
        let id = self.tools[index].meta().id;
        let state = self.states.get_mut(id).expect("every tool has state");
        state.input_stats = TextStats::of(&text);
        state.input = text;

        if index == self.selected {
            // Feeding a tool its own output — encode twice, say. Nothing to
            // navigate to, but it still has to re-run.
            self.dirty = true;
        } else {
            self.select(index);
        }
    }

    fn submit(&mut self) {
        let tool = self.tool().clone();
        let state = self.state();
        self.worker.submit(
            tool,
            state.input.clone(),
            state.right.clone(),
            state.options.clone(),
        );
        self.submitted_at = Some(Instant::now());
        self.dirty = false;
    }

    /// Pushes the current settings into egui's style. Cheap to call every
    /// frame: it does nothing unless a setting actually changed, which also
    /// lets the System theme follow the OS without a restart.
    fn apply_settings(&mut self, ctx: &egui::Context) {
        // Resolving the theme every frame is what lets `System` follow the OS
        // flipping appearance, with no setting of ours having changed.
        //
        // Zoom is the exception to applying settings live: it scales the
        // slider along with everything else, so the handle slides out from
        // under the pointer and the control fights back. Hold the previous
        // zoom until the drag ends, then apply once.
        let ui_scale = if self.scale_dragging {
            self.applied.map_or(self.settings.ui_scale, |a| a.ui_scale)
        } else {
            self.settings.ui_scale
        };
        let inputs = StyleInputs {
            dark: self.resolve_theme(ctx) == Theme::Dark,
            ui_scale,
            font_size: self.settings.font_size,
        };
        if self.applied == Some(inputs) {
            return;
        }

        self.palette = Palette::for_dark_mode(inputs.dark);
        ctx.set_visuals(theme::visuals(self.palette));
        ctx.set_zoom_factor(inputs.ui_scale);
        ctx.style_mut(|style| {
            style.text_styles = theme::text_styles(inputs.font_size);
            theme::apply_spacing(style);
        });

        self.applied = Some(inputs);
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

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let (focus_search, toggle_settings, escape) = ctx.input(|i| {
            (
                i.modifiers.command && i.key_pressed(Key::K),
                i.modifiers.command && i.key_pressed(Key::Comma),
                i.key_pressed(Key::Escape),
            )
        });

        if focus_search {
            ctx.memory_mut(|m| m.request_focus(Id::new(SEARCH_ID)));
        }
        if toggle_settings {
            self.settings_open = !self.settings_open;
        }
        if escape {
            if self.settings_open {
                self.settings_open = false;
            } else if !self.query.is_empty() {
                self.query.clear();
                self.refilter();
            }
        }
    }

    // ---------------------------------------------------------------- sidebar

    fn sidebar(&mut self, ui: &mut Ui) {
        let p = self.palette;

        ui.add_space(14.0 + TITLEBAR_INSET);
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new(icons::WRENCH).size(17.0).color(p.accent));
            ui.label(RichText::new("rustafari").size(17.0).strong().color(p.text));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon_button(ui, icons::SETTINGS, p, self.settings_open)
                    .on_hover_text("Settings  (⌘ ,)")
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
                    let response = ui.add(
                        TextEdit::singleline(&mut self.query)
                            .id(Id::new(SEARCH_ID))
                            .hint_text(RichText::new("Search tools").color(p.text_muted))
                            .desired_width(f32::INFINITY)
                            .frame(false),
                    );
                    if response.changed() {
                        self.refilter();
                    }
                    if !self.query.is_empty()
                        && icon_button(ui, icons::X, p, false)
                            .on_hover_text("Clear  (Esc)")
                            .clicked()
                    {
                        self.query.clear();
                        self.refilter();
                    }
                });
            });

        ui.add_space(10.0);

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut any_shown = false;
                // Selecting mutates state the loop is borrowing; defer it.
                let mut clicked = None;

                for (category, indices) in Category::ALL.iter().zip(&self.filtered) {
                    if indices.is_empty() {
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

                    for &index in indices {
                        let meta = self.tools[index].meta();
                        if tool_row(ui, p, index == self.selected, meta.name, tool_icon(&meta)) {
                            clicked = Some(index);
                        }
                    }
                }

                if let Some(index) = clicked {
                    self.select(index);
                }

                if !any_shown {
                    ui.add_space(16.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("No tools match").color(p.text_muted));
                    });
                }
            });
    }

    // ------------------------------------------------------------------ main

    fn header(&self, ui: &mut Ui) {
        let p = self.palette;
        let meta = self.tool().meta();

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
    fn options_row(&mut self, ui: &mut Ui) -> bool {
        let p = self.palette;
        let specs = self.tool().options();
        if specs.is_empty() {
            return false;
        }

        let options = &mut self.state_mut().options;
        let mut changed = false;

        ui.horizontal_wrapped(|ui| {
            for spec in specs {
                match spec {
                    OptionSpec::Toggle { id, label, .. } => {
                        let mut value = options.bool(id);
                        if toggle(ui, &mut value, p).changed() {
                            options.set(id, OptionValue::Bool(value));
                            changed = true;
                        }
                        ui.label(RichText::new(*label).color(p.text_secondary));
                    }
                    OptionSpec::Choice {
                        id, label, choices, ..
                    } => {
                        ui.label(RichText::new(*label).color(p.text_muted));

                        // A segmented control reads faster than a dropdown for
                        // the handful of choices tools actually declare.
                        let mut picked = None;
                        segmented(ui, p, |ui| {
                            let current = options.choice(id);
                            for (value, choice_label) in *choices {
                                let active = *value == current;
                                if segment(ui, choice_label, p, active).clicked() && !active {
                                    picked = Some(*value);
                                }
                            }
                        });
                        if let Some(value) = picked {
                            options.set(id, OptionValue::Choice(value.to_string()));
                            changed = true;
                        }
                    }
                    OptionSpec::Text {
                        id,
                        label,
                        placeholder,
                        ..
                    } => {
                        ui.label(RichText::new(*label).color(p.text_muted));
                        let mut value = options.text(id).to_owned();
                        let response = ui.add(
                            TextEdit::singleline(&mut value)
                                .id(Id::new(("opt", *id)))
                                .hint_text(RichText::new(*placeholder).color(p.text_muted))
                                // Short by design: these sit inline with the
                                // other options, not in a pane.
                                .desired_width(84.0)
                                .font(TextStyle::Monospace),
                        );
                        if response.changed() {
                            options.set(id, OptionValue::Text(value));
                            changed = true;
                        }
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
                ui.add_space(12.0);
            }
        });

        changed
    }

    // ----------------------------------------------------------------- panes

    /// Lays out input and output — side by side or stacked, with a draggable
    /// divider — in whatever space is left in the central panel.
    fn panes(&mut self, ui: &mut Ui) {
        let input_mode = self.tool().input_mode();
        let area = ui.available_rect_before_wrap();

        if input_mode == InputMode::None {
            // Generators have no input; the output takes everything.
            self.output_pane(ui, area, true);
            ui.allocate_rect(area, Sense::hover());
            return;
        }

        let side_by_side = match self.settings.layout {
            PaneLayout::SideBySide => true,
            PaneLayout::Stacked => false,
            PaneLayout::Auto => area.width() >= SIDE_BY_SIDE_MIN_WIDTH,
        };

        let split = self.settings.pane_split;
        let (first, grip, second) = if side_by_side {
            let usable = area.width() - SPLITTER_GRIP;
            let first_w = usable * split;
            let first = Rect::from_min_size(area.min, Vec2::new(first_w, area.height()));
            let grip = Rect::from_min_size(
                egui::pos2(first.max.x, area.min.y),
                Vec2::new(SPLITTER_GRIP, area.height()),
            );
            let second = Rect::from_min_max(egui::pos2(grip.max.x, area.min.y), area.max);
            (first, grip, second)
        } else {
            let usable = area.height() - SPLITTER_GRIP;
            let first_h = usable * split;
            let first = Rect::from_min_size(area.min, Vec2::new(area.width(), first_h));
            let grip = Rect::from_min_size(
                egui::pos2(area.min.x, first.max.y),
                Vec2::new(area.width(), SPLITTER_GRIP),
            );
            let second = Rect::from_min_max(egui::pos2(area.min.x, grip.max.y), area.max);
            (first, grip, second)
        };

        match input_mode {
            InputMode::Text { placeholder } => {
                self.input_pane(ui, first, Side::Left, "Input", placeholder)
            }
            InputMode::TwoText {
                left_label,
                right_label,
                placeholder,
            } => {
                // Two documents share the input region, split across the axis
                // the main split did not use, so neither ends up a sliver.
                let (a, b) = halve(first, !side_by_side);
                self.input_pane(ui, a, Side::Left, left_label, placeholder);
                self.input_pane(ui, b, Side::Right, right_label, placeholder);
            }
            InputMode::None => unreachable!("handled above"),
        }
        self.output_pane(ui, second, false);

        if let Some(delta) = splitter(ui, grip, side_by_side, self.palette) {
            let total = if side_by_side {
                area.width()
            } else {
                area.height()
            } - SPLITTER_GRIP;
            let range = settings::pane_split_range();
            self.settings.pane_split = (split + delta / total).clamp(*range.start(), *range.end());
            self.split_dragging = true;
        }
        // Persisted like a window size: on release, not on every pixel.
        if self.split_dragging && ui.input(|i| i.pointer.any_released()) {
            self.split_dragging = false;
            self.settings.save();
        }

        ui.allocate_rect(area, Sense::hover());
    }

    fn input_pane(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        side: Side,
        label: &str,
        placeholder: &'static str,
    ) {
        let p = self.palette;
        let wrap = self.settings.wrap;

        // Taken out so the editor can borrow it mutably while the rest of
        // `self` stays available; put back before returning.
        let mut text = std::mem::take(match side {
            Side::Left => &mut self.state_mut().input,
            Side::Right => &mut self.state_mut().right,
        });
        let mut changed = false;

        // The salt scopes every id inside this pane. Without it two panes
        // build child `Ui`s from the same parent at the same nesting depth,
        // so their scroll areas and editors collide.
        let salt = match side {
            Side::Left => "input",
            Side::Right => "input-right",
        };
        ui.allocate_new_ui(UiBuilder::new().max_rect(rect).id_salt(salt), |ui| {
            let cleared = pane_header(ui, p, label, |ui| {
                !text.is_empty()
                    && icon_button(ui, icons::ROTATE, p, false)
                        .on_hover_text("Clear")
                        .clicked()
            });
            if cleared {
                text.clear();
                changed = true;
            }

            pane_body(ui, p, |ui| {
                let hint = RichText::new(placeholder).color(p.text_muted);
                changed |= editor(ui, p, wrap, salt, |ui, layouter| {
                    let mut edit = TextEdit::multiline(&mut text)
                        .hint_text(hint)
                        .desired_width(f32::INFINITY)
                        .min_size(ui.available_size())
                        .frame(false)
                        .code_editor();
                    if let Some(layouter) = layouter {
                        edit = edit.layouter(layouter);
                    }
                    ui.add(edit).changed()
                });
            });
        });

        if changed {
            let stats = TextStats::of(&text);
            match side {
                Side::Left => self.state_mut().input_stats = stats,
                Side::Right => self.state_mut().right_stats = stats,
            }
            self.dirty = true;
        }
        match side {
            Side::Left => self.state_mut().input = text,
            Side::Right => self.state_mut().right = text,
        }
    }

    fn output_pane(&mut self, ui: &mut Ui, rect: Rect, generator: bool) {
        let p = self.palette;
        let wrap = self.settings.wrap;
        let copied = self.copied_at.is_some_and(|at| at.elapsed() < COPIED_FOR);

        let mut action = None;
        ui.allocate_new_ui(UiBuilder::new().max_rect(rect).id_salt("output"), |ui| {
            action = pane_header(ui, p, "Output", |ui| {
                let mut clicked = None;
                if generator
                    && ui
                        .button(RichText::new(format!("{}  Generate", icons::REFRESH)))
                        .clicked()
                {
                    clicked = Some(PaneAction::Generate);
                }
                if let Ok(text) = &self.output {
                    let label = if copied {
                        format!("{}  Copied", icons::CHECK)
                    } else {
                        format!("{}  Copy", icons::COPY)
                    };
                    if ui
                        .add_enabled(!text.is_empty(), egui::Button::new(label))
                        .clicked()
                    {
                        clicked = Some(PaneAction::Copy);
                    }

                    // Chaining by hand: hand this output to the next tool
                    // rather than making the user copy, switch and paste.
                    if !text.is_empty() {
                        ui.menu_button(format!("{}  Send to", icons::ARROW_RIGHT), |ui| {
                            ui.set_min_width(180.0);
                            for (index, tool) in self.tools.iter().enumerate() {
                                let meta = tool.meta();
                                // A generator has nowhere to put the text.
                                if tool.input_mode() == InputMode::None {
                                    continue;
                                }
                                let label = format!("{}  {}", tool_icon(&meta), meta.name);
                                if ui.button(label).clicked() {
                                    clicked = Some(PaneAction::SendTo(index));
                                    ui.close_menu();
                                }
                            }
                        });
                    }
                }
                clicked
            });

            match &self.output {
                Ok(text) if text.is_empty() => {
                    pane_body(ui, p, |ui| {
                        ui.centered_and_justified(|ui| {
                            ui.label(
                                RichText::new(if generator {
                                    "Press Generate"
                                } else {
                                    "Output appears here"
                                })
                                .color(p.text_muted),
                            );
                        });
                    });
                }
                Ok(text) => {
                    pane_body(ui, p, |ui| {
                        editor(ui, p, wrap, "output", |ui, layouter| {
                            // A read-only TextEdit keeps selection and
                            // scrolling while refusing edits.
                            let mut text = text.as_str();
                            let mut edit = TextEdit::multiline(&mut text)
                                .desired_width(f32::INFINITY)
                                .min_size(ui.available_size())
                                .frame(false)
                                .code_editor();
                            if let Some(layouter) = layouter {
                                edit = edit.layouter(layouter);
                            }
                            ui.add(edit);
                            false
                        });
                    });
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
                                    Label::new(RichText::new(error.to_string()).color(p.danger))
                                        .wrap(),
                                );
                            });
                        });
                }
            }
        });

        match action {
            Some(PaneAction::Generate) => self.dirty = true,
            Some(PaneAction::Copy) => {
                if let Ok(text) = &self.output {
                    ui.output_mut(|o| o.copied_text = text.clone());
                    self.copied_at = Some(Instant::now());
                }
            }
            Some(PaneAction::SendTo(index)) => {
                if let Ok(text) = &self.output {
                    let text = text.clone();
                    self.send_to(index, text);
                }
            }
            None => {}
        }
    }

    // ------------------------------------------------------------ status bar

    fn status_bar(&self, ui: &mut Ui) {
        let p = self.palette;
        let state = self.state();
        let input = state.input_stats;
        let right = state.right_stats;
        let output = self.output_stats;
        let mode = self.tool().input_mode();

        ui.horizontal_centered(|ui| {
            ui.add_space(6.0);
            let muted = |s: String| RichText::new(s).size(11.0).color(p.text_muted);
            let pair =
                |t: TextStats| format!("{} · {}", count(t.chars, "char"), count(t.lines, "line"));

            if mode != InputMode::None {
                ui.label(muted(pair(input)));
                // A comparison has two inputs feeding one output.
                if matches!(mode, InputMode::TwoText { .. }) {
                    ui.label(RichText::new("+").size(11.0).color(p.border));
                    ui.label(muted(pair(right)));
                }
                ui.label(RichText::new("→").size(11.0).color(p.border));
            }
            ui.label(muted(pair(output)));

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(6.0);
                ui.label(muted(format!("v{}", env!("CARGO_PKG_VERSION"))));

                let working = self.worker.is_pending()
                    && self
                        .submitted_at
                        .is_some_and(|at| at.elapsed() > SHOW_WORKING_AFTER);
                if working {
                    ui.add_space(12.0);
                    ui.add(egui::Spinner::new().size(11.0).color(p.accent));
                    ui.label(RichText::new("Working").size(11.0).color(p.text_secondary));
                }
            });
        });
    }

    // -------------------------------------------------------------- settings

    fn settings_window(&mut self, ctx: &egui::Context) {
        let p = self.palette;
        let before = self.settings.clone();
        let mut open = self.settings_open;
        // Reset each frame, so closing the window mid-drag cannot leave the
        // zoom pinned at a stale value.
        let mut scale_dragging = false;

        // The colour is not decoration: `.strong()` without one resolves to
        // `strong_text_color`, which is white here so that pressed buttons read
        // against the accent fill — invisible on the light theme's white
        // surfaces. See the note in `theme.rs`.
        egui::Window::new(RichText::new("Settings").size(15.0).strong().color(p.text))
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
                ui.set_min_width(420.0);
                ui.add_space(6.0);

                setting_row(ui, p, icons::MONITOR, "Theme", |ui| {
                    segmented(ui, p, |ui| {
                        for choice in Theme::ALL {
                            let icon = match choice {
                                Theme::System => icons::MONITOR,
                                Theme::Light => icons::SUN,
                                Theme::Dark => icons::MOON,
                            };
                            let label = format!("{icon}  {}", choice.label());
                            if segment(ui, &label, p, self.settings.theme == *choice).clicked() {
                                self.settings.theme = *choice;
                            }
                        }
                    });
                });

                setting_row(ui, p, icons::WRAP_TEXT, "Layout", |ui| {
                    segmented(ui, p, |ui| {
                        for choice in PaneLayout::ALL {
                            if segment(ui, choice.label(), p, self.settings.layout == *choice)
                                .clicked()
                            {
                                self.settings.layout = *choice;
                            }
                        }
                    });
                });

                setting_row(ui, p, icons::SEARCH, "Interface scale", |ui| {
                    let response = slider(
                        ui,
                        p,
                        egui::Slider::new(&mut self.settings.ui_scale, settings::ui_scale_range())
                            .step_by(0.05)
                            .fixed_decimals(2)
                            .suffix("×"),
                    );
                    // Read by `apply_settings` on the next frame to hold the
                    // zoom steady for the duration of the drag.
                    scale_dragging = response.dragged();
                });

                setting_row(ui, p, icons::TYPE, "Editor font size", |ui| {
                    slider(
                        ui,
                        p,
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
                    toggle(ui, &mut self.settings.wrap, p);
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
        let drag_ended = self.scale_dragging && !scale_dragging;
        self.scale_dragging = scale_dragging;

        // Saving on every dragged pixel would rewrite the file continuously,
        // so the scale lands on disk when the drag ends — and that release
        // frame has to be its own trigger, because by then the value matches
        // the snapshot taken at the top of this function and looks unchanged.
        if drag_ended || (self.settings != before && !scale_dragging) {
            self.settings.save();
        }
    }
}

enum PaneAction {
    Generate,
    Copy,
    /// Carry this pane's output into another tool's input.
    SendTo(usize),
}

// ---------------------------------------------------------- pane building

/// Splits a rect down the middle, leaving a gap so the two halves read as
/// separate panes rather than one box with a line in it.
fn halve(rect: Rect, horizontally: bool) -> (Rect, Rect) {
    const GAP: f32 = 10.0;
    if horizontally {
        let width = (rect.width() - GAP) / 2.0;
        (
            Rect::from_min_size(rect.min, Vec2::new(width, rect.height())),
            Rect::from_min_max(egui::pos2(rect.max.x - width, rect.min.y), rect.max),
        )
    } else {
        let height = (rect.height() - GAP) / 2.0;
        (
            Rect::from_min_size(rect.min, Vec2::new(rect.width(), height)),
            Rect::from_min_max(egui::pos2(rect.min.x, rect.max.y - height), rect.max),
        )
    }
}

/// A pane's title row, with room on the right for its actions.
fn pane_header<R>(ui: &mut Ui, p: Palette, title: &str, actions: impl FnOnce(&mut Ui) -> R) -> R {
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

/// The framed box a pane's content sits in, filling whatever is left below the
/// header.
fn pane_body(ui: &mut Ui, p: Palette, content: impl FnOnce(&mut Ui)) {
    Frame::none()
        .fill(p.surface)
        .stroke(theme::hairline(p.border))
        .rounding(Rounding::same(theme::ROUNDING))
        .inner_margin(Margin::same(10.0))
        .show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            content(ui);
        });
}

/// Scroll container for a code editor, in either wrapping mode.
///
/// egui wraps by default at the available width. "No wrap" needs two things:
/// a layouter that ignores the wrap width, and a scroll area that also
/// scrolls horizontally. Passing an infinite desired width does not work — the
/// editor would allocate infinite space and break the scroll bars.
///
/// `id` must differ between panes. Scroll areas persist their offset by id, so
/// two sharing one id fight over the same stored state — which egui reports by
/// painting a "First/Second use of ScrollArea ID" warning over the widget.
fn editor(
    ui: &mut Ui,
    p: Palette,
    wrap: bool,
    id: &str,
    show: impl FnOnce(&mut Ui, Option<&mut dyn FnMut(&Ui, &str, f32) -> Arc<egui::Galley>>) -> bool,
) -> bool {
    let font = TextStyle::Monospace.resolve(ui.style());
    let color = p.text;
    let mut no_wrap = move |ui: &Ui, text: &str, _wrap_width: f32| {
        let job = LayoutJob::simple(text.to_owned(), font.clone(), color, f32::INFINITY);
        ui.fonts(|f| f.layout_job(job))
    };

    if wrap {
        ScrollArea::vertical()
            .id_salt(id)
            .auto_shrink([false, false])
            .show(ui, |ui| show(ui, None))
            .inner
    } else {
        ScrollArea::both()
            .id_salt((id, "nowrap"))
            .auto_shrink([false, false])
            .show(ui, |ui| show(ui, Some(&mut no_wrap)))
            .inner
    }
}

/// One tool in the sidebar: icon, name, and an accent-tinted background when
/// selected. Returns true when clicked.
fn tool_row(ui: &mut Ui, p: Palette, selected: bool, name: &str, icon: &str) -> bool {
    let height = ui.text_style_height(&TextStyle::Body) + 14.0;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());

    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            Rounding::same(theme::ROUNDING_SMALL),
            if selected { p.accent_soft } else { p.elevated },
        );
    }
    if selected {
        // A short accent bar reads as "current" without shouting.
        let bar = Rect::from_min_size(
            rect.left_top() + Vec2::new(0.0, height * 0.25),
            Vec2::new(2.5, height * 0.5),
        );
        ui.painter().rect_filled(bar, Rounding::same(2.0), p.accent);
    }

    let icon_color = if selected { p.accent } else { p.text_secondary };
    let text_color = if selected { p.text } else { p.text_secondary };

    // Painted rather than added as `Label` widgets. Labels are selectable by
    // default, which makes them sense clicks for text selection; sitting on
    // top of the row, they swallowed the click that selects the tool — so the
    // row responded on its padding but went dead over the name.
    let painter = ui.painter();
    let middle = rect.center().y;
    let icon_rect = painter.text(
        egui::pos2(rect.left() + 12.0, middle),
        egui::Align2::LEFT_CENTER,
        icon,
        egui::FontId::proportional(14.0),
        icon_color,
    );
    painter.text(
        egui::pos2(icon_rect.right() + 8.0, middle),
        egui::Align2::LEFT_CENTER,
        name,
        TextStyle::Body.resolve(ui.style()),
        text_color,
    );

    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

/// A labelled row in the settings window: icon, name, control on the right.
fn setting_row(ui: &mut Ui, p: Palette, icon: &str, label: &str, control: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(icon).color(p.text_muted));
        ui.add_space(2.0);
        ui.label(RichText::new(label).color(p.text));
        ui.with_layout(Layout::right_to_left(Align::Center), control);
    });
    ui.add_space(12.0);
}

/// The track a row of `segment`s sits in.
///
/// The direction is pinned left-to-right rather than using `ui.horizontal`,
/// which inherits the parent's direction — inside the right-aligned settings
/// rows that silently reversed every segmented control.
fn segmented(ui: &mut Ui, p: Palette, segments: impl FnOnce(&mut Ui)) {
    Frame::none()
        .fill(p.surface)
        .rounding(Rounding::same(theme::ROUNDING_SMALL))
        .inner_margin(Margin::same(2.0))
        .show(ui, |ui| {
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                segments(ui);
            });
        });
}

/// "1,204 chars", with thousands separators.
fn count(n: usize, noun: &str) -> String {
    let digits = n.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    let plural = if n == 1 { "" } else { "s" };
    format!("{grouped} {noun}{plural}")
}

/// Tools declare no icon — that is UI vocabulary, and keeping it here means a
/// new tool needs no change to this file to look right: unmapped ids fall back
/// to something sensible for their category. Add a case only to do better than
/// the fallback.
fn tool_icon(meta: &rustafari_core::ToolMeta) -> &'static str {
    match meta.id {
        "json-formatter" => icons::BRACES,
        "json-diff" | "yaml-diff" | "xml-diff" => icons::DIFF,
        "base64" => icons::BINARY,
        "url-encode" => icons::LINK,
        "hash" => icons::HASH,
        "uuid" => icons::FINGERPRINT,
        "cron" => icons::CLOCK,
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
        self.shortcuts(ctx);
        let p = self.palette;

        if let Some(result) = self.worker.poll() {
            self.output_stats = result
                .as_ref()
                .map(|s| TextStats::of(s))
                .unwrap_or_default();
            self.output = result;
        }
        // While a run is out, wake up in time to show the "Working" indicator.
        // Completion itself is signalled by the worker.
        if self.worker.is_pending() {
            ctx.request_repaint_after(SHOW_WORKING_AFTER);
        }
        // Wake once more when the "Copied" label is due to revert.
        if let Some(at) = self.copied_at {
            match COPIED_FOR.checked_sub(at.elapsed()) {
                Some(left) => ctx.request_repaint_after(left),
                None => self.copied_at = None,
            }
        }

        egui::TopBottomPanel::bottom("status")
            .exact_height(26.0)
            .show_separator_line(false)
            .frame(
                Frame::none()
                    .fill(p.surface)
                    .inner_margin(Margin::symmetric(10.0, 0.0)),
            )
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                ui.painter().line_segment(
                    [rect.left_top(), rect.right_top()],
                    theme::hairline(p.border),
                );
                self.status_bar(ui);
            });

        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(232.0)
            .width_range(200.0..=340.0)
            .frame(
                Frame::none()
                    .fill(p.surface)
                    .inner_margin(Margin::symmetric(10.0, 0.0)),
            )
            .show(ctx, |ui| self.sidebar(ui));

        egui::CentralPanel::default()
            .frame(Frame::none().fill(p.base).inner_margin(Margin {
                left: 20.0,
                right: 20.0,
                top: 16.0_f32.max(TITLEBAR_INSET),
                bottom: 14.0,
            }))
            .show(ctx, |ui| {
                self.header(ui);
                ui.add_space(14.0);

                if self.options_row(ui) {
                    self.dirty = true;
                }
                ui.add_space(12.0);

                self.panes(ui);
            });

        self.settings_window(ctx);

        if self.dirty {
            self.submit();
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // The selected tool and the pane split change without going through
        // the settings window, so make sure the latest values are recorded.
        self.settings.save();
    }
}
