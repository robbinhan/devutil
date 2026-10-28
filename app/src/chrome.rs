//! Shared workbench chrome.
//!
//! NSH 0.1.1 has no SF Symbols and no icon font, so navigation marks are
//! short glyphs in the system font. `ui-monospace` is SF Mono on macOS
//! (the system monospaced face) and the platform monospace face on Linux.

use crate::json_model::{self, FIELD_LIMIT};
use nsh_core::ui::prelude::*;

/// Platform monospace. On macOS, NSH maps this to the system monospaced font.
pub const MONO: &str = "ui-monospace";

pub fn action_button(
    label: impl IntoReactive<SharedString>,
    test_id: &str,
    primary: bool,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    view()
        .flex_none()
        .h(px(32.0))
        .px(px(12.0))
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .bg(move || {
            if primary {
                theme().primary
            } else {
                AdaptiveColor::TRANSPARENT
            }
        })
        .hover(|style| {
            style.bg(move || {
                if primary {
                    theme().primary_active
                } else {
                    theme().muted
                }
            })
        })
        .active(|style| style.opacity(0.85))
        .role(AccessibilityRole::Button)
        .test_id(test_id)
        .on_press(move |_: &()| on_press())
        .child(
            text(label)
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(move || {
                    if primary {
                        theme().primary_foreground
                    } else {
                        theme().foreground
                    }
                }),
        )
}

pub fn segment<T>(
    label: &'static str,
    current: Signal<T>,
    value: T,
    on_pick: impl Fn(T) + 'static,
) -> impl IntoElement
where
    T: Copy + PartialEq + 'static,
{
    let selected = value;
    view()
        .h(px(26.0))
        .px(px(10.0))
        .items_center()
        .justify_center()
        .rounded(px(7.0))
        .bg(move || {
            if current.get() == selected {
                theme().group_box
            } else {
                AdaptiveColor::TRANSPARENT
            }
        })
        .role(AccessibilityRole::Button)
        .on_press(move |_: &()| on_pick(value))
        .child(
            text(label)
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(move || {
                    if current.get() == selected {
                        theme().foreground
                    } else {
                        theme().muted_foreground
                    }
                }),
        )
}

pub fn segment_row() -> View {
    view()
        .flex_none()
        .flex_row()
        .items_center()
        .gap(px(2.0))
        .p(px(2.0))
        .rounded(px(9.0))
        .bg(move || theme().muted)
}

/// Small semibold section label. Sentence case; not a banner.
pub fn section(label: &'static str) -> impl IntoElement {
    text(label)
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(move || theme().muted_foreground)
}

pub fn title(label: &'static str) -> impl IntoElement {
    text(label)
        .text_lg()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(move || theme().foreground)
}

pub fn hint(message: impl Into<SharedString>) -> impl IntoElement {
    let message = message.into();
    text(message)
        .text_xs()
        .line_height(px(16.0))
        .text_color(move || theme().muted_foreground)
}

pub fn card() -> View {
    view()
        .p(px(12.0))
        .gap(px(8.0))
        .rounded(px(12.0))
        .bg(move || theme().group_box)
        .border_1()
        .border_color(move || theme().border)
        .shadow_xs()
}

pub fn badge(label: &'static str, tint: AdaptiveColor) -> impl IntoElement {
    let text_tint = tint;
    view()
        .flex_none()
        .h(px(22.0))
        .px(px(8.0))
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(tint.opacity(0.14))
        .child(
            text(label)
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(text_tint),
        )
}

pub fn chip(label: &'static str) -> impl IntoElement {
    view()
        .flex_none()
        .h(px(22.0))
        .px(px(8.0))
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(move || theme().muted)
        .child(
            text(label)
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(move || theme().foreground),
        )
}

pub fn empty_state(
    glyph: &'static str,
    heading: &'static str,
    message: impl Into<SharedString>,
    action: &'static str,
    test_id: &'static str,
    on_press: impl Fn() + 'static,
) -> View {
    let message = message.into();
    view()
        .w_full()
        .min_h(px(220.0))
        .items_center()
        .justify_center()
        .gap(px(8.0))
        .p(px(20.0))
        .child(
            view()
                .w(px(44.0))
                .h(px(44.0))
                .items_center()
                .justify_center()
                .rounded(px(14.0))
                .bg(move || theme().muted)
                .child(
                    text(glyph)
                        .text_lg()
                        .text_color(move || theme().primary),
                ),
        )
        .child(
            text(heading)
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(move || theme().foreground),
        )
        .child(
            text(message)
                .text_xs()
                .text_color(move || theme().muted_foreground),
        )
        .child(action_button(action, test_id, true, on_press))
}

pub fn state_card(
    heading: &'static str,
    message: &'static str,
    tint: AdaptiveColor,
) -> impl IntoElement {
    card()
        .child(
            text(heading)
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tint),
        )
        .child(hint(message))
}

/// Quiet copy control, the same height as the other toolbar buttons.
pub fn copy_button(
    id: impl Into<String>,
    copied: Signal<Option<String>>,
    text_of: impl Fn() -> String + 'static,
) -> impl IntoElement {
    let id = id.into();
    let shown_text = id.clone();
    let shown_color = id.clone();
    let pressed = id.clone();
    view()
        .flex_none()
        .h(px(32.0))
        .px(px(10.0))
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .hover(|style| style.bg(move || theme().muted))
        .role(AccessibilityRole::Button)
        .label(format!("Copy {id}"))
        .on_press(move |_: &()| {
            let value = text_of();
            if value.is_empty() {
                return;
            }
            set_clipboard(value);
            copied.set(Some(pressed.clone()));
        })
        .child(
            text(move || {
                if copied.get().as_deref() == Some(shown_text.as_str()) {
                    "Copied"
                } else {
                    "Copy"
                }
            })
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(move || {
                if copied.get().as_deref() == Some(shown_color.as_str()) {
                    theme().success
                } else {
                    theme().muted_foreground
                }
            }),
        )
}

/// Smaller copy mark for dense rows. NSH 0.1.1 cannot reveal a child only
/// while the pointer is over the row, so the mark stays visible and muted.
pub fn copy_icon(
    id: impl Into<String>,
    copied: Signal<Option<String>>,
    text_of: impl Fn() -> String + 'static,
) -> impl IntoElement {
    let id = id.into();
    let shown_text = id.clone();
    let shown_color = id.clone();
    let pressed = id.clone();
    view()
        .flex_none()
        .w(px(28.0))
        .h(px(28.0))
        .items_center()
        .justify_center()
        .rounded(px(7.0))
        .hover(|style| style.bg(move || theme().muted))
        .role(AccessibilityRole::Button)
        .label(format!("Copy {id}"))
        .on_press(move |_: &()| {
            let value = text_of();
            if value.is_empty() {
                return;
            }
            set_clipboard(value);
            copied.set(Some(pressed.clone()));
        })
        .child(
            text(move || {
                if copied.get().as_deref() == Some(shown_text.as_str()) {
                    "✓"
                } else {
                    "⧉"
                }
            })
            .text_sm()
            .text_color(move || {
                if copied.get().as_deref() == Some(shown_color.as_str()) {
                    theme().success
                } else {
                    theme().muted_foreground
                }
            }),
        )
}

pub fn nav_item<T>(
    glyph: &'static str,
    label: &'static str,
    current: Signal<T>,
    value: T,
    on_pick: impl Fn(T) + 'static,
) -> impl IntoElement
where
    T: Copy + PartialEq + 'static,
{
    let selected = value;
    view()
        .flex_row()
        .items_center()
        .gap(px(10.0))
        .h(px(36.0))
        .px(px(8.0))
        .rounded(px(8.0))
        .bg(move || {
            if current.get() == selected {
                theme().primary.opacity(0.12)
            } else {
                AdaptiveColor::TRANSPARENT
            }
        })
        .hover(|style| style.bg(move || theme().muted.opacity(0.7)))
        .role(AccessibilityRole::Button)
        .on_press(move |_: &()| on_pick(value))
        .child(
            view()
                .w(px(22.0))
                .items_center()
                .child(
                    text(glyph)
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(move || {
                            if current.get() == selected {
                                theme().primary
                            } else {
                                theme().muted_foreground
                            }
                        }),
                ),
        )
        .child(
            text(label)
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(move || {
                    if current.get() == selected {
                        theme().primary
                    } else {
                        theme().foreground
                    }
                }),
        )
}

/// Monospace field. Multiline uses AppKit's `NSTextView` (a scroll view);
/// a single line uses `NSTextField`. `editable == false` stays selectable.
///
/// The view is capped at [`FIELD_LIMIT`] UTF-16 units. Callers that need
/// the rest of a paste keep it outside this value.
pub fn mono_field(
    value: impl IntoReactive<String>,
    placeholder: &'static str,
    multiline: bool,
    editable: bool,
    on_change: impl Fn(String) + 'static,
) -> Input {
    let mut field = input()
        .font_family(MONO)
        .text_size(px(13.0))
        .line_height(px(20.0))
        .text_color(move || theme().foreground)
        .placeholder(placeholder)
        .placeholder_color(move || color(theme().muted_foreground))
        .selection_color(move || color(theme().primary))
        .auto_correct(false)
        .auto_capitalize(AutoCapitalize::None)
        .multiline(multiline)
        .editable(editable)
        .value(value)
        .p(px(12.0))
        .rounded(px(12.0))
        .bg(move || theme().group_box)
        .border_1()
        .border_color(move || theme().border);
    if multiline {
        field = field.max_length(FIELD_LIMIT as u32);
    }
    if editable {
        field = field.on_change_text(move |text: &str| on_change(text.to_string()));
    } else {
        drop(on_change);
    }
    field
}

/// Bytes, and for a modest string, characters and lines. A large document
/// is not scanned: counting every scalar on each frame would walk megabytes.
pub fn meta_line(text: &str) -> String {
    let bytes = text.len();
    if bytes >= 100_000 {
        return format!("{} · full text", json_model::format_bytes(bytes));
    }
    if bytes == 0 {
        return "Empty".to_string();
    }
    let lines = text.lines().count().max(1);
    let chars = text.chars().count();
    format!(
        "{chars} chars · {lines} {} · {bytes} bytes",
        if lines == 1 { "line" } else { "lines" }
    )
}

pub fn column_pane() -> View {
    view()
        .flex_1()
        .h_full()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .gap(px(12.0))
}

pub fn workbench() -> View {
    view()
        .flex_1()
        .min_h(px(0.0))
        .flex_row()
        .items_stretch()
        .gap(px(16.0))
        .px(px(20.0))
        .pb(px(20.0))
}
