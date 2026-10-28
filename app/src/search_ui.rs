//! The find bar. One compact row, the same card chrome as the rest of the
//! workbench: group box, hairline border, 12 px corners.

use crate::chrome;
use crate::find::{PieceKind, SnippetPiece, TextSearch};
use crate::json_model::{self, reveal_match};
use nsh_core::ui::prelude::*;

pub fn bar(
    search: TextSearch,
    query_ref: &ElementRef,
    on_edit: impl Fn(String) + 'static,
    on_options: impl Fn() + Copy + 'static,
    on_prev: impl Fn() + Copy + 'static,
    on_next: impl Fn() + Copy + 'static,
    trailing: impl IntoElement,
) -> impl IntoElement {
    view()
        .flex_none()
        .flex_col()
        .mx(px(20.0))
        .mb(px(8.0))
        .px(px(8.0))
        .py(px(6.0))
        .gap(px(6.0))
        .rounded(px(12.0))
        .bg(move || theme().group_box)
        .border_1()
        .border_color(move || theme().border)
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(px(6.0))
                .child(chrome::section("Find"))
                .child(query_field(search, query_ref, on_edit))
                .child(flag("Aa", search.case_sensitive, "find-case", on_options))
                .child(flag(".*", search.regex, "find-regex", on_options))
                .child(step_button("↑", "find-prev", on_prev))
                .child(step_button("↓", "find-next", on_next))
                .child(
                    text(move || search.label.get())
                        .text_xs()
                        .text_color(move || {
                            if search.error.with(|error| !error.is_empty()) {
                                theme().danger
                            } else {
                                theme().muted_foreground
                            }
                        }),
                )
                .child(view().flex_1())
                .child(trailing),
        )
        .child(snippet_list(search))
}

fn query_field(
    search: TextSearch,
    query_ref: &ElementRef,
    on_edit: impl Fn(String) + 'static,
) -> impl IntoElement {
    input()
        .value(search.query)
        .placeholder("Find in the full text")
        .placeholder_color(move || color(theme().muted_foreground))
        .font_family(chrome::MONO)
        .text_size(px(13.0))
        .text_color(move || theme().foreground)
        .auto_correct(false)
        .auto_capitalize(AutoCapitalize::None)
        .blur_on_submit(false)
        .element_ref(query_ref)
        .test_id("find-query")
        .on_change_text(move |text: &str| on_edit(text.to_string()))
        .flex_1()
        .min_w(px(120.0))
        .h(px(28.0))
        .px(px(8.0))
        .rounded(px(8.0))
        .bg(move || theme().background)
        .border_1()
        .border_color(move || theme().border)
}

fn flag(
    label: &'static str,
    on: Signal<bool>,
    test_id: &'static str,
    changed: impl Fn() + Copy + 'static,
) -> impl IntoElement {
    view()
        .flex_none()
        .h(px(26.0))
        .px(px(8.0))
        .items_center()
        .justify_center()
        .rounded(px(7.0))
        .bg(move || {
            if on.get() {
                theme().muted
            } else {
                AdaptiveColor::TRANSPARENT
            }
        })
        .role(AccessibilityRole::Button)
        .test_id(test_id)
        .on_press(move |_: &()| {
            on.set(!on.get());
            changed();
        })
        .child(
            text(label)
                .text_xs()
                .font_family(chrome::MONO)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(move || {
                    if on.get() {
                        theme().foreground
                    } else {
                        theme().muted_foreground
                    }
                }),
        )
}

fn step_button(
    label: &'static str,
    test_id: &'static str,
    on_press: impl Fn() + Copy + 'static,
) -> impl IntoElement {
    view()
        .flex_none()
        .w(px(26.0))
        .h(px(26.0))
        .items_center()
        .justify_center()
        .rounded(px(7.0))
        .hover(|style| style.bg(move || theme().muted))
        .role(AccessibilityRole::Button)
        .test_id(test_id)
        .on_press(move |_: &()| on_press())
        .child(
            text(label)
                .text_sm()
                .text_color(move || theme().foreground),
        )
}

/// Focus the query field on the frame after it is mounted. `set_selection`
/// waits one frame too: the field's value and the selection command have to
/// land together, or the platform applies the range to the previous text.
pub fn watch_focus(search: TextSearch, query_ref: ElementRef) {
    effect(move || {
        let generation = search.focus.get();
        if generation == 0 {
            return;
        }
        let query_ref = query_ref.clone();
        spawn(async move {
            next_frame().await;
            query_ref.focus();
        });
    });
}

pub fn select_later(editor: ElementRef, start: u32, end: u32) {
    spawn(async move {
        next_frame().await;
        editor.set_selection(start, end);
    });
}

/// Plain ink for a run that is not the current match. Text runs cannot
/// carry a background, so a hit is a foreground color and a weight.
#[derive(Clone, Copy)]
pub enum Ink {
    Foreground,
    Muted,
    Success,
    Primary,
    Info,
}

pub fn ink_color(ink: Ink) -> AdaptiveColor {
    match ink {
        Ink::Foreground => theme().foreground,
        Ink::Muted => theme().muted_foreground,
        Ink::Success => theme().success,
        Ink::Primary => theme().primary,
        Ink::Info => theme().info,
    }
}

/// One line of a match. The current hit is semibold in the primary color.
/// Other hits on that line use the info color. NSH has no background on a
/// text run, so the row behind the current line carries the tint.
pub fn piece_line(pieces: Vec<SnippetPiece>, plain: Ink) -> Text {
    let mut line = text("")
        .font_family(chrome::MONO)
        .text_size(px(12.5))
        .number_of_lines(1);
    for piece in pieces {
        let kind = piece.kind;
        let mut span = text(piece.text)
            .font_family(chrome::MONO)
            .text_size(px(12.5))
            .text_color(move || match kind {
                PieceKind::Current => theme().primary,
                PieceKind::Hit => theme().info,
                PieceKind::Plain => ink_color(plain),
            });
        if kind == PieceKind::Current {
            span = span.font_weight(FontWeight::SEMIBOLD);
        }
        line = line.child(span);
    }
    line
}

fn snippet_list(search: TextSearch) -> impl IntoElement {
    dynamic(move || {
        let rows = search.snippets.get();
        if rows.is_empty() {
            return view().into_element();
        }
        let current = search.index.get();
        let mut list = view().flex_col().gap(px(2.0)).test_id("find-snippets");
        for snippet in rows {
            let index = snippet.hit_index;
            let active = index == current;
            let line_no = snippet.line;
            let pieces = snippet.pieces;
            list = list.child(
                view()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(6.0))
                    .py(px(3.0))
                    .rounded(px(8.0))
                    .bg(move || {
                        if active {
                            theme().list_active
                        } else {
                            AdaptiveColor::TRANSPARENT
                        }
                    })
                    .hover(|style| style.bg(move || theme().list_head))
                    .role(AccessibilityRole::Button)
                    .label(format!("Match on line {line_no}"))
                    .on_press(move |_: &()| {
                        search.index.set(index);
                        search.publish_label();
                    })
                    .child(
                        text(format!("{line_no}"))
                            .flex_none()
                            .w(px(40.0))
                            .font_family(chrome::MONO)
                            .text_xs()
                            .text_color(move || theme().muted_foreground),
                    )
                    .child(piece_line(pieces, Ink::Foreground).flex_1().min_w(px(0.0))),
            );
        }
        list.into_element()
    })
}

/// Replace `field` so the current match is the first line, and publish the
/// styled rows. Used for documents that are not already split into a preview
/// by the JSON or Hash tools (URL, Timestamp, Markdown).
pub fn watch_windowed_field(
    search: TextSearch,
    source: Signal<String>,
    field: Signal<String>,
    origin: Signal<usize>,
    line: Signal<u32>,
    capped: Signal<bool>,
    editor: ElementRef,
) {
    effect(move || {
        let _ = search.revision.get();
        let _ = search.index.get();
        untracked(|| {
            if !search.open.get() {
                return;
            }
            let Some(hit) = search.current() else {
                search.snippets.set(Vec::new());
                let shown = source.with(|text| json_model::field_text(text));
                origin.set(0);
                line.set(0);
                capped.set(shown.capped);
                field.set(shown.text);
                return;
            };
            let full = source.get();
            let revealed = reveal_match(&full, hit.start, hit.end);
            origin.set(revealed.preview.byte_start);
            line.set(revealed.line);
            capped.set(revealed.preview.capped);
            field.set(revealed.preview.text);
            search.publish_snippets(&full);
            select_later(editor.clone(), revealed.utf16_start, revealed.utf16_end);
        });
    });
}

