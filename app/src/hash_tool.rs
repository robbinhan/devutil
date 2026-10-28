//! Hash workbench: text digests, HMAC, and a streaming file hash.
//!
//! NSH 0.1.1 has no open panel, so the file button uses `rfd` (NSOpenPanel
//! on macOS, the XDG file portal on Linux). The panel runs off the engine
//! thread. File bytes are hashed on a blocking worker in 1 MB chunks.
//! Signals stay on the engine thread: progress is an atomic counter polled
//! from a timer.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use hash_core::{self, DigestRow};

use crate::chrome::{self, column_pane};
use crate::find::TextSearch;
use crate::hash_model::{self, InputMode, StatusLine, Tone, DEBOUNCE};
use crate::json_model::{self, field_text, reveal_match};
use crate::search_ui;
use nsh_core::ui::prelude::*;

#[derive(Clone, Copy)]
pub struct HashState {
    pub source: Signal<String>,
    pub field: Signal<String>,
    pub field_capped: Signal<bool>,
    pub mode: Signal<InputMode>,
    pub uppercase: Signal<bool>,
    pub base64: Signal<bool>,
    pub hmac: Signal<bool>,
    pub key: Signal<String>,
    pub compare: Signal<String>,
    pub text_rows: Signal<Vec<DigestRow>>,
    pub file_rows: Signal<Vec<DigestRow>>,
    pub text_stamp: Signal<(bool, String)>,
    pub file_stamp: Signal<(bool, String)>,
    pub text_gen: Signal<u64>,
    pub file_gen: Signal<u64>,
    pub busy: Signal<bool>,
    pub file_path: Signal<String>,
    pub file_size: Signal<Option<u64>>,
    pub file_done: Signal<u64>,
    pub file_busy: Signal<bool>,
    pub file_error: Signal<String>,
    pub search: TextSearch,
    pub field_origin: Signal<usize>,
    pub field_line: Signal<u32>,
}

impl HashState {
    pub fn new() -> Self {
        let sample = hash_model::SAMPLE.to_string();
        let rows = hash_core::hash_text(&sample, None);
        Self {
            source: signal(sample.clone()),
            field: signal(sample),
            field_capped: signal(false),
            mode: signal(InputMode::Text),
            uppercase: signal(false),
            base64: signal(false),
            hmac: signal(false),
            key: signal(String::new()),
            compare: signal(String::new()),
            text_rows: signal(rows),
            file_rows: signal(Vec::new()),
            text_stamp: signal((false, String::new())),
            file_stamp: signal((false, String::new())),
            text_gen: signal(0),
            file_gen: signal(0),
            busy: signal(false),
            file_path: signal(String::new()),
            file_size: signal(None),
            file_done: signal(0),
            file_busy: signal(false),
            file_error: signal(String::new()),
            search: TextSearch::new(),
            field_origin: signal(0),
            field_line: signal(0),
        }
    }

    pub fn publish_source(self, text: String, rehash: bool) {
        let shown = hash_model::displayed_input(&text);
        self.field_origin.set(0);
        self.field_line.set(0);
        self.field_capped.set(shown.capped);
        self.field.set(shown.text);
        self.source.set(text);
        if rehash {
            self.schedule_text();
        }
        if self.search.open.get() {
            self.schedule_search();
        }
    }

    pub fn schedule_search(self) {
        if self.mode.get() != InputMode::Text {
        self.search.clear_hits();
        self.search
            .label
            .set("Switch to Text to search. File mode hashes bytes, not this field.".to_string());
        return;
        }
        self.search.schedule(move || self.source.get());
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

    /// A keystroke in the field. The view never holds more than the preview.
    pub fn on_field(self, text: String) {
        if self.field.with(|current| current == &text) {
            return;
        }
        let origin = self.field_origin.get();
        let capped = self.field_capped.get();
        let preview = self.field.get();
        if (capped || origin > 0) && is_preview_edit(&preview, &text) {
            self.field.update(|_| {});
            return;
        }
        if origin == 0 && json_model::utf16_len(&text) + 8 >= json_model::FIELD_LIMIT {
            self.recover_clipboard(text);
            return;
        }
        if origin == 0 {
            self.publish_source(text, true);
            return;
        }
        let Some(spliced) = self
            .source
            .with(|source| json_model::splice_window(source, origin, preview.len(), &text))
        else {
            self.field.update(|_| {});
            return;
        };
        self.publish_source(spliced, true);
    }

    fn recover_clipboard(self, clipped: String) {
        self.field.set(clipped.clone());
        spawn(async move {
            let clip = clipboard_string().await;
            if clip.len() > clipped.len().saturating_add(8) && clip.starts_with(&clipped) {
                self.publish_source(clip, true);
            } else {
                self.publish_source(clipped, true);
            }
        });
    }

    pub fn paste(self) {
        spawn(async move {
            let text = clipboard_string().await;
            if text.is_empty() {
                return;
            }
            self.mode.set(InputMode::Text);
            self.publish_source(text, true);
        });
    }

    pub fn clear(self) {
        self.publish_source(String::new(), true);
    }

    pub fn set_mode(self, mode: InputMode) {
        if self.mode.get() == mode {
            return;
        }
        self.mode.set(mode);
        self.ensure_current();
        if self.search.open.get() {
            self.schedule_search();
        }
    }

    pub fn set_hmac(self, on: bool) {
        if self.hmac.get() == on {
            return;
        }
        self.hmac.set(on);
        self.schedule_active();
    }

    pub fn on_key_text(self, text: String) {
        if self.key.with(|current| current == &text) {
            return;
        }
        self.key.set(text);
        self.schedule_active();
    }

    /// ⌘↩. Skips the debounce and hashes the visible mode now.
    pub fn run(self) {
        match self.mode.get() {
            InputMode::Text => self.kick_text(),
            InputMode::File => self.kick_file(),
        }
    }

    pub fn open_path(self, path: String) {
        self.mode.set(InputMode::File);
        self.file_path.set(path);
        self.kick_file();
    }

    pub fn pick_file(self) {
        spawn(async move {
            let picked = blocking(|| {
                rfd::FileDialog::new()
                    .set_title("Choose a file to hash")
                    .pick_file()
            })
            .await;
            let Ok(Some(path)) = picked else {
                return;
            };
            self.open_path(path.to_string_lossy().into_owned());
        });
    }

    fn ensure_current(self) {
        match self.mode.get() {
            InputMode::Text => {
                if self.text_stamp.get() != self.stamp() {
                    self.kick_text();
                }
            }
            InputMode::File => {
                if self.file_path.get().is_empty() {
                    return;
                }
                if self.file_stamp.get() != self.stamp() {
                    self.kick_file();
                }
            }
        }
    }

    fn schedule_active(self) {
        match self.mode.get() {
            InputMode::Text => self.schedule_text(),
            InputMode::File => {
                if !self.file_path.get().is_empty() {
                    self.schedule_file();
                }
            }
        }
    }

    fn schedule_text(self) {
        let generation = self.text_gen.get().wrapping_add(1);
        self.text_gen.set(generation);
        let large = self.source.with(|source| !hash_model::hash_inline(source.len()));
        if large {
            self.busy.set(true);
        }
        set_timeout(DEBOUNCE, move || {
            if self.text_gen.get() != generation {
                return;
            }
            self.run_text(generation);
        });
    }

    fn kick_text(self) {
        let generation = self.text_gen.get().wrapping_add(1);
        self.text_gen.set(generation);
        self.run_text(generation);
    }

    fn run_text(self, generation: u64) {
        let text = self.source.with(|source| source.clone());
        let key = self.key_bytes();
        if hash_model::hash_inline(text.len()) {
            let rows = hash_core::hash_text(&text, key.as_deref());
            if self.text_gen.get() != generation {
                return;
            }
            self.text_rows.set(rows);
            self.text_stamp.set(self.stamp());
            self.busy.set(false);
            return;
        }
        self.busy.set(true);
        spawn(async move {
            let computed = blocking(move || hash_core::hash_text(&text, key.as_deref())).await;
            if self.text_gen.get() != generation {
                return;
            }
            if let Ok(rows) = computed {
                self.text_rows.set(rows);
                self.text_stamp.set(self.stamp());
            }
            self.busy.set(false);
        });
    }

    fn schedule_file(self) {
        let _ = take_cancel_token();
        let generation = self.file_gen.get().wrapping_add(1);
        self.file_gen.set(generation);
        set_timeout(DEBOUNCE, move || {
            if self.file_gen.get() != generation {
                return;
            }
            let path = self.file_path.get();
            if path.is_empty() {
                return;
            }
            self.start_file(generation, path);
        });
    }

    fn kick_file(self) {
        let generation = self.file_gen.get().wrapping_add(1);
        self.file_gen.set(generation);
        let path = self.file_path.get();
        if path.is_empty() {
            return;
        }
        self.start_file(generation, path);
    }

    fn start_file(self, generation: u64, path: String) {
        let cancel = take_cancel_token();
        let key = self.key_bytes();
        match std::fs::metadata(&path) {
            Ok(meta) => {
                self.file_error.set(String::new());
                self.file_size.set(Some(meta.len()));
                self.file_done.set(0);
                self.file_busy.set(true);
            }
            Err(err) => {
                self.file_error.set(err.to_string());
                self.file_rows.set(Vec::new());
                self.file_size.set(None);
                self.file_done.set(0);
                self.file_busy.set(false);
                return;
            }
        }
        let progress = Arc::new(AtomicU64::new(0));
        let tick_progress = Arc::clone(&progress);
        let ticks = set_interval(Duration::from_millis(100), move || {
            if self.file_gen.get() != generation {
                return;
            }
            self.file_done.set(tick_progress.load(Ordering::Relaxed));
        });
        let path_for_thread = path.clone();
        spawn(async move {
            let computed = blocking(move || {
                hash_core::hash_path(
                    Path::new(&path_for_thread),
                    key.as_deref(),
                    |done, _total| {
                        progress.store(done, Ordering::Relaxed);
                    },
                    &cancel,
                )
            })
            .await;
            ticks.cancel();
            if self.file_gen.get() != generation {
                return;
            }
            match computed {
                Ok(Ok(file)) => {
                    self.file_done.set(file.size);
                    self.file_size.set(Some(file.size));
                    self.file_rows.set(file.rows);
                    self.file_stamp.set(self.stamp());
                    self.file_error.set(String::new());
                }
                Ok(Err(hash_core::HashError::Cancelled)) => {}
                Ok(Err(err)) => {
                    self.file_error.set(err.to_string());
                    self.file_rows.set(Vec::new());
                }
                Err(err) => {
                    self.file_error.set(err.to_string());
                    self.file_rows.set(Vec::new());
                }
            }
            self.file_busy.set(false);
        });
    }

    fn stamp(self) -> (bool, String) {
        (self.hmac.get(), self.key.get())
    }

    fn key_bytes(self) -> Option<Vec<u8>> {
        self.hmac.get().then(|| self.key.get().into_bytes())
    }
}

fn take_cancel_token() -> Arc<AtomicBool> {
    static CURRENT: OnceLock<Mutex<Arc<AtomicBool>>> = OnceLock::new();
    let slot = CURRENT.get_or_init(|| Mutex::new(Arc::new(AtomicBool::new(false))));
    let mut guard = slot.lock().expect("hash cancel");
    guard.store(true, Ordering::Relaxed);
    let next = Arc::new(AtomicBool::new(false));
    *guard = Arc::clone(&next);
    next
}

pub fn body(state: HashState, copied: Signal<Option<String>>, shortcut: String) -> impl IntoElement {
    let header_shortcut = shortcut.clone();
    let editor = element_ref();
    let query_ref = element_ref();
    search_ui::watch_focus(state.search, query_ref.clone());
    let field_editor = editor.clone();
    effect(move || {
        let revision = state.search.revision.get();
        let _ = state.search.index.get();
        untracked(|| {
            if revision == 0 || !state.search.open.get() || state.mode.get() != InputMode::Text {
                return;
            }
            let Some(hit) = state.search.current() else {
                state.search.snippets.set(Vec::new());
                return;
            };
            let full = state.source.get();
            let revealed = reveal_match(&full, hit.start, hit.end);
            state.field_origin.set(revealed.preview.byte_start);
            state.field_line.set(revealed.line);
            state.field_capped.set(revealed.preview.capped);
            state.field.set(revealed.preview.text);
            state.search.publish_snippets(&full);
            search_ui::select_later(field_editor.clone(), revealed.utf16_start, revealed.utf16_end);
        });
    });
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
                .child(input_pane(state, editor))
                .child(result_pane(state, copied)),
        )
}

fn page_header(state: HashState, shortcut: String) -> impl IntoElement {
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
                .child(chrome::title("Hash"))
                .child(chrome::hint("Digests for text and files")),
        )
        .child(view().flex_1())
        .child(chrome::action_button("Paste", "hash-paste", false, move || {
            state.paste()
        }))
        .child(chrome::action_button("Clear", "hash-clear", false, move || {
            state.clear()
        }))
        .child(
            text(shortcut)
                .text_xs()
                .text_color(move || theme().muted_foreground),
        )
        .child(chrome::action_button("Hash", "hash-run", true, move || state.run()))
}

fn input_pane(state: HashState, editor: ElementRef) -> impl IntoElement {
    column_pane()
        .child(header(state))
        .child(mode_row(state))
        .child(option_row(state))
        .child(If::new(
            move || state.hmac.get(),
            move || key_field(state),
        ))
        .child(compare_field(state))
        .child(status_line(state))
        .child(cap_note(state))
        .child(If::new(
            move || state.mode.get() == InputMode::Text,
            move || text_field(state, editor.clone()),
        ))
        .child(If::new(
            move || state.mode.get() == InputMode::File,
            move || file_card(state),
        ))
}

fn header(state: HashState) -> impl IntoElement {
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .child(
            view()
                .gap(px(2.0))
                .child(chrome::section("Input")),
        )
        .child(view().flex_1())
        .child(
            text(move || {
                if state.mode.get() == InputMode::File {
                    return state.file_path.with(|path| {
                        if path.is_empty() {
                            "No file".to_string()
                        } else {
                            hash_model::file_name(path).to_string()
                        }
                    });
                }
                state.source.with(|text| chrome::meta_line(text))
            })
            .text_xs()
            .text_color(move || theme().muted_foreground),
        )
}

fn mode_row(state: HashState) -> impl IntoElement {
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .child(chrome::hint("Source"))
        .child(
            chrome::segment_row()
                .child(chrome::segment("Text", state.mode, InputMode::Text, move |mode| {
                    state.set_mode(mode)
                }))
                .child(chrome::segment("File", state.mode, InputMode::File, move |mode| {
                    state.set_mode(mode)
                })),
        )
}

fn cap_note(state: HashState) -> impl IntoElement {
    dynamic(move || {
        if state.field_origin.get() > 0 {
            let message = state
                .source
                .with(|full| json_model::window_message(full.len(), state.field_line.get()));
            return view().child(
                text(message)
                    .text_xs()
                    .text_color(move || theme().warning),
            );
        }
        if state.mode.get() != InputMode::Text || !state.field_capped.get() {
            return view();
        }
        let message = state.source.with(|full| {
            state
                .field
                .with(|preview| hash_model::cap_message(full.len(), preview.len()))
        });
        view().child(
            text(message)
                .text_xs()
                .text_color(move || theme().warning),
        )
    })
}

fn text_field(state: HashState, editor: ElementRef) -> impl IntoElement {
    let field = state.field;
    chrome::mono_field(field, hash_model::SAMPLE, true, true, move |text| {
        state.on_field(text)
    })
    .element_ref(&editor)
    .test_id("hash-input")
    .flex_1()
    .min_h(px(120.0))
}

fn file_card(state: HashState) -> impl IntoElement {
    chrome::card().child(
        view()
            .gap(px(6.0))
            .child(
                view()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .child(chrome::action_button("Open…", "hash-open", true, move || {
                        state.pick_file()
                    }))
                    .child(
                        view().flex_1().min_w(px(0.0)).child(
                            text(move || {
                                let path = state.file_path.get();
                                if path.is_empty() {
                                    "No file selected".to_string()
                                } else {
                                    path
                                }
                            })
                            .text_xs()
                            .font_family("monospace")
                            .number_of_lines(1)
                            .ellipsize_mode(nsh_core::EllipsizeMode::Tail)
                            .text_color(move || theme().foreground),
                        ),
                    ),
            )
            .child(progress_bar(state)),
    )
}

fn progress_bar(state: HashState) -> impl IntoElement {
    dynamic(move || {
        if !state.file_busy.get() {
            return view();
        }
        let done = state.file_done.get();
        let size = state.file_size.get().unwrap_or(0);
        let width = hash_model::progress_width(done, size);
        view().gap(px(4.0)).child(
            view()
                .h(px(6.0))
                .w(px(220.0))
                .rounded_full()
                .bg(move || theme().muted)
                .child(
                    view()
                        .h(px(6.0))
                        .w(px(width))
                        .rounded_full()
                        .bg(move || theme().primary),
                ),
        )
    })
}

fn option_row(state: HashState) -> impl IntoElement {
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .child(chrome::hint("HMAC"))
        .child(
            chrome::segment_row()
                .child(chrome::segment("Off", state.hmac, false, move |on| {
                    state.set_hmac(on)
                }))
                .child(chrome::segment("On", state.hmac, true, move |on| {
                    state.set_hmac(on)
                })),
        )
        .child(chrome::hint("Hex"))
        .child(
            chrome::segment_row()
                .child(chrome::segment("lower", state.uppercase, false, move |upper| {
                    state.uppercase.set(upper)
                }))
                .child(chrome::segment("UPPER", state.uppercase, true, move |upper| {
                    state.uppercase.set(upper)
                })),
        )
        .child(chrome::hint("Output"))
        .child(
            chrome::segment_row()
                .child(chrome::segment("Hex", state.base64, false, move |on| {
                    state.base64.set(on)
                }))
                .child(chrome::segment("Base64", state.base64, true, move |on| {
                    state.base64.set(on)
                })),
        )
}

fn key_field(state: HashState) -> impl IntoElement {
    view().gap(px(4.0)).child(chrome::hint("Key")).child(
        chrome::mono_field(state.key, "UTF-8 key", false, true, move |text| {
            state.on_key_text(text)
        })
        .test_id("hash-key"),
    )
}

fn compare_field(state: HashState) -> impl IntoElement {
    view().gap(px(4.0)).child(chrome::hint("Compare")).child(
        chrome::mono_field(state.compare, "Paste an expected hash", false, true, move |text| {
            state.compare.set(text)
        })
        .test_id("hash-compare"),
    )
}

fn status_line(state: HashState) -> impl IntoElement {
    text(move || active_status(state).text)
        .text_xs()
        .text_color(move || tone_color(active_status(state).tone))
}

fn active_status(state: HashState) -> StatusLine {
    let compare = compare_note(state);
    if state.compare.with(|text| !text.trim().is_empty()) {
        return compare;
    }
    match state.mode.get() {
        InputMode::Text => text_note(state),
        InputMode::File => file_note(state),
    }
}

fn text_note(state: HashState) -> StatusLine {
    let bytes = state.source.with(|text| text.len());
    hash_model::text_status(bytes, state.busy.get(), state.hmac.get())
}

fn file_note(state: HashState) -> StatusLine {
    state.file_path.with(|path| {
        hash_model::file_status(
            path,
            state.file_size.get(),
            state.file_done.get(),
            state.file_busy.get(),
            &state.file_error.get(),
        )
    })
}

fn compare_note(state: HashState) -> StatusLine {
    let expected = state.compare.get();
    let rows = displayed_rows(state);
    hash_model::compare_status(&rows, &expected)
}

fn displayed_rows(state: HashState) -> Vec<DigestRow> {
    match state.mode.get() {
        InputMode::Text => state.text_rows.get(),
        InputMode::File => state.file_rows.get(),
    }
}

fn result_pane(state: HashState, copied: Signal<Option<String>>) -> impl IntoElement {
    column_pane()
        .child(
            view()
                .gap(px(2.0))
                .child(chrome::section("Digests"))
                .child(chrome::hint(
                    "Case and Base64 only change how a digest is shown.",
                )),
        )
        .child(
            scroll().flex_1().min_h(px(0.0)).child(
                view()
                    .w_full()
                    .gap(px(8.0))
                    .pb(px(8.0))
                    .child(rows(state, copied)),
            ),
        )
}

fn rows(state: HashState, copied: Signal<Option<String>>) -> impl IntoElement {
    dynamic(move || {
        let list = displayed_rows(state);
        if state.mode.get() == InputMode::File && list.is_empty() && !state.file_busy.get() {
            return view().child(chrome::empty_state(
                "#",
                "No file yet",
                "Choose a file. It is read in 1 MB chunks, so a large file stays flat in memory.",
                "Open…",
                "hash-open-empty",
                move || state.pick_file(),
            ));
        }
        let expected = state.compare.get();
        let uppercase = state.uppercase.get();
        let base64 = state.base64.get();
        let mut column = view().w_full().gap(px(6.0));
        for row in list {
            column = column.child(digest_row(row, expected.clone(), uppercase, base64, copied));
        }
        column
    })
}

fn digest_row(
    row: DigestRow,
    expected: String,
    uppercase: bool,
    base64: bool,
    copied: Signal<Option<String>>,
) -> impl IntoElement {
    let matched = row.matches(&expected);
    let label = row.label;
    let shown = row.display(uppercase, base64);
    let bytes = row.bytes.clone();
    let available = row.available;
    view()
        .w_full()
        .gap(px(4.0))
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(12.0))
        .bg(move || {
            if matched {
                theme().success.opacity(0.14)
            } else {
                theme().group_box
            }
        })
        .border_1()
        .border_color(move || {
            if matched {
                theme().success.opacity(0.45)
            } else {
                theme().border
            }
        })
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(px(8.0))
                .child(chrome::chip(label))
                .child(If::new(
                    move || matched,
                    || chrome::badge("Match", theme().success),
                ))
                .child(view().flex_1())
                .child(If::new(
                    move || available,
                    move || {
                        let bytes = bytes.clone();
                        chrome::copy_icon(label, copied, move || {
                            if base64 {
                                hash_core::base64_encode(&bytes)
                            } else {
                                hash_core::hex_encode(&bytes, uppercase)
                            }
                        })
                    },
                )),
        )
        .child(if available {
            text(shown)
                .font_family(chrome::MONO)
                .text_size(px(13.0))
                .line_height(px(20.0))
                .selectable(true)
                .text_color(move || theme().foreground)
        } else {
            text("No HMAC for this algorithm")
                .text_xs()
                .text_color(move || theme().muted_foreground)
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
