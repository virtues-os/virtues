//! Yjs XmlFragment to Markdown serializer
//!
//! Converts Yjs XmlFragment structures back to Markdown text.
//! This is the inverse of the parser module.

use yrs::{Any, GetString, ReadTxn, Text, Value, Xml, XmlElementRef, XmlFragment, XmlFragmentRef, XmlNode, XmlTextRef};
use yrs::types::Attrs;
use yrs::types::text::YChange;

/// Convert a Yjs XmlFragment to markdown text
///
/// This function walks the XmlFragment tree and produces Markdown text
/// that can be parsed back to an equivalent structure.
pub fn xml_fragment_to_markdown<T: ReadTxn>(txn: &T, fragment: XmlFragmentRef) -> String {
    let mut output = String::new();
    let mut serializer = Serializer::new();

    for i in 0..fragment.len(txn) {
        if let Some(node) = fragment.get(txn, i) {
            serializer.serialize_node(txn, &node, &mut output, 0);
        }
    }

    output
}

struct Serializer {
    /// Track if we're in a code block (no mark formatting)
    in_code_block: bool,
}

impl Serializer {
    fn new() -> Self {
        Self {
            in_code_block: false,
        }
    }

    fn serialize_node<T: ReadTxn>(
        &mut self,
        txn: &T,
        node: &XmlNode,
        output: &mut String,
        indent: usize,
    ) {
        match node {
            XmlNode::Element(el) => {
                self.serialize_element(txn, el, output, indent);
            }
            XmlNode::Text(text) => {
                self.serialize_text(txn, text, output);
            }
            XmlNode::Fragment(frag) => {
                for i in 0..frag.len(txn) {
                    if let Some(child) = frag.get(txn, i) {
                        self.serialize_node(txn, &child, output, indent);
                    }
                }
            }
        }
    }

    fn serialize_element<T: ReadTxn>(
        &mut self,
        txn: &T,
        element: &XmlElementRef,
        output: &mut String,
        indent: usize,
    ) {
        let tag = element.tag().to_string();

        match tag.as_str() {
            "paragraph" => {
                self.serialize_inline_content(txn, element, output);
                output.push_str("\n\n");
            }

            "heading" => {
                let level: usize = element
                    .get_attribute(txn, "level")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1);
                output.push_str(&"#".repeat(level));
                output.push(' ');
                self.serialize_inline_content(txn, element, output);
                output.push_str("\n\n");
            }

            "blockquote" => {
                // Serialize children to a buffer, then prefix each line with >
                let mut inner = String::new();
                for i in 0..element.len(txn) {
                    if let Some(child) = element.get(txn, i) {
                        self.serialize_node(txn, &child, &mut inner, indent);
                    }
                }
                for line in inner.trim_end().lines() {
                    output.push_str("> ");
                    output.push_str(line);
                    output.push('\n');
                }
                output.push('\n');
            }

            "code_block" => {
                let lang = element
                    .get_attribute(txn, "language")
                    .unwrap_or_default();
                output.push_str("```");
                output.push_str(&lang);
                output.push('\n');
                self.in_code_block = true;
                self.serialize_text_only(txn, element, output);
                self.in_code_block = false;
                output.push_str("\n```\n\n");
            }

            "bullet_list" => {
                for i in 0..element.len(txn) {
                    if let Some(XmlNode::Element(item)) = element.get(txn, i) {
                        self.serialize_list_item(txn, &item, output, "- ", indent);
                    }
                }
            }

            "ordered_list" => {
                let start: usize = element
                    .get_attribute(txn, "order")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(1);
                let mut idx = 0;
                for i in 0..element.len(txn) {
                    if let Some(XmlNode::Element(item)) = element.get(txn, i) {
                        let marker = format!("{}. ", start + idx);
                        self.serialize_list_item(txn, &item, output, &marker, indent);
                        idx += 1;
                    }
                }
            }

            "list_item" => {
                // List items are handled by bullet_list/ordered_list
                // But if we encounter one directly, serialize its contents
                for i in 0..element.len(txn) {
                    if let Some(child) = element.get(txn, i) {
                        self.serialize_node(txn, &child, output, indent);
                    }
                }
            }

            "horizontal_rule" => {
                output.push_str("---\n\n");
            }

            "hard_break" => {
                output.push_str("  \n");
            }

            "table" => {
                self.serialize_table(txn, element, output);
            }

            "table_row" | "table_header" | "table_cell" => {
                // These are handled by serialize_table
                // But if encountered directly, serialize children
                for i in 0..element.len(txn) {
                    if let Some(child) = element.get(txn, i) {
                        self.serialize_node(txn, &child, output, indent);
                    }
                }
            }

            // Custom nodes
            "entity_link" => {
                let href = element
                    .get_attribute(txn, "href")
                    .unwrap_or_default();
                let label = element
                    .get_attribute(txn, "label")
                    .unwrap_or_default();
                output.push_str(&format!("[{}]({})", label, href));
            }

            "image" => {
                let src = element
                    .get_attribute(txn, "src")
                    .unwrap_or_default();
                let alt = element
                    .get_attribute(txn, "alt")
                    .unwrap_or_default();
                output.push_str(&format!("![{}]({})\n\n", alt, src));
            }

            "audio_player" | "video_player" => {
                let src = element
                    .get_attribute(txn, "src")
                    .unwrap_or_default();
                let name = element
                    .get_attribute(txn, "name")
                    .unwrap_or_default();
                output.push_str(&format!("![{}]({})\n\n", name, src));
            }

            "checkbox" => {
                let checked = element
                    .get_attribute(txn, "checked")
                    .map(|v| v == "true")
                    .unwrap_or(false);
                output.push_str(if checked { "[x] " } else { "[ ] " });
            }

            _ => {
                // Unknown element - just serialize children
                for i in 0..element.len(txn) {
                    if let Some(child) = element.get(txn, i) {
                        self.serialize_node(txn, &child, output, indent);
                    }
                }
            }
        }
    }

    fn serialize_inline_content<T: ReadTxn>(
        &mut self,
        txn: &T,
        element: &XmlElementRef,
        output: &mut String,
    ) {
        for i in 0..element.len(txn) {
            if let Some(child) = element.get(txn, i) {
                match &child {
                    XmlNode::Text(text) => {
                        self.serialize_text(txn, text, output);
                    }
                    XmlNode::Element(el) => {
                        // Inline elements like hard_break, checkbox, entity_link
                        self.serialize_element(txn, el, output, 0);
                    }
                    XmlNode::Fragment(frag) => {
                        for j in 0..frag.len(txn) {
                            if let Some(node) = frag.get(txn, j) {
                                self.serialize_node(txn, &node, output, 0);
                            }
                        }
                    }
                }
            }
        }
    }

    fn serialize_text<T: ReadTxn>(&mut self, txn: &T, text: &XmlTextRef, output: &mut String) {
        // In code blocks, don't apply any formatting
        if self.in_code_block {
            output.push_str(&text.get_string(txn));
            return;
        }

        // Use diff() to iterate through formatted text chunks
        // Each chunk has text content and optional formatting attributes
        // YChange::identity is passed as the compute_ychange function
        for diff in text.diff(txn, YChange::identity) {
            // Extract the text content from the diff
            let text_content = match &diff.insert {
                Value::Any(Any::String(s)) => s.to_string(),
                Value::Any(Any::BigInt(n)) => n.to_string(),
                Value::Any(Any::Number(n)) => n.to_string(),
                Value::Any(Any::Bool(b)) => b.to_string(),
                _ => continue, // Skip non-text content (embeds, shared types, etc.)
            };

            // Apply markdown formatting based on attributes
            let formatted = if let Some(attrs) = &diff.attributes {
                wrap_with_attrs(&text_content, attrs)
            } else {
                text_content
            };

            output.push_str(&formatted);
        }
    }

    fn serialize_text_only<T: ReadTxn>(
        &mut self,
        txn: &T,
        element: &XmlElementRef,
        output: &mut String,
    ) {
        // For code blocks - just get raw text content
        for i in 0..element.len(txn) {
            if let Some(child) = element.get(txn, i) {
                match &child {
                    XmlNode::Text(text) => {
                        output.push_str(&text.get_string(txn));
                    }
                    XmlNode::Element(el) => {
                        self.serialize_text_only(txn, el, output);
                    }
                    XmlNode::Fragment(frag) => {
                        for j in 0..frag.len(txn) {
                            if let Some(XmlNode::Text(t)) = frag.get(txn, j) {
                                output.push_str(&t.get_string(txn));
                            }
                        }
                    }
                }
            }
        }
    }

    fn serialize_list_item<T: ReadTxn>(
        &mut self,
        txn: &T,
        item: &XmlElementRef,
        output: &mut String,
        marker: &str,
        indent: usize,
    ) {
        let indent_str = "  ".repeat(indent);

        // Check for checkbox at start
        let mut has_checkbox = false;
        let mut checkbox_checked = false;

        for i in 0..item.len(txn) {
            if let Some(XmlNode::Element(el)) = item.get(txn, i) {
                if el.tag().as_ref() == "checkbox" {
                    has_checkbox = true;
                    checkbox_checked = el
                        .get_attribute(txn, "checked")
                        .map(|v| v == "true")
                        .unwrap_or(false);
                    break;
                }
            }
        }

        // Output marker
        output.push_str(&indent_str);
        output.push_str(marker);

        if has_checkbox {
            output.push_str(if checkbox_checked { "[x] " } else { "[ ] " });
        }

        // Serialize children (skip the checkbox if present)
        let mut first = true;
        for i in 0..item.len(txn) {
            if let Some(child) = item.get(txn, i) {
                match &child {
                    XmlNode::Element(el) => {
                        let tag = el.tag().to_string();

                        // Skip checkbox (already handled)
                        if tag == "checkbox" {
                            continue;
                        }

                        // Handle nested lists with increased indent
                        if tag == "bullet_list" || tag == "ordered_list" {
                            output.push('\n');
                            self.serialize_element(txn, el, output, indent + 1);
                        } else if tag == "paragraph" {
                            if !first {
                                output.push_str(&indent_str);
                                output.push_str(&" ".repeat(marker.len()));
                            }
                            self.serialize_inline_content(txn, el, output);
                            output.push('\n');
                        } else {
                            self.serialize_element(txn, el, output, indent);
                        }
                    }
                    XmlNode::Text(text) => {
                        output.push_str(&text.get_string(txn));
                    }
                    _ => {}
                }
                first = false;
            }
        }
    }

    fn serialize_table<T: ReadTxn>(
        &mut self,
        txn: &T,
        table: &XmlElementRef,
        output: &mut String,
    ) {
        let mut rows: Vec<Vec<String>> = vec![];
        let mut alignments: Vec<Option<&str>> = vec![];
        let mut is_first_row = true;

        // Collect all rows
        for i in 0..table.len(txn) {
            if let Some(XmlNode::Element(row_el)) = table.get(txn, i) {
                let row_tag = row_el.tag().to_string();
                if row_tag != "table_row" {
                    continue;
                }

                let mut row_cells: Vec<String> = vec![];

                for j in 0..row_el.len(txn) {
                    if let Some(XmlNode::Element(cell_el)) = row_el.get(txn, j) {
                        let cell_tag = cell_el.tag().to_string();
                        if cell_tag != "table_cell" && cell_tag != "table_header" {
                            continue;
                        }

                        // Get cell content
                        let mut cell_content = String::new();
                        self.serialize_inline_content(txn, &cell_el, &mut cell_content);
                        row_cells.push(cell_content.trim().to_string());

                        // Get alignment (only from first row)
                        if is_first_row {
                            let align = cell_el.get_attribute(txn, "align").and_then(|s| {
                                match s.as_str() {
                                    "left" => Some("left"),
                                    "center" => Some("center"),
                                    "right" => Some("right"),
                                    _ => None,
                                }
                            });
                            alignments.push(align);
                        }
                    }
                }

                rows.push(row_cells);
                is_first_row = false;
            }
        }

        if rows.is_empty() {
            return;
        }

        // Calculate column widths
        let num_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        let mut col_widths: Vec<usize> = vec![3; num_cols]; // Minimum width of 3

        for row in &rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_widths.len() {
                    col_widths[i] = col_widths[i].max(cell.len());
                }
            }
        }

        // Output header row
        if let Some(header) = rows.first() {
            output.push('|');
            for (i, cell) in header.iter().enumerate() {
                let width = col_widths.get(i).copied().unwrap_or(3);
                output.push_str(&format!(" {:width$} |", cell, width = width));
            }
            output.push('\n');

            // Output separator row
            output.push('|');
            for (i, width) in col_widths.iter().enumerate() {
                let align = alignments.get(i).copied().flatten();
                let separator = match align {
                    Some("left") => format!(":{}|", "-".repeat(*width + 1)),
                    Some("center") => format!(":{}:|", "-".repeat(*width)),
                    Some("right") => format!("{}:|", "-".repeat(*width + 1)),
                    _ => format!("{}|", "-".repeat(*width + 2)),
                };
                output.push_str(&separator);
            }
            output.push('\n');
        }

        // Output data rows
        for row in rows.iter().skip(1) {
            output.push('|');
            for (i, cell) in row.iter().enumerate() {
                let width = col_widths.get(i).copied().unwrap_or(3);
                output.push_str(&format!(" {:width$} |", cell, width = width));
            }
            output.push('\n');
        }

        output.push('\n');
    }
}

/// Wrap text with markdown formatting based on yrs attributes
///
/// Attrs is HashMap<Arc<str>, Any> where keys are mark names like "strong", "em", etc.
/// and values are typically Any::Bool(true) or Any::String for link hrefs.
fn wrap_with_attrs(text: &str, attrs: &Attrs) -> String {
    let mut result = text.to_string();

    // Helper to check if an attribute is set (truthy)
    let has_attr = |key: &str| -> bool {
        attrs.get(key).map(|v| matches!(v, Any::Bool(true))).unwrap_or(false)
    };

    // Helper to get string attribute value
    let get_string_attr = |key: &str| -> Option<String> {
        attrs.get(key).and_then(|v| {
            if let Any::String(s) = v {
                Some(s.to_string())
            } else {
                None
            }
        })
    };

    // Apply marks in order (innermost first, then outer marks wrap around)
    // Code should be innermost to prevent markdown interpretation
    if has_attr("code") {
        result = format!("`{}`", result);
    }
    if has_attr("em") {
        result = format!("*{}*", result);
    }
    if has_attr("strong") {
        result = format!("**{}**", result);
    }
    if has_attr("strikethrough") {
        result = format!("~~{}~~", result);
    }
    // Links should be outermost - wrap the formatted text with link syntax
    if has_attr("link") {
        if let Some(href) = get_string_attr("href") {
            result = format!("[{}]({})", result, href);
        }
    }

    result
}
