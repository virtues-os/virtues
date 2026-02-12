//! Markdown to Yjs XmlFragment parser
//!
//! Converts Markdown text into a Yjs XmlFragment structure compatible with
//! y-prosemirror and ProseMirror's document model.
//!
//! Handles detection of:
//! - Entity links: [label](/person/id), [label](/page/id), etc.
//! - File cards: [name](/drive/id)
//! - Media type by extension: ![alt](file.mp3) → audio, ![alt](file.mp4) → video
//! - Checkboxes as list_item attributes: - [ ] / - [x]
//! - Underline via HTML: <u>text</u>

use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use yrs::{Any, Text, TransactionMut, Xml, XmlElementPrelim, XmlFragment, XmlFragmentRef, XmlTextPrelim};

use super::Result;

// =============================================================================
// URL Detection
// =============================================================================

const ENTITY_PREFIXES: &[&str] = &[
    "/person/", "/place/", "/org/", "/page/", "/day/", "/year/", "/source/", "/chat/",
];

fn is_entity_url(url: &str) -> bool {
    ENTITY_PREFIXES.iter().any(|p| url.starts_with(p))
}

fn is_drive_url(url: &str) -> bool {
    url.starts_with("/drive/")
}

/// Detect media type from file extension
fn detect_media_type(src: &str) -> MediaType {
    let ext = src.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "mp3" | "wav" | "m4a" | "ogg" | "flac" | "aac" | "wma" => MediaType::Audio,
        "mp4" | "mov" | "webm" | "avi" | "mkv" | "m4v" | "wmv" => MediaType::Video,
        _ => MediaType::Image,
    }
}

// =============================================================================
// AST Types
// =============================================================================

/// Intermediate AST node representation
#[derive(Debug, Clone)]
enum AstNode {
    Paragraph { children: Vec<InlineContent> },
    Heading { level: u8, children: Vec<InlineContent> },
    BlockQuote { children: Vec<AstNode> },
    CodeBlock { language: String, content: String },
    BulletList { items: Vec<ListItem> },
    OrderedList { start: u64, items: Vec<ListItem> },
    HorizontalRule,
    Table { rows: Vec<TableRow>, alignments: Vec<Alignment> },
    Media { src: String, alt: String, title: Option<String>, media_type: MediaType },
}

#[derive(Debug, Clone)]
#[derive(PartialEq)]
enum MediaType {
    Image,
    Audio,
    Video,
}

impl MediaType {
    fn as_str(&self) -> &'static str {
        match self {
            MediaType::Image => "image",
            MediaType::Audio => "audio",
            MediaType::Video => "video",
        }
    }
}

#[derive(Debug, Clone)]
struct ListItem {
    checked: Option<bool>,
    children: Vec<AstNode>,
}

#[derive(Debug, Clone)]
struct TableRow {
    cells: Vec<TableCell>,
    is_header: bool,
}

#[derive(Debug, Clone)]
struct TableCell {
    content: Vec<InlineContent>,
    align: Option<Alignment>,
}

#[derive(Debug, Clone)]
enum InlineContent {
    Text { text: String, marks: Vec<Mark> },
    HardBreak,
    EntityLink { href: String, label: String },
    FileCard { href: String, name: String },
}

#[derive(Debug, Clone, PartialEq)]
enum Mark {
    Strong,
    Em,
    Code,
    Strikethrough,
    Underline,
    Link { href: String, title: Option<String> },
}

// =============================================================================
// Special Contexts
// =============================================================================

/// Tracks when we're inside a special link (entity or file card)
#[derive(Debug)]
enum SpecialLink {
    EntityLink { href: String, accumulated_text: String },
    FileCard { href: String, accumulated_text: String },
}

/// Tracks when we're inside an image/media tag, accumulating alt text
#[derive(Debug)]
struct MediaContext {
    src: String,
    title: Option<String>,
    accumulated_alt: String,
}

// =============================================================================
// Public API
// =============================================================================

/// Convert markdown text to Yjs XmlFragment nodes
///
/// This function parses markdown and creates XmlFragment nodes that are compatible
/// with y-prosemirror's expected structure. The fragment should be the "content"
/// fragment from a Yjs document.
pub fn markdown_to_xml_fragment(
    txn: &mut TransactionMut,
    fragment: XmlFragmentRef,
    markdown: &str,
) -> Result<()> {
    // Configure parser with GFM features
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES;

    let parser = Parser::new_ext(markdown, options);

    // Convert to a flat list of events
    let events: Vec<Event> = parser.collect();

    // Build intermediate representation first
    let nodes = build_ast(&events);

    // Convert AST to yrs nodes
    for node in nodes {
        insert_node(txn, fragment.clone(), &node);
    }

    Ok(())
}

// =============================================================================
// AST Builder
// =============================================================================

/// Build AST from pulldown-cmark events
fn build_ast(events: &[Event]) -> Vec<AstNode> {
    let mut builder = AstBuilder::new();
    for event in events {
        builder.handle_event(event);
    }
    builder.finish()
}

enum NodeBuilder {
    Document(Vec<AstNode>),
    Paragraph(Vec<InlineContent>),
    Heading { level: u8, children: Vec<InlineContent> },
    BlockQuote(Vec<AstNode>),
    CodeBlock { language: String, content: String },
    BulletList(Vec<ListItem>),
    OrderedList { start: u64, items: Vec<ListItem> },
    ListItem { checked: Option<bool>, children: Vec<AstNode>, inline_content: Vec<InlineContent> },
    Table { rows: Vec<TableRow>, alignments: Vec<Alignment> },
    TableRow { cells: Vec<TableCell>, is_header: bool },
    TableCell { content: Vec<InlineContent>, align: Option<Alignment> },
}

struct AstBuilder {
    /// Stack of nodes being built
    node_stack: Vec<NodeBuilder>,
    /// Current active marks for inline text
    active_marks: Vec<Mark>,
    /// Table state
    table_alignments: Vec<Alignment>,
    in_table_header: bool,
    /// Special link context (entity_link or file_card)
    special_link: Option<SpecialLink>,
    /// Media context for accumulating alt text
    media_context: Option<MediaContext>,
    /// Buffered media node (when image appears inside a paragraph)
    pending_media: Option<AstNode>,
}

impl AstBuilder {
    fn new() -> Self {
        Self {
            node_stack: vec![NodeBuilder::Document(vec![])],
            active_marks: vec![],
            table_alignments: vec![],
            in_table_header: false,
            special_link: None,
            media_context: None,
            pending_media: None,
        }
    }

    fn handle_event(&mut self, event: &Event) {
        match event {
            // ===== BLOCK ELEMENTS =====
            Event::Start(Tag::Paragraph) => {
                self.node_stack.push(NodeBuilder::Paragraph(vec![]));
            }
            Event::End(TagEnd::Paragraph) => {
                if let Some(NodeBuilder::Paragraph(children)) = self.node_stack.pop() {
                    if let Some(media) = self.pending_media.take() {
                        // Image was inside this paragraph
                        if children.is_empty() {
                            // Standalone image — promote to block-level media
                            self.push_block(media);
                        } else {
                            // Mixed content — paragraph + media as separate blocks
                            self.push_block(AstNode::Paragraph { children });
                            self.push_block(media);
                        }
                    } else {
                        self.push_block(AstNode::Paragraph { children });
                    }
                }
            }

            Event::Start(Tag::Heading { level, .. }) => {
                self.node_stack.push(NodeBuilder::Heading {
                    level: *level as u8,
                    children: vec![],
                });
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(NodeBuilder::Heading { level, children }) = self.node_stack.pop() {
                    self.push_block(AstNode::Heading { level, children });
                }
            }

            Event::Start(Tag::BlockQuote) => {
                self.node_stack.push(NodeBuilder::BlockQuote(vec![]));
            }
            Event::End(TagEnd::BlockQuote) => {
                if let Some(NodeBuilder::BlockQuote(children)) = self.node_stack.pop() {
                    self.push_block(AstNode::BlockQuote { children });
                }
            }

            Event::Start(Tag::CodeBlock(kind)) => {
                let language = match kind {
                    CodeBlockKind::Fenced(lang) => lang.to_string(),
                    CodeBlockKind::Indented => String::new(),
                };
                self.node_stack.push(NodeBuilder::CodeBlock {
                    language,
                    content: String::new(),
                });
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(NodeBuilder::CodeBlock { language, content }) = self.node_stack.pop() {
                    self.push_block(AstNode::CodeBlock { language, content });
                }
            }

            Event::Rule => {
                self.push_block(AstNode::HorizontalRule);
            }

            // ===== LISTS =====
            Event::Start(Tag::List(start)) => {
                match start {
                    Some(n) => {
                        self.node_stack.push(NodeBuilder::OrderedList {
                            start: *n,
                            items: vec![],
                        });
                    }
                    None => {
                        self.node_stack.push(NodeBuilder::BulletList(vec![]));
                    }
                }
            }
            Event::End(TagEnd::List(ordered)) => {
                if *ordered {
                    if let Some(NodeBuilder::OrderedList { start, items }) = self.node_stack.pop() {
                        self.push_block(AstNode::OrderedList { start, items });
                    }
                } else if let Some(NodeBuilder::BulletList(items)) = self.node_stack.pop() {
                    self.push_block(AstNode::BulletList { items });
                }
            }

            Event::Start(Tag::Item) => {
                self.node_stack.push(NodeBuilder::ListItem {
                    checked: None,
                    children: vec![],
                    inline_content: vec![],
                });
            }
            Event::End(TagEnd::Item) => {
                if let Some(NodeBuilder::ListItem { checked, mut children, inline_content }) = self.node_stack.pop() {
                    // For tight lists, pulldown-cmark sends text directly without paragraph wrapper
                    // Wrap inline content in an implicit paragraph
                    if !inline_content.is_empty() {
                        children.insert(0, AstNode::Paragraph { children: inline_content });
                    }
                    // Safety: consume any pending media that wasn't caught by End(Paragraph)
                    if let Some(media) = self.pending_media.take() {
                        children.push(media);
                    }
                    self.push_list_item(ListItem { checked, children });
                }
            }

            Event::TaskListMarker(checked) => {
                // Set the checked state on the current list item
                if let Some(NodeBuilder::ListItem { checked: c, .. }) = self.node_stack.last_mut() {
                    *c = Some(*checked);
                }
            }

            // ===== TABLES =====
            Event::Start(Tag::Table(alignments)) => {
                self.table_alignments = alignments.to_vec();
                self.node_stack.push(NodeBuilder::Table {
                    rows: vec![],
                    alignments: alignments.to_vec(),
                });
            }
            Event::End(TagEnd::Table) => {
                if let Some(NodeBuilder::Table { rows, alignments }) = self.node_stack.pop() {
                    self.push_block(AstNode::Table { rows, alignments });
                }
                self.table_alignments.clear();
            }

            Event::Start(Tag::TableHead) => {
                self.in_table_header = true;
                self.node_stack.push(NodeBuilder::TableRow {
                    cells: vec![],
                    is_header: true,
                });
            }
            Event::End(TagEnd::TableHead) => {
                self.in_table_header = false;
                if let Some(NodeBuilder::TableRow { cells, is_header }) = self.node_stack.pop() {
                    self.push_table_row(TableRow { cells, is_header });
                }
            }

            Event::Start(Tag::TableRow) => {
                self.node_stack.push(NodeBuilder::TableRow {
                    cells: vec![],
                    is_header: false,
                });
            }
            Event::End(TagEnd::TableRow) => {
                if let Some(NodeBuilder::TableRow { cells, is_header }) = self.node_stack.pop() {
                    self.push_table_row(TableRow { cells, is_header });
                }
            }

            Event::Start(Tag::TableCell) => {
                let cell_idx = self.current_cell_index();
                let align = self.table_alignments.get(cell_idx).copied();
                self.node_stack.push(NodeBuilder::TableCell {
                    content: vec![],
                    align,
                });
            }
            Event::End(TagEnd::TableCell) => {
                if let Some(NodeBuilder::TableCell { content, align }) = self.node_stack.pop() {
                    self.push_table_cell(TableCell { content, align });
                }
            }

            // ===== INLINE MARKS =====
            Event::Start(Tag::Strong) => {
                self.active_marks.push(Mark::Strong);
            }
            Event::End(TagEnd::Strong) => {
                self.remove_mark(|m| matches!(m, Mark::Strong));
            }

            Event::Start(Tag::Emphasis) => {
                self.active_marks.push(Mark::Em);
            }
            Event::End(TagEnd::Emphasis) => {
                self.remove_mark(|m| matches!(m, Mark::Em));
            }

            Event::Start(Tag::Strikethrough) => {
                self.active_marks.push(Mark::Strikethrough);
            }
            Event::End(TagEnd::Strikethrough) => {
                self.remove_mark(|m| matches!(m, Mark::Strikethrough));
            }

            // ===== LINKS (with entity/drive detection) =====
            Event::Start(Tag::Link { dest_url, title, .. }) => {
                let url = dest_url.to_string();
                if is_entity_url(&url) {
                    // Entity link: accumulate text as label
                    self.special_link = Some(SpecialLink::EntityLink {
                        href: url,
                        accumulated_text: String::new(),
                    });
                } else if is_drive_url(&url) {
                    // Drive file card: accumulate text as name
                    self.special_link = Some(SpecialLink::FileCard {
                        href: url,
                        accumulated_text: String::new(),
                    });
                } else {
                    // Regular link mark
                    self.active_marks.push(Mark::Link {
                        href: url,
                        title: if title.is_empty() { None } else { Some(title.to_string()) },
                    });
                }
            }
            Event::End(TagEnd::Link) => {
                if let Some(special) = self.special_link.take() {
                    match special {
                        SpecialLink::EntityLink { href, accumulated_text } => {
                            self.push_inline(InlineContent::EntityLink {
                                href,
                                label: accumulated_text,
                            });
                        }
                        SpecialLink::FileCard { href, accumulated_text } => {
                            self.push_inline(InlineContent::FileCard {
                                href,
                                name: accumulated_text,
                            });
                        }
                    }
                } else {
                    self.remove_mark(|m| matches!(m, Mark::Link { .. }));
                }
            }

            // ===== IMAGES/MEDIA =====
            Event::Start(Tag::Image { dest_url, title, .. }) => {
                // Buffer alt text — we'll create the media node on End(Image)
                self.media_context = Some(MediaContext {
                    src: dest_url.to_string(),
                    title: if title.is_empty() { None } else { Some(title.to_string()) },
                    accumulated_alt: String::new(),
                });
            }
            Event::End(TagEnd::Image) => {
                if let Some(ctx) = self.media_context.take() {
                    let mut media_type = detect_media_type(&ctx.src);
                    // If src has no recognizable extension, try the alt text (filename)
                    if media_type == MediaType::Image {
                        let alt_type = detect_media_type(&ctx.accumulated_alt);
                        if alt_type != MediaType::Image {
                            media_type = alt_type;
                        }
                    }
                    let media = AstNode::Media {
                        src: ctx.src,
                        alt: ctx.accumulated_alt,
                        title: ctx.title,
                        media_type,
                    };
                    // Try to push as a block node
                    if let Some(node) = self.try_push_block(media) {
                        // Inside a paragraph or other non-block container — buffer for later
                        self.pending_media = Some(node);
                    }
                }
            }

            // ===== INLINE CONTENT =====
            Event::Text(text) => {
                // Route text to the appropriate accumulator
                if let Some(ref mut ctx) = self.media_context {
                    ctx.accumulated_alt.push_str(text);
                } else if let Some(ref mut special) = self.special_link {
                    match special {
                        SpecialLink::EntityLink { accumulated_text, .. } |
                        SpecialLink::FileCard { accumulated_text, .. } => {
                            accumulated_text.push_str(text);
                        }
                    }
                } else {
                    self.push_inline(InlineContent::Text {
                        text: text.to_string(),
                        marks: self.active_marks.clone(),
                    });
                }
            }

            Event::Code(text) => {
                if let Some(ref mut ctx) = self.media_context {
                    ctx.accumulated_alt.push_str(text);
                } else if let Some(ref mut special) = self.special_link {
                    match special {
                        SpecialLink::EntityLink { accumulated_text, .. } |
                        SpecialLink::FileCard { accumulated_text, .. } => {
                            accumulated_text.push_str(text);
                        }
                    }
                } else {
                    let mut marks = self.active_marks.clone();
                    marks.push(Mark::Code);
                    self.push_inline(InlineContent::Text {
                        text: text.to_string(),
                        marks,
                    });
                }
            }

            Event::SoftBreak => {
                if self.media_context.is_none() && self.special_link.is_none() {
                    self.push_inline(InlineContent::Text {
                        text: " ".to_string(),
                        marks: self.active_marks.clone(),
                    });
                }
            }

            Event::HardBreak => {
                if self.media_context.is_none() && self.special_link.is_none() {
                    self.push_inline(InlineContent::HardBreak);
                }
            }

            // ===== INLINE HTML (for <u> underline) =====
            Event::InlineHtml(html) => {
                let tag = html.trim();
                if tag.eq_ignore_ascii_case("<u>") {
                    self.active_marks.push(Mark::Underline);
                } else if tag.eq_ignore_ascii_case("</u>") {
                    self.remove_mark(|m| matches!(m, Mark::Underline));
                }
                // Other inline HTML is ignored
            }

            _ => {}
        }
    }

    fn push_block(&mut self, node: AstNode) {
        match self.node_stack.last_mut() {
            Some(NodeBuilder::Document(nodes)) => nodes.push(node),
            Some(NodeBuilder::BlockQuote(nodes)) => nodes.push(node),
            Some(NodeBuilder::ListItem { children, .. }) => children.push(node),
            _ => {}
        }
    }

    /// Try to push a block node. Returns the node if the current context doesn't accept blocks.
    fn try_push_block(&mut self, node: AstNode) -> Option<AstNode> {
        match self.node_stack.last_mut() {
            Some(NodeBuilder::Document(nodes)) => { nodes.push(node); None }
            Some(NodeBuilder::BlockQuote(nodes)) => { nodes.push(node); None }
            Some(NodeBuilder::ListItem { children, .. }) => { children.push(node); None }
            _ => Some(node)
        }
    }

    fn push_inline(&mut self, inline: InlineContent) {
        match self.node_stack.last_mut() {
            Some(NodeBuilder::Paragraph(children)) => children.push(inline),
            Some(NodeBuilder::Heading { children, .. }) => children.push(inline),
            Some(NodeBuilder::TableCell { content, .. }) => content.push(inline),
            Some(NodeBuilder::CodeBlock { content, .. }) => {
                if let InlineContent::Text { text, .. } = inline {
                    content.push_str(&text);
                }
            }
            // Handle tight lists where text comes directly in list item without paragraph wrapper
            Some(NodeBuilder::ListItem { inline_content, .. }) => inline_content.push(inline),
            _ => {}
        }
    }

    fn push_list_item(&mut self, item: ListItem) {
        match self.node_stack.last_mut() {
            Some(NodeBuilder::BulletList(items)) => items.push(item),
            Some(NodeBuilder::OrderedList { items, .. }) => items.push(item),
            _ => {}
        }
    }

    fn push_table_row(&mut self, row: TableRow) {
        if let Some(NodeBuilder::Table { rows, .. }) = self.node_stack.last_mut() {
            rows.push(row);
        }
    }

    fn push_table_cell(&mut self, cell: TableCell) {
        if let Some(NodeBuilder::TableRow { cells, .. }) = self.node_stack.last_mut() {
            cells.push(cell);
        }
    }

    fn current_cell_index(&self) -> usize {
        for node in self.node_stack.iter().rev() {
            if let NodeBuilder::TableRow { cells, .. } = node {
                return cells.len();
            }
        }
        0
    }

    fn remove_mark<F>(&mut self, predicate: F)
    where
        F: Fn(&Mark) -> bool,
    {
        if let Some(pos) = self.active_marks.iter().rposition(|m| predicate(m)) {
            self.active_marks.remove(pos);
        }
    }

    fn finish(mut self) -> Vec<AstNode> {
        // Drain any pending media that wasn't consumed
        if let Some(media) = self.pending_media.take() {
            self.push_block(media);
        }
        if let Some(NodeBuilder::Document(nodes)) = self.node_stack.pop() {
            nodes
        } else {
            vec![]
        }
    }
}

// ============================================================================
// Yrs Node Insertion
// ============================================================================

/// Insert an AST node into the fragment
fn insert_node(txn: &mut TransactionMut, fragment: XmlFragmentRef, node: &AstNode) {
    match node {
        AstNode::Paragraph { children } => {
            let element = XmlElementPrelim::empty("paragraph");
            let idx = fragment.len(txn);
            let el_ref = fragment.insert(txn, idx, element);
            insert_inline_content(txn, &el_ref, children);
        }

        AstNode::Heading { level, children } => {
            let element = XmlElementPrelim::empty("heading");
            let idx = fragment.len(txn);
            let el_ref = fragment.insert(txn, idx, element);
            el_ref.insert_attribute(txn, "level", level.to_string());
            insert_inline_content(txn, &el_ref, children);
        }

        AstNode::BlockQuote { children } => {
            let element = XmlElementPrelim::empty("blockquote");
            let idx = fragment.len(txn);
            let el_ref = fragment.insert(txn, idx, element);

            for child in children {
                insert_node_into_element(txn, &el_ref, child);
            }
        }

        AstNode::CodeBlock { language, content } => {
            let text = vec![XmlTextPrelim::new(content.clone())];
            let element = XmlElementPrelim::new("code_block", text);
            let idx = fragment.len(txn);
            let el_ref = fragment.insert(txn, idx, element);
            if !language.is_empty() {
                el_ref.insert_attribute(txn, "language", language.clone());
            }
        }

        AstNode::BulletList { items } => {
            let element = XmlElementPrelim::empty("bullet_list");
            let idx = fragment.len(txn);
            let el_ref = fragment.insert(txn, idx, element);

            for item in items {
                insert_list_item(txn, &el_ref, item);
            }
        }

        AstNode::OrderedList { start, items } => {
            let element = XmlElementPrelim::empty("ordered_list");
            let idx = fragment.len(txn);
            let el_ref = fragment.insert(txn, idx, element);
            if *start != 1 {
                el_ref.insert_attribute(txn, "order", start.to_string());
            }

            for item in items {
                insert_list_item(txn, &el_ref, item);
            }
        }

        AstNode::HorizontalRule => {
            let element = XmlElementPrelim::empty("horizontal_rule");
            let idx = fragment.len(txn);
            fragment.insert(txn, idx, element);
        }

        AstNode::Table { rows, alignments } => {
            let element = XmlElementPrelim::empty("table");
            let idx = fragment.len(txn);
            let table_ref = fragment.insert(txn, idx, element);

            for row in rows {
                insert_table_row(txn, &table_ref, row, alignments);
            }
        }

        AstNode::Media { src, alt, title, media_type } => {
            let element = XmlElementPrelim::empty("media");
            let idx = fragment.len(txn);
            let el_ref = fragment.insert(txn, idx, element);
            el_ref.insert_attribute(txn, "src", src.clone());
            el_ref.insert_attribute(txn, "type", media_type.as_str().to_string());
            if !alt.is_empty() {
                el_ref.insert_attribute(txn, "alt", alt.clone());
            }
            if let Some(t) = title {
                el_ref.insert_attribute(txn, "title", t.clone());
            }
        }
    }
}

/// Insert an AST node into an element (for nested structures like blockquote, list items)
fn insert_node_into_element(txn: &mut TransactionMut, element: &yrs::XmlElementRef, node: &AstNode) {
    match node {
        AstNode::Paragraph { children } => {
            let para = XmlElementPrelim::empty("paragraph");
            let idx = element.len(txn);
            let el_ref = element.insert(txn, idx, para);
            insert_inline_content(txn, &el_ref, children);
        }

        AstNode::Heading { level, children } => {
            let heading = XmlElementPrelim::empty("heading");
            let idx = element.len(txn);
            let el_ref = element.insert(txn, idx, heading);
            el_ref.insert_attribute(txn, "level", level.to_string());
            insert_inline_content(txn, &el_ref, children);
        }

        AstNode::BulletList { items } => {
            let list = XmlElementPrelim::empty("bullet_list");
            let idx = element.len(txn);
            let list_ref = element.insert(txn, idx, list);
            for item in items {
                insert_list_item(txn, &list_ref, item);
            }
        }

        AstNode::OrderedList { start, items } => {
            let list = XmlElementPrelim::empty("ordered_list");
            let idx = element.len(txn);
            let list_ref = element.insert(txn, idx, list);
            if *start != 1 {
                list_ref.insert_attribute(txn, "order", start.to_string());
            }
            for item in items {
                insert_list_item(txn, &list_ref, item);
            }
        }

        AstNode::CodeBlock { language, content } => {
            let text = vec![XmlTextPrelim::new(content.clone())];
            let cb = XmlElementPrelim::new("code_block", text);
            let idx = element.len(txn);
            let el_ref = element.insert(txn, idx, cb);
            if !language.is_empty() {
                el_ref.insert_attribute(txn, "language", language.clone());
            }
        }

        AstNode::HorizontalRule => {
            let hr = XmlElementPrelim::empty("horizontal_rule");
            let idx = element.len(txn);
            element.insert(txn, idx, hr);
        }

        AstNode::Media { src, alt, title, media_type } => {
            let el = XmlElementPrelim::empty("media");
            let idx = element.len(txn);
            let el_ref = element.insert(txn, idx, el);
            el_ref.insert_attribute(txn, "src", src.clone());
            el_ref.insert_attribute(txn, "type", media_type.as_str().to_string());
            if !alt.is_empty() {
                el_ref.insert_attribute(txn, "alt", alt.clone());
            }
            if let Some(t) = title {
                el_ref.insert_attribute(txn, "title", t.clone());
            }
        }

        AstNode::BlockQuote { children } => {
            let bq = XmlElementPrelim::empty("blockquote");
            let idx = element.len(txn);
            let bq_ref = element.insert(txn, idx, bq);
            for child in children {
                insert_node_into_element(txn, &bq_ref, child);
            }
        }

        AstNode::Table { rows, alignments } => {
            let table = XmlElementPrelim::empty("table");
            let idx = element.len(txn);
            let table_ref = element.insert(txn, idx, table);
            for row in rows {
                insert_table_row(txn, &table_ref, row, alignments);
            }
        }
    }
}

fn insert_list_item(txn: &mut TransactionMut, list: &yrs::XmlElementRef, item: &ListItem) {
    let li = XmlElementPrelim::empty("list_item");
    let idx = list.len(txn);
    let li_ref = list.insert(txn, idx, li);

    // Set checked attribute if this is a task item
    if let Some(checked) = item.checked {
        li_ref.insert_attribute(txn, "checked", if checked { "true" } else { "false" });
    }

    // Add children
    for child in &item.children {
        insert_node_into_element(txn, &li_ref, child);
    }
}

fn insert_table_row(txn: &mut TransactionMut, table: &yrs::XmlElementRef, row: &TableRow, alignments: &[Alignment]) {
    let tr = XmlElementPrelim::empty("table_row");
    let idx = table.len(txn);
    let tr_ref = table.insert(txn, idx, tr);

    for (i, cell) in row.cells.iter().enumerate() {
        let cell_type = if row.is_header { "table_header" } else { "table_cell" };
        let td = XmlElementPrelim::empty(cell_type);
        let cell_idx = tr_ref.len(txn);
        let td_ref = tr_ref.insert(txn, cell_idx, td);

        // Insert cell content
        insert_inline_content(txn, &td_ref, &cell.content);

        // Set alignment
        let align = cell.align.or_else(|| alignments.get(i).copied());
        if let Some(a) = align {
            let align_str = match a {
                Alignment::Left => "left",
                Alignment::Center => "center",
                Alignment::Right => "right",
                Alignment::None => continue,
            };
            td_ref.insert_attribute(txn, "align", align_str);
        }
    }
}

// ============================================================================
// Inline Content Insertion
// ============================================================================

/// Insert inline content into an element, handling mixed text and inline elements
///
/// Groups consecutive text items into XmlText nodes, and inserts entity_link/file_card
/// as XmlElement children between text runs.
fn insert_inline_content(txn: &mut TransactionMut, element: &yrs::XmlElementRef, children: &[InlineContent]) {
    let mut i = 0;
    while i < children.len() {
        match &children[i] {
            InlineContent::EntityLink { href, label } => {
                let el = XmlElementPrelim::empty("entity_link");
                let idx = element.len(txn);
                let el_ref = element.insert(txn, idx, el);
                el_ref.insert_attribute(txn, "href", href.clone());
                el_ref.insert_attribute(txn, "label", label.clone());
                i += 1;
            }
            InlineContent::FileCard { href, name } => {
                let el = XmlElementPrelim::empty("file_card");
                let idx = element.len(txn);
                let el_ref = element.insert(txn, idx, el);
                el_ref.insert_attribute(txn, "href", href.clone());
                el_ref.insert_attribute(txn, "name", name.clone());
                i += 1;
            }
            _ => {
                // Collect consecutive text/hardbreak items into a single text run
                let start = i;
                while i < children.len()
                    && !matches!(&children[i], InlineContent::EntityLink { .. } | InlineContent::FileCard { .. })
                {
                    i += 1;
                }
                insert_text_run(txn, element, &children[start..i]);
            }
        }
    }
}

/// Insert a run of text/hardbreak items as a single XmlText node with formatting
fn insert_text_run(txn: &mut TransactionMut, element: &yrs::XmlElementRef, items: &[InlineContent]) {
    // Build the text content
    let mut text = String::new();
    for item in items {
        match item {
            InlineContent::Text { text: t, .. } => text.push_str(t),
            InlineContent::HardBreak => text.push('\n'),
            _ => {}
        }
    }

    if text.is_empty() {
        return;
    }

    // Insert the text node
    let text_prelim = XmlTextPrelim::new(text);
    let idx = element.len(txn);
    let text_ref = element.insert(txn, idx, text_prelim);

    // Apply marks to text ranges
    let mut offset: u32 = 0;
    for item in items {
        match item {
            InlineContent::Text { text, marks } => {
                let char_len = text.chars().count() as u32;
                if !marks.is_empty() {
                    for mark in marks {
                        let attrs = mark_to_attrs(mark);
                        text_ref.format(txn, offset, char_len, attrs);
                    }
                }
                offset += char_len;
            }
            InlineContent::HardBreak => {
                offset += 1;
            }
            _ => {}
        }
    }
}

/// Convert a mark to yrs Attrs for formatting
fn mark_to_attrs(mark: &Mark) -> yrs::types::Attrs {
    let mut attrs = yrs::types::Attrs::new();

    match mark {
        Mark::Strong => {
            attrs.insert("strong".into(), Any::Bool(true));
        }
        Mark::Em => {
            attrs.insert("em".into(), Any::Bool(true));
        }
        Mark::Code => {
            attrs.insert("code".into(), Any::Bool(true));
        }
        Mark::Strikethrough => {
            attrs.insert("strikethrough".into(), Any::Bool(true));
        }
        Mark::Underline => {
            attrs.insert("underline".into(), Any::Bool(true));
        }
        Mark::Link { href, title } => {
            attrs.insert("link".into(), Any::Bool(true));
            attrs.insert("href".into(), Any::String(href.clone().into()));
            if let Some(t) = title {
                attrs.insert("title".into(), Any::String(t.clone().into()));
            }
        }
    }

    attrs
}
