//! DevUtil workbench on Native Shell 0.1.1.
//!
//! The tools call `json_core`, `timestamp_core`, `url_codec_core`, and
//! `hash_core` directly. `App.tsx` mounts this same `App` when `nsh run`
//! hosts the TSX entry.

mod chrome;
mod find;
mod hash_model;
mod hash_tool;
mod json_model;
mod json_tool;
mod markdown_tool;
mod search_ui;
mod timestamp_model;
mod timestamp_tool;
mod url_model;
mod url_tool;

use hash_tool::HashState;
use json_tool::JsonState;
use markdown_tool::MarkdownState;
use nsh_core::keymap::When;
use nsh_core::ui::prelude::*;
use timestamp_tool::TimestampState;
use url_tool::UrlState;

/// Light and dark palettes, loaded from `theme.json`.
pub fn app_theme() -> Theme {
    Theme::from_json(include_str!("theme.json")).expect("DevUtil theme")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tool {
    Json,
    Timestamp,
    Url,
    Hash,
    Markdown,
}

/// The components the TSX entry imports from `app/native`.
#[native_module("app/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}

/// One window, five local tools. ⌘↩ (and Ctrl+Enter) runs the visible tool.
/// Markdown has no run action; its preview updates as you type.
/// Appearance follows the system; `theme.json` has a light and a dark palette.
#[component]
pub fn App() -> impl IntoElement {
    let tool = signal(Tool::Json);
    let copied: Signal<Option<String>> = signal(None);
    let json = JsonState::new();
    load_dev_json(json);
    let timestamp = TimestampState::new();
    let url = UrlState::new();
    let hash = HashState::new();
    load_dev_hash(hash);
    let markdown = MarkdownState::new();
    match std::env::var("DEVUTIL_TOOL").ok().as_deref() {
        Some("timestamp") => tool.set(Tool::Timestamp),
        Some("url") => tool.set(Tool::Url),
        Some("hash") => tool.set(Tool::Hash),
        Some("markdown") => tool.set(Tool::Markdown),
        _ => {}
    }
    let shortcut = key_label("mod+enter");

    // Bound once. Each page stays mounted, so a binding inside a page would
    // be overwritten by whichever page registered last.
    bind_convert("mod+enter", tool, json, timestamp, url, hash, markdown);
    bind_convert("ctrl+enter", tool, json, timestamp, url, hash, markdown);
    bind_find(tool, json, timestamp, url, hash, markdown);

    // `flex_1` is what makes this host fill the window. A percentage height
    // does not: the root's child otherwise stays as tall as its content.
    view()
        .flex_1()
        .flex_row()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
                .child(sidebar(tool, copied, shortcut.clone()))
                .child(
                    view()
                        .flex_1()
                        .min_w(px(0.0))
                        .min_h(px(0.0))
                        .flex_col()
                        .child(pages(
                            tool, json, timestamp, url, hash, markdown, copied, shortcut,
                        )),
                )
                .child(find_keys(tool, json, timestamp, url, hash, markdown))
}

/// `DEVUTIL_HASH_FILE=/path` hashes that file on startup, in the file mode.
fn load_dev_hash(hash: HashState) {
    let Ok(path) = std::env::var("DEVUTIL_HASH_FILE") else {
        return;
    };
    hash.open_path(path);
}

/// `DEVUTIL_JSON=/path/to/file.json` loads that document into the JSON tool.
/// `DEVUTIL_ANALYZE=1` analyzes it on startup. Used to measure large files.
fn load_dev_json(json: JsonState) {
    let Ok(path) = std::env::var("DEVUTIL_JSON") else {
        return;
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            let bytes = text.len();
            json.publish_source(text);
            if std::env::var("DEVUTIL_ANALYZE").ok().as_deref() == Some("1") {
                let started = std::time::Instant::now();
                json.analyze(json_model::OutputMode::Pretty);
                eprintln!("DEVUTIL_ANALYZE {bytes} bytes in {:?}", started.elapsed());
            }
        }
        Err(err) => eprintln!("DEVUTIL_JSON {path}: {err}"),
    }
}

fn bind_convert(
    keystroke: &str,
    tool: Signal<Tool>,
    json: JsonState,
    timestamp: TimestampState,
    url: UrlState,
    hash: HashState,
    _markdown: MarkdownState,
) {
    on_key_with(
        keystroke,
        KeyOptions {
            when: When::Always,
            repeat: false,
        },
        move |_| match tool.get() {
            Tool::Json => json.analyze(json_model::OutputMode::Pretty),
            Tool::Timestamp => timestamp.convert(),
            Tool::Url => url.convert(),
            Tool::Hash => hash.run(),
            Tool::Markdown => {}
        },
    );
}

fn bind_find(
    tool: Signal<Tool>,
    json: JsonState,
    timestamp: TimestampState,
    url: UrlState,
    hash: HashState,
    markdown: MarkdownState,
) {
    let always = KeyOptions {
        when: When::Always,
        repeat: false,
    };
    let repeat = KeyOptions {
        when: When::Always,
        repeat: true,
    };
    on_key_with("mod+f", always, move |_| {
        toggle_find(tool, json, timestamp, url, hash, markdown)
    });
    on_key_with("mod+g", repeat, move |_| {
        open_find(tool, json, timestamp, url, hash, markdown);
        step_find(tool, json, timestamp, url, hash, markdown, 1);
    });
    on_key_with("shift+mod+g", repeat, move |_| {
        open_find(tool, json, timestamp, url, hash, markdown);
        step_find(tool, json, timestamp, url, hash, markdown, -1);
    });
}

fn find_keys(
    tool: Signal<Tool>,
    json: JsonState,
    timestamp: TimestampState,
    url: UrlState,
    hash: HashState,
    markdown: MarkdownState,
) -> impl IntoElement {
    let open = computed(move || match tool.get() {
        Tool::Json => json.search.open.get(),
        Tool::Timestamp => timestamp.search.open.get(),
        Tool::Url => url.search.open.get(),
        Tool::Hash => hash.search.open.get(),
        Tool::Markdown => markdown.search.open.get(),
    });
    If::new(open, move || {
        let always = KeyOptions {
            when: When::Always,
            repeat: true,
        };
        on_key_with("escape", KeyOptions { repeat: false, ..always }, move |_| {
            close_find(tool, json, timestamp, url, hash, markdown);
        });
        on_key_with("enter", always, move |_| {
            step_find(tool, json, timestamp, url, hash, markdown, 1);
        });
        on_key_with("shift+enter", always, move |_| {
            step_find(tool, json, timestamp, url, hash, markdown, -1);
        });
        view()
    })
}

fn toggle_find(
    tool: Signal<Tool>,
    json: JsonState,
    timestamp: TimestampState,
    url: UrlState,
    hash: HashState,
    markdown: MarkdownState,
) {
    let open = match tool.get() {
        Tool::Json => json.search.open.get(),
        Tool::Timestamp => timestamp.search.open.get(),
        Tool::Url => url.search.open.get(),
        Tool::Hash => hash.search.open.get(),
        Tool::Markdown => markdown.search.open.get(),
    };
    if open {
        close_find(tool, json, timestamp, url, hash, markdown);
    } else {
        open_find(tool, json, timestamp, url, hash, markdown);
    }
}

fn open_find(
    tool: Signal<Tool>,
    json: JsonState,
    timestamp: TimestampState,
    url: UrlState,
    hash: HashState,
    markdown: MarkdownState,
) {
    match tool.get() {
        Tool::Json => {
            json.search.open.set(true);
            json.search.request_focus();
            json.schedule_search();
        }
        Tool::Timestamp => {
            timestamp.search.open.set(true);
            timestamp.search.request_focus();
            timestamp.schedule_search();
        }
        Tool::Url => {
            url.search.open.set(true);
            url.search.request_focus();
            url.schedule_search();
        }
        Tool::Hash => {
            hash.search.open.set(true);
            hash.search.request_focus();
            hash.schedule_search();
        }
        Tool::Markdown => {
            markdown.search.open.set(true);
            markdown.search.request_focus();
            markdown.schedule_search();
        }
    }
}

fn close_find(
    tool: Signal<Tool>,
    json: JsonState,
    timestamp: TimestampState,
    url: UrlState,
    hash: HashState,
    markdown: MarkdownState,
) {
    match tool.get() {
        Tool::Json => json.close_search(),
        Tool::Timestamp => timestamp.close_search(),
        Tool::Url => url.close_search(),
        Tool::Hash => hash.close_search(),
        Tool::Markdown => markdown.close_search(),
    }
}

fn step_find(
    tool: Signal<Tool>,
    json: JsonState,
    timestamp: TimestampState,
    url: UrlState,
    hash: HashState,
    markdown: MarkdownState,
    delta: isize,
) {
    match tool.get() {
        Tool::Json => {
            if json.find_target.get() == json_tool::FindTarget::Tree {
                json.step_tree(delta);
            } else {
                json.search.step(delta);
            }
        }
        Tool::Timestamp => timestamp.search.step(delta),
        Tool::Url => url.search.step(delta),
        Tool::Hash => hash.search.step(delta),
        Tool::Markdown => markdown.search.step(delta),
    }
}

fn select_tool(tool: Signal<Tool>, copied: Signal<Option<String>>, next: Tool) {
    copied.set(None);
    tool.set(next);
}

fn sidebar(tool: Signal<Tool>, copied: Signal<Option<String>>, shortcut: String) -> impl IntoElement {
    view()
        .flex_none()
        .w(px(200.0))
        .h_full()
        .flex_col()
        .gap(px(4.0))
        .p(px(12.0))
        .bg(move || theme().accent)
        .border_r_1()
        .border_color(move || theme().border)
        .child(
            view()
                .gap(px(2.0))
                .px(px(8.0))
                .pt(px(8.0))
                .pb(px(12.0))
                .child(
                    text("DevUtil")
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(move || theme().foreground),
                )
                .child(chrome::hint("Local developer tools")),
        )
        .child(chrome::nav_item("{ }", "JSON", tool, Tool::Json, move |next| {
            select_tool(tool, copied, next)
        }))
        .child(chrome::nav_item(
            "◷",
            "Timestamp",
            tool,
            Tool::Timestamp,
            move |next| select_tool(tool, copied, next),
        ))
        .child(chrome::nav_item("↗", "URL", tool, Tool::Url, move |next| {
            select_tool(tool, copied, next)
        }))
        .child(chrome::nav_item("#", "Hash", tool, Tool::Hash, move |next| {
            select_tool(tool, copied, next)
        }))
        .child(chrome::nav_item("M↓", "Markdown", tool, Tool::Markdown, move |next| {
            select_tool(tool, copied, next)
        }))
        .child(view().flex_1())
        .child(
            text(move || format!("{shortcut} runs this tool"))
                .text_xs()
                .px(px(8.0))
                .pb(px(8.0))
                .text_color(move || theme().muted_foreground),
        )
}

fn pages(
    tool: Signal<Tool>,
    json: JsonState,
    timestamp: TimestampState,
    url: UrlState,
    hash: HashState,
    markdown: MarkdownState,
    copied: Signal<Option<String>>,
    shortcut: String,
) -> impl IntoElement {
    let timestamp_shortcut = shortcut.clone();
    let url_shortcut = shortcut.clone();
    let hash_shortcut = shortcut.clone();
    let markdown_shortcut = shortcut.clone();
    view()
        .flex_1()
        .min_h(px(0.0))
        .child(page(
            move || tool.get() == Tool::Json,
            json_tool::body(json, copied, shortcut),
        ))
        .child(page(
            move || tool.get() == Tool::Timestamp,
            timestamp_tool::body(timestamp, copied, timestamp_shortcut),
        ))
        .child(page(
            move || tool.get() == Tool::Url,
            url_tool::body(url, copied, url_shortcut),
        ))
        .child(page(
            move || tool.get() == Tool::Hash,
            hash_tool::body(hash, copied, hash_shortcut),
        ))
        .child(page(
            move || tool.get() == Tool::Markdown,
            markdown_tool::body(markdown, copied, markdown_shortcut),
        ))
}

/// Hidden tools stay mounted, so their fields and results survive a tab
/// change, and `display: none` keeps them from sharing the column.
fn page(
    active: impl Fn() -> bool + 'static,
    body: impl IntoElement,
) -> impl IntoElement {
    view()
        .flex_1()
        .min_h(px(0.0))
        .style_when(move || !active(), |style| style.hidden())
        .child(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_file_loads() {
        let theme = app_theme();
        assert_eq!(theme.radius, 12.0);
        assert_ne!(theme.background.light, theme.background.dark);
        assert_ne!(theme.foreground.light, theme.foreground.dark);
    }
}
