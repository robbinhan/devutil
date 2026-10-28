//! Markdown parsed into a block model, plus HTML export.
//!
//! GitHub-flavored tables, task lists, strikethrough, and footnotes come from
//! `pulldown-cmark`. The workbench renders [`Block`] with native views. The
//! preview is capped so a multi-megabyte note cannot be typeset in full;
//! [`to_html`] is the whole document.

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Tag, TagEnd};

/// Nested blocks kept for the preview. Deeper notes set [`Document::truncated`].
pub const PREVIEW_BLOCK_CAP: usize = 2_000;

/// UTF-8 bytes of text kept for the preview.
pub const PREVIEW_BYTE_CAP: usize = 256 * 1024;

const TEXT_CAP: usize = 16 * 1024;
const MAX_DEPTH: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuoteKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl QuoteKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Tip => "Tip",
            Self::Important => "Important",
            Self::Warning => "Warning",
            Self::Caution => "Caution",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListItem {
    /// `Some` for a task-list checkbox.
    pub checked: Option<bool>,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Heading { level: u8, inlines: Vec<Inline> },
    Paragraph { inlines: Vec<Inline> },
    Quote { kind: Option<QuoteKind>, blocks: Vec<Block> },
    List { ordered: bool, start: u64, items: Vec<ListItem> },
    Code { language: String, text: String },
    Table {
        aligns: Vec<Align>,
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    Rule,
    Footnote { label: String, blocks: Vec<Block> },
    Html(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    SoftBreak,
    HardBreak,
    Code(String),
    Emphasis(Vec<Inline>),
    Strong(Vec<Inline>),
    Strike(Vec<Inline>),
    Link { dest: String, title: String, children: Vec<Inline> },
    Image { alt: String, url: String, title: String },
    Footnote(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    pub blocks: Vec<Block>,
    pub words: usize,
    pub chars: usize,
    /// The preview stopped early, or a single run was shortened.
    pub truncated: bool,
}

/// Where an image can be loaded from. Anything else is alt text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageLocation {
    Source(String),
    Missing,
}

pub fn parse(markdown: &str) -> Document {
    parse_limited(markdown, PREVIEW_BLOCK_CAP)
}

pub fn parse_limited(markdown: &str, max_blocks: usize) -> Document {
    let (words, chars) = counts(markdown);
    let parser = pulldown_cmark::Parser::new_ext(markdown, options()).peekable();
    let mut walk = Walk {
        events: parser,
        blocks_left: max_blocks,
        bytes_left: PREVIEW_BYTE_CAP,
        truncated: false,
        depth: 0,
    };
    let blocks = walk.blocks();
    Document {
        blocks,
        words,
        chars,
        truncated: walk.truncated,
    }
}

pub fn to_html(markdown: &str) -> String {
    let parser = pulldown_cmark::Parser::new_ext(markdown, options());
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    html
}

/// `http` and `https` only. Anything else stays as text.
pub fn browser_url(url: &str) -> Option<&str> {
    let url = url.trim();
    if url.starts_with("https://") || url.starts_with("http://") {
        Some(url)
    } else {
        None
    }
}

/// A remote image, a `data:image` URL, or a file that exists. Otherwise missing.
pub fn locate_image(base_dir: &str, url: &str) -> ImageLocation {
    let url = url.trim();
    if url.is_empty() || url.chars().any(char::is_control) {
        return ImageLocation::Missing;
    }
    if url.starts_with("https://") || url.starts_with("http://") {
        return ImageLocation::Source(url.to_string());
    }
    if let Some(rest) = url.strip_prefix("data:") {
        if rest.starts_with("image/") && url.len() <= 100_000 {
            return ImageLocation::Source(url.to_string());
        }
        return ImageLocation::Missing;
    }
    if let Some(path) = url.strip_prefix("file://") {
        let path = path.split('?').next().unwrap_or(path);
        let decoded = percent_decode_path(path);
        if std::path::Path::new(&decoded).is_file() {
            return ImageLocation::Source(decoded);
        }
        return ImageLocation::Missing;
    }
    if url.contains("://") {
        return ImageLocation::Missing;
    }
    let relative = std::path::Path::new(url);
    let path = if relative.is_absolute() || base_dir.is_empty() {
        relative.to_path_buf()
    } else {
        std::path::Path::new(base_dir).join(relative)
    };
    if path.is_file() {
        ImageLocation::Source(path.to_string_lossy().into_owned())
    } else {
        ImageLocation::Missing
    }
}

pub fn counts(text: &str) -> (usize, usize) {
    let mut chars = 0usize;
    let mut words = 0usize;
    let mut in_word = false;
    for ch in text.chars() {
        chars += 1;
        if ch.is_whitespace() {
            in_word = false;
        } else if !in_word {
            words += 1;
            in_word = true;
        }
    }
    (words, chars)
}

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM
}

fn percent_decode_path(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or(""),
                16,
            ) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| path.to_string())
}

struct Walk<'a> {
    events: std::iter::Peekable<pulldown_cmark::Parser<'a>>,
    blocks_left: usize,
    bytes_left: usize,
    truncated: bool,
    depth: usize,
}

enum Seen {
    End,
    Rule,
    Paragraph,
    Heading(u8),
    Quote(Option<QuoteKind>),
    Code { language: String },
    HtmlBlock,
    List { ordered: bool, start: u64 },
    Footnote(String),
    Table(Vec<Align>),
    Html(String),
    Text(String),
    Skip,
}

enum InlineSeen {
    End,
    Emphasis,
    Strong,
    Strike,
    Link { dest: String, title: String },
    Image { url: String, title: String },
    Text(String),
    Code(String),
    Soft,
    Hard,
    Footnote(String),
    Html(String),
    SkipStart,
    Skip,
}

impl<'a> Walk<'a> {
    fn blocks(&mut self) -> Vec<Block> {
        let mut out = Vec::new();
        loop {
            if self.blocks_left == 0 || self.bytes_left == 0 {
                self.truncated = true;
                self.drain_rest();
                break;
            }
            match self.classify() {
                Seen::End => break,
                Seen::Skip => {
                    let is_start = matches!(self.events.peek(), Some(Event::Start(_)));
                    self.events.next();
                    if is_start {
                        self.skip_until_end();
                    }
                }
                seen => {
                    if let Some(block) = self.consume(seen) {
                        out.push(block);
                    }
                }
            }
        }
        out
    }

    fn classify(&mut self) -> Seen {
        match self.events.peek() {
            None | Some(Event::End(_)) => Seen::End,
            Some(Event::Rule) => Seen::Rule,
            Some(Event::Start(Tag::Paragraph)) => Seen::Paragraph,
            Some(Event::Start(Tag::Heading { level, .. })) => Seen::Heading(heading_level(*level)),
            Some(Event::Start(Tag::BlockQuote(kind))) => Seen::Quote(quote_kind(*kind)),
            Some(Event::Start(Tag::CodeBlock(kind))) => Seen::Code {
                language: language_of(kind),
            },
            Some(Event::Start(Tag::HtmlBlock)) => Seen::HtmlBlock,
            Some(Event::Start(Tag::List(start))) => Seen::List {
                ordered: start.is_some(),
                start: start.unwrap_or(1),
            },
            Some(Event::Start(Tag::FootnoteDefinition(label))) => Seen::Footnote(label.to_string()),
            Some(Event::Start(Tag::Table(aligns))) => {
                Seen::Table(aligns.iter().copied().map(align_of).collect())
            }
            Some(Event::Html(html)) => Seen::Html(html.to_string()),
            Some(Event::Text(text)) => Seen::Text(text.to_string()),
            Some(_) => Seen::Skip,
        }
    }

    fn consume(&mut self, seen: Seen) -> Option<Block> {
        self.events.next();
        self.blocks_left = self.blocks_left.saturating_sub(1);
        Some(match seen {
            Seen::Rule => Block::Rule,
            Seen::Paragraph => Block::Paragraph { inlines: self.inlines() },
            Seen::Heading(level) => Block::Heading {
                level,
                inlines: self.inlines(),
            },
            Seen::Quote(kind) => self.quote(kind),
            Seen::Code { language } => self.code(language),
            Seen::HtmlBlock => Block::Html(self.raw_until_end()),
            Seen::List { ordered, start } => self.list(ordered, start),
            Seen::Footnote(label) => self.footnote(label),
            Seen::Table(aligns) => self.table(aligns),
            Seen::Html(html) => Block::Html(self.store(html)),
            Seen::Text(text) => Block::Paragraph {
                inlines: vec![Inline::Text(self.store(text))],
            },
            Seen::End | Seen::Skip => return None,
        })
    }

    fn quote(&mut self, kind: Option<QuoteKind>) -> Block {
        if !self.nest() {
            return Block::Quote { kind, blocks: Vec::new() };
        }
        let blocks = self.blocks();
        self.unnest();
        Block::Quote { kind, blocks }
    }

    fn footnote(&mut self, label: String) -> Block {
        let label = self.store(label);
        if !self.nest() {
            return Block::Footnote { label, blocks: Vec::new() };
        }
        let blocks = self.blocks();
        self.unnest();
        Block::Footnote { label, blocks }
    }

    fn list(&mut self, ordered: bool, start: u64) -> Block {
        if !self.nest() {
            return Block::List {
                ordered,
                start,
                items: Vec::new(),
            };
        }
        let mut items = Vec::new();
        while matches!(self.events.peek(), Some(Event::Start(Tag::Item))) {
            if self.blocks_left == 0 || self.bytes_left == 0 {
                self.truncated = true;
                self.drain_rest();
                break;
            }
            self.events.next();
            items.push(self.item());
        }
        self.unnest();
        Block::List { ordered, start, items }
    }

    fn item(&mut self) -> ListItem {
        if !self.nest() {
            return ListItem {
                checked: None,
                blocks: Vec::new(),
            };
        }
        let checked = match self.events.peek() {
            Some(Event::TaskListMarker(on)) => Some(*on),
            _ => None,
        };
        if checked.is_some() {
            self.events.next();
        }
        let blocks = self.blocks();
        self.unnest();
        ListItem { checked, blocks }
    }

    fn code(&mut self, language: String) -> Block {
        let mut text = String::new();
        loop {
            match self.events.peek() {
                Some(Event::End(TagEnd::CodeBlock)) | None => {
                    self.consume_end();
                    break;
                }
                Some(Event::Text(_)) => {
                    if let Some(Event::Text(chunk)) = self.events.next() {
                        text.push_str(&chunk);
                    }
                }
                Some(Event::SoftBreak) | Some(Event::HardBreak) => {
                    self.events.next();
                    text.push('\n');
                }
                Some(_) => {
                    self.events.next();
                }
            }
        }
        if text.ends_with('\n') {
            text.pop();
        }
        Block::Code {
            language: self.store(language),
            text: self.store(text),
        }
    }

    fn raw_until_end(&mut self) -> String {
        let mut text = String::new();
        loop {
            match self.events.peek() {
                Some(Event::End(_)) | None => {
                    self.consume_end();
                    break;
                }
                Some(Event::Html(_)) | Some(Event::Text(_)) => match self.events.next() {
                    Some(Event::Html(chunk)) | Some(Event::Text(chunk)) => text.push_str(&chunk),
                    _ => {}
                },
                Some(_) => {
                    self.events.next();
                }
            }
        }
        self.store(text)
    }

    fn table(&mut self, aligns: Vec<Align>) -> Block {
        let mut header = Vec::new();
        let mut rows = Vec::new();
        let mut current = Vec::new();
        let mut in_head = false;
        loop {
            if matches!(self.events.peek(), Some(Event::End(TagEnd::Table)) | None) {
                self.consume_end();
                break;
            }
            if matches!(self.events.peek(), Some(Event::Start(Tag::TableHead))) {
                self.events.next();
                in_head = true;
                current.clear();
                continue;
            }
            if matches!(self.events.peek(), Some(Event::End(TagEnd::TableHead))) {
                self.events.next();
                if header.is_empty() {
                    header = std::mem::take(&mut current);
                }
                in_head = false;
                continue;
            }
            if matches!(self.events.peek(), Some(Event::Start(Tag::TableRow))) {
                self.events.next();
                current.clear();
                continue;
            }
            if matches!(self.events.peek(), Some(Event::End(TagEnd::TableRow))) {
                self.events.next();
                let row = std::mem::take(&mut current);
                if in_head && header.is_empty() {
                    header = row;
                } else {
                    rows.push(row);
                }
                continue;
            }
            if matches!(self.events.peek(), Some(Event::Start(Tag::TableCell))) {
                self.events.next();
                if self.bytes_left == 0 {
                    self.truncated = true;
                    let _ = self.inlines();
                    continue;
                }
                current.push(self.inlines());
                continue;
            }
            let is_start = matches!(self.events.peek(), Some(Event::Start(_)));
            self.events.next();
            if is_start {
                self.skip_until_end();
            }
        }
        Block::Table { aligns, header, rows }
    }

    fn inlines(&mut self) -> Vec<Inline> {
        let mut out = Vec::new();
        loop {
            match self.classify_inline() {
                InlineSeen::End => {
                    self.events.next();
                    break;
                }
                InlineSeen::Emphasis => {
                    self.events.next();
                    out.push(Inline::Emphasis(self.inlines()));
                }
                InlineSeen::Strong => {
                    self.events.next();
                    out.push(Inline::Strong(self.inlines()));
                }
                InlineSeen::Strike => {
                    self.events.next();
                    out.push(Inline::Strike(self.inlines()));
                }
                InlineSeen::Link { dest, title } => {
                    self.events.next();
                    out.push(Inline::Link {
                        dest: self.store(dest),
                        title: self.store(title),
                        children: self.inlines(),
                    });
                }
                InlineSeen::Image { url, title } => {
                    self.events.next();
                    let alt = inline_text(&self.inlines());
                    out.push(Inline::Image {
                        alt: self.store(alt),
                        url: self.store(url),
                        title: self.store(title),
                    });
                }
                InlineSeen::Text(text) => {
                    self.events.next();
                    push_text(&mut out, self.store(text));
                }
                InlineSeen::Code(text) => {
                    self.events.next();
                    out.push(Inline::Code(self.store(text)));
                }
                InlineSeen::Soft => {
                    self.events.next();
                    out.push(Inline::SoftBreak);
                }
                InlineSeen::Hard => {
                    self.events.next();
                    out.push(Inline::HardBreak);
                }
                InlineSeen::Footnote(label) => {
                    self.events.next();
                    out.push(Inline::Footnote(self.store(label)));
                }
                InlineSeen::Html(html) => {
                    self.events.next();
                    push_text(&mut out, self.store(html));
                }
                InlineSeen::SkipStart => {
                    self.events.next();
                    self.skip_until_end();
                }
                InlineSeen::Skip => {
                    self.events.next();
                }
            }
        }
        out
    }

    fn classify_inline(&mut self) -> InlineSeen {
        match self.events.peek() {
            None | Some(Event::End(_)) => InlineSeen::End,
            Some(Event::Start(Tag::Emphasis)) => InlineSeen::Emphasis,
            Some(Event::Start(Tag::Strong)) => InlineSeen::Strong,
            Some(Event::Start(Tag::Strikethrough)) => InlineSeen::Strike,
            Some(Event::Start(Tag::Link { dest_url, title, .. })) => InlineSeen::Link {
                dest: dest_url.to_string(),
                title: title.to_string(),
            },
            Some(Event::Start(Tag::Image { dest_url, title, .. })) => InlineSeen::Image {
                url: dest_url.to_string(),
                title: title.to_string(),
            },
            Some(Event::Text(text)) => InlineSeen::Text(text.to_string()),
            Some(Event::Code(text)) => InlineSeen::Code(text.to_string()),
            Some(Event::SoftBreak) => InlineSeen::Soft,
            Some(Event::HardBreak) => InlineSeen::Hard,
            Some(Event::FootnoteReference(label)) => InlineSeen::Footnote(label.to_string()),
            Some(Event::Html(html)) | Some(Event::InlineHtml(html)) => InlineSeen::Html(html.to_string()),
            Some(Event::Start(_)) => InlineSeen::SkipStart,
            Some(_) => InlineSeen::Skip,
        }
    }

    fn nest(&mut self) -> bool {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            self.truncated = true;
            self.skip_until_end();
            self.depth -= 1;
            return false;
        }
        true
    }

    fn unnest(&mut self) {
        self.consume_end();
        self.depth = self.depth.saturating_sub(1);
    }

    fn consume_end(&mut self) {
        if matches!(self.events.peek(), Some(Event::End(_))) {
            self.events.next();
        }
    }

    /// Drop events until the `End` that closes the container we are inside.
    fn skip_until_end(&mut self) {
        let mut depth = 1;
        while depth > 0 {
            match self.events.next() {
                Some(Event::Start(_)) => depth += 1,
                Some(Event::End(_)) => depth -= 1,
                None => break,
                Some(_) => {}
            }
        }
    }

    /// Drop the rest of this container, leaving its `End` for the caller.
    fn drain_rest(&mut self) {
        let mut depth = 0;
        loop {
            match self.events.peek() {
                None => break,
                Some(Event::End(_)) if depth == 0 => break,
                Some(Event::End(_)) => {
                    depth -= 1;
                    self.events.next();
                }
                Some(Event::Start(_)) => {
                    depth += 1;
                    self.events.next();
                }
                Some(_) => {
                    self.events.next();
                }
            }
        }
    }

    fn store(&mut self, mut text: String) -> String {
        if text.len() > TEXT_CAP {
            self.truncated = true;
            let mut end = TEXT_CAP;
            while end > 0 && !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
            text.push('…');
        }
        if text.len() > self.bytes_left {
            self.truncated = true;
            self.bytes_left = 0;
        } else {
            self.bytes_left -= text.len();
        }
        text
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn quote_kind(kind: Option<pulldown_cmark::BlockQuoteKind>) -> Option<QuoteKind> {
    match kind {
        Some(pulldown_cmark::BlockQuoteKind::Note) => Some(QuoteKind::Note),
        Some(pulldown_cmark::BlockQuoteKind::Tip) => Some(QuoteKind::Tip),
        Some(pulldown_cmark::BlockQuoteKind::Important) => Some(QuoteKind::Important),
        Some(pulldown_cmark::BlockQuoteKind::Warning) => Some(QuoteKind::Warning),
        Some(pulldown_cmark::BlockQuoteKind::Caution) => Some(QuoteKind::Caution),
        None => None,
    }
}

fn align_of(align: Alignment) -> Align {
    match align {
        Alignment::Left => Align::Left,
        Alignment::Center => Align::Center,
        Alignment::Right => Align::Right,
        Alignment::None => Align::None,
    }
}

fn language_of(kind: &CodeBlockKind<'_>) -> String {
    match kind {
        CodeBlockKind::Indented => String::new(),
        CodeBlockKind::Fenced(info) => info.split_whitespace().next().unwrap_or("").to_string(),
    }
}

fn push_text(out: &mut Vec<Inline>, text: String) {
    if text.is_empty() {
        return;
    }
    if let Some(Inline::Text(existing)) = out.last_mut() {
        existing.push_str(&text);
    } else {
        out.push(Inline::Text(text));
    }
}

fn inline_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(text) | Inline::Code(text) => out.push_str(text),
            Inline::SoftBreak | Inline::HardBreak => out.push(' '),
            Inline::Emphasis(children)
            | Inline::Strong(children)
            | Inline::Strike(children)
            | Inline::Link { children, .. } => out.push_str(&inline_text(children)),
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::Footnote(label) => {
                out.push('[');
                out.push_str(label);
                out.push(']');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_is_an_empty_document() {
        let doc = parse("");
        assert!(doc.blocks.is_empty());
        assert_eq!((doc.words, doc.chars, doc.truncated), (0, 0, false));
    }

    #[test]
    fn counts_words_and_characters() {
        assert_eq!(counts("one  two\n"), (2, 9));
    }

    #[test]
    fn headings_emphasis_code_and_a_link() {
        let doc = parse("# Title\n\nA **bold** *em* `code` [Docs](https://example.com \"t\").");
        assert!(matches!(doc.blocks[0], Block::Heading { level: 1, .. }));
        let Block::Paragraph { inlines } = &doc.blocks[1] else {
            panic!("paragraph");
        };
        assert!(inlines.iter().any(|inline| matches!(inline, Inline::Strong(_))));
        assert!(inlines.iter().any(|inline| matches!(inline, Inline::Emphasis(_))));
        assert!(inlines.iter().any(|inline| matches!(inline, Inline::Code(text) if text == "code")));
        assert!(inlines.iter().any(|inline| matches!(
            inline,
            Inline::Link { dest, title, .. } if dest == "https://example.com" && title == "t"
        )));
    }

    #[test]
    fn gfm_table_task_list_strike_footnote_and_quote() {
        let md = "\
| a | b |
| :--- | ---: |
| 1 | 2 |

- [x] done
- [ ] todo
  - nested

~~gone~~

> [!NOTE]
> quoted

See[^n].

[^n]: The note.

```rust
fn main() {}
```

---
";
        let doc = parse(md);
        let Block::Table { aligns, header, rows } = &doc.blocks[0] else {
            panic!("table: {:?}", doc.blocks[0]);
        };
        assert_eq!(aligns, &[Align::Left, Align::Right]);
        assert_eq!(inline_text(&header[0]), "a");
        assert_eq!(inline_text(&rows[0][1]), "2");

        let Block::List { ordered, items, .. } = &doc.blocks[1] else {
            panic!("list");
        };
        assert!(!ordered);
        assert_eq!(items[0].checked, Some(true));
        assert_eq!(items[1].checked, Some(false));
        assert!(items[1].blocks.iter().any(|block| matches!(block, Block::List { .. })));

        assert!(doc.blocks.iter().any(|block| matches!(
            block,
            Block::Paragraph { inlines } if inlines.iter().any(|inline| matches!(inline, Inline::Strike(_)))
        )));
        assert!(doc.blocks.iter().any(|block| matches!(
            block,
            Block::Quote { kind: Some(QuoteKind::Note), .. }
        )));
        assert!(doc.blocks.iter().any(|block| matches!(
            block,
            Block::Footnote { label, .. } if label == "n"
        )));
        assert!(doc.blocks.iter().any(|block| matches!(
            block,
            Block::Code { language, text } if language == "rust" && text.contains("fn main")
        )));
        assert!(doc.blocks.iter().any(|block| matches!(block, Block::Rule)));

        let html = to_html(md);
        assert!(html.contains("<h1>") || html.contains("<table>"));
        assert!(html.contains("<table>"));
        assert!(html.contains("<del>") || html.contains("<s>"));
        assert!(html.contains("checkbox"));
        assert!(html.contains("checked"));
        assert!(html.contains("<blockquote"));
        assert!(html.contains("<pre"));
        assert!(html.contains("fn main"));
    }

    #[test]
    fn h6_image_and_ordered_list() {
        let doc = parse("###### tiny\n\n1. first\n2. second\n\n![Alt](https://example.com/a.png)\n");
        assert!(matches!(doc.blocks[0], Block::Heading { level: 6, .. }));
        let Block::List { ordered, start, items } = &doc.blocks[1] else {
            panic!("ordered");
        };
        assert!(*ordered);
        assert_eq!(*start, 1);
        assert_eq!(items.len(), 2);
        assert!(doc.blocks.iter().any(|block| matches!(
            block,
            Block::Paragraph { inlines } if inlines.iter().any(|inline| matches!(
                inline,
                Inline::Image { alt, url, .. } if alt == "Alt" && url == "https://example.com/a.png"
            ))
        )));
    }

    #[test]
    fn the_preview_stops_at_the_block_cap() {
        let md = (0..20).map(|index| format!("para {index}\n\n")).collect::<String>();
        let doc = parse_limited(&md, 3);
        assert!(doc.truncated);
        assert!(doc.blocks.len() <= 3);
        assert_eq!(doc.words, 40);
    }

    #[test]
    fn a_huge_code_block_is_shortened() {
        let body = "a".repeat(TEXT_CAP + 50);
        let md = format!("```\n{body}\n```\n");
        let doc = parse(&md);
        let Block::Code { text, .. } = &doc.blocks[0] else {
            panic!("code");
        };
        assert!(doc.truncated);
        assert!(text.len() <= TEXT_CAP + "…".len());
        assert!(text.ends_with('…'));
    }

    #[test]
    fn the_byte_budget_stops_a_long_preview() {
        let md = format!("{}\n\n", "a".repeat(1000)).repeat(400);
        let doc = parse(&md);
        assert!(doc.truncated);
        assert!(doc.blocks.len() < 400);
    }

    #[test]
    fn browser_urls_are_http_only() {
        assert_eq!(browser_url(" https://example.com/a "), Some("https://example.com/a"));
        assert_eq!(browser_url("http://example.com"), Some("http://example.com"));
        assert_eq!(browser_url("javascript:alert(1)"), None);
        assert_eq!(browser_url("file:///etc/passwd"), None);
        assert_eq!(browser_url("notes.md"), None);
    }

    #[test]
    fn locate_image_accepts_http_and_a_real_file() {
        assert!(matches!(
            locate_image("", "https://example.com/a.png"),
            ImageLocation::Source(_)
        ));
        assert!(matches!(locate_image("", "javascript:alert(1)"), ImageLocation::Missing));
        assert!(matches!(locate_image("", "../missing.png"), ImageLocation::Missing));
        let path = std::env::temp_dir().join(format!("devutil-md-{}.png", std::process::id()));
        std::fs::write(&path, b"png").unwrap();
        let parent = path.parent().unwrap().to_str().unwrap();
        let name = path.file_name().unwrap().to_str().unwrap();
        assert_eq!(
            locate_image(parent, name),
            ImageLocation::Source(path.to_string_lossy().into_owned())
        );
        let _ = std::fs::remove_file(&path);
    }

}
