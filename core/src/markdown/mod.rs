//! Markdown <-> Yjs XmlFragment conversion for AI editing
//!
//! This module provides bidirectional conversion between Markdown text and
//! Yjs XmlFragment structures compatible with y-prosemirror.
//!
//! # Architecture
//!
//! ```text
//! Markdown String
//!       │
//!       ▼ markdown_to_xml_fragment()
//! Yjs XmlFragment (ProseMirror-compatible)
//!       │
//!       ▼ xml_fragment_to_markdown()
//! Markdown String
//! ```
//!
//! # Features
//!
//! - Standard CommonMark/GFM: paragraphs, headings, lists, code blocks, tables
//! - Inline formatting: bold, italic, strikethrough, code, links
//! - Custom nodes: entity links, checkboxes, media embeds
//!
//! # Example
//!
//! ```rust,ignore
//! use yrs::{Doc, Transact};
//! use virtues::markdown::{markdown_to_xml_fragment, xml_fragment_to_markdown};
//!
//! let doc = Doc::new();
//! let mut txn = doc.transact_mut();
//! let fragment = txn.get_or_insert_xml_fragment("content");
//!
//! // Parse markdown into XmlFragment
//! markdown_to_xml_fragment(&mut txn, &fragment, "# Hello\n\n**World**")?;
//!
//! // Serialize back to markdown
//! let markdown = xml_fragment_to_markdown(&txn, &fragment);
//! ```

mod parser;
mod serializer;

pub use parser::markdown_to_xml_fragment;
pub use serializer::xml_fragment_to_markdown;

use thiserror::Error;

/// Errors that can occur during markdown conversion
#[derive(Debug, Error)]
pub enum MarkdownError {
    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Invalid document structure: {0}")]
    Structure(String),

    #[error("Unsupported feature: {0}")]
    Unsupported(String),
}

/// Result type for markdown operations
pub type Result<T> = std::result::Result<T, MarkdownError>;

#[cfg(test)]
mod tests;
