//! The find bar. One compact row, the same card chrome as the rest of the
//! workbench: group box, hairline border, 12 px corners.

use crate::chrome;
use crate::find::{self, TextSearch};
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
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(6.0))
        .mx(px(20.0))
        .mb(px(8.0))
        .px(px(8.0))
        .py(px(6.0))
        .rounded(px(12.0))
        .bg(move || theme().group_box)
        .border_1()
        .border_color(move || theme().border)
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
        .child(trailing)
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

/// Select `hit` in a field whose value is the whole `text`.
pub fn watch_plain_field(search: TextSearch, text: Signal<String>, editor: ElementRef) {
    effect(move || {
        let _ = search.revision.get();
        let _ = search.index.get();
        untracked(|| {
            if !search.open.get() {
                return;
            }
            let Some(hit) = search.current() else {
                return;
            };
            let (start, end) = text.with(|text| find::utf16_span(text, hit.start, hit.end));
            select_later(editor.clone(), start, end);
        });
    });
}
