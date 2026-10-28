//! Markdown preview. The editor is the same capped text view as the other
//! tools. Parsing runs off the engine thread once the note is large, and the
//! preview keeps a bounded block model from `markdown_core`.

use std::time::Duration;

use markdown_core::{
    Align, Block, ImageLocation, Inline, ListItem, QuoteKind, PREVIEW_BLOCK_CAP, PREVIEW_BYTE_CAP,
};

use crate::chrome::{self, column_pane};
use crate::find::{self, TextSearch};
use crate::json_model::{self, field_text, window_message, FieldWrite, FIELD_LIMIT};
use crate::search_ui;
use nsh_core::ui::prelude::*;

const DEBOUNCE: Duration = Duration::from_millis(200);

pub const SAMPLE: &str = "\
# DevUtil

A local **markdown** preview with *emphasis*, ~~strikethrough~~, and `inline code`.

> A block quote, for a note you want set apart.

## Checklist

- [x] Search this editor with ⌘F
- [ ] Open a `.md` file

### Nested list

1. First
   - Nested item
2. Second

```rust
fn main() {
    println!(\"hello\");
}
```

| Tool | Shortcut |
| --- | --- |
| Find | ⌘F |
| Run | ⌘↩ |

---

[Example](https://example.com)

![Mark](https://example.com/mark.png)
";

#[derive(Clone, Copy)]
pub struct MarkdownState {
    pub source: Signal<String>,
    pub field: Signal<String>,
    pub field_origin: Signal<usize>,
    pub field_line: Signal<u32>,
    pub field_capped: Signal<bool>,
    pub blocks: Signal<Vec<Block>>,
    pub words: Signal<usize>,
    pub chars: Signal<usize>,
    pub truncated: Signal<bool>,
    pub busy: Signal<bool>,
    pub preview_gen: Signal<u64>,
    pub base_dir: Signal<String>,
    pub error: Signal<String>,
    pub search: TextSearch,
}

impl MarkdownState {
    pub fn new() -> Self {
        Self {
            source: signal(String::new()),
            field: signal(String::new()),
            field_origin: signal(0),
            field_line: signal(0),
            field_capped: signal(false),
            blocks: signal(Vec::new()),
            words: signal(0),
            chars: signal(0),
            truncated: signal(false),
            busy: signal(false),
            preview_gen: signal(0),
            base_dir: signal(String::new()),
            error: signal(String::new()),
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
        self.error.set(String::new());
        self.schedule_preview();
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
            FieldWrite::Replace(next) => {
                if json_model::utf16_len(&next) + 8 >= FIELD_LIMIT {
                    self.recover_clipboard(next);
                } else {
                    self.publish_source(next);
                }
            }
            FieldWrite::Splice(next) => {
                self.field.set(text);
                self.source.set(next);
                self.schedule_preview();
                if self.search.open.get() {
                    self.schedule_search();
                }
            }
        }
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

    pub fn schedule_preview(self) {
        let generation = self.preview_gen.get().wrapping_add(1);
        self.preview_gen.set(generation);
        set_timeout(DEBOUNCE, move || {
            if self.preview_gen.get() != generation {
                return;
            }
            let text = self.source.get();
            if text.len() <= find::INLINE_BYTES {
                self.store_preview(generation, markdown_core::parse_limited(&text, PREVIEW_BLOCK_CAP));
                return;
            }
            self.busy.set(true);
            spawn(async move {
                let parsed = blocking(move || markdown_core::parse_limited(&text, PREVIEW_BLOCK_CAP)).await;
                if self.preview_gen.get() != generation {
                    return;
                }
                match parsed {
                    Ok(doc) => self.store_preview(generation, doc),
                    Err(err) => {
                        self.busy.set(false);
                        self.error.set(err.to_string());
                    }
                }
            });
        });
    }

    fn store_preview(self, generation: u64, doc: markdown_core::Document) {
        if self.preview_gen.get() != generation {
            return;
        }
        self.blocks.set(doc.blocks);
        self.words.set(doc.words);
        self.chars.set(doc.chars);
        self.truncated.set(doc.truncated);
        self.busy.set(false);
    }

    pub fn schedule_search(self) {
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

    pub fn paste(self) {
        spawn(async move {
            let text = clipboard_string().await;
            if text.is_empty() {
                return;
            }
            self.base_dir.set(String::new());
            self.publish_source(text);
        });
    }

    pub fn copy_html(self, copied: Signal<Option<String>>) {
        let text = self.source.get();
        if text.is_empty() {
            return;
        }
        if text.len() <= find::INLINE_BYTES {
            set_clipboard(markdown_core::to_html(&text));
            copied.set(Some("md-html".to_string()));
            return;
        }
        self.busy.set(true);
        spawn(async move {
            let html = blocking(move || markdown_core::to_html(&text)).await;
            self.busy.set(false);
            match html {
                Ok(html) => {
                    set_clipboard(html);
                    copied.set(Some("md-html".to_string()));
                }
                Err(err) => self.error.set(err.to_string()),
            }
        });
    }

    pub fn clear(self) {
        self.base_dir.set(String::new());
        self.publish_source(String::new());
    }

    pub fn load_sample(self) {
        self.base_dir.set(String::new());
        self.publish_source(SAMPLE.to_string());
    }

    pub fn pick_file(self) {
        spawn(async move {
            let picked = blocking(|| {
                rfd::FileDialog::new()
                    .set_title("Open Markdown")
                    .add_filter("Markdown", &["md", "markdown", "txt"])
                    .pick_file()
            })
            .await;
            let Ok(Some(path)) = picked else {
                return;
            };
            let path = path.to_string_lossy().into_owned();
            let base = std::path::Path::new(&path)
                .parent()
                .map(|parent| parent.to_string_lossy().into_owned())
                .unwrap_or_default();
            let read = blocking(move || std::fs::read_to_string(&path)).await;
            match read {
                Ok(Ok(text)) => {
                    self.base_dir.set(base);
                    self.publish_source(text);
                }
                Ok(Err(err)) => self.error.set(err.to_string()),
                Err(err) => self.error.set(err.to_string()),
            }
        });
    }
}

pub fn body(state: MarkdownState, copied: Signal<Option<String>>, shortcut: String) -> impl IntoElement {
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
        .child(page_header(state, copied, shortcut))
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
                .child(editor_pane(state, editor))
                .child(preview_pane(state, copied)),
        )
}

fn page_header(state: MarkdownState, copied: Signal<Option<String>>, shortcut: String) -> impl IntoElement {
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
                .child(chrome::title("Markdown"))
                .child(chrome::hint("Write on the left, read on the right")),
        )
        .child(view().flex_1())
        .child(chrome::action_button("Paste", "md-paste", false, move || state.paste()))
        .child(chrome::action_button(
            move || {
                if copied.get().as_deref() == Some("md-html") {
                    "Copied".to_string()
                } else {
                    "Copy HTML".to_string()
                }
            },
            "md-copy-html",
            false,
            move || state.copy_html(copied),
        ))
        .child(chrome::action_button("Open", "md-open", false, move || state.pick_file()))
        .child(chrome::action_button("Clear", "md-clear", false, move || state.clear()))
        .child(
            text(shortcut)
                .text_xs()
                .text_color(move || theme().muted_foreground),
        )
}

fn editor_pane(state: MarkdownState, editor: ElementRef) -> impl IntoElement {
    column_pane()
        .child(editor_header(state))
        .child(field_note(state))
        .child(
            chrome::mono_field(state.field, "Paste markdown", true, true, move |text| {
                state.on_field(text);
            })
            .element_ref(&editor)
            .test_id("md-input")
            .flex_1()
            .min_h(px(160.0)),
        )
}

fn editor_header(state: MarkdownState) -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .gap(px(8.0))
        .child(chrome::section("Editor"))
        .child(view().flex_1())
        .child(
            text(move || state.source.with(|text| chrome::meta_line(text)))
                .text_xs()
                .text_color(move || theme().muted_foreground),
        )
}

fn field_note(state: MarkdownState) -> impl IntoElement {
    dynamic(move || {
        let origin = state.field_origin.get();
        if origin > 0 {
            let message = state
                .source
                .with(|full| window_message(full.len(), state.field_line.get()));
            return view()
                .child(
                    text(message)
                        .text_xs()
                        .text_color(move || theme().warning),
                )
                .into_element();
        }
        if !state.field_capped.get() {
            return view().into_element();
        }
        let message = state.source.with(|full| {
            json_model::cap_message(full.len(), state.field.with(|preview| preview.len()))
        });
        view()
            .child(
                text(message)
                    .text_xs()
                    .text_color(move || theme().warning),
            )
            .into_element()
    })
}

fn preview_pane(state: MarkdownState, copied: Signal<Option<String>>) -> impl IntoElement {
    column_pane()
        .child(preview_header(state))
        .child(
            scroll().flex_1().min_h(px(0.0)).child(
                view()
                    .w_full()
                    .pb(px(12.0))
                    .child(preview_body(state, copied)),
            ),
        )
}

fn preview_header(state: MarkdownState) -> impl IntoElement {
    view()
        .flex_none()
        .gap(px(4.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .child(chrome::section("Preview"))
                .child(view().flex_1())
                .child(
                    text(move || {
                        if state.busy.get() {
                            return "Rendering…".to_string();
                        }
                        let words = state.words.get();
                        let chars = state.chars.get();
                        format!(
                            "{words} {} · {chars} {}",
                            if words == 1 { "word" } else { "words" },
                            if chars == 1 { "character" } else { "characters" }
                        )
                    })
                    .text_xs()
                    .text_color(move || theme().muted_foreground),
                ),
        )
        .child(dynamic(move || {
            if state.error.with(|error| error.is_empty()) && !state.truncated.get() {
                return view().into_element();
            }
            let message = if let Some(error) = state.error.with(|error| {
                if error.is_empty() {
                    None
                } else {
                    Some(error.clone())
                }
            }) {
                error
            } else {
                format!(
                    "Preview shows the first {PREVIEW_BLOCK_CAP} blocks or {}. Copy HTML uses the full note.",
                    json_model::format_bytes(PREVIEW_BYTE_CAP)
                )
            };
            view()
                .child(
                    text(message)
                        .text_xs()
                        .text_color(move || theme().warning),
                )
                .into_element()
        }))
}

fn preview_body(state: MarkdownState, copied: Signal<Option<String>>) -> impl IntoElement {
    dynamic(move || {
        if state.source.with(|text| text.trim().is_empty()) {
            return chrome::empty_state(
                "M↓",
                "Nothing to preview",
                "Paste markdown, open a .md file, or load a sample with headings, a list, a table, and code.",
                "Load sample",
                "md-sample",
                move || state.load_sample(),
            )
            .into_element();
        }
        let base = state.base_dir.get();
        state.blocks.with(|blocks| {
            let mut ctx = Ctx {
                copied,
                base_dir: base,
                code: 0,
            };
            let mut column = view().w_full().gap(px(10.0));
            if blocks.is_empty() {
                column = column.child(chrome::hint(if state.busy.get() {
                    "Rendering…"
                } else {
                    "Nothing to preview yet."
                }));
            }
            for block in blocks {
                column = column.child(render_block(block, &mut ctx, 0));
            }
            column.into_element()
        })
    })
}

struct Ctx {
    copied: Signal<Option<String>>,
    base_dir: String,
    code: usize,
}

fn render_block(block: &Block, ctx: &mut Ctx, depth: u32) -> AnyElement {
    match block {
        Block::Heading { level, inlines } => render_heading(*level, inlines, ctx).into_element(),
        Block::Paragraph { inlines } => render_inlines(inlines, Look::body(), None, ctx)
            .line_height(px(22.0))
            .into_element(),
        Block::Quote { kind, blocks } => render_quote(*kind, blocks, ctx, depth),
        Block::List { ordered, start, items } => render_list(*ordered, *start, items, depth, ctx),
        Block::Code { language, text } => render_code(language, text, ctx),
        Block::Table { aligns, header, rows } => render_table(aligns, header, rows, ctx),
        Block::Rule => view()
            .h(px(1.0))
            .w_full()
            .my(px(4.0))
            .bg(move || theme().border)
            .into_element(),
        Block::Footnote { label, blocks } => render_footnote(label, blocks, ctx),
        Block::Html(html) => text(html.clone())
            .font_family(chrome::MONO)
            .text_xs()
            .text_color(move || theme().muted_foreground)
            .into_element(),
    }
}

fn render_heading(level: u8, inlines: &[Inline], ctx: &Ctx) -> Text {
    let look = Look {
        scale: heading_scale(level),
        weight: if level <= 2 { FontWeight::BOLD } else { FontWeight::SEMIBOLD },
        italic: false,
        strike: false,
    };
    render_inlines(inlines, look, None, ctx)
}

fn render_quote(kind: Option<QuoteKind>, blocks: &[Block], ctx: &mut Ctx, depth: u32) -> AnyElement {
    let mut column = view()
        .border_l_1()
        .border_l_color(move || theme().primary)
        .pl(px(12.0))
        .gap(px(6.0));
    if let Some(kind) = kind {
        column = column.child(
            text(kind.label())
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(move || theme().primary),
        );
    }
    for block in blocks {
        column = column.child(render_block(block, ctx, depth));
    }
    column.into_element()
}

fn render_list(ordered: bool, start: u64, items: &[ListItem], depth: u32, ctx: &mut Ctx) -> AnyElement {
    let mut column = view().gap(px(4.0));
    for (index, item) in items.iter().enumerate() {
        let marker = match item.checked {
            Some(true) => "☑".to_string(),
            Some(false) => "☐".to_string(),
            None if ordered => format!("{}.", start + index as u64),
            None => "•".to_string(),
        };
        let done = item.checked == Some(true);
        let mut body = view().flex_1().min_w(px(0.0)).gap(px(4.0));
        for block in &item.blocks {
            body = body.child(render_block(block, ctx, depth + 1));
        }
        column = column.child(
            view()
                .flex_row()
                .items_start()
                .gap(px(8.0))
                .pl(px(depth as f32 * 16.0))
                .child(
                    text(marker)
                        .flex_none()
                        .w(px(28.0))
                        .text_sm()
                        .text_color(move || {
                            if done {
                                theme().success
                            } else {
                                theme().muted_foreground
                            }
                        }),
                )
                .child(body),
        );
    }
    column.into_element()
}

fn render_code(language: &str, code: &str, ctx: &mut Ctx) -> AnyElement {
    let index = ctx.code;
    ctx.code += 1;
    let id = format!("md-code-{index}");
    let body = code.to_string();
    let label = if language.is_empty() { "text".to_string() } else { language.to_string() };
    view()
        .w_full()
        .gap(px(6.0))
        .p(px(10.0))
        .rounded(px(12.0))
        .bg(move || theme().primary.opacity(0.08))
        .border_1()
        .border_color(move || theme().border)
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(8.0))
                .child(
                    text(label)
                        .flex_1()
                        .font_family(chrome::MONO)
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(move || theme().muted_foreground),
                )
                .child(chrome::copy_button(id, ctx.copied, move || body.clone())),
        )
        .child(
            text(code.to_string())
                .font_family(chrome::MONO)
                .text_xs()
                .line_height(px(18.0))
                .selectable(true)
                .text_color(move || theme().foreground),
        )
        .into_element()
}

fn render_table(aligns: &[Align], header: &[Vec<Inline>], rows: &[Vec<Vec<Inline>>], ctx: &Ctx) -> AnyElement {
    let mut column = view()
        .w_full()
        .rounded(px(12.0))
        .border_1()
        .border_color(move || theme().border);
    if !header.is_empty() {
        column = column.child(table_row(header, aligns, true, ctx));
    }
    for row in rows {
        column = column.child(table_row(row, aligns, false, ctx));
    }
    column.into_element()
}

fn table_row(cells: &[Vec<Inline>], aligns: &[Align], head: bool, ctx: &Ctx) -> impl IntoElement {
    let mut row = view()
        .flex_row()
        .w_full()
        .items_stretch()
        .border_b_1()
        .border_color(move || theme().border)
        .bg(move || {
            if head {
                theme().list_head
            } else {
                AdaptiveColor::TRANSPARENT
            }
        });
    for (index, cell) in cells.iter().enumerate() {
        let align = aligns.get(index).copied().unwrap_or(Align::None);
        let mut node = render_inlines(cell, Look::body(), None, ctx);
        node = match align {
            Align::Center => node.text_center(),
            Align::Right => node.text_right(),
            Align::Left | Align::None => node.text_left(),
        };
        if head {
            node = node.font_weight(FontWeight::SEMIBOLD);
        }
        row = row.child(
            view()
                .flex_1()
                .min_w(px(0.0))
                .p(px(6.0))
                .child(node),
        );
    }
    row
}

fn render_footnote(label: &str, blocks: &[Block], ctx: &mut Ctx) -> AnyElement {
    let mut column = view().gap(px(4.0));
    column = column.child(
        text(format!("[{label}]"))
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(move || theme().info),
    );
    for block in blocks {
        column = column.child(render_block(block, ctx, 0));
    }
    column.into_element()
}

#[derive(Clone, Copy)]
struct Look {
    scale: Scale,
    weight: FontWeight,
    italic: bool,
    strike: bool,
}

impl Look {
    fn body() -> Self {
        Self {
            scale: Scale::Sm,
            weight: FontWeight::NORMAL,
            italic: false,
            strike: false,
        }
    }
}

#[derive(Clone, Copy)]
enum Scale {
    Xs,
    Sm,
    Base,
    Lg,
    Xl,
    X2,
}

fn heading_scale(level: u8) -> Scale {
    match level {
        1 => Scale::X2,
        2 => Scale::Xl,
        3 => Scale::Lg,
        4 => Scale::Base,
        5 => Scale::Sm,
        _ => Scale::Xs,
    }
}

fn render_inlines(inlines: &[Inline], look: Look, link: Option<&str>, ctx: &Ctx) -> Text {
    let mut node = apply_scale(text(""), look.scale);
    for inline in inlines {
        node = push_inline(node, inline, look, link, ctx);
    }
    node
}

fn push_inline(node: Text, inline: &Inline, look: Look, link: Option<&str>, ctx: &Ctx) -> Text {
    match inline {
        Inline::Text(value) => node.child(style_span(text(value.clone()), look, link)),
        Inline::SoftBreak => node.child(style_span(text(" "), look, link)),
        Inline::HardBreak => node.child(style_span(text("\n"), look, link)),
        Inline::Code(value) => node.child(
            view()
                .px(px(4.0))
                .rounded(px(4.0))
                .bg(move || theme().muted)
                .child(
                    text(value.clone())
                        .font_family(chrome::MONO)
                        .text_size(px(12.5))
                        .text_color(move || theme().foreground),
                ),
        ),
        Inline::Emphasis(children) => {
            let look = Look { italic: true, ..look };
            node.child(render_inlines(children, look, link, ctx))
        }
        Inline::Strong(children) => {
            let look = Look {
                weight: FontWeight::BOLD,
                ..look
            };
            node.child(render_inlines(children, look, link, ctx))
        }
        Inline::Strike(children) => {
            let look = Look { strike: true, ..look };
            node.child(render_inlines(children, look, link, ctx))
        }
        Inline::Link { dest, children, .. } => node.child(render_inlines(children, look, Some(dest), ctx)),
        Inline::Footnote(label) => node.child(
            text(format!("[{label}]"))
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(move || theme().info),
        ),
        Inline::Image { alt, url, .. } => node.child(render_image(alt, url, ctx)),
    }
}

fn render_image(alt: &str, url: &str, ctx: &Ctx) -> AnyElement {
    let alt_label = if alt.is_empty() { "Image".to_string() } else { alt.to_string() };
    match markdown_core::locate_image(&ctx.base_dir, url) {
        ImageLocation::Source(src) => view()
            .flex_col()
            .gap(px(4.0))
            .my(px(4.0))
            .child(
                image(src)
                    .w(px(280.0))
                    .h(px(160.0))
                    .resize_mode(nsh_core::ResizeMode::Contain)
                    .rounded(px(8.0)),
            )
            .child(
                text(alt_label)
                    .text_xs()
                    .text_color(move || theme().muted_foreground),
            )
            .into_element(),
        ImageLocation::Missing => text(alt_label)
            .italic()
            .text_sm()
            .text_color(move || theme().muted_foreground)
            .into_element(),
    }
}

fn style_span(node: Text, look: Look, link: Option<&str>) -> Text {
    let mut node = apply_scale(node, look.scale).font_weight(look.weight);
    if look.italic {
        node = node.italic();
    }
    if look.strike {
        node = node.line_through();
    }
    let url = link.and_then(markdown_core::browser_url).map(str::to_string);
    let linked = url.is_some();
    if linked {
        node = node.underline();
    }
    node = node.text_color(move || {
        if linked {
            theme().info
        } else {
            theme().foreground
        }
    });
    if let Some(url) = url {
        node = node.on_press(move |_: &()| open_browser(url.clone()));
    }
    node
}

fn apply_scale(node: Text, scale: Scale) -> Text {
    match scale {
        Scale::Xs => node.text_xs(),
        Scale::Sm => node.text_sm(),
        Scale::Base => node.text_base(),
        Scale::Lg => node.text_lg(),
        Scale::Xl => node.text_xl(),
        Scale::X2 => node.text_2xl(),
    }
}

fn open_browser(url: String) {
    spawn(async move {
        let _ = blocking(move || {
            #[cfg(target_os = "macos")]
            {
                let _ = std::process::Command::new("open").arg(&url).status();
            }
            #[cfg(target_os = "linux")]
            {
                let _ = std::process::Command::new("xdg-open").arg(&url).status();
            }
            #[cfg(target_os = "windows")]
            {
                let _ = std::process::Command::new("cmd")
                    .args(["/C", "start", "", &url])
                    .status();
            }
        })
        .await;
    });
}
