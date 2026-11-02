//! Text compare. Two editors (Original, Changed) feed `diff_core`;
//! the result is a virtual list in a side-by-side or unified layout.
//!
//! A changed line is a tinted row, a `+` or `-` marker, and a colored run.
//! Words that changed inside a line get their own background.

use std::time::Duration;

use diff_core::{Cell, Comparison, Layout, Options, SideRow, Span, Tag, UnifiedRow};

use crate::chrome::{self, column_pane};
use crate::diff_model::{self, INLINE_BYTES, MAX_BYTES, ROW_CAP, plural};
use crate::json_model;
use nsh_base::theme;
use nsh_core::prelude::*;

const DEBOUNCE: Duration = Duration::from_millis(250);
const ROW_HEIGHT: f32 = 22.0;
const NUMBER_WIDTH: f32 = 40.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Split,
    Unified,
}

/// One editor: the full text, the prefix the text view shows, and the
/// file it came from (for the patch header).
#[derive(Clone, Copy)]
pub struct Side {
    pub source: Signal<String>,
    pub field: Signal<String>,
    pub field_capped: Signal<bool>,
    pub name: Signal<String>,
}

impl Side {
    fn new() -> Self {
        Self {
            source: signal(String::new()),
            field: signal(String::new()),
            field_capped: signal(false),
            name: signal(String::new()),
        }
    }

    fn publish(self, text: String) {
        let text = json_model::normalize_newlines(text);
        self.field_capped.set(false);
        self.field.set(text.clone());
        self.source.set(text);
    }

    /// A keystroke in the editor. The editor holds the whole side.
    fn write(self, text: String) -> bool {
        if self.source.with(|full| full == &text) {
            return false;
        }
        self.publish(text);
        true
    }
}

#[derive(Clone, Copy)]
pub struct DiffState {
    pub left: Side,
    pub right: Side,
    pub view: Signal<ViewMode>,
    pub changes_only: Signal<bool>,
    pub ignore_whitespace: Signal<bool>,
    pub ignore_case: Signal<bool>,
    pub format_json: Signal<bool>,
    pub result: Signal<Option<Comparison>>,
    /// Both sides were JSON and were compared formatted, with sorted keys.
    pub json_applied: Signal<bool>,
    pub busy: Signal<bool>,
    pub generation: Signal<u64>,
    /// Bumped each time a comparison is stored.
    pub result_generation: Signal<u64>,
    pub error: Signal<String>,
}

impl DiffState {
    pub fn new() -> Self {
        Self {
            left: Side::new(),
            right: Side::new(),
            view: signal(ViewMode::Split),
            changes_only: signal(false),
            ignore_whitespace: signal(false),
            ignore_case: signal(false),
            format_json: signal(false),
            result: signal(None),
            json_applied: signal(false),
            busy: signal(false),
            generation: signal(0),
            result_generation: signal(0),
            error: signal(String::new()),
        }
    }

    fn options(self) -> Options {
        Options {
            ignore_whitespace: self.ignore_whitespace.get(),
            ignore_case: self.ignore_case.get(),
        }
    }

    fn layout(self) -> Layout {
        Layout {
            context: if self.changes_only.get() {
                Some(diff_core::DEFAULT_CONTEXT)
            } else {
                None
            },
            row_cap: ROW_CAP,
        }
    }

    fn side(self, right: bool) -> Side {
        if right { self.right } else { self.left }
    }

    fn on_field(self, right: bool, text: String) {
        let side = self.side(right);
        if side.write(text) {
            self.schedule();
        }
    }

    /// Debounced compare of whatever both sides hold when the timer fires.
    pub fn schedule(self) {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        set_timeout(DEBOUNCE, move || self.run_generation(generation));
    }

    /// ⌘↩ and the Compare button: no debounce.
    pub fn compare_now(self) {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        self.run_generation(generation);
    }

    fn run_generation(self, generation: u64) {
        if self.generation.get() != generation {
            return;
        }
        let old = self.left.source.get();
        let new = self.right.source.get();
        if old.is_empty() && new.is_empty() {
            self.result.set(None);
            self.json_applied.set(false);
            self.error.set(String::new());
            self.busy.set(false);
            return;
        }
        if old.len().saturating_add(new.len()) > MAX_BYTES {
            self.result.set(None);
            self.busy.set(false);
            self.error.set(format!(
                "These texts are {} together. Diff compares up to {}.",
                json_model::format_bytes(old.len() + new.len()),
                json_model::format_bytes(MAX_BYTES)
            ));
            return;
        }
        let options = self.options();
        let layout = self.layout();
        let format_json = self.format_json.get();
        let work = move || {
            let (old, new, applied) = diff_model::prepare(old, new, format_json);
            (diff_core::compare(&old, &new, options, layout), applied)
        };
        if self.left.source.with(String::len) + self.right.source.with(String::len) <= INLINE_BYTES
        {
            let (comparison, applied) = work();
            self.store(generation, comparison, applied);
            return;
        }
        self.busy.set(true);
        spawn(async move {
            let outcome = blocking(work).await;
            if self.generation.get() != generation {
                return;
            }
            match outcome {
                Ok((comparison, applied)) => self.store(generation, comparison, applied),
                Err(err) => {
                    self.busy.set(false);
                    self.error.set(err.to_string());
                }
            }
        });
    }

    fn store(self, generation: u64, comparison: Comparison, applied: bool) {
        if self.generation.get() != generation {
            return;
        }
        self.error.set(String::new());
        self.json_applied.set(applied);
        self.result_generation
            .update(|value| *value = value.wrapping_add(1));
        self.result.set(Some(comparison));
        self.busy.set(false);
    }

    /// A view option changed: compare again right away.
    fn option_changed(self) {
        self.compare_now();
    }

    pub fn swap(self) {
        let (left, right) = (self.left, self.right);
        let old = left.source.get();
        let new = right.source.get();
        let old_name = left.name.get();
        let new_name = right.name.get();
        left.publish(new);
        right.publish(old);
        left.name.set(new_name);
        right.name.set(old_name);
        self.compare_now();
    }

    pub fn clear(self) {
        for side in [self.left, self.right] {
            side.name.set(String::new());
            side.publish(String::new());
        }
        self.compare_now();
    }

    pub fn load_sample(self) {
        self.left.name.set(String::new());
        self.right.name.set(String::new());
        self.left.publish(diff_model::SAMPLE_ORIGINAL.to_string());
        self.right.publish(diff_model::SAMPLE_CHANGED.to_string());
        self.compare_now();
    }

    pub fn paste(self, right: bool) {
        spawn(async move {
            let text = clipboard_string().await;
            if text.is_empty() {
                return;
            }
            let side = self.side(right);
            side.name.set(String::new());
            side.publish(text);
            self.compare_now();
        });
    }

    pub fn pick_file(self, right: bool) {
        spawn(async move {
            let picked = blocking(move || {
                rfd::FileDialog::new()
                    .set_title(if right {
                        "Open Changed"
                    } else {
                        "Open Original"
                    })
                    .pick_file()
            })
            .await;
            let Ok(Some(path)) = picked else {
                return;
            };
            let path = path.to_string_lossy().into_owned();
            let read_path = path.clone();
            let read = blocking(move || std::fs::read(&read_path)).await;
            let side = self.side(right);
            match read {
                Ok(Ok(bytes)) => {
                    if bytes.len() > MAX_BYTES {
                        self.error.set(format!(
                            "{} is {}. Diff opens files up to {}.",
                            diff_model::patch_name("", &path, "file").trim_start_matches('/'),
                            json_model::format_bytes(bytes.len()),
                            json_model::format_bytes(MAX_BYTES)
                        ));
                        return;
                    }
                    match String::from_utf8(bytes) {
                        Ok(text) => {
                            side.name.set(path);
                            side.publish(text);
                            self.compare_now();
                        }
                        Err(_) => self.error.set(
                            "That file is not UTF-8 text, so it cannot be compared.".to_string(),
                        ),
                    }
                }
                Ok(Err(err)) => self.error.set(err.to_string()),
                Err(err) => self.error.set(err.to_string()),
            }
        });
    }

    /// The full unified diff (not capped), the same normalization and
    /// ignore options as the view.
    pub fn copy_patch(self, copied: Signal<Option<String>>) {
        let old = self.left.source.get();
        let new = self.right.source.get();
        if old.is_empty() && new.is_empty() {
            return;
        }
        let options = self.options();
        let format_json = self.format_json.get();
        let old_name = self
            .left
            .name
            .with(|name| diff_model::patch_name("a", name, "original"));
        let new_name = self
            .right
            .name
            .with(|name| diff_model::patch_name("b", name, "changed"));
        let work = move || {
            let (old, new, _) = diff_model::prepare(old, new, format_json);
            diff_core::unified_patch(
                &old,
                &new,
                options,
                &old_name,
                &new_name,
                diff_core::DEFAULT_CONTEXT,
            )
        };
        let done = move |patch: String| {
            if patch.is_empty() {
                copied.set(Some("diff-none".to_string()));
                return;
            }
            set_clipboard(patch);
            copied.set(Some("diff-patch".to_string()));
        };
        if self.left.source.with(String::len) + self.right.source.with(String::len) <= INLINE_BYTES
        {
            done(work());
            return;
        }
        self.busy.set(true);
        spawn(async move {
            let patch = blocking(work).await;
            self.busy.set(false);
            match patch {
                Ok(patch) => done(patch),
                Err(err) => self.error.set(err.to_string()),
            }
        });
    }
}

pub fn body(
    state: DiffState,
    copied: Signal<Option<String>>,
    shortcut: String,
) -> impl IntoElement {
    view()
        .flex_1()
        .min_h(px(0.0))
        .flex_col()
        .child(page_header(state, copied, shortcut))
        .child(option_row(state))
        .child(
            view()
                .flex_none()
                .h(px(220.0))
                .flex_row()
                .items_stretch()
                .gap(px(16.0))
                .px(px(20.0))
                .pb(px(12.0))
                .child(editor_pane(state, false))
                .child(editor_pane(state, true)),
        )
        .child(
            view()
                .flex_1()
                .min_h(px(0.0))
                .px(px(20.0))
                .pb(px(20.0))
                .child(result_card(state)),
        )
}

fn page_header(
    state: DiffState,
    copied: Signal<Option<String>>,
    shortcut: String,
) -> impl IntoElement {
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
                .child(chrome::title("Diff"))
                .child(chrome::hint("Compare two texts line by line")),
        )
        .child(view().flex_1())
        .child(chrome::action_button(
            "Swap",
            "diff-swap",
            false,
            move || state.swap(),
        ))
        .child(chrome::action_button(
            move || match copied.get().as_deref() {
                Some("diff-patch") => "Copied".to_string(),
                Some("diff-none") => "No changes".to_string(),
                _ => "Copy diff".to_string(),
            },
            "diff-copy-patch",
            false,
            move || state.copy_patch(copied),
        ))
        .child(chrome::action_button(
            "Clear",
            "diff-clear",
            false,
            move || state.clear(),
        ))
        .child(
            text(shortcut)
                .text_xs()
                .text_color(move || theme().muted_foreground),
        )
        .child(chrome::action_button(
            "Compare",
            "diff-run",
            true,
            move || state.compare_now(),
        ))
}

fn option_row(state: DiffState) -> impl IntoElement {
    view()
        .flex_none()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .px(px(20.0))
        .pt(px(8.0))
        .pb(px(12.0))
        .child(chrome::hint("View"))
        .child(
            chrome::segment_row()
                .child(chrome::segment(
                    "Side by side",
                    state.view,
                    ViewMode::Split,
                    move |mode| state.view.set(mode),
                ))
                .child(chrome::segment(
                    "Unified",
                    state.view,
                    ViewMode::Unified,
                    move |mode| state.view.set(mode),
                )),
        )
        .child(chrome::hint("Lines"))
        .child(
            chrome::segment_row()
                .child(chrome::segment(
                    "All",
                    state.changes_only,
                    false,
                    move |on| {
                        state.changes_only.set(on);
                        state.option_changed();
                    },
                ))
                .child(chrome::segment(
                    "Changes",
                    state.changes_only,
                    true,
                    move |on| {
                        state.changes_only.set(on);
                        state.option_changed();
                    },
                )),
        )
        .child(option_flag(
            "Ignore whitespace",
            state.ignore_whitespace,
            "diff-ignore-ws",
            state,
        ))
        .child(option_flag(
            "Ignore case",
            state.ignore_case,
            "diff-ignore-case",
            state,
        ))
        .child(option_flag(
            "Format JSON",
            state.format_json,
            "diff-format-json",
            state,
        ))
}

/// A two-state segment pair for an on/off option, the same control as
/// the Hash tool's HMAC switch.
fn option_flag(
    label: &'static str,
    on: Signal<bool>,
    test_id: &'static str,
    state: DiffState,
) -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .test_id(test_id)
        .child(chrome::hint(label))
        .child(
            chrome::segment_row()
                .child(chrome::segment("Off", on, false, move |value| {
                    on.set(value);
                    state.option_changed();
                }))
                .child(chrome::segment("On", on, true, move |value| {
                    on.set(value);
                    state.option_changed();
                })),
        )
}

fn editor_pane(state: DiffState, right: bool) -> impl IntoElement {
    let side = state.side(right);
    let (label, placeholder, _id) = if right {
        ("Changed", "Paste the changed text", "diff-changed")
    } else {
        ("Original", "Paste the original text", "diff-original")
    };
    column_pane()
        .gap(px(8.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(6.0))
                .child(chrome::section(label))
                .child(
                    text(move || {
                        side.name.with(|name| {
                            if name.is_empty() {
                                String::new()
                            } else {
                                diff_model::patch_name("", name, "")
                                    .trim_start_matches('/')
                                    .to_string()
                            }
                        })
                    })
                    .flex_1()
                    .min_w(px(0.0))
                    .number_of_lines(1)
                    .ellipsize_mode(nsh_core::EllipsizeMode::Middle)
                    .font_family(chrome::MONO)
                    .text_xs()
                    .text_color(move || theme().foreground),
                )
                .child(
                    text(move || side.source.with(|text| chrome::meta_line(text)))
                        .text_xs()
                        .text_color(move || theme().muted_foreground),
                )
                .child(chrome::action_button(
                    "Paste",
                    if right {
                        "diff-paste-changed"
                    } else {
                        "diff-paste-original"
                    },
                    false,
                    move || state.paste(right),
                ))
                .child(chrome::action_button(
                    "Open",
                    if right {
                        "diff-open-changed"
                    } else {
                        "diff-open-original"
                    },
                    false,
                    move || state.pick_file(right),
                )),
        )
        .child(dynamic(move || {
            if !side.field_capped.get() {
                return view().into_element();
            }
            let message = side.source.with(|full| {
                format!(
                    "Showing the first {} of {}. The compare and Copy diff use the full text.",
                    side.field
                        .with(|preview| json_model::format_bytes(preview.len())),
                    json_model::format_bytes(full.len())
                )
            });
            text(message)
                .text_xs()
                .text_color(move || crate::palette::warning())
                .into_element()
        }))
        .child(
            chrome::mono_editor(
                side.source,
                placeholder,
                true,
                || None,
                move |text| state.on_field(right, text),
            )
            .flex_1()
            .min_h(px(100.0)),
        )
}

fn result_card(state: DiffState) -> impl IntoElement {
    chrome::card()
        .h_full()
        .min_h(px(0.0))
        .gap(px(8.0))
        .child(summary_row(state))
        .child(notice_line(state))
        .child(result_body(state))
}

fn summary_row(state: DiffState) -> impl IntoElement {
    view()
        .flex_none()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .child(chrome::section("Result"))
        .child(dynamic(move || {
            let counts = state.result.with(|result| {
                result
                    .as_ref()
                    .map(|result| (result.summary.clone(), result.exact))
            });
            let Some((summary, exact)) = counts else {
                return view().into_element();
            };
            if summary.identical() {
                let label = if exact { "Identical" } else { "Equivalent" };
                return count_pill(label.to_string(), crate::palette::success()).into_element();
            }
            view()
                .flex_row()
                .items_center()
                .gap(px(6.0))
                .child(count_pill(format!("+{}", summary.added), crate::palette::success()))
                .child(count_pill(format!("−{}", summary.removed), crate::palette::danger()))
                .child(
                    text(format!(
                        "{} · {} → {}",
                        plural(summary.changes, "change", "changes"),
                        plural(summary.old_lines, "line", "lines"),
                        plural(summary.new_lines, "line", "lines"),
                    ))
                    .text_xs()
                    .text_color(move || theme().muted_foreground),
                )
                .into_element()
        }))
        .child(view().flex_1())
        .child(
            text(move || {
                if state.busy.get() {
                    "Comparing…".to_string()
                } else if state.json_applied.get() {
                    "Compared as formatted JSON, keys sorted".to_string()
                } else if state.format_json.get() && state.result.with(|result| result.is_some()) {
                    "Not JSON on both sides; compared as text".to_string()
                } else {
                    String::new()
                }
            })
            .text_xs()
            .text_color(move || theme().muted_foreground),
        )
}

/// A pill like `chrome::badge`, for a count.
fn count_pill(label: String, tint: AdaptiveColor) -> impl IntoElement {
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
                .font_family(chrome::MONO)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tint),
        )
}

fn notice_line(state: DiffState) -> impl IntoElement {
    dynamic(move || {
        if let Some(error) = state
            .error
            .with(|error| (!error.is_empty()).then(|| error.clone()))
        {
            return text(error)
                .text_xs()
                .text_color(move || crate::palette::danger())
                .into_element();
        }
        if state
            .result
            .with(|result| result.as_ref().is_some_and(|result| result.truncated))
        {
            return text(format!(
                "Showing the first {} rows. The counts and Copy diff cover the full texts.",
                ROW_CAP
            ))
            .text_xs()
            .text_color(move || crate::palette::warning())
            .into_element();
        }
        view().into_element()
    })
}

/// What the result card shows. A memo, so typing in an editor does not
/// rebuild the row list until a new comparison lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Body {
    Empty,
    Pending { busy: bool },
    Identical,
    Equivalent,
    Rows(ViewMode),
}

fn result_body(state: DiffState) -> impl IntoElement {
    let body = computed(move || {
        let empty =
            state.left.source.with(String::is_empty) && state.right.source.with(String::is_empty);
        if empty {
            return Body::Empty;
        }
        let identical = state.result.with(|result| {
            result
                .as_ref()
                .map(|result| (result.summary.identical(), result.exact))
        });
        match identical {
            None => Body::Pending {
                busy: state.busy.get(),
            },
            Some((true, true)) => Body::Identical,
            Some((true, false)) if state.changes_only.get() => Body::Equivalent,
            Some(_) => Body::Rows(state.view.get()),
        }
    });
    dynamic(move || match body.get() {
        Body::Empty => chrome::empty_state(
            "±",
            "Nothing to compare",
            "Paste or open an original and a changed text, or load a synthetic sample.",
            "Load sample",
            "diff-sample",
            move || state.load_sample(),
        )
        .into_element(),
        Body::Pending { busy } => chrome::hint(if busy {
            "Comparing…"
        } else {
            "Press Compare."
        })
        .into_element(),
        Body::Identical => chrome::state_card(
            "Identical",
            "Both sides are the same text.",
            crate::palette::success(),
        )
        .into_element(),
        Body::Equivalent => chrome::state_card(
            "No changes",
            "The sides match under the current options (whitespace, case, or JSON format).",
            crate::palette::success(),
        )
        .into_element(),
        Body::Rows(ViewMode::Split) => split_list(state).into_element(),
        Body::Rows(ViewMode::Unified) => unified_list(state).into_element(),
    })
}

/// Row identities change with each comparison, so recycled cells do not
/// keep a size measured for another row.
fn row_keys(state: DiffState, count: usize) -> Vec<(u64, usize)> {
    let generation = state.result_generation.get();
    (0..count).map(|index| (generation, index)).collect()
}

fn split_list(state: DiffState) -> impl IntoElement {
    let count = move || {
        state
            .result
            .with(|result| result.as_ref().map_or(0, |r| r.side.len()))
    };
    virtual_list(count, move |at| {
        dynamic(move || {
            let row = state.result.with(|result| {
                result
                    .as_ref()
                    .and_then(|r| r.side.get(at.index()).cloned())
            });
            match row {
                Some(SideRow::Pair { left, right }) => view()
                    .flex_row()
                    .w_full()
                    .items_stretch()
                    .gap(px(8.0))
                    .py(px(1.0))
                    .child(split_cell(left))
                    .child(split_cell(right))
                    .into_element(),
                Some(SideRow::Fold { hidden }) => fold_row(hidden).into_element(),
                None => view().into_element(),
            }
        })
    })
    .keys(move || row_keys(state, count()))
    .estimated_item_size(px(ROW_HEIGHT))
    .flex_1()
    .min_h(px(0.0))
    .w_full()
    .test_id("diff-split")
}

fn split_cell(cell: Option<Cell>) -> impl IntoElement {
    let tag = cell.as_ref().map(|cell| cell.tag);
    let mut row = view()
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(ROW_HEIGHT))
        .flex_row()
        .items_start()
        .gap(px(6.0))
        .px(px(4.0))
        .py(px(2.0))
        .rounded(px(6.0))
        .bg(move || row_tint(tag));
    let Some(cell) = cell else {
        return row;
    };
    row = row
        .child(number(Some(cell.number)))
        .child(marker(cell.tag))
        .child(line_text(cell.spans, cell.tag));
    row
}

fn unified_list(state: DiffState) -> impl IntoElement {
    let count = move || {
        state
            .result
            .with(|result| result.as_ref().map_or(0, |r| r.unified.len()))
    };
    virtual_list(count, move |at| {
        dynamic(move || {
            let row = state.result.with(|result| {
                result
                    .as_ref()
                    .and_then(|r| r.unified.get(at.index()).cloned())
            });
            match row {
                Some(UnifiedRow::Line {
                    tag,
                    old,
                    new,
                    spans,
                }) => view()
                    .w_full()
                    .min_h(px(ROW_HEIGHT))
                    .flex_row()
                    .items_start()
                    .gap(px(6.0))
                    .px(px(4.0))
                    .py(px(2.0))
                    .my(px(0.5))
                    .rounded(px(6.0))
                    .bg(move || row_tint(Some(tag)))
                    .child(number(old))
                    .child(number(new))
                    .child(marker(tag))
                    .child(line_text(spans, tag))
                    .into_element(),
                Some(UnifiedRow::Fold { hidden }) => fold_row(hidden).into_element(),
                None => view().into_element(),
            }
        })
    })
    .keys(move || row_keys(state, count()))
    .estimated_item_size(px(ROW_HEIGHT))
    .flex_1()
    .min_h(px(0.0))
    .w_full()
    .test_id("diff-unified")
}

/// Behind a row: red for removed, green for added, a muted fill where the
/// other side has no line, nothing for an unchanged line.
fn row_tint(tag: Option<Tag>) -> AdaptiveColor {
    match tag {
        Some(Tag::Delete) => crate::palette::danger().opacity(0.12),
        Some(Tag::Insert) => crate::palette::success().opacity(0.12),
        Some(Tag::Equal) => AdaptiveColor::TRANSPARENT,
        None => theme().muted.opacity(0.35),
    }
}

fn word_mark(tag: Tag) -> AdaptiveColor {
    match tag {
        Tag::Delete => crate::palette::danger().opacity(0.28),
        Tag::Insert => crate::palette::success().opacity(0.28),
        Tag::Equal => theme().primary.opacity(0.22),
    }
}

fn line_ink(tag: Tag) -> AdaptiveColor {
    match tag {
        Tag::Delete => crate::palette::danger(),
        Tag::Insert => crate::palette::success(),
        Tag::Equal => theme().foreground,
    }
}

fn number(value: Option<usize>) -> impl IntoElement {
    text(value.map(|n| n.to_string()).unwrap_or_default())
        .flex_none()
        .w(px(NUMBER_WIDTH))
        .text_right()
        .font_family(chrome::MONO)
        .text_xs()
        .line_height(px(18.0))
        .text_color(move || theme().muted_foreground)
}

fn marker(tag: Tag) -> impl IntoElement {
    let glyph = match tag {
        Tag::Delete => "-",
        Tag::Insert => "+",
        Tag::Equal => " ",
    };
    text(glyph)
        .flex_none()
        .w(px(12.0))
        .font_family(chrome::MONO)
        .text_size(px(12.5))
        .line_height(px(18.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(move || line_ink(tag))
}

/// The line itself. Unchanged runs of a changed line take the line color.
/// A changed word is semibold on its own background.
fn line_text(spans: Vec<Span>, tag: Tag) -> impl IntoElement {
    let mut line = text("")
        .flex_1()
        .min_w(px(0.0))
        .font_family(chrome::MONO)
        .text_size(px(12.5))
        .line_height(px(18.0))
        .selectable(true);
    let blank = spans.iter().all(|span| span.text.is_empty());
    if blank {
        return line.child(text(" ").font_family(chrome::MONO).text_size(px(12.5)));
    }
    for span in spans {
        let mut run = text(span.text)
            .font_family(chrome::MONO)
            .text_size(px(12.5))
            .text_color(move || line_ink(tag));
        if span.changed {
            let mark = tag;
            run = run
                .font_weight(FontWeight::SEMIBOLD)
                .rounded(px(2.0))
                .bg(move || word_mark(mark));
        }
        line = line.child(run);
    }
    line
}

fn fold_row(hidden: usize) -> impl IntoElement {
    view()
        .w_full()
        .min_h(px(ROW_HEIGHT))
        .my(px(2.0))
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .bg(move || crate::palette::list_head())
        .child(
            text(format!("⋯ {} unchanged", plural(hidden, "line", "lines")))
                .text_xs()
                .text_color(move || theme().muted_foreground),
        )
}
