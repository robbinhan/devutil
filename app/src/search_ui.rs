//! The find bar. One compact row, the same card chrome as the rest of the
//! workbench: group box, hairline border, 12 px corners.

use crate::chrome;
use crate::find::{PieceKind, SnippetPiece, TextSearch};
use nsh_base::theme;
use nsh_base::{input, Editor, EditorApi, EditorPosition, EditorSearch, EditorSelection};
use nsh_core::prelude::*;

/// Handle of a workbench [`Editor`].
pub type EditorHandle = ComponentRef<Editor>;

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
        .bg(move || theme().card)
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
                                crate::palette::danger()
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
        .submit_behavior(SubmitBehavior::Submit)
        .element_ref(query_ref)
        .test_id("find-query")
        .on_change(move |text: &str| on_edit(text.to_string()))
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

/// The find query, when find is open and the query is not empty. The
/// editor paints every match and scrolls the one the selection covers.
pub fn editor_query(search: TextSearch) -> Option<EditorSearch> {
    if !search.open.get() {
        return None;
    }
    let query = search.query.get();
    if query.is_empty() {
        return None;
    }
    let mut found = EditorSearch::new(query);
    found.case_sensitive = search.case_sensitive.get();
    found.regex = search.regex.get();
    Some(found)
}

/// Select `start..end` (byte offsets into `text`) and scroll that range
/// into view. The find field keeps the keyboard focus: this does not
/// focus the editor unless `focus` is set.
pub fn reveal_in_editor(editor: EditorHandle, text: &str, start: usize, end: usize, focus: bool) {
    let anchor = editor_point(text, start);
    let head = editor_point(text, end);
    spawn(async move {
        next_frame().await;
        let Some(api) = editor.api::<EditorApi>() else {
            return;
        };
        api.set_selections(vec![EditorSelection::new(anchor, head)]);
        if focus {
            api.focus();
        }
    });
}

fn editor_point(text: &str, byte: usize) -> EditorPosition {
    let point = crate::find::point_at(text, byte);
    EditorPosition::new(point.line, point.column)
}

/// Plain ink for a run that is not the current match.
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
        Ink::Success => crate::palette::success(),
        Ink::Primary => theme().primary,
        Ink::Info => crate::palette::info(),
    }
}

/// One line of a match. The current hit is semibold on a primary tint.
/// Other hits on that line use the info color and a lighter tint. The row
/// behind the current line is tinted as well.
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
                PieceKind::Hit => crate::palette::info(),
                PieceKind::Plain => ink_color(plain),
            });
        if kind == PieceKind::Current {
            span = span
                .font_weight(FontWeight::SEMIBOLD)
                .rounded(px(2.0))
                .bg(move || theme().primary.opacity(0.22));
        } else if kind == PieceKind::Hit {
            span = span.rounded(px(2.0)).bg(move || crate::palette::info().opacity(0.18));
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
                            crate::palette::list_active()
                        } else {
                            AdaptiveColor::TRANSPARENT
                        }
                    })
                    .hover(|style| style.bg(move || crate::palette::list_head()))
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

/// Publish the rows under the find bar and scroll the current match into
/// view inside `editor`. The editor keeps the whole document.
pub fn watch_editor(search: TextSearch, source: Signal<String>, editor: EditorHandle) {
    effect(move || {
        let _ = search.revision.get();
        let _ = search.index.get();
        untracked(|| {
            if !search.open.get() {
                search.snippets.set(Vec::new());
                return;
            }
            let Some(hit) = search.current() else {
                search.snippets.set(Vec::new());
                return;
            };
            let full = source.get();
            search.publish_snippets(&full);
            reveal_in_editor(editor, &full, hit.start, hit.end, false);
        });
    });
}

