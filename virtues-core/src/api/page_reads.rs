//! Read bases: what the model has been shown of a block page, kept so its
//! next edit can be merged against exactly that (`YjsState::edit_tree`).
//!
//! A base is named by a hash of the tree it holds, block ids included, so
//! deleting a block changes it where a state vector would not move. Bases
//! live in `app_page_read_bases` rather than beside the cached doc: an edit
//! can arrive after the doc has been evicted, or after a restart.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use std::collections::{BTreeMap, BTreeSet};
use virtues_document::{Mark, Node};

/// The most bases kept for one page; older ones are pruned as new ones are
/// kept. A model reads a page a few times per edit, so fifty spans several
/// conversations at once.
pub const KEEP_PER_PAGE: i64 = 50;

/// How long a base is kept after it was last recorded. An edit from a read
/// older than this is treated as having no base.
pub const KEEP_DAYS: i64 = 7;

/// The name of a base: 16 lowercase hex of the SHA-256 of the tree's JSON.
/// The JSON is canonical (`serde_json` keeps attributes sorted), so the same
/// tree always hashes the same.
pub fn tree_hash(nodes: &[Node]) -> String {
    let json = serde_json::to_vec(nodes).expect("a page tree serializes");
    let digest = Sha256::digest(&json);
    hex::encode(&digest[..8])
}

/// The hash of one block, as [`ReadBase::whole`] holds it.
pub fn node_hash(node: &Node) -> String {
    tree_hash(std::slice::from_ref(node))
}

/// What reads showed of a block page, kept as the base of the next edit
/// ([`keep_read_base`]): the tree as shown, and how some of it was shown.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReadBase {
    /// The blocks the reads showed, each as it stood then.
    pub tree: Vec<Node>,
    /// Blocks shown only as markdown, each named, the blocks inside one
    /// included. Markdown does not say what a block's HTML is (a mention
    /// reads as a link, a callout as a quote), so a replace that would
    /// write one of them (the block, or a block around it) waits for a
    /// read of its HTML, which takes it out of this set.
    pub markdown_only: BTreeSet<String>,
    /// Blocks shown in part (a list opened by its first items, the list
    /// around one item read by id), each with its [`node_hash`] whole as it
    /// stood when shown: deleting one stands while it still hashes so, since
    /// nothing in it changed unseen. Replacing one waits until reads have
    /// shown all of it.
    pub whole: BTreeMap<String, String>,
}

impl ReadBase {
    /// A base that is `tree` as shown, HTML and whole.
    pub fn of(tree: Vec<Node>) -> Self {
        Self {
            tree,
            ..Self::default()
        }
    }
}

/// A kept base that says more than its tree.
#[derive(Serialize, Deserialize)]
struct Kept {
    tree: Vec<Flat>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    markdown_only: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    whole: BTreeMap<String, String>,
}

/// One node of a kept tree, in document order, its children counted rather
/// than nested in it. A tree as deep as the contract allows
/// (`virtues_document::MAX_DEPTH`) nests about twice as deep as JSON, past
/// the depth `serde_json` reads back, and reading nested JSON costs stack
/// for each level; a flat list reads back at any depth, in a loop.
#[derive(Serialize, Deserialize)]
struct Flat {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    attrs: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    marks: Vec<Mark>,
    #[serde(default, skip_serializing_if = "is_zero")]
    children: usize,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// `nodes` as the flat list a base is stored as: each node, then its
/// children, as `Node::walk` visits them.
fn flatten(nodes: &[Node]) -> Vec<Flat> {
    let mut out = vec![];
    for n in nodes {
        n.walk(&mut |n| {
            out.push(Flat {
                kind: n.kind.clone(),
                attrs: n.attrs.clone(),
                text: n.text.clone(),
                marks: n.marks.clone(),
                children: n.content.len(),
            })
        });
    }
    out
}

/// The tree a flat list holds, or `None` when the counts do not add up.
fn unflatten(flat: Vec<Flat>) -> Option<Vec<Node>> {
    // The nodes still taking children, each with how many it still takes;
    // the root takes every top-level node there is.
    let mut open: Vec<(Node, usize)> = vec![(Node::element("", Map::new(), vec![]), usize::MAX)];
    for f in flat {
        let node = Node {
            kind: f.kind,
            attrs: f.attrs,
            content: Vec::with_capacity(f.children.min(1024)),
            text: f.text,
            marks: f.marks,
        };
        let parent = open.last_mut()?;
        parent.1 = parent.1.checked_sub(1)?;
        if f.children > 0 {
            open.push((node, f.children));
            continue;
        }
        open.last_mut()?.0.content.push(node);
        // A node whose last child has come is done.
        while open.len() > 1 && open.last()?.1 == 0 {
            let (done, _) = open.pop()?;
            open.last_mut()?.0.content.push(done);
        }
    }
    match open.pop() {
        Some((root, _)) if open.is_empty() => Some(root.content),
        _ => None,
    }
}

/// Keep `kept` as a base of the page and return its name. A base that is
/// its tree alone is stored as its flat tree and named by [`tree_hash`]; one
/// that says more is named by the hash of all it says. Keeping a base that
/// is already kept only marks it recent. Prunes the page's bases past
/// [`KEEP_PER_PAGE`] and every base older than [`KEEP_DAYS`].
pub async fn keep_read_base(pool: &PgPool, page_id: &str, kept: &ReadBase) -> Result<String> {
    let encoded = if kept.markdown_only.is_empty() && kept.whole.is_empty() {
        serde_json::to_vec(&flatten(&kept.tree)).map(|json| (tree_hash(&kept.tree), json))
    } else {
        serde_json::to_vec(&Kept {
            tree: flatten(&kept.tree),
            markdown_only: kept.markdown_only.clone(),
            whole: kept.whole.clone(),
        })
        .map(|json| (hex::encode(&Sha256::digest(&json)[..8]), json))
    };
    let (base, json) =
        encoded.map_err(|e| Error::Other(format!("could not encode the page's tree: {e}")))?;
    sqlx::query(
        "INSERT INTO app_page_read_bases (page_id, base, tree_json, created_at, updated_at) \
         VALUES ($1, $2, $3, clock_timestamp(), clock_timestamp()) \
         ON CONFLICT (page_id, base) DO UPDATE SET updated_at = clock_timestamp()",
    )
    .bind(page_id)
    .bind(&base)
    .bind(&json)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to keep the read base: {e}")))?;

    sqlx::query(
        "DELETE FROM app_page_read_bases \
         WHERE page_id = $1 AND base NOT IN ( \
             SELECT base FROM app_page_read_bases WHERE page_id = $1 \
             ORDER BY updated_at DESC LIMIT $2)",
    )
    .bind(page_id)
    .bind(KEEP_PER_PAGE)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to prune the page's read bases: {e}")))?;

    sqlx::query(
        "DELETE FROM app_page_read_bases \
         WHERE updated_at < now() - make_interval(days => $1::int)",
    )
    .bind(KEEP_DAYS as i32)
    .execute(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to prune old read bases: {e}")))?;

    Ok(base)
}

/// The tree kept as `base` for the page, or `None` when it was never kept,
/// was pruned, or is older than [`KEEP_DAYS`]. A kept row that does not read
/// back as a tree is logged and is `None` too: a base the server cannot read
/// is one it does not have, and the edit falls back as it does for a pruned
/// one.
pub async fn read_base(pool: &PgPool, page_id: &str, base: &str) -> Result<Option<ReadBase>> {
    let json: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT tree_json FROM app_page_read_bases \
         WHERE page_id = $1 AND base = $2 \
           AND updated_at >= now() - make_interval(days => $3::int)",
    )
    .bind(page_id)
    .bind(base)
    .bind(KEEP_DAYS as i32)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("Failed to read the read base: {e}")))?;
    let Some(json) = json else { return Ok(None) };
    let kept = match json.first() {
        Some(b'[') => serde_json::from_slice::<Vec<Flat>>(&json).map(|tree| Kept {
            tree,
            markdown_only: BTreeSet::new(),
            whole: BTreeMap::new(),
        }),
        _ => serde_json::from_slice::<Kept>(&json),
    };
    let read = kept.map_err(|e| e.to_string()).and_then(|kept| {
        let tree = unflatten(kept.tree).ok_or("its child counts do not add up")?;
        Ok(ReadBase {
            tree,
            markdown_only: kept.markdown_only,
            whole: kept.whole,
        })
    });
    match read {
        Ok(read) => Ok(Some(read)),
        Err(error) => {
            tracing::warn!(page = %page_id, base, %error, "a kept read base does not read back; treating it as gone");
            Ok(None)
        }
    }
}

/// A long page's markdown, as one read shows it: the export with ids of
/// whole top-level blocks from `start`, as many as fit in `limit` as
/// `measure` counts the text. A read's base names only the blocks it lists
/// ([`Window::listed`]), so a block past the end of what was listed, or one
/// too long for the window that another read can show, is read before it
/// is replaced, never written over unseen.
pub struct Window {
    pub markdown: String,
    /// The top-level blocks shown, whole, in page order.
    pub shown: Vec<Node>,
    /// Every top-level block the window shows, and every one too long for
    /// any read that it lists by id alone, in page order: what a read of it
    /// has seen, for its base. A block too long for any read is in the base
    /// as it stands, so it can be deleted or replaced whole, and a change
    /// made to it since is still caught. One a read can show, or show the
    /// blocks inside, is not: it is read before it is replaced.
    pub listed: Vec<Node>,
    /// The id of the last block listed, when more blocks follow it: where
    /// the next read starts.
    pub next: Option<String>,
}

impl Window {
    /// What a read of this window has seen, as its base: the blocks it
    /// listed, the ones it showed, and every block inside them, said to be
    /// shown as markdown alone.
    pub fn read_base(&self) -> ReadBase {
        let mut markdown_only = BTreeSet::new();
        for node in &self.shown {
            node.walk(&mut |n| {
                if let Some(id) = n.id() {
                    markdown_only.insert(id.to_string());
                }
            });
        }
        ReadBase {
            tree: self.listed.clone(),
            markdown_only,
            whole: BTreeMap::new(),
        }
    }
}

/// Whether a block too long to show whole is shown by the blocks inside it
/// (`get_page_content` with ids): it holds blocks, each with an id, as a
/// list holds items and a table rows. A paragraph or a code block holds
/// only text.
pub fn opens(node: &Node) -> bool {
    !node.content.is_empty() && node.content.iter().all(|n| n.id().is_some())
}

/// What a block too long to show where it is can be read as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TooLong {
    /// It holds blocks with ids: a read with ids shows them.
    Inside,
    /// A read with ids shows it whole: the window was too short for it.
    ById,
    /// No read shows it: it is in the base as it stands, to delete or
    /// replace whole.
    WholeOnly,
}

impl TooLong {
    /// `node`, too long for where it is, and whether a read with ids shows
    /// it whole.
    pub fn of(node: &Node, fits_a_read: bool) -> Self {
        if opens(node) {
            TooLong::Inside
        } else if fits_a_read {
            TooLong::ById
        } else {
            TooLong::WholeOnly
        }
    }

    /// What to do with it, as the comment that stands for it says.
    pub fn advice(self) -> &'static str {
        match self {
            TooLong::Inside => {
                "too long to show here; read it with get_page_content and ids to see the blocks inside it"
            }
            TooLong::ById => "too long to show here; read it with get_page_content, ids and this base",
            TooLong::WholeOnly => "too long to show in any read; delete it or replace it whole",
        }
    }
}

/// What to do with a block too long for a read with ids: read inside it,
/// or, for one too long for any read, delete it or replace it whole, which
/// needs no read (it is in the base as it stands).
pub fn too_long_advice(node: &Node) -> &'static str {
    TooLong::of(node, false).advice()
}

/// What stands for a block too long for the window: its id, so the model
/// can write beside it, and what to do with it.
fn too_long(node: &Node, kind: TooLong) -> String {
    let id = node.id().unwrap_or_default();
    format!("<!-- {id} -->\n<!-- {} -->", kind.advice())
}

/// See [`Window`]. A block too long for any window of `limit` is listed by
/// its id alone and the window goes on past it, so reading on always moves.
/// `fits_a_read` says whether a read with ids shows a block whole: a window
/// shorter than a read (the chat's view of an open page) says to read such
/// a block, and does not put it in the base unread.
pub fn markdown_window(
    tree: &[Node],
    start: usize,
    limit: usize,
    measure: impl Fn(&str) -> usize,
    fits_a_read: impl Fn(&Node) -> bool,
) -> Window {
    enum Part {
        Shown(usize),
        TooLong(usize, TooLong),
    }
    let separator = measure("\n\n");
    let mut parts: Vec<Part> = vec![];
    let mut used = 0;
    for (i, node) in tree.iter().enumerate().skip(start) {
        let gap = if parts.is_empty() { 0 } else { separator };
        let text = virtues_document::to_markdown_with_ids(std::slice::from_ref(node));
        let alone = measure(text.trim_end_matches('\n'));
        if used + gap + alone <= limit {
            used += gap + alone;
            parts.push(Part::Shown(i));
            continue;
        }
        let kind = TooLong::of(node, fits_a_read(node));
        match node.id() {
            Some(_) if alone > limit && used + gap + measure(&too_long(node, kind)) <= limit => {
                used += gap + measure(&too_long(node, kind));
                parts.push(Part::TooLong(i, kind));
            }
            _ => break,
        }
    }
    // Written run by run, so the text is the export's own; measured again,
    // since the export of a run can differ by a separator from its blocks'.
    loop {
        let mut pieces: Vec<String> = vec![];
        let mut run: Vec<Node> = vec![];
        for part in &parts {
            match part {
                Part::Shown(i) => run.push(tree[*i].clone()),
                Part::TooLong(i, kind) => {
                    if !run.is_empty() {
                        let text = virtues_document::to_markdown_with_ids(&run);
                        pieces.push(text.trim_end_matches('\n').to_string());
                        run.clear();
                    }
                    pieces.push(too_long(&tree[*i], *kind));
                }
            }
        }
        if !run.is_empty() {
            let text = virtues_document::to_markdown_with_ids(&run);
            pieces.push(text.trim_end_matches('\n').to_string());
        }
        let mut markdown = pieces.join("\n\n");
        markdown.push('\n');
        if measure(&markdown) > limit && parts.len() > 1 {
            parts.pop();
            continue;
        }
        let end = match parts.last() {
            Some(Part::Shown(i) | Part::TooLong(i, _)) => i + 1,
            None => start,
        };
        let next = (end < tree.len())
            .then(|| end.checked_sub(1).and_then(|i| tree[i].id()))
            .flatten()
            .map(str::to_string);
        let shown = parts
            .iter()
            .filter_map(|p| match p {
                Part::Shown(i) => Some(tree[*i].clone()),
                Part::TooLong(..) => None,
            })
            .collect();
        let listed = parts
            .iter()
            .filter_map(|p| match p {
                Part::Shown(i) | Part::TooLong(i, TooLong::WholeOnly) => Some(tree[*i].clone()),
                Part::TooLong(..) => None,
            })
            .collect();
        return Window {
            markdown,
            shown,
            listed,
            next,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::pages::{self, PageFormat};

    async fn tree_page(pool: &PgPool, markdown: &str) -> String {
        pages::create_page_as(
            pool,
            pages::CreatePageRequest {
                title: "Trip".into(),
                content: markdown.into(),
                project_id: None,
                icon: None,
                icon_color: None,
                cover_url: None,
                tags: None,
                format: None,
            },
            PageFormat::Tree,
        )
        .await
        .unwrap()
        .page
        .id
    }

    async fn bases(pool: &PgPool, page_id: &str) -> Vec<String> {
        sqlx::query_scalar(
            "SELECT base FROM app_page_read_bases WHERE page_id = $1 ORDER BY updated_at",
        )
        .bind(page_id)
        .fetch_all(pool)
        .await
        .unwrap()
    }

    /// Top-level paragraphs with fixed ids, one per text.
    fn paragraphs(texts: &[&str]) -> Vec<Node> {
        let html: String = texts
            .iter()
            .enumerate()
            .map(|(i, t)| format!("<p data-id=\"block{i:03}\">{t}</p>"))
            .collect();
        let o = virtues_document::parse_html(&html, "doc");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        o.nodes
    }

    /// Paragraphs `count` of them, about `chars` characters each, with ids.
    fn long_paragraphs(count: usize, chars: usize) -> Vec<Node> {
        let texts: Vec<String> = (0..count)
            .map(|i| {
                format!("Paragraph {i} ").repeat(chars / 10 + 1)[..chars]
                    .trim_end()
                    .to_string()
            })
            .collect();
        paragraphs(&texts.iter().map(String::as_str).collect::<Vec<_>>())
    }

    #[test]
    fn a_window_that_holds_the_page_is_its_export() {
        let tree = long_paragraphs(20, 50);
        let w = markdown_window(&tree, 0, usize::MAX, str::len, |_| false);
        assert_eq!(w.markdown, virtues_document::to_markdown_with_ids(&tree));
        assert_eq!(w.shown, tree);
        assert_eq!(w.listed, tree);
        assert_eq!(w.next, None);
    }

    /// A window ends between blocks, never inside one, and names where the
    /// next read starts; reading on from there shows the rest.
    #[test]
    fn a_window_ends_between_blocks_and_reads_on() {
        let tree = long_paragraphs(12, 1_000);
        let chars = |s: &str| s.chars().count();
        let first = markdown_window(&tree, 0, 10_000, chars, |_| false);
        assert!(chars(&first.markdown) <= 10_000);
        let k = first.shown.len();
        assert!(k > 0 && k < tree.len(), "{k}");
        assert_eq!(first.shown, tree[..k]);
        assert_eq!(
            first.markdown,
            virtues_document::to_markdown_with_ids(&tree[..k])
        );
        assert!(!first.markdown.contains(tree[k].id().unwrap()));
        assert_eq!(first.next.as_deref(), tree[k - 1].id());

        let mut seen = first.shown.clone();
        let mut next = first.next;
        while let Some(after) = next {
            let at = tree
                .iter()
                .position(|n| n.id() == Some(after.as_str()))
                .unwrap();
            let w = markdown_window(&tree, at + 1, 10_000, chars, |_| false);
            assert!(!w.shown.is_empty());
            seen.extend(w.shown);
            next = w.next;
        }
        assert_eq!(seen, tree);
    }

    /// A block too long for any window is listed by its id and passed over,
    /// so a read moves on. It is not among what the read showed. Too long
    /// for any read, it is among what the read listed, which the base
    /// holds, to delete or replace whole.
    #[test]
    fn a_block_too_long_for_any_window_is_listed_not_shown() {
        let long = "Long. ".repeat(1_000);
        let tree = paragraphs(&["Short one.", &long, "Short two.", "Short three."]);
        let w = markdown_window(&tree, 0, 1_000, str::len, |_| false);
        assert_eq!(w.shown, [tree[0].clone(), tree[2].clone(), tree[3].clone()]);
        assert_eq!(w.listed, tree);
        assert_eq!(
            w.markdown,
            "<!-- block000 -->\nShort one.\n\n\
             <!-- block001 -->\n<!-- too long to show in any read; delete it or replace it whole -->\n\n\
             <!-- block002 -->\nShort two.\n\n<!-- block003 -->\nShort three.\n"
        );
        assert_eq!(w.next, None);

        // A list too long for any window says to read inside it, and is not
        // in the base: replacing it unread would write over what it holds.
        let items: String = (0..200).map(|i| format!("<li><p>Item {i}.</p></li>")).collect();
        let o = virtues_document::parse_html(&format!("<p>Before.</p><ul>{items}</ul>"), "doc");
        let mut tree = o.nodes;
        virtues_document::ensure_ids(&mut tree);
        let w = markdown_window(&tree, 0, 1_000, str::len, |_| false);
        assert_eq!(w.listed, tree[..1]);
        assert!(w.markdown.contains("read it with get_page_content and ids"), "{}", w.markdown);
    }

    /// A window shorter than a read (the chat's view of an open page) says
    /// to read a block it has no room for that a read shows whole, and
    /// keeps it out of the base: it is read before it is replaced. Only a
    /// block no read shows is in the base unread.
    #[test]
    fn a_block_too_long_for_the_window_but_not_for_a_read_is_read_first() {
        let long = "Long. ".repeat(1_000);
        let tree = paragraphs(&["Short one.", &long, "Short two."]);
        let w = markdown_window(&tree, 0, 1_000, str::len, |_| true);
        assert_eq!(w.shown, [tree[0].clone(), tree[2].clone()]);
        assert_eq!(w.listed, [tree[0].clone(), tree[2].clone()]);
        assert!(
            w.markdown.contains(
                "<!-- block001 -->\n<!-- too long to show here; read it with get_page_content, ids and this base -->"
            ),
            "{}",
            w.markdown
        );
        assert!(!w.markdown.contains("any read"), "{}", w.markdown);
    }

    #[sqlx::test]
    async fn a_read_base_is_kept_once_and_named_by_its_tree(pool: PgPool) {
        let page_id = tree_page(&pool, "One.\n").await;
        // A NUL in the text, which a jsonb column would refuse.
        let mut tree = paragraphs(&["One.", "Two."]);
        tree[1].content = vec![Node::text("Tw\u{0}o.", vec![])];

        let base = keep_read_base(&pool, &page_id, &ReadBase::of(tree.clone())).await.unwrap();
        assert_eq!(base.len(), 16);
        assert!(base.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(base, tree_hash(&tree));
        assert_eq!(keep_read_base(&pool, &page_id, &ReadBase::of(tree.clone())).await.unwrap(), base);
        assert_eq!(bases(&pool, &page_id).await, [base.as_str()], "one row");

        // Deleting a block names a different tree.
        let fewer = tree[..1].to_vec();
        assert_ne!(tree_hash(&fewer), base);
        // So does an id alone.
        let mut renamed = tree.clone();
        renamed[0]
            .attrs
            .insert("id".into(), serde_json::Value::String("other001".into()));
        assert_ne!(tree_hash(&renamed), base);

        // A restart: a new server reads the base back from the table.
        let after_restart = crate::server::yjs::YjsState::new(pool.clone());
        let read = read_base(&after_restart.pool, &page_id, &base).await.unwrap();
        assert_eq!(read, Some(ReadBase::of(tree.clone())));
        assert_eq!(read_base(&pool, &page_id, "0000000000000000").await.unwrap(), None);

        // A base that says how its blocks were shown is named by all of it,
        // and reads back whole.
        let mut said = ReadBase::of(tree.clone());
        said.markdown_only.insert("block000".into());
        said.whole.insert("block001".into(), node_hash(&tree[1]));
        let named = keep_read_base(&pool, &page_id, &said).await.unwrap();
        assert_ne!(named, base);
        assert_eq!(read_base(&pool, &page_id, &named).await.unwrap(), Some(said));
    }

    /// A page as deep as the contract allows: quotes nested to
    /// `MAX_DEPTH`, a list nested nearly as far, and marks, attributes and a
    /// mention along the way.
    fn deepest_page() -> Vec<Node> {
        let quotes = virtues_document::MAX_DEPTH - 1;
        let lists = (virtues_document::MAX_DEPTH - 1) / 2;
        let html = format!(
            "<p data-id=\"top00001\">Lunch with <virtues-mention to=\"/person/person_1\" label=\"Nick\">\
             </virtues-mention> <strong><em>on Friday</em></strong>.</p>\
             {}<p>Deep.</p>{}{}{}<h2>After</h2>",
            "<blockquote>".repeat(quotes),
            "</blockquote>".repeat(quotes),
            "<ul><li><p>Level.</p>".repeat(lists),
            "</li></ul>".repeat(lists),
        );
        let o = virtues_document::parse_html(&html, "doc");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        let mut nodes = o.nodes;
        virtues_document::ensure_ids(&mut nodes);
        assert_eq!(virtues_document::model::depth(&nodes), virtues_document::MAX_DEPTH);
        nodes
    }

    #[test]
    fn a_tree_flattens_and_reads_back_whole() {
        let tree = deepest_page();
        assert_eq!(unflatten(flatten(&tree)), Some(tree.clone()));
        assert_eq!(unflatten(flatten(&[])), Some(vec![]));
        // Counts that do not add up read as nothing, not as part of a tree.
        let mut cut = flatten(&tree);
        cut.pop();
        assert_eq!(unflatten(cut), None);
        let mut extra = flatten(&tree[..1]);
        extra[0].children += 1;
        assert_eq!(unflatten(extra), None);
    }

    /// A base is kept for every read, so one the contract allows has to read
    /// back, however deep: otherwise every edit of that page fails.
    #[sqlx::test]
    async fn a_base_as_deep_as_the_contract_allows_reads_back(pool: PgPool) {
        let page_id = tree_page(&pool, "One.\n").await;
        let tree = deepest_page();
        let base = keep_read_base(&pool, &page_id, &ReadBase::of(tree.clone())).await.unwrap();
        assert_eq!(read_base(&pool, &page_id, &base).await.unwrap(), Some(ReadBase::of(tree)));
    }

    /// A kept row that does not read back is a base the server does not
    /// have, so an edit falls back as it does when the base was pruned.
    #[sqlx::test]
    async fn a_base_that_does_not_read_back_is_none(pool: PgPool) {
        let page_id = tree_page(&pool, "One.\n").await;
        sqlx::query(
            "INSERT INTO app_page_read_bases (page_id, base, tree_json) VALUES ($1, $2, $3)",
        )
        .bind(&page_id)
        .bind("00000000000000aa")
        .bind(b"[{\"type\":\"paragraph\",\"children\":2}]".as_slice())
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(read_base(&pool, &page_id, "00000000000000aa").await.unwrap(), None);
    }

    #[sqlx::test]
    async fn read_bases_are_pruned_by_count_and_age_and_go_with_the_page(pool: PgPool) {
        let page_id = tree_page(&pool, "One.\n").await;
        let mut kept = vec![];
        for i in 0..=KEEP_PER_PAGE {
            let text = format!("Version {i}.");
            kept.push(keep_read_base(&pool, &page_id, &ReadBase::of(paragraphs(&[&text]))).await.unwrap());
        }
        let now = bases(&pool, &page_id).await;
        assert_eq!(now.len() as i64, KEEP_PER_PAGE);
        assert_eq!(now, kept[1..], "the oldest went");
        assert_eq!(read_base(&pool, &page_id, &kept[0]).await.unwrap(), None);

        // A base not recorded for a week goes, and is not read meanwhile.
        sqlx::query(
            "UPDATE app_page_read_bases SET updated_at = now() - interval '8 days' \
             WHERE page_id = $1 AND base = $2",
        )
        .bind(&page_id)
        .bind(&kept[1])
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(read_base(&pool, &page_id, &kept[1]).await.unwrap(), None);
        let other = tree_page(&pool, "Two.\n").await;
        keep_read_base(&pool, &other, &ReadBase::of(paragraphs(&["Two."]))).await.unwrap();
        assert!(!bases(&pool, &page_id).await.contains(&kept[1]));

        // Trash keeps them; purging the page takes them.
        pages::delete_page(&pool, &page_id).await.unwrap();
        assert_eq!(bases(&pool, &page_id).await.len() as i64, KEEP_PER_PAGE - 1);
        crate::api::trash::purge_trashed(&pool, crate::api::trash::TrashKind::Page, &page_id)
            .await
            .unwrap();
        assert!(bases(&pool, &page_id).await.is_empty());
        assert_eq!(bases(&pool, &other).await.len(), 1);
    }
}
