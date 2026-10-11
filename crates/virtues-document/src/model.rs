//! The document tree, shaped like ProseMirror's `node.toJSON()` so the two
//! implementations can be compared field for field.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub attrs: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<Node>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<Mark>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mark {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub attrs: Map<String, Value>,
}

/// The most nodes one page holds, every text run counted. Every check of an
/// editor's keystroke (`check_update`) reads the whole page, and so does
/// every edit and every save: their cost grows with the page, and this is
/// what bounds it. A page of plain paragraphs reaches it at some fifty
/// thousand of them; a 300-row table of five columns is under five
/// thousand nodes. A write that would take a page past it is refused.
pub const MAX_NODES: usize = 100_000;

/// How many nodes `nodes` hold, every text run and every level counted.
pub fn node_count(nodes: &[Node]) -> usize {
    let mut count = 0;
    for n in nodes {
        n.walk(&mut |_| count += 1);
    }
    count
}

/// What a write past [`MAX_NODES`] is told.
pub fn too_many_nodes() -> String {
    format!(
        "the page would hold more than {MAX_NODES} nodes (its blocks, their text runs and what \
         they hold); split it into pages"
    )
}

/// How deep a page's tree may nest: a node inside a node, a hundred times
/// (text, a leaf, is not a level of its own). Deeper than any page a person
/// writes (a list nested twenty levels is forty), and shallow enough that
/// every walk over a tree, each of which recurses once per level, fits a
/// 2 MiB thread stack (tokio's worker default) in a debug build. Past the
/// stack a walk does not fail, it aborts the process, so every way a tree
/// comes in (HTML, markdown, a Yjs update) refuses one deeper than this
/// before walking it.
pub const MAX_DEPTH: usize = 100;

/// The refusal for a tree past [`MAX_DEPTH`].
pub fn too_deep() -> String {
    format!("the content nests more than {MAX_DEPTH} levels deep")
}

/// How many levels of nodes `nodes` nest, text not counted: 1 for a page of
/// paragraphs, 2 once one holds a mention. Recursive, so only for a tree
/// whose depth is already bounded (ingest's DOM, or a document's walk).
pub fn depth(nodes: &[Node]) -> usize {
    nodes
        .iter()
        .filter(|n| !n.is_text())
        .map(|n| 1 + depth(&n.content))
        .max()
        .unwrap_or(0)
}

/// Something the contract refuses (an error) or a change made on the way in
/// (a note), with where it happened.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Problem {
    pub at: String,
    pub message: String,
}

impl Problem {
    pub fn new(at: impl Into<String>, message: impl Into<String>) -> Self {
        Problem {
            at: at.into(),
            message: message.into(),
        }
    }
}

impl Node {
    pub fn element(kind: &str, attrs: Map<String, Value>, content: Vec<Node>) -> Self {
        Node {
            kind: kind.to_string(),
            attrs,
            content,
            text: None,
            marks: vec![],
        }
    }

    pub fn text(text: &str, marks: Vec<Mark>) -> Self {
        Node {
            kind: "text".into(),
            attrs: Map::new(),
            content: vec![],
            text: Some(text.into()),
            marks,
        }
    }

    pub fn is_text(&self) -> bool {
        self.kind == "text"
    }

    /// The block id. The contract names the attribute `id`.
    pub fn id(&self) -> Option<&str> {
        self.attrs
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
    }

    /// Plain text, for messages and diffs.
    pub fn text_content(&self) -> String {
        match &self.text {
            Some(t) => t.clone(),
            None => self.content.iter().map(Node::text_content).collect(),
        }
    }

    pub fn walk_mut(&mut self, f: &mut impl FnMut(&mut Node)) {
        f(self);
        for c in &mut self.content {
            c.walk_mut(f);
        }
    }

    pub fn walk(&self, f: &mut impl FnMut(&Node)) {
        f(self);
        for c in &self.content {
            c.walk(f);
        }
    }
}

/// Eight base-36 characters: short enough that ids on every block cost the
/// model little, wide enough (2.8e12) that one document never collides. The
/// browser's `newId` uses the same alphabet and width.
pub fn new_id() -> String {
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    (0..8)
        .map(|_| ALPHABET[fastrand::usize(..36)] as char)
        .collect()
}
