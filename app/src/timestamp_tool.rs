//! Timestamp workbench. ⌘↩ is bound once in the shell and calls [`TimestampState::convert`].

use timestamp_core::{TimestampEngine, TimestampUnit};

use crate::chrome::{self, column_pane};
use crate::find::TextSearch;
use crate::json_model::{self, field_text, window_message, FieldWrite};
use crate::search_ui;
use crate::timestamp_model::{self, DisplayZone, Tone};
use nsh_core::ui::prelude::*;

pub const SAMPLE: &str = "\
1700000000
1700000000000
2026-08-12T15:30:00+08:00
2026-08-12 15:30:00";

#[derive(Clone, Copy)]
pub struct TimestampState {
    pub source: Signal<String>,
    /// What the text view shows. Find replaces this with a window that
    /// starts on the match line. `source` stays the whole document.
    pub field: Signal<String>,
    pub field_origin: Signal<usize>,
    pub field_line: Signal<u32>,
    pub field_capped: Signal<bool>,
    pub unit: Signal<TimestampUnit>,
    pub zone: Signal<DisplayZone>,
    pub analysis: Signal<Option<timestamp_core::TimestampAnalysis>>,
    pub analysis_zone: Signal<Option<DisplayZone>>,
    pub search: TextSearch,
}

impl TimestampState {
    pub fn new() -> Self {
        let sample = SAMPLE.to_string();
        Self {
            source: signal(sample.clone()),
            field: signal(sample),
            field_origin: signal(0),
            field_line: signal(0),
            field_capped: signal(false),
            unit: signal(TimestampUnit::Automatic),
            zone: signal(DisplayZone::Local),
            analysis: signal(None),
            analysis_zone: signal(None),
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
        let zone = self.zone.get();
        let analysis =
            TimestampEngine::new().analyze(&self.source.get(), self.unit.get(), zone.time_zone());
        self.analysis_zone.set(Some(zone));
        self.analysis.set(Some(analysis));
    }

    pub fn set_unit(self, unit: TimestampUnit) {
        self.unit.set(unit);
        if timestamp_model::should_recompute(self.analysis.get().is_some(), &self.source.get()) {
            self.convert();
        }
    }

    pub fn set_zone(self, zone: DisplayZone) {
        self.zone.set(zone);
        if timestamp_model::should_recompute(self.analysis.get().is_some(), &self.source.get()) {
            self.convert();
        }
    }

    pub fn clear(self) {
        self.publish_source(String::new());
        self.analysis.set(None);
        self.analysis_zone.set(None);
    }
}

pub fn body(
    state: TimestampState,
    copied: Signal<Option<String>>,
    shortcut: String,
) -> impl IntoElement {
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

fn page_header(state: TimestampState, shortcut: String) -> impl IntoElement {
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
                .child(chrome::title("Timestamp"))
                .child(chrome::hint("Unix time and calendar dates")),
        )
        .child(view().flex_1())
        .child(chrome::action_button("Clear", "ts-clear", false, move || {
            state.clear()
        }))
        .child(
            text(shortcut)
                .text_xs()
                .text_color(move || theme().muted_foreground),
        )
        .child(chrome::action_button("Convert", "ts-convert", true, move || {
            state.convert()
        }))
}

fn input_pane(state: TimestampState, _shortcut: String, editor: ElementRef) -> impl IntoElement {
    let source = state.source;
    column_pane()
        .child(header(source))
        .child(unit_row(state))
        .child(zone_row(state))
        .child(status_line(state))
        .child(field_note(state))
        .child(
            chrome::mono_field(state.field, "1700000000", true, true, move |text| {
                state.on_field(text);
            })
            .element_ref(&editor)
            .test_id("timestamp-input")
                .flex_1()
                .min_h(px(120.0)),
        )
}

fn field_note(state: TimestampState) -> impl IntoElement {
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
                .child(chrome::hint(
                    "Unix seconds, milliseconds, or a date",
                )),
        )
        .child(view().flex_1())
        .child(
            text(move || chrome::meta_line(&source.get()))
                .text_xs()
                .text_color(move || theme().muted_foreground),
        )
}

fn unit_row(state: TimestampState) -> impl IntoElement {
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .child(chrome::hint("Interpret as"))
        .child(
            chrome::segment_row()
                .child(chrome::segment(
                    "Automatic",
                    state.unit,
                    TimestampUnit::Automatic,
                    move |unit| state.set_unit(unit),
                ))
                .child(chrome::segment(
                    "Seconds",
                    state.unit,
                    TimestampUnit::Seconds,
                    move |unit| state.set_unit(unit),
                ))
                .child(chrome::segment(
                    "Milliseconds",
                    state.unit,
                    TimestampUnit::Milliseconds,
                    move |unit| state.set_unit(unit),
                )),
        )
}

fn zone_row(state: TimestampState) -> impl IntoElement {
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .child(chrome::hint("Timezone"))
        .child(
            chrome::segment_row()
                .child(chrome::segment("Local", state.zone, DisplayZone::Local, move |zone| {
                    state.set_zone(zone)
                }))
                .child(chrome::segment("UTC", state.zone, DisplayZone::Utc, move |zone| {
                    state.set_zone(zone)
                })),
        )
}

fn status_line(state: TimestampState) -> impl IntoElement {
    text(move || {
        timestamp_model::status_line(
            &state.source.get(),
            state.zone.get(),
            state.analysis.get().as_ref(),
            state.analysis_zone.get(),
        )
        .text
    })
    .text_xs()
    .text_color(move || tone_color(status_tone(state)))
}

fn status_tone(state: TimestampState) -> Tone {
    timestamp_model::status_line(
        &state.source.get(),
        state.zone.get(),
        state.analysis.get().as_ref(),
        state.analysis_zone.get(),
    )
    .tone
}

fn tone_color(tone: Tone) -> AdaptiveColor {
    match tone {
        Tone::Muted => theme().muted_foreground,
        Tone::Info => theme().info,
        Tone::Success => theme().success,
        Tone::Danger => theme().danger,
    }
}

fn result_pane(state: TimestampState, copied: Signal<Option<String>>) -> impl IntoElement {
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

fn results(state: TimestampState, copied: Signal<Option<String>>) -> impl IntoElement {
    dynamic(move || match state.analysis.get() {
        None => chrome::empty_state(
            "◷",
            "No conversion yet",
            "Convert the time in the editor, or load a sample of seconds, milliseconds, and dates.",
            "Load sample",
            "ts-sample",
            move || {
                state.publish_source(SAMPLE.to_string());
                state.convert();
            },
        )
        .into_element(),
        Some(analysis) if analysis.diagnostic.is_some() => chrome::state_card(
            "Couldn't convert this input",
            "The message under the controls describes what to change.",
            theme().danger,
        )
        .into_element(),
        Some(analysis) => cards(timestamp_model::result_cards(&analysis), copied).into_element(),
    })
}

fn cards(
    cards: Vec<timestamp_model::ResultCard>,
    copied: Signal<Option<String>>,
) -> impl IntoElement {
    let mut grid = view().w_full().gap(px(8.0));
    for pair in cards.chunks(2) {
        let mut row = view().flex_row().flex_wrap().gap(px(8.0)).w_full();
        for card in pair {
            let value = card.value.clone();
            let id = card.id.to_string();
            row = row.child(
                chrome::card()
                    .flex_1()
                    .min_w(px(220.0))
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
                                        text(card.title)
                                            .text_xs()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(move || theme().foreground),
                                    )
                                    .child(
                                        text(card.subtitle.clone())
                                            .text_xs()
                                            .text_color(move || theme().muted_foreground),
                                    ),
                            )
                            .child(chrome::copy_button(id, copied, move || value.clone())),
                    )
                    .child(
                        chrome::mono_field(card.value.clone(), "", false, false, |_| {})
                            .w_full()
                            .shadow_none(),
                    ),
            );
        }
        grid = grid.child(row);
    }
    grid
}
