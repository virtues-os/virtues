//! Read bases: what the model has been shown of a block page
//! (`virtues_document::Seen`), kept so its next edit is checked against
//! exactly that (`YjsState::edit_tree`).
//!
//! A base is named by a hash of what it holds, so it changes when a read
//! shows anything new, and a base that is a read of the whole page as it
//! stands is named by the page's own tree hash, as a read of an unchanged
//! page names it. Bases live in `app_page_read_bases` rather than beside
//! the cached doc: an edit can arrive after the doc has been evicted, or
//! after a restart.

use crate::error::{Error, Result};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use virtues_document::{Node, Part, Seen, Shown};

/// The most bases kept for one page; older ones are pruned as new ones are
/// kept. A model reads a page a few times per edit, so fifty spans several
/// conversations at once.
pub const KEEP_PER_PAGE: i64 = 50;

/// How long a base is kept after it was last recorded. An edit from a read
/// older than this is treated as having no base.
pub const KEEP_DAYS: i64 = 7;

/// The tag a base made in a chat turn carries after its hash
/// (`<hash>.<tag>`): the first bytes of a hash of the turn.
fn turn_tag(turn: &str) -> String {
    hex::encode(&Sha256::digest(turn.as_bytes())[..4])
}

/// Whether `base` may be used in `turn`. A chat turn's tool results replay
/// to later turns clipped (`api::compaction`: 32 KiB for the last reply, 2
/// KiB before it), while a base would still let an edit write over all a
/// read showed; a later turn's model holds only part of what such a base
/// says it saw, and a replace written from that part drops the rest. So in
/// a turn only a base that turn made serves: a later one reads again, or
/// takes the open page's base, which each turn makes afresh. Outside a turn
/// (the CLI, an applet's run), any base serves.
pub fn serves_turn(base: &str, turn: Option<&str>) -> bool {
    match turn {
        None => true,
        Some(turn) => base.split_once('.').is_some_and(|(_, tag)| tag == turn_tag(turn)),
    }
}

/// The hash a base is named by, its turn's tag left off.
pub fn base_hash(base: &str) -> &str {
    base.split_once('.').map_or(base, |(hash, _)| hash)
}

/// Keep `seen` as a base of the page and return its name: the tree hash of
/// `page` (the page as it stands) when `seen` is a read of it whole, which
/// is the name a read of an unchanged page gives, or else the hash of what
/// `seen` holds; made in a chat `turn`, the hash carries that turn's tag
/// ([`serves_turn`]). Keeping a base that is already kept only marks it
/// recent. Prunes the page's bases past [`KEEP_PER_PAGE`] and every base
/// older than [`KEEP_DAYS`].
pub async fn keep_read_base(
    pool: &PgPool,
    page_id: &str,
    seen: &Seen,
    page: &[Node],
    turn: Option<&str>,
) -> Result<String> {
    let json = serde_json::to_vec(seen)
        .map_err(|e| Error::Other(format!("could not encode the page's read base: {e}")))?;
    let hash = if seen.is_page(page) {
        virtues_document::tree_hash(page)
    } else {
        hex::encode(&Sha256::digest(&json)[..8])
    };
    let base = match turn {
        Some(turn) => format!("{hash}.{}", turn_tag(turn)),
        None => hash,
    };
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

/// The base kept as `base` for the page, or `None` when it was never kept,
/// was pruned, or is older than [`KEEP_DAYS`]. A kept row that does not read
/// back as a base is logged and is `None` too: a base the server cannot read
/// is one it does not have, and the edit falls back as it does for a pruned
/// one. A row kept before bases were kept block by block is such a row.
pub async fn read_base(pool: &PgPool, page_id: &str, base: &str) -> Result<Option<Seen>> {
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
    match serde_json::from_slice::<Seen>(&json) {
        Ok(seen) => Ok(Some(seen)),
        Err(error) => {
            tracing::warn!(page = %page_id, base, %error, "a kept read base does not read back; treating it as gone");
            Ok(None)
        }
    }
}

/// A long page's markdown, as one read shows it: whole top-level blocks from
/// `start`, as many as fit in `limit` as `measure` counts the text, each
/// under its id ([`Shown::text`]). What it showed is what a base learns from
/// it, so a block past the end of the window, or one too long for it that
/// another read can show, is read before it is replaced.
pub struct Window {
    pub shown: Shown,
    /// The id of the last block listed, when more blocks follow it: where
    /// the next read starts.
    pub next: Option<String>,
}

/// Whether a block too long to show whole is shown by the blocks inside it
/// (`get_page_content` with ids): it holds blocks, each with an id, as a
/// list holds items and a table rows. A paragraph or a code block holds
/// only text. A table row holds its blocks inside cells, which carry no id,
/// so a row too long for any read is listed whole; the blocks in its cells
/// are still read by id, and an edit that would take them unread is refused
/// with their ids to read.
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
    /// No read shows it: an op naming it deletes it, or replaces it whole,
    /// while it stays as it was when listed.
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
            TooLong::WholeOnly => "too long to show in any read; delete it or replace it whole, by its id",
        }
    }
}

/// What stands for a block too long for where it is: a comment naming it,
/// so the model can write beside it, with what to do with it. One too long
/// for any read carries its hash: an op naming it takes it while it stays
/// as listed.
pub fn listed(node: &Node, kind: TooLong) -> Part {
    Part::Listed {
        id: node.id().unwrap_or_default().to_string(),
        say: kind.advice().to_string(),
        opaque: (kind == TooLong::WholeOnly).then(|| virtues_document::node_hash(node)),
    }
}

/// See [`Window`]. A block too long for any window of `limit` is listed by
/// its id alone and the window goes on past it, so reading on always moves.
/// `fits_a_read` says whether a read with ids shows a block whole: a window
/// shorter than a read (the chat's view of an open page) says to read such
/// a block.
pub fn markdown_window(
    tree: &[Node],
    start: usize,
    limit: usize,
    measure: impl Fn(&str) -> usize,
    fits_a_read: impl Fn(&Node) -> bool,
) -> Window {
    let separator = measure("\n\n");
    let mut parts: Vec<(usize, Part)> = vec![];
    let mut used = 0;
    for (i, node) in tree.iter().enumerate().skip(start) {
        let gap = if parts.is_empty() { 0 } else { separator };
        let text = virtues_document::to_markdown_with_ids(std::slice::from_ref(node));
        let alone = measure(text.trim_end_matches('\n'));
        if used + gap + alone <= limit {
            used += gap + alone;
            parts.push((i, Part::Whole(node.clone())));
            continue;
        }
        let part = listed(node, TooLong::of(node, fits_a_read(node)));
        let said = measure(Shown::markdown(vec![part.clone()]).text().trim_end_matches('\n'));
        match node.id() {
            Some(_) if alone > limit && used + gap + said <= limit => {
                used += gap + said;
                parts.push((i, part));
            }
            _ => break,
        }
    }
    // Measured again as a whole, since the export of a run can differ by a
    // separator from its blocks'.
    loop {
        let shown = Shown::markdown(parts.iter().map(|(_, p)| p.clone()).collect());
        if measure(&shown.text()) > limit && parts.len() > 1 {
            parts.pop();
            continue;
        }
        let end = parts.last().map_or(start, |(i, _)| i + 1);
        let next = (end < tree.len())
            .then(|| end.checked_sub(1).and_then(|i| tree[i].id()))
            .flatten()
            .map(str::to_string);
        return Window { shown, next };
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
                format: Some(crate::api::pages::PageFormat::Markdown),
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

    /// The blocks a window shows whole, in page order.
    fn wholes(w: &Window) -> Vec<Node> {
        w.shown
            .parts
            .iter()
            .filter_map(|p| match p {
                Part::Whole(n) => Some(n.clone()),
                _ => None,
            })
            .collect()
    }

    /// What a base learns from a window of `tree` alone.
    fn learned(w: &Window, tree: &[Node]) -> Seen {
        let mut seen = Seen::default();
        seen.learn(&w.shown, tree, &Default::default());
        seen
    }

    #[test]
    fn a_window_that_holds_the_page_is_its_export() {
        let tree = long_paragraphs(20, 50);
        let w = markdown_window(&tree, 0, usize::MAX, str::len, |_| false);
        assert_eq!(w.shown.text(), virtues_document::to_markdown_with_ids(&tree));
        assert_eq!(wholes(&w), tree);
        let seen = learned(&w, &tree);
        for n in &tree {
            assert_eq!(seen.known(n.id().unwrap()).unwrap().form, virtues_document::Form::Markdown);
        }
        assert_eq!(w.next, None);
    }

    /// A window ends between blocks, never inside one, and names where the
    /// next read starts; reading on from there shows the rest.
    #[test]
    fn a_window_ends_between_blocks_and_reads_on() {
        let tree = long_paragraphs(12, 1_000);
        let chars = |s: &str| s.chars().count();
        let first = markdown_window(&tree, 0, 10_000, chars, |_| false);
        assert!(chars(&first.shown.text()) <= 10_000);
        let k = wholes(&first).len();
        assert!(k > 0 && k < tree.len(), "{k}");
        assert_eq!(wholes(&first), tree[..k]);
        assert_eq!(first.shown.text(), virtues_document::to_markdown_with_ids(&tree[..k]));
        assert!(!first.shown.text().contains(tree[k].id().unwrap()));
        assert_eq!(first.next.as_deref(), tree[k - 1].id());

        let mut seen = wholes(&first);
        let mut next = first.next;
        while let Some(after) = next {
            let at = tree
                .iter()
                .position(|n| n.id() == Some(after.as_str()))
                .unwrap();
            let w = markdown_window(&tree, at + 1, 10_000, chars, |_| false);
            assert!(!wholes(&w).is_empty());
            seen.extend(wholes(&w));
            next = w.next;
        }
        assert_eq!(seen, tree);
    }

    /// A block too long for any window is listed by its id and passed over,
    /// so a read moves on. It is not among what the read showed: too long
    /// for any read, it is listed with its hash, which an op naming it
    /// takes it by.
    #[test]
    fn a_block_too_long_for_any_window_is_listed_not_shown() {
        let long = "Long. ".repeat(1_000);
        let tree = paragraphs(&["Short one.", &long, "Short two.", "Short three."]);
        let w = markdown_window(&tree, 0, 1_000, str::len, |_| false);
        assert_eq!(wholes(&w), [tree[0].clone(), tree[2].clone(), tree[3].clone()]);
        assert_eq!(
            w.shown.text(),
            "<!-- block000 -->\nShort one.\n\n\
             <!-- block001 -->\n<!-- too long to show in any read; delete it or replace it whole, by its id -->\n\n\
             <!-- block002 -->\nShort two.\n\n<!-- block003 -->\nShort three.\n"
        );
        assert_eq!(w.next, None);
        let seen = learned(&w, &tree);
        assert!(!seen.knows("block001"));
        assert_eq!(seen.opaque_hash("block001"), Some(virtues_document::node_hash(&tree[1]).as_str()));

        // A list too long for any window says to read inside it, and is
        // neither known nor listed whole: replacing it unread would write
        // over what it holds.
        let items: String = (0..200).map(|i| format!("<li><p>Item {i}.</p></li>")).collect();
        let o = virtues_document::parse_html(&format!("<p>Before.</p><ul>{items}</ul>"), "doc");
        let mut tree = o.nodes;
        virtues_document::ensure_ids(&mut tree);
        let w = markdown_window(&tree, 0, 1_000, str::len, |_| false);
        assert_eq!(wholes(&w), tree[..1]);
        assert!(w.shown.text().contains("read it with get_page_content and ids"), "{}", w.shown.text());
        let list = tree[1].id().unwrap();
        assert!(!learned(&w, &tree).knows(list) && learned(&w, &tree).opaque_hash(list).is_none());
    }

    /// A window shorter than a read (the chat's view of an open page) says
    /// to read a block it has no room for that a read shows whole: it is
    /// read before it is replaced.
    #[test]
    fn a_block_too_long_for_the_window_but_not_for_a_read_is_read_first() {
        let long = "Long. ".repeat(1_000);
        let tree = paragraphs(&["Short one.", &long, "Short two."]);
        let w = markdown_window(&tree, 0, 1_000, str::len, |_| true);
        assert_eq!(wholes(&w), [tree[0].clone(), tree[2].clone()]);
        assert!(
            w.shown.text().contains(
                "<!-- block001 -->\n<!-- too long to show here; read it with get_page_content, ids and this base -->"
            ),
            "{}",
            w.shown.text()
        );
        assert!(!w.shown.text().contains("any read"), "{}", w.shown.text());
        let seen = learned(&w, &tree);
        assert!(!seen.knows("block001") && seen.opaque_hash("block001").is_none());
    }

    #[sqlx::test]
    async fn a_read_base_is_kept_once_and_named_by_its_tree(pool: PgPool) {
        let page_id = tree_page(&pool, "One.\n").await;
        // A NUL in the text, which a jsonb column would refuse.
        let mut tree = paragraphs(&["One.", "Two."]);
        tree[1].content = vec![Node::text("Tw\u{0}o.", vec![])];
        let whole = Seen::of_page(&tree);

        let base = keep_read_base(&pool, &page_id, &whole, &tree, None).await.unwrap();
        assert_eq!(base.len(), 16);
        assert!(base.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(base, virtues_document::tree_hash(&tree));
        assert_eq!(keep_read_base(&pool, &page_id, &whole, &tree, None).await.unwrap(), base);
        assert_eq!(bases(&pool, &page_id).await, [base.as_str()], "one row");

        // Deleting a block names a different tree.
        let fewer = tree[..1].to_vec();
        assert_ne!(virtues_document::tree_hash(&fewer), base);
        // So does an id alone.
        let mut renamed = tree.clone();
        renamed[0]
            .attrs
            .insert("id".into(), serde_json::Value::String("other001".into()));
        assert_ne!(virtues_document::tree_hash(&renamed), base);

        // A restart: a new server reads the base back from the table.
        let after_restart = crate::server::yjs::YjsState::new(pool.clone());
        let read = read_base(&after_restart.pool, &page_id, &base).await.unwrap();
        assert_eq!(read, Some(whole.clone()));
        assert_eq!(read_base(&pool, &page_id, "0000000000000000").await.unwrap(), None);

        // A base that is not a read of the page whole is named by all it
        // holds, and reads back whole.
        let mut part = Seen::default();
        part.learn(&Shown::markdown(vec![Part::Whole(tree[0].clone())]), &tree, &Default::default());
        let named = keep_read_base(&pool, &page_id, &part, &tree, None).await.unwrap();
        assert_ne!(named, base);
        assert_eq!(read_base(&pool, &page_id, &named).await.unwrap(), Some(part));
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

    /// A base is kept for every read, so one the contract allows has to read
    /// back, however deep the page: a base holds each block by itself, so
    /// it nests no deeper than a row's cells.
    #[sqlx::test]
    async fn a_base_as_deep_as_the_contract_allows_reads_back(pool: PgPool) {
        let page_id = tree_page(&pool, "One.\n").await;
        let tree = deepest_page();
        let seen = Seen::of_page(&tree);
        let base = keep_read_base(&pool, &page_id, &seen, &tree, None).await.unwrap();
        let read = read_base(&pool, &page_id, &base).await.unwrap().unwrap();
        assert_eq!(read, seen);
        assert_eq!(read.subtree(tree[1].id().unwrap()).as_ref(), Some(&tree[1]));
    }

    /// A kept row that does not read back is a base the server does not
    /// have, so an edit falls back as it does when the base was pruned: a
    /// row in no shape a base takes, and one kept as a tree before bases
    /// were kept block by block.
    #[sqlx::test]
    async fn a_base_that_does_not_read_back_is_none(pool: PgPool) {
        let page_id = tree_page(&pool, "One.\n").await;
        for (base, json) in [
            ("00000000000000aa", b"[{\"type\":\"paragraph\",\"children\":2}]".as_slice()),
            ("00000000000000bb", b"{\"tree\":[{\"type\":\"paragraph\"}],\"markdown_only\":[\"a\"]}".as_slice()),
        ] {
            sqlx::query(
                "INSERT INTO app_page_read_bases (page_id, base, tree_json) VALUES ($1, $2, $3)",
            )
            .bind(&page_id)
            .bind(base)
            .bind(json)
            .execute(&pool)
            .await
            .unwrap();
            assert_eq!(read_base(&pool, &page_id, base).await.unwrap(), None, "{base}");
        }
    }

    #[sqlx::test]
    async fn read_bases_are_pruned_by_count_and_age_and_go_with_the_page(pool: PgPool) {
        let page_id = tree_page(&pool, "One.\n").await;
        let mut kept = vec![];
        for i in 0..=KEEP_PER_PAGE {
            let text = format!("Version {i}.");
            let tree = paragraphs(&[&text]);
            kept.push(keep_read_base(&pool, &page_id, &Seen::of_page(&tree), &tree, None).await.unwrap());
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
        let two = paragraphs(&["Two."]);
        keep_read_base(&pool, &other, &Seen::of_page(&two), &two, None).await.unwrap();
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
