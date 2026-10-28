//! URL percent encode / decode. ⌘↩ is bound once in the shell.

use url_codec_core::{UrlCodecEngine, UrlCodecMode, UrlStructure};

use crate::chrome::{self, column_pane};
use crate::find::TextSearch;
use crate::json_model::{self, field_text, window_message, FieldWrite};
use crate::search_ui;
use crate::url_model::{self, Tone};
use nsh_core::ui::prelude::*;

pub const SAMPLE: &str = "\
hello%20world%21
name%3DDevUtil%26ok%3D1
a+b%2Bc";

#[derive(Clone, Copy)]
pub struct UrlState {
    pub source: Signal<String>,
    /// What the text view shows. A find hit replaces this with a window that
    /// starts on the match line. `source` stays the whole document.
    pub field: Signal<String>,
    pub field_origin: Signal<usize>,
    pub field_line: Signal<u32>,
    pub field_capped: Signal<bool>,
    pub mode: Signal<UrlCodecMode>,
    pub analysis: Signal<Option<url_codec_core::UrlCodecAnalysis>>,
    pub search: TextSearch,
}

impl UrlState {
    pub fn new() -> Self {
        let sample = SAMPLE.to_string();
        Self {
            source: signal(sample.clone()),
            field: signal(sample),
            field_origin: signal(0),
            field_line: signal(0),
            field_capped: signal(false),
            mode: signal(UrlCodecMode::Decode),
            analysis: signal(None),
            search: TextSearch::new(),
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

    pub fn on_field(self, text: String) {
        let preview = self.field.get();
        let origin = self.field_origin.get();
        let write = self
            .source
            .with(|source| json_model::field_write(source, &preview, origin, &text));
        match write {
            FieldWrite::Echo => {}
            FieldWrite::KeepPreview => {
                self.field.update(|_| {});
            }
            FieldWrite::Replace(next) => self.publish_source(next),
            FieldWrite::Splice(next) => {
                self.field.set(text);
                self.source.set(next);
                if self.search.open.get() {
                    self.schedule_search();
                }
            }
        }
    }

    pub fn close_search(self) {
        self.search.open.set(false);
        self.search.snippets.set(Vec::new());
        let shown = self.source.with(|text| field_text(text));
        self.field_origin.set(0);
        self.field_line.set(0);
        self.field_capped.set(shown.capped);
        self.field.set(shown.text);
    }

    pub fn schedule_search(self) {
        self.search.schedule(move || self.source.get());
    }

    pub fn convert(self) {
        let analysis = UrlCodecEngine::new().convert(&self.source.get(), self.mode.get());
        self.analysis.set(Some(analysis));
    }

    pub fn set_mode(self, mode: UrlCodecMode) {
        self.mode.set(mode);
        if url_model::should_recompute(self.analysis.get().is_some(), &self.source.get()) {
            self.convert();
        }
    }

    pub fn clear(self) {
        self.publish_source(String::new());
        self.analysis.set(None);
    }
}

pub fn body(state: UrlState, copied: Signal<Option<String>>, shortcut: String) -> impl IntoElement {
    let header_shortcut = shortcut.clone();
    let editor = element_ref();
    let query_ref = element_ref();
    search_ui::watch_focus(state.search, query_ref.clone());
    search_ui::watch_windowed_field(
        state.search,
        state.source,
        state.field,
        state.field_origin,
        state.field_line,
        state.field_capped,
        editor.clone(),
    );
    view()
        .flex_1()
        .min_h(px(0.0))
        .flex_col()
        .child(page_header(state, header_shortcut))
        .child(dynamic(move || {
            if !state.search.open.get() {
                return view().into_element();
            }
            search_ui::bar(
                state.search,
                &query_ref,
                move |text| {
                    state.search.query.set(text);
                    state.schedule_search();
                },
                move || state.schedule_search(),
                move || state.search.step(-1),
                move || state.search.step(1),
                view(),
            )
            .into_element()
        }))
        .child(
            chrome::workbench()
                .child(input_pane(state, shortcut, editor))
                .child(result_pane(state, copied)),
        )
}

fn page_header(state: UrlState, shortcut: String) -> impl IntoElement {
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
                .child(chrome::title("URL"))
                .child(chrome::hint("Percent-encoding for form bodies")),
        )
        .child(view().flex_1())
        .child(chrome::action_button("Clear", "url-clear", false, move || {
            state.clear()
        }))
        .child(
            text(shortcut)
                .text_xs()
                .text_color(move || theme().muted_foreground),
        )
        .child(chrome::action_button(
            move || url_model::action_label(state.mode.get()),
            "url-convert",
            true,
            move || state.convert(),
        ))
}

fn input_pane(state: UrlState, shortcut: String, editor: ElementRef) -> impl IntoElement {
    let source = state.source;
    column_pane()
        .child(header(source))
        .child(mode_row(state, shortcut))
        .child(status_line(state))
        .child(field_note(state))
        .child(
            chrome::mono_field(state.field, "hello%20world", true, true, move |text| {
                state.on_field(text);
            })
            .element_ref(&editor)
            .test_id("url-input")
                .flex_1()
                .min_h(px(120.0)),
        )
}

fn field_note(state: UrlState) -> impl IntoElement {
    dynamic(move || {
        let origin = state.field_origin.get();
        if origin == 0 {
            return view().into_element();
        }
        let message = state
            .source
            .with(|full| window_message(full.len(), state.field_line.get()));
        view()
            .child(
                text(message)
                    .text_xs()
                    .text_color(move || theme().warning),
            )
            .into_element()
    })
}

fn header(source: Signal<String>) -> impl IntoElement {
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .child(
            view()
                .gap(px(2.0))
                .child(chrome::section("Input"))
                .child(chrome::hint("Spaces become +, and other bytes become %XX")),
        )
        .child(view().flex_1())
        .child(
            text(move || chrome::meta_line(&source.get()))
                .text_xs()
                .text_color(move || theme().muted_foreground),
        )
}

fn mode_row(state: UrlState, _shortcut: String) -> impl IntoElement {
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .child(chrome::hint("Mode"))
        .child(
            chrome::segment_row()
                .child(chrome::segment("Decode", state.mode, UrlCodecMode::Decode, move |mode| {
                    state.set_mode(mode)
                }))
                .child(chrome::segment("Encode", state.mode, UrlCodecMode::Encode, move |mode| {
                    state.set_mode(mode)
                })),
        )
}

fn status_line(state: UrlState) -> impl IntoElement {
    text(move || {
        url_model::status_line(&state.source.get(), state.mode.get(), state.analysis.get().as_ref()).text
    })
    .text_xs()
    .text_color(move || {
        let tone = url_model::status_line(
            &state.source.get(),
            state.mode.get(),
            state.analysis.get().as_ref(),
        )
        .tone;
        tone_color(tone)
    })
}

fn tone_color(tone: Tone) -> AdaptiveColor {
    match tone {
        Tone::Muted => theme().muted_foreground,
        Tone::Info => theme().info,
        Tone::Success => theme().success,
        Tone::Danger => theme().danger,
    }
}

fn result_pane(state: UrlState, copied: Signal<Option<String>>) -> impl IntoElement {
    column_pane()
        .child(chrome::section("Result"))
        .child(
            scroll().flex_1().min_h(px(0.0)).child(
                view()
                    .w_full()
                    .gap(px(8.0))
                    .pb(px(8.0))
                    .child(results(state, copied)),
            ),
        )
}

fn results(state: UrlState, copied: Signal<Option<String>>) -> impl IntoElement {
    dynamic(move || match state.analysis.get() {
        None => chrome::empty_state(
            "↗",
            "Nothing converted yet",
            "Decode or encode the text, or load a percent-encoded sample.",
            "Load sample",
            "url-sample",
            move || {
                state.publish_source(SAMPLE.to_string());
                state.convert();
            },
        )
        .into_element(),
        Some(analysis) if analysis.diagnostic.is_some() => {
            let message = analysis.diagnostic.clone().unwrap_or_default();
            view()
                .child(chrome::state_card(
                    "Couldn't convert this input",
                    "See the message under the controls.",
                    theme().danger,
                ))
                .child(
                    text(message)
                        .text_xs()
                        .text_color(move || theme().danger),
                )
                .into_element()
        }
        Some(analysis) => success(analysis, copied).into_element(),
    })
}

fn success(
    analysis: url_codec_core::UrlCodecAnalysis,
    copied: Signal<Option<String>>,
) -> impl IntoElement {
    let (title, subtitle) = match analysis.mode {
        UrlCodecMode::Decode => (
            "Decoded text",
            "Percent-decoded (form-urlencoded)",
        ),
        UrlCodecMode::Encode => (
            "Encoded text",
            "Percent-encoded (form-urlencoded)",
        ),
    };
    let value = analysis.output.clone().unwrap_or_default();
    let mut column = view().w_full().gap(px(8.0)).child(output_card(
        title,
        subtitle,
        value,
        copied,
    ));
    if analysis.mode == UrlCodecMode::Decode {
        if let Some(structure) = analysis.structure.clone() {
            column = column.child(structure_view(structure, copied));
        }
    }
    column
}

fn output_card(
    title: &'static str,
    subtitle: &'static str,
    value: String,
    copied: Signal<Option<String>>,
) -> impl IntoElement {
    let copy_value = value.clone();
    chrome::card()
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .child(
                    view()
                        .flex_1()
                        .gap(px(2.0))
                        .child(
                            text(title)
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(move || theme().foreground),
                        )
                        .child(
                            text(subtitle)
                                .text_xs()
                                .text_color(move || theme().muted_foreground),
                        ),
                )
                .child(chrome::copy_button("url-output", copied, move || {
                    copy_value.clone()
                })),
        )
        .child(
            chrome::mono_field(value, "", true, false, |_| {})
                .test_id("url-output")
                .w_full()
                .min_h(px(72.0)),
        )
}

fn structure_view(structure: UrlStructure, copied: Signal<Option<String>>) -> impl IntoElement {
    let mut column = view().w_full().gap(px(8.0));
    if structure.has_parts() {
        let mut card = chrome::card().child(chrome::section("URL parts"));
        if let Some(value) = &structure.scheme {
            card = card.child(part_row("Scheme", value.clone(), "url-scheme", copied));
        }
        if let Some(value) = &structure.host {
            card = card.child(part_row("Host", value.clone(), "url-host", copied));
        }
        if let Some(value) = &structure.path {
            card = card.child(part_row("Path", value.clone(), "url-path", copied));
        }
        column = column.child(card);
    }
    if structure.has_query() {
        let count = structure.query_params.len();
        let mut card = chrome::card().child(
            chrome::section("Query"),
        );
        card = card.child(
            text(format!(
                "{count} {}",
                if count == 1 { "parameter" } else { "parameters" }
            ))
            .text_xs()
            .text_color(move || theme().muted_foreground),
        );
        for (index, param) in structure.query_params.iter().enumerate() {
            let id = format!("url-param-{index}");
            let value = param.value.clone();
            card = card.child(
                view()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .w_full()
                    .child(
                        text(param.key.clone())
                            .w(relative(0.3))
                            .min_w(px(0.0))
                            .number_of_lines(1)
                            .ellipsize_mode(nsh_core::EllipsizeMode::Tail)
                            .font_family("monospace")
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .selectable(true)
                            .text_color(move || theme().foreground),
                    )
                    .child(
                        text(param.value.clone())
                            .flex_1()
                            .min_w(px(0.0))
                            .number_of_lines(2)
                            .ellipsize_mode(nsh_core::EllipsizeMode::Tail)
                            .font_family("monospace")
                            .text_xs()
                            .selectable(true)
                            .text_color(move || theme().foreground),
                    )
                    .child(chrome::copy_button(id, copied, move || value.clone())),
            );
        }
        column = column.child(card);
    }
    column
}

fn part_row(
    label: &'static str,
    value: String,
    id: &'static str,
    copied: Signal<Option<String>>,
) -> impl IntoElement {
    let copy_value = value.clone();
    view()
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .w_full()
        .child(
            text(label)
                .w(px(64.0))
                .text_xs()
                .text_color(move || theme().muted_foreground),
        )
        .child(
            text(value)
                .flex_1()
                .min_w(px(0.0))
                .font_family("monospace")
                .text_xs()
                .selectable(true)
                .text_color(move || theme().foreground),
        )
        .child(chrome::copy_button(id, copied, move || copy_value.clone()))
}
