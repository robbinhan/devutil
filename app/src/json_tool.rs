//! JSON workbench: paste, analyze, format, minify, diagnostics, and a tree.

use std::rc::Rc;

use json_core::{AnalysisEngine, AnalysisResult, Diagnostic, Disposition, SourceRange};

use crate::chrome::{self, column_pane};
use crate::find::{self, TextSearch};
use crate::search_ui;

const SAMPLE: &str = "{\n  \"ok\": true,\n  \"name\": \"DevUtil\",\n  \"count\": 3\n}";
use crate::json_model::{
    self, OutputMode, TreeRow, cap_message, field_text, filtered_tree_rows, output_text,
    reveal_match, severity_name, tree_rows, window_message,
};
use nsh_core::ui::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindTarget {
    Input,
    Result,
    Tree,
}
use nsh_core::EllipsizeMode;

#[derive(Clone, Copy)]
pub struct JsonState {
    /// Full document. Analyze, Format, Minify, and Copy read this.
    pub source: Signal<String>,
    /// What the input `NSTextView` is allowed to hold.
    pub field: Signal<String>,
    pub field_capped: Signal<bool>,
    pub result: Signal<Option<AnalysisResult>>,
    pub mode: Signal<OutputMode>,
    /// Full formatted text for Copy. Not bound to a text view.
    pub formatted: Signal<String>,
    /// Prefix of `formatted` bound to the output text view.
    pub output_field: Signal<String>,
    pub output_capped: Signal<bool>,
    pub has_output: Signal<bool>,
    pub expanded: Signal<Vec<String>>,
    pub tree_open: Signal<bool>,
    pub steps_open: Signal<bool>,
    pub search: TextSearch,
    pub find_target: Signal<FindTarget>,
    pub field_origin: Signal<usize>,
    pub field_line: Signal<u32>,
    pub output_origin: Signal<usize>,
    pub output_line: Signal<u32>,
    /// `None` shows the normal tree. `Some` is the filtered tree, possibly empty.
    pub filtered_tree: Signal<Option<Vec<TreeRow>>>,
    pub tree_cursor: Signal<usize>,
}

impl JsonState {
    pub fn new() -> Self {
        Self {
            source: signal(String::new()),
            field: signal(String::new()),
            field_capped: signal(false),
            result: signal(None),
            mode: signal(OutputMode::Pretty),
            formatted: signal(String::new()),
            output_field: signal(String::new()),
            output_capped: signal(false),
            has_output: signal(false),
            expanded: signal(vec!["root".to_string()]),
            tree_open: signal(true),
            steps_open: signal(true),
            search: TextSearch::new(),
            find_target: signal(FindTarget::Input),
            field_origin: signal(0),
            field_line: signal(0),
            output_origin: signal(0),
            output_line: signal(0),
            filtered_tree: signal(None),
            tree_cursor: signal(0),
        }
    }

    pub fn publish_source(self, text: String) {
        let shown = field_text(&text);
        self.field_origin.set(0);
        self.field_line.set(0);
        self.field_capped.set(shown.capped);
        self.field.set(shown.text);
        self.source.set(text);
        if self.search.open.get() {
            self.schedule_search();
        }
    }

    pub fn schedule_search(self) {
        let target = self.find_target;
        self.search.schedule(move || match target.get() {
            FindTarget::Result => self.formatted.get(),
            FindTarget::Input | FindTarget::Tree => self.source.get(),
        });
    }

    pub fn close_search(self) {
        self.search.open.set(false);
        self.restore_input();
        self.restore_output();
        self.filtered_tree.set(None);
    }

    pub fn step_tree(self, delta: isize) {
        let next = self.filtered_tree.with(|rows| {
            let Some(rows) = rows else {
                return 0;
            };
            let matches = rows
                .iter()
                .enumerate()
                .filter(|(_, row)| row.matched)
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            if matches.is_empty() {
                return 0;
            }
            let pos = matches
                .iter()
                .position(|index| *index == self.tree_cursor.get())
                .unwrap_or(0);
            let next = (pos as isize + delta).rem_euclid(matches.len() as isize) as usize;
            matches[next]
        });
        self.tree_cursor.set(next);
        self.publish_find_label();
    }

    fn restore_input(self) {
        let shown = self.source.with(|text| field_text(text));
        self.field_origin.set(0);
        self.field_line.set(0);
        self.field_capped.set(shown.capped);
        self.field.set(shown.text);
    }

    fn restore_output(self) {
        let shown = self.formatted.with(|text| field_text(text));
        self.output_origin.set(0);
        self.output_line.set(0);
        self.output_capped.set(!self.formatted.with(String::is_empty) && shown.capped);
        self.output_field.set(shown.text);
    }

    fn publish_find_label(self) {
        if self.find_target.get() != FindTarget::Tree {
            self.search.publish_label();
            return;
        }
        let (index, total) = self.filtered_tree.with(|rows| {
            let Some(rows) = rows else {
                return (0, 0);
            };
            let matches = rows
                .iter()
                .enumerate()
                .filter(|(_, row)| row.matched)
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            let pos = matches
                .iter()
                .position(|index| *index == self.tree_cursor.get())
                .unwrap_or(0);
            (pos, matches.len())
        });
        let label = self.search.query.with(|query| {
            self.search.error.with(|error| {
                find::status_label(query.is_empty(), false, error, index, total, false)
            })
        });
        self.search.label.set(label);
    }

    /// A keystroke in the field. The view never holds more than the preview,
    /// so a large paste arrives clipped; the clipboard still has the rest.
    pub fn on_field(self, text: String) {
        if self.field.with(|current| current == &text) {
            return;
        }
        let origin = self.field_origin.get();
        let capped = self.field_capped.get();
        let preview = self.field.get();
        if (capped || origin > 0) && is_preview_edit(&preview, &text) {
            // Put the preview back. The full document stays in `source`.
            self.field.update(|_| {});
            return;
        }
        if origin == 0 && json_model::utf16_len(&text) + 8 >= json_model::FIELD_LIMIT {
            self.recover_clipboard(text);
            return;
        }
        if origin == 0 {
            self.publish_source(text);
            return;
        }
        let Some(spliced) = self
            .source
            .with(|source| json_model::splice_window(source, origin, preview.len(), &text))
        else {
            self.field.update(|_| {});
            return;
        };
        self.publish_source(spliced);
    }

    fn recover_clipboard(self, clipped: String) {
        self.field.set(clipped.clone());
        spawn(async move {
            let clip = clipboard_string().await;
            if clip.len() > clipped.len().saturating_add(8) && clip.starts_with(&clipped) {
                self.publish_source(clip);
            } else {
                self.publish_source(clipped);
            }
        });
    }

    pub fn analyze(self, mode: OutputMode) {
        let result = self
            .source
            .with(|source| AnalysisEngine::new().analyze(source));
        let text = output_text(&result, mode);
        self.result.set(Some(result));
        self.mode.set(mode);
        self.expanded.set(vec!["root".to_string()]);
        self.store_output(text);
    }

    pub fn set_mode(self, mode: OutputMode) {
        if self.mode.get() == mode {
            return;
        }
        let text = self.result.with(|result| {
            result
                .as_ref()
                .and_then(|result| output_text(result, mode))
        });
        self.mode.set(mode);
        self.store_output(text);
    }

    fn store_output(self, text: Option<String>) {
        let Some(full) = text else {
            self.formatted.set(String::new());
            self.output_field.set(String::new());
            self.output_origin.set(0);
            self.output_line.set(0);
            self.output_capped.set(false);
            self.has_output.set(false);
            if self.search.open.get() {
                self.schedule_search();
            }
            return;
        };
        let shown = field_text(&full);
        self.output_origin.set(0);
        self.output_line.set(0);
        self.formatted.set(full);
        self.output_field.set(shown.text);
        self.output_capped.set(shown.capped);
        self.has_output.set(true);
        if self.search.open.get() {
            self.schedule_search();
        }
    }

    pub fn use_sample(self) {
        self.publish_source(SAMPLE.to_string());
        self.analyze(OutputMode::Pretty);
    }

    pub fn reset(self) {
        self.publish_source(String::new());
        self.result.set(None);
        self.mode.set(OutputMode::Pretty);
        self.store_output(None);
        self.expanded.set(vec!["root".to_string()]);
        self.tree_open.set(true);
        self.steps_open.set(true);
        self.filtered_tree.set(None);
    }

    pub fn paste(self) {
        spawn(async move {
            let text = clipboard_string().await;
            if text.is_empty() {
                return;
            }
            self.publish_source(text);
            self.analyze(OutputMode::Pretty);
        });
    }

    pub fn apply(self, text: String) {
        self.publish_source(text);
        self.analyze(OutputMode::Pretty);
    }

    /// The full formatted document, for Copy. Not the text-view preview.
    pub fn output(self) -> String {
        self.formatted.get()
    }
}

fn is_preview_edit(preview: &str, incoming: &str) -> bool {
    if incoming.is_empty() || preview.is_empty() {
        return false;
    }
    let shared = preview
        .bytes()
        .zip(incoming.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    shared > preview.len() / 2
}

pub fn toolbar_buttons(state: JsonState, copied: Signal<Option<String>>) -> impl IntoElement {
    view()
        .flex_none()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(6.0))
        .child(chrome::action_button("Paste", "paste", false, move || {
            state.paste()
        }))
        .child(chrome::action_button("Analyze", "analyze", true, move || {
            state.analyze(OutputMode::Pretty)
        }))
        .child(chrome::action_button("Format", "format", false, move || {
            state.analyze(OutputMode::Pretty)
        }))
        .child(chrome::action_button("Minify", "minify", false, move || {
            state.analyze(OutputMode::Compact)
        }))
        .child(chrome::copy_button("output", copied, move || state.output()))
        .child(chrome::action_button("Reset", "reset", false, move || state.reset()))
}

pub fn body(state: JsonState, copied: Signal<Option<String>>, shortcut: String) -> impl IntoElement {
    let editor = element_ref();
    let output_editor = element_ref();
    let tree_list = element_ref();
    let query_ref = element_ref();
    install_search(state, editor.clone(), output_editor.clone(), tree_list.clone(), query_ref.clone());
    let panes_shortcut = shortcut.clone();
    let bar_state = state;
    view()
        .flex_1()
        .h_full()
        .min_h(px(0.0))
        .flex_col()
        .child(page_header(state, copied, shortcut))
        .child(dynamic(move || {
            if !bar_state.search.open.get() {
                return view().into_element();
            }
            let state = bar_state;
            search_ui::bar(
                state.search,
                &query_ref,
                move |text| {
                    state.search.query.set(text);
                    state.schedule_search();
                },
                move || state.schedule_search(),
                move || step_json(state, -1),
                move || step_json(state, 1),
                find_target_row(state),
            )
            .into_element()
        }))
        .child(
            chrome::workbench()
                .child(input_pane(state, editor.clone()))
                .child(result_pane(
                    state,
                    copied,
                    editor,
                    output_editor,
                    tree_list,
                    panes_shortcut,
                )),
        )
}

fn install_search(
    state: JsonState,
    input: ElementRef,
    output: ElementRef,
    tree_list: ElementRef,
    query_ref: ElementRef,
) {
    search_ui::watch_focus(state.search, query_ref);
    let input_step = input.clone();
    let output_step = output.clone();
    effect(move || {
        let revision = state.search.revision.get();
        untracked(|| {
            if revision == 0 || !state.search.open.get() {
                return;
            }
            refresh_tree(state);
            reveal(state, input.clone(), output.clone());
        });
    });
    effect(move || {
        let _ = state.search.index.get();
        untracked(|| {
            if !state.search.open.get() || state.find_target.get() == FindTarget::Tree {
                return;
            }
            reveal(state, input_step.clone(), output_step.clone());
        });
    });
    effect(move || {
        let cursor = state.tree_cursor.get();
        untracked(|| {
            if !state.search.open.get() || state.find_target.get() != FindTarget::Tree {
                return;
            }
            let tree_list = tree_list.clone();
            spawn(async move {
                next_frame().await;
                tree_list.scroll_to_index(cursor as u32, 0.25, 0.0, false);
            });
        });
    });
}

fn step_json(state: JsonState, delta: isize) {
    if state.find_target.get() == FindTarget::Tree {
        state.step_tree(delta);
    } else {
        state.search.step(delta);
    }
}

fn find_target_row(state: JsonState) -> impl IntoElement {
    let target = state.find_target;
    chrome::segment_row()
        .child(chrome::segment("Input", target, FindTarget::Input, move |value| {
            state.find_target.set(value);
            state.schedule_search();
        }))
        .child(chrome::segment("Result", target, FindTarget::Result, move |value| {
            state.find_target.set(value);
            state.schedule_search();
        }))
        .child(chrome::segment("Tree", target, FindTarget::Tree, move |value| {
            state.find_target.set(value);
            state.schedule_search();
        }))
}

fn refresh_tree(state: JsonState) {
    if state.search.query.with(String::is_empty) {
        state.filtered_tree.set(None);
        state.publish_find_label();
        return;
    }
    let pattern = state.search.query.get();
    let case_sensitive = state.search.case_sensitive.get();
    let regex_mode = state.search.regex.get();
    let Ok(needle) = find::Needle::compile(&pattern, case_sensitive, regex_mode) else {
        state.filtered_tree.set(None);
        state.publish_find_label();
        return;
    };
    let Some(rows) = state.result.with(|result| {
        let value = result.as_ref().and_then(|result| result.value.as_ref())?;
        Some(filtered_tree_rows(value, |text| needle.matches(text)))
    }) else {
        state.filtered_tree.set(None);
        state.publish_find_label();
        return;
    };
    let cursor = rows.iter().position(|row| row.matched).unwrap_or(0);
    state.tree_cursor.set(cursor);
    state.filtered_tree.set(Some(rows));
    state.publish_find_label();
}

fn reveal(state: JsonState, input: ElementRef, output: ElementRef) {
    if state.find_target.get() == FindTarget::Tree {
        state.restore_input();
        state.restore_output();
        return;
    }
    let Some(hit) = state.search.current() else {
        match state.find_target.get() {
            FindTarget::Result => state.restore_output(),
            FindTarget::Input | FindTarget::Tree => state.restore_input(),
        }
        return;
    };
    let target = state.find_target.get();
    let full = match target {
        FindTarget::Result => state.formatted.get(),
        FindTarget::Input | FindTarget::Tree => state.source.get(),
    };
    let revealed = reveal_match(&full, hit.start, hit.end);
    if target == FindTarget::Result {
        state.output_origin.set(revealed.preview.byte_start);
        state.output_line.set(revealed.line);
        state.output_capped.set(revealed.preview.capped);
        state.output_field.set(revealed.preview.text);
        search_ui::select_later(output, revealed.utf16_start, revealed.utf16_end);
    } else {
        state.field_origin.set(revealed.preview.byte_start);
        state.field_line.set(revealed.line);
        state.field_capped.set(revealed.preview.capped);
        state.field.set(revealed.preview.text);
        search_ui::select_later(input, revealed.utf16_start, revealed.utf16_end);
    }
}

fn page_header(state: JsonState, copied: Signal<Option<String>>, shortcut: String) -> impl IntoElement {
    view()
        .flex_none()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .px(px(20.0))
        .pt(px(16.0))
        .pb(px(4.0))
        .child(
            view()
                .gap(px(2.0))
                .child(chrome::title("JSON"))
                .child(chrome::hint("Format, inspect, and walk a document")),
        )
        .child(json_badge(state))
        .child(view().flex_1())
        .child(toolbar_buttons(state, copied))
        .child(
            text(shortcut)
                .text_xs()
                .text_color(move || theme().muted_foreground),
        )
}

fn json_badge(state: JsonState) -> impl IntoElement {
    dynamic(move || {
        let kind = state.result.with(|result| {
            result.as_ref().map(|result| match &result.disposition {
                Disposition::Accepted => Badge::Valid,
                Disposition::Recovered => Badge::Recovered,
                Disposition::Inspect(_) => Badge::Review,
                Disposition::Rejected => Badge::Invalid,
            })
        });
        match kind {
            Some(Badge::Valid) => chrome::badge("Valid", theme().success).into_element(),
            Some(Badge::Recovered) => chrome::badge("Recovered", theme().warning).into_element(),
            Some(Badge::Review) => chrome::badge("Review", theme().info).into_element(),
            Some(Badge::Invalid) => chrome::badge("Invalid", theme().danger).into_element(),
            None => view().into_element(),
        }
    })
}

#[derive(Clone, Copy)]
enum Badge {
    Valid,
    Recovered,
    Review,
    Invalid,
}

fn input_pane(state: JsonState, editor: ElementRef) -> impl IntoElement {
    column_pane()
        .child(input_header(state))
        .child(cap_note(
            state.field_capped,
            state.source,
            state.field,
            state.field_origin,
            state.field_line,
        ))
        .child(
            chrome::mono_field(
                state.field,
                "Paste JSON, or something close to it",
                true,
                true,
                move |text| state.on_field(text),
            )
            .element_ref(&editor)
            .test_id("json-input")
            .flex_1()
            .min_h(px(160.0)),
        )
}

fn input_header(state: JsonState) -> impl IntoElement {
    let source = state.source;
    view()
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .child(chrome::section("Input"))
        .child(view().flex_1())
        .child(
            text(move || source.with(|text| chrome::meta_line(text)))
                .text_xs()
                .text_color(move || theme().muted_foreground),
        )
}

fn cap_note(
    show: Signal<bool>,
    full: Signal<String>,
    preview: Signal<String>,
    origin: Signal<usize>,
    line: Signal<u32>,
) -> impl IntoElement {
    dynamic(move || {
        let origin = origin.get();
        if origin > 0 {
            let message = full.with(|full| window_message(full.len(), line.get()));
            return view().child(
                text(message)
                    .text_xs()
                    .text_color(move || theme().warning),
            );
        }
        if !show.get() {
            return view();
        }
        let message = full.with(|full| preview.with(|preview| cap_message(full.len(), preview.len())));
        view().child(
            text(message)
                .text_xs()
                .text_color(move || theme().warning),
        )
    })
}

fn result_pane(
    state: JsonState,
    copied: Signal<Option<String>>,
    editor: ElementRef,
    output_editor: ElementRef,
    tree_list: ElementRef,
    shortcut: String,
) -> impl IntoElement {
    column_pane()
        .child(result_header(state))
        .child(
            scroll()
                .flex_1()
                .min_h(px(0.0))
                .child(
                    view()
                        .w_full()
                        .gap(px(8.0))
                        .pb(px(8.0))
                        .child(banner_and_output(state, editor, output_editor, shortcut))
                        .child(tree_section(state, copied, tree_list))
                        .child(steps_section(state)),
                ),
        )
}

fn result_header(state: JsonState) -> impl IntoElement {
    let mode = state.mode;
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .child(chrome::section("Result"))
        .child(view().flex_1())
        .child(
            chrome::segment_row()
                .child(chrome::segment("Pretty", mode, OutputMode::Pretty, move |value| {
                    state.set_mode(value)
                }))
                .child(chrome::segment("Compact", mode, OutputMode::Compact, move |value| {
                    state.set_mode(value)
                }))
                .child(chrome::segment("Source", mode, OutputMode::Source, move |value| {
                    state.set_mode(value)
                })),
        )
}

fn banner_and_output(
    state: JsonState,
    editor: ElementRef,
    output_editor: ElementRef,
    shortcut: String,
) -> impl IntoElement {
    dynamic(move || {
        let diagnostics = state.result.with(|result| {
            result.as_ref().map(|result| result.diagnostics.clone())
        });
        let Some(diagnostics) = diagnostics else {
            return empty_result(state, shortcut.clone());
        };
        let mut column = view().w_full().gap(px(12.0));
        if !diagnostics.is_empty() {
            column = column.child(notes(state, diagnostics, editor.clone()));
        }
        if state.has_output.get() {
            column = column.child(output_block(state, output_editor.clone()));
        }
        column
    })
}

fn empty_result(state: JsonState, shortcut: String) -> View {
    let empty = state.source.with(|text| text.is_empty());
    if empty {
        chrome::empty_state(
            "{ }",
            "Nothing here yet",
            "Paste JSON-like text, or start from a small sample.",
            "Use a sample",
            "json-sample",
            move || state.use_sample(),
        )
    } else {
        chrome::empty_state(
            "{ }",
            "Ready to analyze",
            format!("Press Analyze ({shortcut})."),
            "Analyze",
            "json-empty-analyze",
            move || state.analyze(OutputMode::Pretty),
        )
    }
}

fn notes(state: JsonState, diagnostics: Vec<Diagnostic>, editor: ElementRef) -> impl IntoElement {
    let mut card = chrome::card().child(chrome::section("Notes"));
    for diagnostic in diagnostics {
        let tint = tint_for(&diagnostic);
        card = card.child(diagnostic_row(state, diagnostic, tint, editor.clone()));
    }
    card
}

fn tint_for(diagnostic: &Diagnostic) -> AdaptiveColor {
    match diagnostic.severity {
        json_core::DiagnosticSeverity::Error => theme().danger,
        json_core::DiagnosticSeverity::Warning => theme().warning,
        json_core::DiagnosticSeverity::Info => theme().muted_foreground,
    }
}

fn diagnostic_row(
    state: JsonState,
    diagnostic: Diagnostic,
    tint: AdaptiveColor,
    editor: ElementRef,
) -> impl IntoElement {
    let line = format!(
        "{} · {} · line {}, column {}",
        severity_name(diagnostic.severity),
        diagnostic.message,
        diagnostic.line,
        diagnostic.column
    );
    let range = diagnostic.range;
    view()
        .role(AccessibilityRole::Button)
        .on_press(move |_: &()| select_range(state, &editor, range))
        .child(
            text(line)
                .text_xs()
                .text_color(tint),
        )
}

fn select_range(state: JsonState, editor: &ElementRef, range: Option<SourceRange>) {
    let Some(range) = range else {
        return;
    };
    let full = state.source.get();
    let start = find::byte_at_utf16(&full, range.utf16_start);
    let end = find::byte_at_utf16(&full, range.utf16_end);
    let revealed = reveal_match(&full, start, end);
    state.field_origin.set(revealed.preview.byte_start);
    state.field_line.set(revealed.line);
    state.field_capped.set(revealed.preview.capped);
    state.field.set(revealed.preview.text);
    search_ui::select_later(editor.clone(), revealed.utf16_start, revealed.utf16_end);
    let editor = editor.clone();
    spawn(async move {
        next_frame().await;
        editor.focus();
    });
}

fn output_block(state: JsonState, output_editor: ElementRef) -> impl IntoElement {
    view()
        .w_full()
        .gap(px(6.0))
        .child(cap_note(
            state.output_capped,
            state.formatted,
            state.output_field,
            state.output_origin,
            state.output_line,
        ))
        .child(output_field(state.output_field, output_editor))
}

fn output_field(value: Signal<String>, output_editor: ElementRef) -> impl IntoElement {
    chrome::mono_field(value, "", true, false, |_| {})
        .element_ref(&output_editor)
        .test_id("json-output")
        .h(px(220.0))
        .w_full()
}

fn tree_section(
    state: JsonState,
    copied: Signal<Option<String>>,
    tree_list: ElementRef,
) -> impl IntoElement {
    dynamic(move || {
        let open = state.tree_open.get();
        let filtering = state.filtered_tree.with(|rows| rows.is_some());
        let rows = if let Some(filtered) = state.filtered_tree.get() {
            if open {
                Some(filtered)
            } else {
                Some(Vec::new())
            }
        } else {
            state.result.with(|result| {
                result.as_ref().and_then(|result| result.value.as_ref()).map(|value| {
                    if open {
                        tree_rows(value, &state.expanded.get())
                    } else {
                        Vec::new()
                    }
                })
            })
        };
        let Some(rows) = rows else {
            return view();
        };
        let mut block = chrome::card().child(
            view()
                .role(AccessibilityRole::Button)
                .test_id("tree-toggle")
                .on_press(move |_: &()| state.tree_open.set(!state.tree_open.get()))
                .child(
                    text(if open { "▾  Tree" } else { "▸  Tree" })
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(move || theme().foreground),
                ),
        );
        if open && filtering && rows.is_empty() {
            block = block.child(chrome::hint("No matching nodes"));
        } else if open {
            let rows = Rc::new(rows);
            let keys = Rc::clone(&rows);
            block = block.child(
                virtual_list(rows.len(), {
                    let rows = Rc::clone(&rows);
                    move |at| {
                        tree_row(
                            Rc::clone(&rows),
                            at,
                            state.expanded,
                            copied,
                            state.tree_cursor,
                            filtering,
                        )
                    }
                })
                .element_ref(&tree_list)
                .keys(move || keys.iter().map(|row| row.id.clone()).collect::<Vec<_>>())
                .estimated_item_size(28.0)
                .h(px(280.0))
                .w_full(),
            );
        }
        block
    })
}

fn toggle_row(rows: &[TreeRow], index: usize, expanded: Signal<Vec<String>>) {
    let Some(row) = rows.get(index) else {
        return;
    };
    if !row.expandable {
        return;
    }
    let id = row.id.clone();
    let mut open = expanded.get();
    if let Some(found) = open.iter().position(|item| item == &id) {
        open.remove(found);
    } else {
        open.push(id);
    }
    expanded.set(open);
}

fn tree_row(
    rows: Rc<Vec<TreeRow>>,
    at: Position,
    expanded: Signal<Vec<String>>,
    copied: Signal<Option<String>>,
    cursor: Signal<usize>,
    filtering: bool,
) -> impl IntoElement {
    let depth_rows = Rc::clone(&rows);
    let mark_rows = Rc::clone(&rows);
    let label_rows = Rc::clone(&rows);
    let copy_rows = Rc::clone(&rows);
    let copied_rows = Rc::clone(&rows);
    let disclosure_rows = Rc::clone(&rows);
    let label_press_rows = Rc::clone(&rows);
    let tint_rows = Rc::clone(&rows);
    view()
        .flex_row()
        .items_center()
        .h(px(28.0))
        .w_full()
        .min_w(px(0.0))
        .px(px(4.0))
        .gap(px(4.0))
        .rounded(px(6.0))
        .bg(move || {
            let matched = tint_rows.get(at.index()).is_some_and(|row| row.matched);
            if matched && cursor.get() == at.index() {
                theme().list_active
            } else if matched {
                theme().selection
            } else {
                AdaptiveColor::TRANSPARENT
            }
        })
        .hover(|style| style.bg(move || theme().list_head))
        .child(
            text(move || {
                let depth = depth_rows.get(at.index()).map(|row| row.depth).unwrap_or(0);
                if depth == 0 {
                    String::new()
                } else {
                    "│".repeat(depth as usize)
                }
            })
            .font_family(chrome::MONO)
            .text_xs()
            .text_color(move || theme().border),
        )
        .child(
            text(move || match mark_rows.get(at.index()) {
                Some(row) if row.expandable && row.expanded => "▾".to_string(),
                Some(row) if row.expandable => "▸".to_string(),
                _ => " ".to_string(),
            })
            .w(px(14.0))
            .text_xs()
            .text_color(move || theme().muted_foreground)
            .on_press(move |_: &()| {
                if !filtering {
                    toggle_row(&disclosure_rows, at.index(), expanded);
                }
            }),
        )
        .child(tree_label(label_rows, at, expanded, label_press_rows, filtering))
        .child(tree_copy(copy_rows, copied_rows, at, copied))
}

fn tree_copy(
    rows: Rc<Vec<TreeRow>>,
    id_rows: Rc<Vec<TreeRow>>,
    at: Position,
    copied: Signal<Option<String>>,
) -> impl IntoElement {
    view()
        .flex_none()
        .w(px(28.0))
        .h(px(28.0))
        .items_center()
        .justify_center()
        .rounded(px(7.0))
        .hover(|style| style.bg(move || theme().muted))
        .role(AccessibilityRole::Button)
        .label("Copy row")
        .on_press(move |_: &()| {
            let Some(row) = rows.get(at.index()) else {
                return;
            };
            if row.label.is_empty() {
                return;
            }
            set_clipboard(row.label.clone());
            copied.set(Some(row.id.clone()));
        })
        .child(
            text(move || {
                let id = id_rows.get(at.index()).map(|row| row.id.clone());
                if id.is_some() && copied.get() == id {
                    "✓"
                } else {
                    "⧉"
                }
            })
            .text_sm()
            .text_color(move || theme().muted_foreground),
        )
}

fn tree_label(
    rows: Rc<Vec<TreeRow>>,
    at: Position,
    expanded: Signal<Vec<String>>,
    press_rows: Rc<Vec<TreeRow>>,
    filtering: bool,
) -> impl IntoElement {
    let key_rows = Rc::clone(&rows);
    let value_rows = Rc::clone(&rows);
    let color_rows = rows;
    view()
        .flex_1()
        .min_w(px(0.0))
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .on_press(move |_: &()| {
            if !filtering {
                toggle_row(&press_rows, at.index(), expanded);
            }
        })
        .child(
            text(move || {
                key_rows
                    .get(at.index())
                    .map(|row| label_key(&row.label))
                    .unwrap_or_default()
            })
            .flex_none()
            .number_of_lines(1)
            .ellipsize_mode(EllipsizeMode::Tail)
            .font_family(chrome::MONO)
            .text_size(px(12.5))
            .font_weight(FontWeight::MEDIUM)
            .text_color(move || theme().foreground),
        )
        .child(
            text(move || {
                value_rows
                    .get(at.index())
                    .map(|row| label_value(&row.label))
                    .unwrap_or_default()
            })
            .flex_1()
            .min_w(px(0.0))
            .number_of_lines(1)
            .ellipsize_mode(EllipsizeMode::Tail)
            .font_family(chrome::MONO)
            .text_size(px(12.5))
            .text_color(move || {
                color_rows
                    .get(at.index())
                    .map(|row| value_color(&row.label))
                    .unwrap_or_else(|| theme().foreground)
            }),
        )
}

fn label_key(label: &str) -> String {
    label.split_once(": ").map(|(key, _)| key.to_string()).unwrap_or_else(|| label.to_string())
}

fn label_value(label: &str) -> String {
    label.split_once(": ").map(|(_, value)| value.to_string()).unwrap_or_default()
}

fn value_color(label: &str) -> AdaptiveColor {
    let Some((_, value)) = label.split_once(": ") else {
        return theme().muted_foreground;
    };
    let value = value.trim();
    if value.starts_with('"') {
        theme().success
    } else if value == "true" || value == "false" {
        theme().primary
    } else if value == "null" {
        theme().muted_foreground
    } else if value.starts_with(|ch: char| ch.is_ascii_digit() || ch == '-') {
        theme().info
    } else {
        theme().foreground
    }
}

fn steps_section(state: JsonState) -> impl IntoElement {
    dynamic(move || {
        let Some((steps, candidates)) = state.result.with(|result| {
            let result = result.as_ref()?;
            let candidates = match &result.disposition {
                Disposition::Inspect(items) => items.clone(),
                _ => Vec::new(),
            };
            if result.steps.is_empty() && candidates.is_empty() {
                None
            } else {
                Some((result.steps.clone(), candidates))
            }
        }) else {
            return view();
        };
        let open = state.steps_open.get();
        let mut block = view().w_full().gap(px(4.0)).child(
            view()
                .role(AccessibilityRole::Button)
                .test_id("transforms-toggle")
                .on_press(move |_: &()| state.steps_open.set(!state.steps_open.get()))
                .child(
                    text(if open {
                        "▾  Transformations"
                    } else {
                        "▸  Transformations"
                    })
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(move || theme().foreground),
                ),
        );
        if !open {
            return block;
        }
        for step in &steps {
            let mark = if step.automatic { "✦" } else { "?" };
            block = block.child(
                text(format!("{mark} {}", step.summary))
                    .text_xs()
                    .text_color(move || theme().muted_foreground),
            );
        }
        for candidate in candidates {
            let summary = if candidate.steps.is_empty() {
                "Suggested normalization".to_string()
            } else {
                candidate
                    .steps
                    .iter()
                    .map(|step| step.summary.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let text_value = candidate.text;
            block = block.child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        text(summary)
                            .flex_1()
                            .text_xs()
                            .text_color(move || theme().muted_foreground),
                    )
                    .child(chrome::action_button(
                        "Apply",
                        "apply-candidate",
                        true,
                        move || state.apply(text_value.clone()),
                    )),
            );
        }
        block
    })
}

