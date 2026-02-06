//! Markdown to Yjs XmlFragment parser
//!
//! Converts Markdown text into a Yjs XmlFragment structure compatible with
//! y-prosemirror and ProseMirror's document model.

use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use yrs::{Any, Text, TransactionMut, Xml, XmlElementPrelim, XmlFragment, XmlFragmentRef, XmlTextPrelim};

use super::Result;

/// Convert markdown text to Yjs XmlFragment nodes
///
/// This function parses markdown and creates XmlFragment nodes that are compatible
/// with y-prosemirror's expected structure. The fragment should be the "content"
/// fragment from a Yjs document.
///
/// # Arguments
///
/// * `txn` - A mutable Yjs transaction
/// * `fragment` - The XmlFragment to populate (should be empty or will be appended to)
/// * `markdown` - The markdown text to parse
///
/// # Example
///
/// ```rust,ignore
/// let doc = Doc::new();
/// let mut txn = doc.transact_mut();
/// let fragment = txn.get_or_insert_xml_fragment("content");
///
/// markdown_to_xml_fragment(&mut txn, &fragment, "# Hello\n\nWorld")?;
/// ```
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
    Image { src: String, alt: String },
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
    Checkbox { checked: bool },
}

#[derive(Debug, Clone, PartialEq)]
enum Mark {
    Strong,
    Em,
    Code,
    Strikethrough,
    Link { href: String, title: Option<String> },
}

/// Build AST from pulldown-cmark events
fn build_ast(events: &[Event]) -> Vec<AstNode> {
    let mut builder = AstBuilder::new();
    for event in events {
        builder.handle_event(event);
    }
    builder.finish()
}

struct AstBuilder {
    /// Stack of nodes being built
    node_stack: Vec<NodeBuilder>,
    /// Current active marks for inline text
    active_marks: Vec<Mark>,
    /// Table state
    table_alignments: Vec<Alignment>,
    in_table_header: bool,
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

impl AstBuilder {
    fn new() -> Self {
        Self {
            node_stack: vec![NodeBuilder::Document(vec![])],
            active_marks: vec![],
            table_alignments: vec![],
            in_table_header: false,
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
                    self.push_block(AstNode::Paragraph { children });
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
                    self.push_list_item(ListItem { checked, children });
                }
            }

            Event::TaskListMarker(checked) => {
                // Set the checked state on the current list item
                if let Some(NodeBuilder::ListItem { checked: c, children: _, inline_content: _ }) = self.node_stack.last_mut() {
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

            Event::Start(Tag::Link { dest_url, title, .. }) => {
                self.active_marks.push(Mark::Link {
                    href: dest_url.to_string(),
                    title: if title.is_empty() { None } else { Some(title.to_string()) },
                });
            }
            Event::End(TagEnd::Link) => {
                self.remove_mark(|m| matches!(m, Mark::Link { .. }));
            }

            // ===== INLINE CONTENT =====
            Event::Text(text) => {
                self.push_inline(InlineContent::Text {
                    text: text.to_string(),
                    marks: self.active_marks.clone(),
                });
            }

            Event::Code(text) => {
                let mut marks = self.active_marks.clone();
                marks.push(Mark::Code);
                self.push_inline(InlineContent::Text {
                    text: text.to_string(),
                    marks,
                });
            }

            Event::SoftBreak => {
                self.push_inline(InlineContent::Text {
                    text: " ".to_string(),
                    marks: self.active_marks.clone(),
                });
            }

            Event::HardBreak => {
                self.push_inline(InlineContent::HardBreak);
            }

            // ===== IMAGES/MEDIA =====
            Event::Start(Tag::Image { dest_url, title, .. }) => {
                self.push_block(AstNode::Image {
                    src: dest_url.to_string(),
                    alt: title.to_string(),
                });
            }
            Event::End(TagEnd::Image) => {
                // Alt text handled by Text events inside image
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
            let content = inline_to_text_prelim(children);
            let element = XmlElementPrelim::new("paragraph", content);
            let idx = fragment.len(txn);
            let el_ref = fragment.insert(txn, idx, element);
            apply_inline_marks(txn, &el_ref, children);
        }

        AstNode::Heading { level, children } => {
            let content = inline_to_text_prelim(children);
            let element = XmlElementPrelim::new("heading", content);
            let idx = fragment.len(txn);
            let el_ref = fragment.insert(txn, idx, element);
            el_ref.insert_attribute(txn, "level", level.to_string());
            apply_inline_marks(txn, &el_ref, children);
        }

        AstNode::BlockQuote { children } => {
            let element = XmlElementPrelim::empty("blockquote");
            let idx = fragment.len(txn);
            let el_ref = fragment.insert(txn, idx, element);

            // Create a temporary fragment-like context for children
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

        AstNode::Image { src, alt } => {
            let ext = src.rsplit('.').next().unwrap_or("").to_lowercase();
            let node_type = match ext.as_str() {
                "mp3" | "wav" | "m4a" | "ogg" | "flac" | "aac" | "wma" => "audio_player",
                "mp4" | "mov" | "webm" | "avi" | "mkv" | "m4v" | "wmv" => "video_player",
                _ => "image",
            };

            let element = XmlElementPrelim::empty(node_type);
            let idx = fragment.len(txn);
            let el_ref = fragment.insert(txn, idx, element);
            el_ref.insert_attribute(txn, "src", src.clone());
            if !alt.is_empty() {
                if node_type == "image" {
                    el_ref.insert_attribute(txn, "alt", alt.clone());
                } else {
                    el_ref.insert_attribute(txn, "name", alt.clone());
                }
            }
        }
    }
}

/// Insert an AST node into an element (for nested structures)
fn insert_node_into_element(txn: &mut TransactionMut, element: &yrs::XmlElementRef, node: &AstNode) {
    match node {
        AstNode::Paragraph { children } => {
            let content = inline_to_text_prelim(children);
            let para = XmlElementPrelim::new("paragraph", content);
            let idx = element.len(txn);
            let el_ref = element.insert(txn, idx, para);
            apply_inline_marks(txn, &el_ref, children);
        }

        AstNode::Heading { level, children } => {
            let content = inline_to_text_prelim(children);
            let heading = XmlElementPrelim::new("heading", content);
            let idx = element.len(txn);
            let el_ref = element.insert(txn, idx, heading);
            el_ref.insert_attribute(txn, "level", level.to_string());
            apply_inline_marks(txn, &el_ref, children);
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

        _ => {
            // For other node types, create a temporary fragment approach
            // This is a simplification - complex nesting might need more work
        }
    }
}

fn insert_list_item(txn: &mut TransactionMut, list: &yrs::XmlElementRef, item: &ListItem) {
    let li = XmlElementPrelim::empty("list_item");
    let idx = list.len(txn);
    let li_ref = list.insert(txn, idx, li);

    // Add checkbox if present
    if let Some(checked) = item.checked {
        let checkbox = XmlElementPrelim::empty("checkbox");
        let cb_ref = li_ref.insert(txn, 0, checkbox);
        cb_ref.insert_attribute(txn, "checked", if checked { "true" } else { "false" });
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
        let content = inline_to_text_prelim(&cell.content);
        let td = XmlElementPrelim::new(cell_type, content);
        let cell_idx = tr_ref.len(txn);
        let td_ref = tr_ref.insert(txn, cell_idx, td);

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

        apply_inline_marks(txn, &td_ref, &cell.content);
    }
}

/// Convert inline content to text prelim (plain text only)
fn inline_to_text_prelim(children: &[InlineContent]) -> Vec<XmlTextPrelim<String>> {
    let mut text = String::new();
    for child in children {
        match child {
            InlineContent::Text { text: t, .. } => text.push_str(t),
            InlineContent::HardBreak => text.push('\n'),
            InlineContent::Checkbox { .. } => {}
        }
    }
    if text.is_empty() {
        vec![]
    } else {
        vec![XmlTextPrelim::new(text)]
    }
}

/// Apply marks to text in an element
fn apply_inline_marks(txn: &mut TransactionMut, element: &yrs::XmlElementRef, children: &[InlineContent]) {
    // Get the text node (first child)
    use yrs::XmlNode;

    if element.len(txn) == 0 {
        return;
    }

    // yrs XmlElement children are accessed differently
    // For now, marks are embedded in the text content
    // A proper implementation would use XmlText.format() to apply marks

    let mut offset: u32 = 0;
    for child in children {
        match child {
            InlineContent::Text { text, marks } => {
                if !marks.is_empty() {
                    // Get first child as XmlText and format it
                    if let Some(XmlNode::Text(text_ref)) = element.get(txn, 0) {
                        let len = text.len() as u32;
                        for mark in marks {
                            let attrs = mark_to_attrs(mark);
                            text_ref.format(txn, offset, len, attrs);
                        }
                    }
                }
                offset += text.len() as u32;
            }
            InlineContent::HardBreak => {
                offset += 1;
            }
            InlineContent::Checkbox { .. } => {}
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
