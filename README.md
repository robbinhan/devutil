# DevUtil

A local developer workbench: JSON, timestamps, URL percent-encoding, hashes, and a Markdown preview in one native window. JSON, timestamps, and URLs call the original Rust cores unchanged. Hashes live in `hash_core`. Markdown lives in `markdown_core`. The window is an [NSH](https://nsh.dev) 0.1.1 Rust app. On macOS it renders with AppKit.

Documents stay on the machine. There is no account and no upload. Markdown can open an `http` or `https` link in the system browser and show an image from a URL.

## Features

- **JSON** — paste or load a document, analyze it, format or minify, and walk a tree. Recovery reports diagnostics with line and column. ⌘F searches the full input, the result (Pretty, Compact, or Source), and the tree.
- **Timestamp** — convert Unix seconds, Unix milliseconds, ISO 8601, and `yyyy-MM-dd HH:mm:ss`, shown as local time, UTC, seconds, and milliseconds.
- **URL** — percent-decode and encode `application/x-www-form-urlencoded` text, and split a URL into scheme, host, path, and query parameters.
- **Hash** — MD5, SHA-1, SHA-2, SHA3-256, BLAKE3, and CRC32 for text or a file, plus HMAC, hex or Base64, and a compare field.
- **Markdown** — split editor and preview. GitHub-flavored headings, lists, task lists, tables, quotes, code, and footnotes. Copy HTML, open a `.md` file, and search the editor with ⌘F.

## Layout

| Path | Role |
| --- | --- |
| `crates/json_core` | Parse, recover, format, and walk JSON. Unchanged. |
| `crates/timestamp_core` | Unix seconds/milliseconds, ISO 8601, and `yyyy-MM-dd HH:mm:ss`. Unchanged. |
| `crates/url_codec_core` | `application/x-www-form-urlencoded` decode and encode. Unchanged. |
| `crates/hash_core` | MD5, SHA-1, SHA-2, SHA3-256, BLAKE3, CRC32, and HMAC. Streaming file hashes. |
| `crates/markdown_core` | GitHub-flavored Markdown as a block model, plus HTML export. |
| `app` | The NSH workbench (`devutil` library and binary). |
| `App.tsx` | Mounts the Rust `App` so `nsh run` and `nsh package` can host it. |
| `nsh.json` | App name `DevUtil`, id `app.devutil.nsh`, icon. Appearance follows the system. |
| `assets/icon.png` | 1024×1024 icon taken from the previous app icon set. |

The window opens on JSON. A sidebar switches among JSON, Timestamp, URL, Hash, and Markdown. ⌘↩ (and Ctrl+Enter) runs the action for the visible tool: Analyze, Convert, Decode/Encode, or Hash. Markdown has no run action; the preview updates as you type. ⌘F opens find in the visible tool. Enter and ⌘G go to the next match, Shift+Enter and Shift+⌘G go to the previous one, and Esc closes the bar. Aa is case sensitive. `.*` is a regular expression. The count reads `3 / 17`. The current match is the first line of the field, and the rows under the find bar color every hit on those lines.

## Requirements

- Rust newer than 1.94. `rust-toolchain.toml` pins the `stable` channel. The `app` crate uses edition 2024.
- A checkout of [NSH v0.1.1](https://github.com/longbridge/nsh) (tag `v0.1.1`) and the NSH CLI 0.1.1. Building needs `NSH_SOURCE` pointed at that checkout.
- Xcode, for the AppKit window. Apple silicon is the first target.

```sh
git clone --branch v0.1.1 --depth 1 https://github.com/longbridge/nsh.git ~/src/nsh
export NSH_SOURCE=~/src/nsh
```

`nsh.json` sets `"rust": "app"`. In 0.1.1 a Rust app is rebuilt against NSH's sources. If `NSH_SOURCE` is unset, `nsh run` and `nsh package` stop with:

```text
error an app with a Rust crate (nsh.json "rust") builds against nsh's sources: set NSH_SOURCE to a nsh checkout
```

Current pages on nsh.dev describe a later API (`nsh-base`, a `root` field, shadcn theme tokens). This project stays on the 0.1.1 crates. Do not point `NSH_SOURCE` at `main`.

On machines whose `cc` is GCC (for example from Nix or flox), the Apple target still needs the system clang:

```sh
export CC=/usr/bin/clang
export CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=/usr/bin/clang
```

## Build

Development:

```sh
nsh run
```

An installable `.app` (icon from `assets/icon.png`, a 1024×1024 PNG):

```sh
nsh package --target macos
```

`nsh check` typechecks `App.tsx` and compiles the `devutil` crate. `cargo run -p devutil` links the published `nsh-platform-macos` 0.1.1 crate and opens the same window without the NSH CLI.

Tests, from the repo root:

```sh
cargo test --workspace
```

## What the window does

**JSON.** Paste, Analyze, Format (pretty), Minify (compact), and Copy output. Analyze reports a disposition (Valid JSON, Recovered safely, Review suggested transformation, Could not parse safely), diagnostics with line and column, a virtualized tree, and the normalization steps. Pressing a diagnostic selects that range in the editor. Output can be shown as pretty JSON, the compact normalized text, or the original source. A string whose text is itself JSON can be expanded in the tree.

A multiline field shows at most 32,768 UTF-16 units and 200 lines. Analyze, Format, Minify, and Copy still use the whole document. A paste bigger than that is clipped by the text view (`max_length`) and, when the clipboard still starts with what landed in the field, the rest is recovered from the clipboard. The Paste button reads the clipboard directly. An expanded array or object lists the first 1,000 children and a row for how many were left out.

Load a file without pasting it:

```sh
DEVUTIL_JSON=big.json DEVUTIL_ANALYZE=1 cargo run -p devutil
```

**Timestamp.** The field starts with a sample (Unix seconds, Unix milliseconds, an offset ISO time, and a wall-clock time). Unit is Automatic, Seconds, or Milliseconds. Zone is Local or UTC and is only the zone used to read a date that has no offset. Results are four cards: local (always this machine's zone), UTC, Unix seconds, and Unix milliseconds. Each card copies.

**URL.** The field starts with a percent-encoded sample. Decode or Encode, then Convert (⌘↩). Decode shows the text plus scheme, host, path, and query parameters when the text is a URL.

**Hash.** Text mode shows MD5, SHA-1, SHA-224, SHA-256, SHA-384, SHA-512, SHA3-256, BLAKE3, and CRC32 for the UTF-8 input. Hex can be lower or upper case, and the same bytes can be shown as standard Base64. Those toggles do not rehash. HMAC takes a UTF-8 key and covers HMAC-MD5, SHA-1, SHA-224, SHA-256, SHA-384, SHA-512, and SHA3-256. BLAKE3 and CRC32 have no HMAC, so those rows stay blank instead of showing a plain digest. Paste an expected hash into Compare: a match is highlighted, ignoring case, surrounding whitespace, a leading `0x`, and a single `sha256:`-style label.

The text view still shows at most 32,768 UTF-16 units and 200 lines. Hash uses the full string. Edits wait 200 ms before hashing, and input larger than 64 KB is hashed on a background thread.

File mode opens a native file panel, then hashes the file in 1 MB chunks on a background thread. The row shows the file size and, while the read is in progress, how far it has gotten. A new hash cancels the one still reading.

```sh
DEVUTIL_TOOL=hash DEVUTIL_HASH_FILE=some.bin cargo run -p devutil
```

**Markdown.** The sidebar glyph is `M↓`. The left pane is the editor. The right pane is the rendered note: headings, paragraphs, bold, italic, strikethrough, inline code, links, quotes, lists (including nested lists and task checkboxes), fenced code with a language label and a Copy button, tables, rules, images, and footnotes. Paste, Copy HTML, Open, and Clear sit in the header, with a word and character count on the preview. An empty note offers a sample. ⌘F searches the editor. A large note is parsed off the UI thread. The preview keeps at most 2,000 blocks and 256 KB of text; Copy HTML still uses the whole note. `DEVUTIL_TOOL=markdown` opens this tool.

## Migration report

Ported from the GPUI workbench (`gpui-kit` 0.6.6) onto NSH 0.1.1 Rust. The cores were copied as-is. The legacy Swift package was left behind.

The GTK 4 backend of the same binary was exercised on Linux (1100×720 window): empty JSON state, Analyze of `{"ok": true}` with a Valid JSON banner and a tree row, Timestamp convert of the multiline sample (the engine's unsupported-format error, same as GPUI), and URL decode of the sample to `hello world!` and `name=DevUtil&ok=1`. AppKit was not opened here. This environment is Ubuntu x86_64, so `nsh package --target macos` was not run. `nsh check` with `NSH_SOURCE` on the v0.1.1 tag compiled the Rust crate. `cargo test` covers the cores and the workbench models.

### What matches

- One window, sidebar switcher, light and dark workbench chrome, monospace fields.
- JSON paste, analyze, diagnostics, format, minify, copy, reset, tree, and apply-a-candidate.
- Timestamp unit and interpretation zone, four result cards, copy, clear, ⌘↩.
- URL decode/encode, structure and query rows, copy, clear, ⌘↩.
- Hash text and file digests, hex case, Base64, HMAC, compare, and copy.
- Edit menu (undo, cut, copy, paste, select all) from the NSH host. Clipboard has no extra capability flag in 0.1.1.

### What is missing or degraded, and why

- **No syntax highlighting, line numbers, gutter, or rope.** Find is in the workbench, not in the system find bar: NSH 0.1.1 multiline input is a plain `NSTextView` inside an `NSScrollView` with `setRichText(false)`, and it has no find bar, no `scrollRangeToVisible`, and no find indicator. Single-line input is an `NSTextField`. The text container tracks the view width (`setHorizontallyResizable(false)`), so lines wrap and there is no horizontal scroller and no wrap toggle. A rope editor would be a custom native view. `set_selection` sets a UTF-16 range and does not scroll. On macOS an unfocused `NSTextView` with `drawsBackground` false draws no selection, and focus stays in the find field so the query keeps the caret. The field text is therefore replaced so the current match's line is the first line, including small documents and the URL, Timestamp, Hash, and Markdown editors. Earlier text stays hidden until Find closes. Under the bar, nearby lines show every hit in the info color and the current hit in the primary color, semibold, on a tinted row. Click a row to jump. Tree rows color the matched substring the same way. NSH text runs have a foreground color and a weight, not a background, so the tint is the row behind the line. A custom `NSTextView` would need `AppView::register_component`, and `run_app` does not hand the app that view, so this build does not fork NSH.
- **The text view cannot hold a multi-megabyte document.** Two NSH 0.1.1 mechanisms typeset the entire `input` value. The core's `measure_input` builds an attributed string with `max_lines: 0` and asks the platform text engine (CoreText on macOS, Pango on Linux) for the height of every line, even when the view's height is already set. The macOS view is an `NSTextView` with `setVerticallyResizable(true)` and a container height of `CGFloat::MAX`, so it lays out every line to size itself. One 3.8 MB line, then the pretty-printed copy of it, climbed past 6 GB here and did not settle. The fields therefore show a capped preview. Copy and Analyze read the full strings, which never go back into the text view.
- **Object source ranges used to copy the whole prefix.** `StrictJSONParser::make_range` allocated `characters[..index]` into a `String` three times per object member, so a large array was quadratic. Ranges are now a UTF-16 offset table built once. The reported offsets are unchanged.
- **The whole string is still the value of a small field.** `.value` is controlled text. Each keystroke delivers the full string through `on_change_text`. There is no rope. Above the preview limit the view is not given that string.
- **Selection is per field.** 0.1.1 has no `SelectionArea`. Each result is its own text view, so a selection cannot cross cards. Readonly output uses `editable(false)` and stays selectable, so selecting and copying inside one field works. Binding `.selection` reapplies whenever the bound value changes and resets the caret, so the diagnostic jump uses `element_ref().set_selection` (UTF-16, the same units as `json_core::SourceRange`) and then focuses the editor.
- **Return inserts a newline.** Multiline fields use submit behavior Newline. ⌘↩ is bound with `on_key_with(..., When::Always)` on the window so it runs before the field sees the key. Ctrl+Enter is bound the same way. ⌘Q, ⌘H, ⌥⌘H, ⌘M, and ⌃⌘F are reserved by the host. Quit stays on the application menu.
- **The tree is a virtual list, not a tree widget.** 0.1.1 has no tree view and no tooltip (`nsh-ui-mobile` has lists and text fields, not those). Rows are one line and ellipsize. A Copy control on the row copies the full label. `virtual_list` recycles cells, so the row index is read inside the binding, not captured when the cell is built. Child ids are `{parent}/{index}`. The GPUI tree used `{id}/{key}`, which collides when one object has two fields with the same key. Index ids avoid that. A string of JSON gets a nested node id `{id}/nested` that starts collapsed. An expanded collection lists the first 1,000 children; GPUI built a node for every value up front. Find filters that list to matching keys and values, expands their ancestors, and still stops at 1,000 matching children per collection. The matched substring is primary and semibold. The current row uses the active list color; other matching rows use the selection color.
- **Markdown has no in-app browser.** A link with an `http` or `https` URL is opened with `open` (macOS), `xdg-open` (Linux), or `start` (Windows). Other URLs stay as text. An image is an NSH `image` when the URL is `http(s)`, a `data:image` URL, or a file that exists next to an opened note (or an absolute path). Anything else is the alt text. Task checkboxes are drawn, not toggled in the source. The preview is native views in the app theme, so it follows light and dark mode. It is not a web view.
- **Narrow windows squish the two columns.** Panes use `min_w(0)` and `h_full` so they fill the 1100×720 window. They do not stack. `flex_wrap` is set, but a full-height pane does not drop onto a second row.
- **No minimum window size and no dock icon on `AppOptions`.** The macOS options in 0.1.1 have a title, a size, and an appearance. `cargo run -p devutil` uses the process icon. The 1024 PNG is applied by `nsh package` from `nsh.json`.
- **`nsh-ui-mobile` is the wrong chrome.** Those controls are sized for touch. The toolbar, segments, and buttons are small desktop views in `app`.
- **Seeded samples are real text, not placeholders.** The first Convert on the untouched timestamp sample can fail (`Unsupported date format...`) because the field holds four lines. Decode on the untouched URL sample decodes the whole blob. Clearing the field shows the same text as a placeholder. The zone control does not redraw the Local card. That card is always the system zone. Local versus UTC only changes how an offset-less date is parsed.
- **Appearance follows the system.** `theme.json` defines a light palette and a dark palette. `nsh.json` does not force `"appearance": "light"`, and the window asks for `Appearance::System`. NSH 0.1.1 has no SF Symbols, so the sidebar marks are glyphs. A row cannot show its copy control only while the pointer is over it, so tree and digest rows keep a small muted copy mark. There is no syntax color for the editor itself; the tree tints keys, strings, numbers, and booleans from the label text.
- **NSH 0.1.1 has no file panel.** There is no `NSOpenPanel` or GTK file chooser in the 0.1.1 API. Hash's Open button uses the `rfd` crate, which presents `NSOpenPanel` on macOS (hopping to the main thread) and an XDG desktop-portal dialog on Linux. The panel is not reachable from `NSH_SCRIPT`. `DEVUTIL_HASH_FILE` starts a file hash without it.
- **Workers cannot write signals.** `spawn` runs on the engine thread. File and large-text hashes use `blocking` (`tokio::task::spawn_blocking`). Progress is an atomic counter the engine reads on a 100 ms timer.
- **No tooltip.** A matching digest is a green border, a Match label, and the status line. BLAKE3's keyed mode is not HMAC, so HMAC mode does not invent a BLAKE3 or CRC32 result.

### Recommendations

- Use `nsh run` while iterating, with `NSH_SOURCE` pinned to tag `v0.1.1`. Package with `nsh package --target macos`. `cargo run -p devutil` opens the same window without the NSH CLI.
- Ignore current nsh.dev Rust snippets when editing this tree. They target a newer API than crates.io 0.1.1.
- Keep large-document work out of the stock text view. If the JSON editor needs a rope, highlighting, or a gutter, add a native view rather than stretching `NSTextView`.
- Keep the index-based tree ids. They are stricter than the GPUI ids and stay stable when keys repeat.
