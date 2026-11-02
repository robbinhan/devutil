# DevUtil

A local developer workbench: JSON, timestamps, URL percent-encoding, hashes, a Markdown preview, and a text diff in one native window. JSON, timestamps, and URLs call the original Rust cores unchanged. Hashes live in `hash_core`. Markdown lives in `markdown_core`. Diff lives in `diff_core`. The window is an [NSH](https://nsh.dev) 0.3.0 Rust app. On macOS it renders with AppKit.

Documents stay on the machine. There is no account and no upload. Markdown can open an `http` or `https` link in the system browser and show an image from a URL.

## Features

- **JSON** — paste or load a document, analyze it, format or minify, and walk a tree. Recovery reports diagnostics with line and column. ⌘F searches the full input, the result (Pretty, Compact, or Source), and the tree.
- **Timestamp** — convert Unix seconds, Unix milliseconds, ISO 8601, and `yyyy-MM-dd HH:mm:ss`, shown as local time, UTC, seconds, and milliseconds.
- **URL** — percent-decode and encode `application/x-www-form-urlencoded` text, and split a URL into scheme, host, path, and query parameters.
- **Hash** — MD5, SHA-1, SHA-2, SHA3-256, BLAKE3, and CRC32 for text or a file, plus HMAC, hex or Base64, and a compare field.
- **Markdown** — split editor and preview. GitHub-flavored headings, lists, task lists, tables, quotes, code, and footnotes. Copy HTML, open a `.md` file, and search the editor with ⌘F.
- **Diff** — compare an original and a changed text side by side or as a unified view, with line numbers, `+`/`-` markers, and the changed words marked inside a line. Ignore whitespace, ignore case, or format both sides as JSON with sorted keys. Swap, Clear, Paste and Open for each side, and Copy diff as a unified patch.

## Layout

| Path | Role |
| --- | --- |
| `crates/json_core` | Parse, recover, format, and walk JSON. Unchanged. |
| `crates/timestamp_core` | Unix seconds/milliseconds, ISO 8601, and `yyyy-MM-dd HH:mm:ss`. Unchanged. |
| `crates/url_codec_core` | `application/x-www-form-urlencoded` decode and encode. Unchanged. |
| `crates/hash_core` | MD5, SHA-1, SHA-2, SHA3-256, BLAKE3, CRC32, and HMAC. Streaming file hashes. |
| `crates/markdown_core` | GitHub-flavored Markdown as a block model, plus HTML export. |
| `crates/diff_core` | Line diff (via `similar`) with word spans inside changed lines, folded context, and unified patch output. |
| `app` | The NSH workbench (`devutil` library and binary). |
| `App.tsx` | Mounts the Rust `App` so `nsh run` and `nsh package` can host it. |
| `nsh.json` | App name `DevUtil`, id `app.devutil.nsh`, icon. Appearance follows the system. |
| `assets/icon.png` | 1024×1024 icon taken from the previous app icon set. |

The window opens on JSON. A sidebar switches among JSON, Timestamp, URL, Hash, Markdown, and Diff. ⌘↩ (and Ctrl+Enter) runs the action for the visible tool: Analyze, Convert, Decode/Encode, Hash, or Compare. Markdown has no run action; the preview updates as you type. ⌘F opens find in the visible tool. Enter and ⌘G go to the next match, Shift+Enter and Shift+⌘G go to the previous one, and Esc closes the bar. Aa is case sensitive. `.*` is a regular expression. The count reads `3 / 17`. Matches are highlighted in the editor and the current one scrolls into view. The rows under the find bar still list the nearby hits.

## Requirements

- Rust stable (`rust-toolchain.toml` pins the `stable` channel). The `app` crate uses edition 2024.
- The NSH CLI 0.3.0, the same release as the crates in `app/Cargo.toml` (`nsh-core`, `nsh-base`, and the platform crates, all `0.3.0`). Tag `v0.3.0` is commit `76fa8a7fed47ea51aea01fa85bb40960be15d984` (2026-10-26). `nsh run` and `nsh package` build against those published crates. This app does not use `NSH_SOURCE`.
- Xcode, for the AppKit window. Apple silicon is the first target.

```sh
cargo install nsh-cli --version 0.3.0 --locked
```

To build that CLI from the NSH repository instead of crates.io, check out tag `v0.3.0` and install `crates/nsh-cli`. Do not point it at a later `main` unless you also move the crate versions.

`nsh.json` sets `"rust": "app"` and `"root": "App"`. `nsh run` and `nsh package` mount that Rust component. `nsh run App.tsx` still mounts it through the TSX entry. The theme file is shadcn `cssVars` (`app/src/theme.json`), with the same light and dark workbench colors.

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

`nsh check` typechecks `App.tsx` and runs `cargo check` on the `devutil` crate. `cargo run -p devutil` links the published `nsh-platform-macos` 0.3.0 crate and opens the same window without the NSH CLI. `nsh package --target macos` builds the installable app from the same `nsh.json` root. No `NSH_SOURCE` checkout is required for that command.

Tests, from the repo root:

```sh
cargo test --workspace
```

## What the window does

**JSON.** Paste, Analyze, Format (pretty), Minify (compact), and Copy output. Analyze reports a disposition (Valid JSON, Recovered safely, Review suggested transformation, Could not parse safely), diagnostics with line and column, a virtualized tree, and the normalization steps. Pressing a diagnostic selects that range in the editor. Output can be shown as pretty JSON, the compact normalized text, or the original source. A string whose text is itself JSON can be expanded in the tree.

The editor is NSH 0.3's `Editor`: it keeps the whole document and only builds the rows on screen. Analyze, Format, Minify, and Copy read that same text. The Paste button reads the clipboard directly. An expanded array or object lists the first 1,000 children and a row for how many were left out. Find highlights matches in the editor and scrolls the current match into view. The current match is the editor selection (the theme's selection color). Other matches use the editor's search highlight. The list under the find bar still shows the nearby lines. Line breaks are stored as LF.

Load a file without pasting it:

```sh
DEVUTIL_JSON=big.json DEVUTIL_ANALYZE=1 cargo run -p devutil
```

**Timestamp.** The field starts with a sample (Unix seconds, Unix milliseconds, an offset ISO time, and a wall-clock time). Unit is Automatic, Seconds, or Milliseconds. Zone is Local or UTC and is only the zone used to read a date that has no offset. Results are four cards: local (always this machine's zone), UTC, Unix seconds, and Unix milliseconds. Each card copies.

**URL.** The field starts with a percent-encoded sample. Decode or Encode, then Convert (⌘↩). Decode shows the text plus scheme, host, path, and query parameters when the text is a URL.

**Hash.** Text mode shows MD5, SHA-1, SHA-224, SHA-256, SHA-384, SHA-512, SHA3-256, BLAKE3, and CRC32 for the UTF-8 input. Hex can be lower or upper case, and the same bytes can be shown as standard Base64. Those toggles do not rehash. HMAC takes a UTF-8 key and covers HMAC-MD5, SHA-1, SHA-224, SHA-256, SHA-384, SHA-512, and SHA3-256. BLAKE3 and CRC32 have no HMAC, so those rows stay blank instead of showing a plain digest. Paste an expected hash into Compare: a match is highlighted, ignoring case, surrounding whitespace, a leading `0x`, and a single `sha256:`-style label.

The text editor holds the whole string. Hash uses that string. Edits wait 200 ms before hashing, and input larger than 64 KB is hashed on a background thread. Find works in text mode.

File mode opens a native file panel, then hashes the file in 1 MB chunks on a background thread. The row shows the file size and, while the read is in progress, how far it has gotten. A new hash cancels the one still reading.

```sh
DEVUTIL_TOOL=hash DEVUTIL_HASH_FILE=some.bin cargo run -p devutil
```

**Markdown.** The sidebar glyph is `M↓`. The left pane is the editor. The right pane is the rendered note: headings, paragraphs, bold, italic, strikethrough, inline code, links, quotes, lists (including nested lists and task checkboxes), fenced code with a language label and a Copy button, tables, rules, images, and footnotes. Paste, Copy HTML, Open, and Clear sit in the header, with a word and character count on the preview. An empty note offers a sample. ⌘F searches the editor. A large note is parsed off the UI thread. The preview keeps at most 2,000 blocks and 256 KB of text; Copy HTML still uses the whole note. `DEVUTIL_TOOL=markdown` opens this tool.

**Diff.** The sidebar glyph is `±`. Original and Changed are two editors side by side, each with Paste and Open. The header has Swap, Copy diff, Clear, and the primary Compare button. The result card shows `+added` and `−removed` line counts, the number of change blocks, and the line counts of both sides, or an Identical state. Side by side pairs a removed line with the added line that replaced it; Unified lists removed lines before added ones with both line numbers. Removed rows are red and added rows green, with a `-` or `+` marker and a tinted row behind the text; words that changed inside a paired line are semibold on a tinted background. Lines can be All or Changes (three lines of context, the rest folded into an `⋯ N unchanged lines` row). Ignore whitespace (compare with all whitespace removed) and Ignore case change how lines match, not the text shown. Format JSON parses both sides as strict JSON and compares them pretty-printed with sorted keys; when either side is not JSON, both are compared as text. Copy diff writes a `diff -u` style patch (`--- a/…`, `+++ b/…`, `@@` hunks, three lines of context) of the full texts, named after opened files. Edits wait 250 ms before comparing, inputs larger than 256 KB together are compared on a background thread, and each view builds at most 5,000 rows (the counts and the patch still cover everything). An empty Diff offers a synthetic sample. Diff has no find bar. `DEVUTIL_TOOL=diff` opens this tool.

## Migration report

Ported from the GPUI workbench (`gpui-kit` 0.6.6) onto NSH, now 0.3.0. The cores were copied as-is. The legacy Swift package was left behind.

`cargo test --workspace` covers the cores and the workbench models. The AppKit window was not opened here (this environment is Linux). `nsh package --target macos` was not run here; it does not need an `NSH_SOURCE` checkout. Use NSH CLI 0.3.0, matching the crates.

### What matches

- One window, sidebar switcher, light and dark workbench chrome, monospace fields.
- JSON paste, analyze, diagnostics, format, minify, copy, reset, tree, and apply-a-candidate.
- Timestamp unit and interpretation zone, four result cards, copy, clear, ⌘↩.
- URL decode/encode, structure and query rows, copy, clear, ⌘↩.
- Hash text and file digests, hex case, Base64, HMAC, compare, and copy.
- Edit menu (undo, cut, copy, paste, select all) from the NSH host. Clipboard has no extra capability flag in 0.1.1.

### What is missing or degraded, and why

- **Find uses the editor, not the system find bar.** NSH 0.3's macOS text input sets `selection_color` through `setSelectedTextAttributes`, but only while that view is focused, and `setDrawsBackground(false)` still keeps an unfocused selection from drawing. `set_selection` still does not call `scrollRangeToVisible`. A custom `NativeComponent` can wrap an `NSTextView` and call `scrollRangeToVisible` / `showFindIndicatorForRange`, but `nsh package` and `nsh run` do not register app components, so that view would be empty in the packaged app. The multiline fields are therefore NSH's `Editor`, which draws its own text, paints match backgrounds, and scrolls the current match into view while the find field keeps the keyboard focus. Text runs can take a background (`backgroundColor` on a nested text), used for the find-bar rows, the tree, and changed words in a diff. The editor counts at most 10,000 highlighted matches; the count under the bar can be higher. Single-line fields (the find query, HMAC key, compare, result cards) stay native inputs.
- **A single enormous line is still one row.** The editor virtualizes rows, so a long file scrolls like a short one. One line of many megabytes is still one row. URL's decoded result is a native multiline field, not the editor.
- **Object source ranges used to copy the whole prefix.** `StrictJSONParser::make_range` allocated `characters[..index]` into a `String` three times per object member, so a large array was quadratic. Ranges are now a UTF-16 offset table built once. The reported offsets are unchanged.
- **The whole string is still the value of a small field.** `.value` is controlled text. Each keystroke delivers the full string through `on_change_text`. There is no rope. Above the preview limit the view is not given that string.
- **Selection is per field.** 0.1.1 has no `SelectionArea`. Each result is its own text view, so a selection cannot cross cards. Readonly output uses `editable(false)` and stays selectable, so selecting and copying inside one field works. Binding `.selection` reapplies whenever the bound value changes and resets the caret, so the diagnostic jump selects that range in the editor and focuses it.
- **Return inserts a newline.** Multiline fields use submit behavior Newline. ⌘↩ is bound with `on_key_with(..., When::Always)` on the window so it runs before the field sees the key. Ctrl+Enter is bound the same way. ⌘Q, ⌘H, ⌥⌘H, ⌘M, and ⌃⌘F are reserved by the host. Quit stays on the application menu.
- **The tree is a virtual list, not a tree widget.** 0.1.1 has no tree view and no tooltip (`nsh-ui-mobile` has lists and text fields, not those). Rows are one line and ellipsize. A Copy control on the row copies the full label. `virtual_list` recycles cells, so the row index is read inside the binding, not captured when the cell is built. Child ids are `{parent}/{index}`. The GPUI tree used `{id}/{key}`, which collides when one object has two fields with the same key. Index ids avoid that. A string of JSON gets a nested node id `{id}/nested` that starts collapsed. An expanded collection lists the first 1,000 children; GPUI built a node for every value up front. Find filters that list to matching keys and values, expands their ancestors, and still stops at 1,000 matching children per collection. The matched substring is primary and semibold, on a tinted background. The current row uses the active list color; other matching rows use the selection color.
- **Markdown has no in-app browser.** A link with an `http` or `https` URL is opened with `open` (macOS), `xdg-open` (Linux), or `start` (Windows). Other URLs stay as text. An image is an NSH `image` when the URL is `http(s)`, a `data:image` URL, or a file that exists next to an opened note (or an absolute path). Anything else is the alt text. Task checkboxes are drawn, not toggled in the source. The preview is native views in the app theme, so it follows light and dark mode. It is not a web view.
- **Narrow windows squish the two columns.** Panes use `min_w(0)` and `h_full` so they fill the 1100×720 window. They do not stack. `flex_wrap` is set, but a full-height pane does not drop onto a second row.
- **No minimum window size and no dock icon on `AppOptions`.** The macOS options in 0.1.1 have a title, a size, and an appearance. `cargo run -p devutil` uses the process icon. The 1024 PNG is applied by `nsh package` from `nsh.json`.
- **`nsh-ui-mobile` is the wrong chrome.** Those controls are sized for touch. The toolbar, segments, and buttons are small desktop views in `app`.
- **Seeded samples are real text, not placeholders.** The first Convert on the untouched timestamp sample can fail (`Unsupported date format...`) because the field holds four lines. Decode on the untouched URL sample decodes the whole blob. Clearing the field shows the same text as a placeholder. The zone control does not redraw the Local card. That card is always the system zone. Local versus UTC only changes how an offset-less date is parsed.
- **Appearance follows the system.** `theme.json` defines a light palette and a dark palette. `nsh.json` does not force `"appearance": "light"`, and the window asks for `Appearance::System`. NSH 0.1.1 has no SF Symbols, so the sidebar marks are glyphs. A row cannot show its copy control only while the pointer is over it, so tree and digest rows keep a small muted copy mark. There is no syntax color for the editor itself; the tree tints keys, strings, numbers, and booleans from the label text.
- **NSH 0.1.1 has no file panel.** There is no `NSOpenPanel` or GTK file chooser in the 0.1.1 API. Hash's Open button uses the `rfd` crate, which presents `NSOpenPanel` on macOS (hopping to the main thread) and an XDG desktop-portal dialog on Linux. The panel is not reachable from `NSH_SCRIPT`. `DEVUTIL_HASH_FILE` starts a file hash without it.
- **Workers cannot write signals.** `spawn` runs on the engine thread. File and large-text hashes use `blocking` (`tokio::task::spawn_blocking`). Progress is an atomic counter the engine reads on a 100 ms timer.
- **Diff rows are views, not a text view.** A changed line is a tinted row, a `+` or `-` marker, and a colored run. A changed word is semibold on its own background. The rows are a `virtual_list`; long lines wrap. Selection works within one line, not across rows. The two editors hold the full sides. ⌘F does nothing on Diff: there are two editors and a row list, not one text.
- **No tooltip.** A matching digest is a green border, a Match label, and the status line. BLAKE3's keyed mode is not HMAC, so HMAC mode does not invent a BLAKE3 or CRC32 result.

### Recommendations

- Use `nsh run` while iterating, with the NSH CLI 0.3.0. Package with `nsh package --target macos`. `cargo run -p devutil` opens the same window without the NSH CLI. Do not set `NSH_SOURCE`.
- The crate versions are 0.3.0 (tag `v0.3.0`, commit `76fa8a7`). A later `main` commit was not required.
- Keep large-document work out of the stock text view. If the JSON editor needs a rope, highlighting, or a gutter, add a native view rather than stretching `NSTextView`.
- Keep the index-based tree ids. They are stricter than the GPUI ids and stay stable when keys repeat.
