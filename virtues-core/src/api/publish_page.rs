//! Freezing a page for sharing: its text, rendered once to a standalone HTML
//! file the door serves (`api::publications`).
//!
//! What a page carries out, and only that:
//!
//! - **Its text**: a markdown page rendered from its markdown, a block page
//!   (`PageFormat::Tree`) rendered from its tree as canonical HTML, which
//!   escapes every character typed, without block ids.
//! - **Names, not records.** A person, place or other ref in the page
//!   (`[⟦Nick⟧](/person/…)`, a mention) becomes its label alone: no link,
//!   nothing looked up. A page that mentions someone must not quietly carry
//!   their number. The names are listed so the Share modal can show them.
//! - **Its own images**, from Drive, inlined. An image from another site is
//!   left out (the loader's frame may not fetch anything), and counted, so the
//!   modal can say so.
//! - **What the owner accepted.** A pending proposal leaves as the text it
//!   would replace: what nobody accepted stays on the box.
//!
//! Applets, audio, video and files work only on the box. A block page leaves
//! them out, a file or a recording as its name, and counts them for the
//! modal.

use std::collections::{BTreeSet, HashMap};
use std::sync::OnceLock;

use base64::Engine;
use regex::Regex;
use serde_json::Value;
use sqlx::PgPool;
use virtues_document::{Mark, Node};

use crate::api::drive::DriveConfig;
use crate::api::pages::PageFormat;
use crate::error::{Error, Result};
use crate::server::yjs::page_text_of_state;

pub struct FrozenPage {
    pub title: String,
    pub html: String,
    /// The people, places and other things the page names, as their labels.
    pub names: Vec<String>,
    /// Images that are not in Drive and so are not in the shared page.
    pub images_left_out: usize,
    /// Applets, audio, video and files: they work only on the box, so the
    /// shared page leaves them out.
    pub embeds_left_out: usize,
}

/// The error for a block page whose document does not read. Publishing it
/// from `content`, an export that drops what markdown cannot hold, would
/// share a different page than the owner sees.
const UNREADABLE: &str = "Your server couldn't read this page's document, so it didn't share the page. \
                          Nothing on the page changed.";

/// The refusal for a block page holding a node or mark this server's
/// contract does not name: a newer version wrote it, and the HTML would
/// leave that part out without a word.
const NEWER: &str = "A newer version of Virtues wrote this page, so your server can't share all of it. \
                     Update your server, then share it again.";

#[derive(sqlx::FromRow)]
struct PageRow {
    title: String,
    content: String,
    yjs_state: Option<Vec<u8>>,
    icon: Option<String>,
    cover_url: Option<String>,
    #[sqlx(try_from = "String")]
    format: PageFormat,
}

/// The shared page's body, and what the Share modal says about it.
struct Body {
    html: String,
    names: Vec<String>,
    images_left_out: usize,
    embeds_left_out: usize,
}

fn image_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"!\[([^\]]*)\]\(\s*([^)\s]+)(?:\s+"[^"]*")?\s*\)"#).unwrap())
}

/// A link to something inside virtues: `[label](/kind/id)`, label optionally
/// in the ref brackets `⟦ ⟧`.
fn internal_link_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r#"\[⟦?([^\]]*?)⟧?\]\((/[a-z_]+/[^)\s]*)\)"#).unwrap())
}

/// Whether a ref to `route` names a person, a place or an organization, whose
/// label the Share modal lists. One rule for markdown links, links in a block
/// page and mentions.
fn names_someone(route: &str) -> bool {
    let kind = route.trim_start_matches('/').split('/').next().unwrap_or("");
    matches!(kind, "person" | "place" | "org" | "organization")
}

/// The Drive file an image URL points at, if it is one.
fn drive_file_id(url: &str) -> Option<&str> {
    if let Some(rest) = url.strip_prefix("/api/drive/files/") {
        return rest.strip_suffix("/download").filter(|id| !id.is_empty() && !id.contains('/'));
    }
    url.strip_prefix("/drive/").filter(|id| !id.is_empty() && !id.contains(['/', '?', '#']))
}

/// A Drive image as a `data:` URI, or `None` when it is not an image or
/// cannot be read.
async fn inline_image(pool: &PgPool, drive: &DriveConfig, url: &str) -> Option<String> {
    let id = drive_file_id(url)?;
    // Absent means left out: a file that is gone, trashed or unreadable is
    // counted with the images from other sites, not an error that stops the
    // share.
    let (file, bytes) = crate::api::drive::download_file(pool, drive, id).await.ok()?;
    let mime = file.mime_type.filter(|m| m.starts_with("image/"))?;
    Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

pub(crate) fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Labels for internal links, and which of them name people or places.
pub(crate) fn strip_internal_links(markdown: &str) -> (String, Vec<String>) {
    let mut names = Vec::new();
    let text = internal_link_re().replace_all(markdown, |c: &regex::Captures| {
        let label = c[1].trim().to_string();
        if names_someone(&c[2]) && !label.is_empty() {
            names.push(label.clone());
        }
        label
    });
    names.sort();
    names.dedup();
    (text.into_owned(), names)
}

pub(crate) fn render(markdown: &str) -> String {
    use pulldown_cmark::{html, Options, Parser};
    let options = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut out = String::new();
    html::push_html(&mut out, Parser::new_ext(markdown, options));
    out
}

// ---------------------------------------------------------------- block pages

/// What [`publishable`] took out of a block page, for the Share modal and for
/// the images still to fetch.
#[derive(Debug, Default, PartialEq)]
struct Collected {
    /// Labels of the people, places and organizations the page names, sorted,
    /// each once.
    names: Vec<String>,
    /// Applets, audio, video and files left out.
    embeds_left_out: usize,
    /// Every image's `src`, each once, for `tree_body` to fetch from Drive.
    images: BTreeSet<String>,
}

/// A string attribute, when set.
fn attr<'a>(node: &'a Node, key: &str) -> Option<&'a str> {
    node.attrs.get(key).and_then(Value::as_str)
}

/// A ref's label as it leaves: trimmed, without the ref brackets `⟦ ⟧` a day
/// article wraps a name in.
fn ref_label(raw: &str) -> &str {
    let label = raw.trim();
    let label = label.strip_prefix('⟦').unwrap_or(label);
    label.strip_suffix('⟧').unwrap_or(label).trim()
}

/// Whether a link works off the box: a scheme the contract allows (another
/// site, mail, a phone). Anything else is a path on the box, which a shared
/// page cannot reach.
fn leaves_the_box(href: &str) -> bool {
    virtues_document::contract::url_scheme(&virtues_document::contract::normalize_url(href))
        .is_some_and(|scheme| virtues_document::contract().url_schemes.contains(&scheme))
}

/// `text` in italics as a paragraph of its own, or nothing when it is blank:
/// what stands in for an image, a recording or a file left out.
fn italic_paragraph(text: Option<&str>) -> Option<Node> {
    let text = text.map(str::trim).filter(|t| !t.is_empty())?;
    let italic = Mark { kind: "italic".into(), attrs: Default::default() };
    Some(Node::element("paragraph", Default::default(), vec![Node::text(text, vec![italic])]))
}

/// A block page's tree as it may leave the box, and what was taken out.
///
/// | In the page | In the shared page |
/// |---|---|
/// | a mention | its label, as text |
/// | a link to a path on the box | its text, unlinked |
/// | a link to another site | kept |
/// | text a proposal inserts | nothing |
/// | text a proposal deletes | the text, unmarked |
/// | an applet | nothing |
/// | audio, video, a file | its name in italics, or nothing |
/// | an image | kept, for [`place_images`] |
///
/// Names count for a mention or a link to a person, place or organization.
fn publishable(nodes: Vec<Node>) -> (Vec<Node>, Collected) {
    let mut collected = Collected::default();
    let nodes = publish_blocks(nodes, &mut collected);
    collected.names.sort();
    collected.names.dedup();
    (nodes, collected)
}

fn publish_blocks(nodes: Vec<Node>, out: &mut Collected) -> Vec<Node> {
    let c = virtues_document::contract();
    let mut kept = Vec::with_capacity(nodes.len());
    for mut node in nodes {
        match node.kind.as_str() {
            "applet" => out.embeds_left_out += 1,
            "audio" | "video" | "file" => {
                out.embeds_left_out += 1;
                kept.extend(italic_paragraph(attr(&node, "name")));
            }
            "image" => {
                if let Some(src) = attr(&node, "src") {
                    out.images.insert(src.to_string());
                }
                kept.push(node);
            }
            kind if c.is_textblock(kind) => {
                node.content = publish_inline(std::mem::take(&mut node.content), out);
                kept.push(node);
            }
            _ => {
                node.content = publish_blocks(std::mem::take(&mut node.content), out);
                kept.push(node);
            }
        }
    }
    kept
}

/// A text block's content as it leaves. A run of text linked to one path on
/// the box loses its link and keeps its words, as `strip_internal_links`
/// does for markdown.
fn publish_inline(nodes: Vec<Node>, out: &mut Collected) -> Vec<Node> {
    let mut kept: Vec<Node> = Vec::with_capacity(nodes.len());
    // The run of text linked to a path on the box being read: the path, and
    // where the run starts in `kept`.
    let mut run: Option<(String, usize)> = None;
    for mut node in nodes {
        if node.marks.iter().any(|m| m.kind == "proposedInsertion") {
            continue;
        }
        node.marks.retain(|m| m.kind != "proposedDeletion");
        if node.kind == "mention" {
            let label = ref_label(attr(&node, "label").unwrap_or_default()).to_string();
            if !label.is_empty() && names_someone(attr(&node, "to").unwrap_or_default()) {
                out.names.push(label.clone());
            }
            node = Node::text(&label, std::mem::take(&mut node.marks));
        }
        let on_the_box = node
            .marks
            .iter()
            .position(|m| m.kind == "link" && !attr_of(m, "href").is_some_and(leaves_the_box))
            .map(|i| node.marks.remove(i))
            .map(|m| attr_of(&m, "href").unwrap_or_default().to_string());
        if run.as_ref().is_some_and(|(path, _)| on_the_box.as_ref() != Some(path)) {
            close_run(&mut kept, run.take(), out);
        }
        if let Some(path) = on_the_box {
            run.get_or_insert((path, kept.len()));
        }
        kept.push(node);
    }
    close_run(&mut kept, run.take(), out);
    kept.retain(|n| !n.is_text() || n.text.as_deref().is_some_and(|t| !t.is_empty()));
    kept
}

fn attr_of<'a>(mark: &'a Mark, key: &str) -> Option<&'a str> {
    mark.attrs.get(key).and_then(Value::as_str)
}

/// Ends a run of text that was linked to `path`: a label in the ref brackets
/// leaves without them, and a name's label is listed.
fn close_run(kept: &mut [Node], run: Option<(String, usize)>, out: &mut Collected) {
    let Some((path, start)) = run else { return };
    let run = &mut kept[start..];
    if let Some(first) = run.first_mut().and_then(|n| n.text.as_mut()) {
        if let Some(rest) = first.strip_prefix('⟦') {
            *first = rest.to_string();
        }
    }
    if let Some(last) = run.last_mut().and_then(|n| n.text.as_mut()) {
        if let Some(rest) = last.strip_suffix('⟧') {
            *last = rest.to_string();
        }
    }
    let label: String = run.iter().map(Node::text_content).collect();
    let label = ref_label(&label);
    if !label.is_empty() && names_someone(&path) {
        out.names.push(label.to_string());
    }
}

/// Whether every node and mark in `nodes` is one this server's contract
/// names. The sync server refuses anything else, so only a newer version of
/// Virtues writes one.
fn all_known(nodes: &[Node]) -> bool {
    let c = virtues_document::contract();
    nodes.iter().all(|n| {
        (n.is_text() || c.node(&n.kind).is_some())
            && n.marks.iter().all(|m| c.mark(&m.kind).is_some())
            && all_known(&n.content)
    })
}

/// Each image whose bytes `inlined` holds, by `src`, gets them as a `data:`
/// URI; any other becomes its alt in italics, or nothing, and is counted in
/// `left_out`.
fn place_images(nodes: Vec<Node>, inlined: &HashMap<String, String>, left_out: &mut usize) -> Vec<Node> {
    let mut kept = Vec::with_capacity(nodes.len());
    for mut node in nodes {
        if node.kind != "image" {
            if !node.content.is_empty() {
                node.content = place_images(std::mem::take(&mut node.content), inlined, left_out);
            }
            kept.push(node);
            continue;
        }
        match attr(&node, "src").and_then(|src| inlined.get(src)) {
            Some(data) => {
                node.attrs.insert("src".into(), Value::String(data.clone()));
                kept.push(node);
            }
            None => {
                *left_out += 1;
                kept.extend(italic_paragraph(attr(&node, "alt")));
            }
        }
    }
    kept
}

/// A block page's body. Its tree comes from the saved document; a document
/// that does not read publishes nothing rather than something blank.
async fn tree_body(pool: &PgPool, drive: &DriveConfig, page_id: &str, row: &PageRow) -> Result<Body> {
    let tree = match row.yjs_state.as_deref().map(page_text_of_state) {
        Some(Ok(text)) => text.tree.ok_or_else(|| "its saved document holds text, not blocks".to_string()),
        Some(Err(e)) => Err(format!("its saved document did not read: {e}")),
        None => Err("it has no saved document".to_string()),
    };
    let nodes = tree.map_err(|why| {
        tracing::error!(page_id, why = %why, "a block page was not shared");
        Error::Other(UNREADABLE.into())
    })?;
    if !all_known(&nodes) {
        return Err(Error::InvalidInput(NEWER.into()));
    }

    let (nodes, collected) = publishable(nodes);
    let mut inlined = HashMap::new();
    for src in &collected.images {
        if let Some(data) = inline_image(pool, drive, src).await {
            inlined.insert(src.clone(), data);
        }
    }
    let mut images_left_out = 0;
    let nodes = place_images(nodes, &inlined, &mut images_left_out);
    Ok(Body {
        html: virtues_document::to_html(&nodes, false),
        names: collected.names,
        images_left_out,
        embeds_left_out: collected.embeds_left_out,
    })
}

// ------------------------------------------------------------- markdown pages

/// A markdown page's body, from its saved document, or from `content` when
/// the document does not read.
async fn markdown_body(pool: &PgPool, drive: &DriveConfig, page_id: &str, row: &PageRow) -> Body {
    let markdown = match row.yjs_state.as_deref().map(page_text_of_state) {
        Some(Ok(text)) => text.markdown,
        Some(Err(error)) => {
            tracing::warn!(page_id, %error, "a page's saved document did not read; sharing its content");
            row.content.clone()
        }
        None => row.content.clone(),
    };

    // Images first: their URLs also start with `/`, and must not be taken
    // for internal links.
    let mut images_left_out = 0;
    let mut with_images = String::with_capacity(markdown.len());
    let mut last = 0;
    for c in image_re().captures_iter(&markdown) {
        let whole = c.get(0).expect("match");
        with_images.push_str(&markdown[last..whole.start()]);
        let alt = c[1].to_string();
        match inline_image(pool, drive, &c[2]).await {
            Some(data) => with_images.push_str(&format!("![{alt}]({data})")),
            None => {
                images_left_out += 1;
                if !alt.trim().is_empty() {
                    with_images.push_str(&format!("*{}*", alt.trim()));
                }
            }
        }
        last = whole.end();
    }
    with_images.push_str(&markdown[last..]);

    let (text, names) = strip_internal_links(&with_images);
    Body { html: render(&text), names, images_left_out, embeds_left_out: 0 }
}

const PAGE_CSS: &str = r#"
:root { color-scheme: light dark; --fg: #1d1f1e; --muted: #6a706d; --bg: #fdfcfa; --line: #e6e3dd; --code: #f2f0ec; --mark: #f6e9a8; --tip: #9fbf9a; --warn: #d6a54a; }
@media (prefers-color-scheme: dark) { :root { --fg: #e9ebe9; --muted: #9aa19e; --bg: #151716; --line: #2c302e; --code: #1f2321; --mark: #4d4521; --tip: #4c6e4a; --warn: #8a6a2c; } }
html, body { margin: 0; background: var(--bg); color: var(--fg); }
body { font: 18px/1.65 Georgia, "Iowan Old Style", "Times New Roman", serif; }
main { max-width: 680px; margin: 0 auto; padding: 40px 20px 80px; }
.cover { width: 100%; max-height: 320px; object-fit: cover; border-radius: 10px; margin-bottom: 28px; }
h1 { font-size: 2.1em; line-height: 1.2; font-weight: normal; margin: 0 0 24px; }
h2, h3, h4 { font-weight: normal; line-height: 1.3; margin: 1.6em 0 0.5em; }
p, ul, ol, blockquote, aside, table, pre { margin: 0 0 1em; }
li > p { margin: 0; }
li > * + * { margin-top: 0.4em; }
a { color: inherit; }
img { max-width: 100%; height: auto; border-radius: 6px; }
blockquote { border-left: 3px solid var(--line); padding-left: 16px; color: var(--muted); }
aside { border-left: 3px solid var(--line); padding-left: 16px; }
aside[data-tone="tip"] { border-left-color: var(--tip); }
aside[data-tone="warning"] { border-left-color: var(--warn); }
blockquote > :last-child, aside > :last-child { margin-bottom: 0; }
mark { background: var(--mark); color: inherit; padding: 0 0.1em; border-radius: 2px; }
u { text-underline-offset: 0.15em; }
code { font: 0.85em ui-monospace, Menlo, monospace; background: var(--code); padding: 0.1em 0.3em; border-radius: 4px; }
pre { font: 0.85em/1.5 ui-monospace, Menlo, monospace; background: var(--code); padding: 12px 14px; border-radius: 8px; overflow-x: auto; }
pre code { font: inherit; background: none; padding: 0; }
pre[data-language]::before { content: attr(data-language); display: block; color: var(--muted); font-size: 0.85em; margin-bottom: 6px; }
table { border-collapse: collapse; width: 100%; font-size: 0.9em; }
th, td { border-bottom: 1px solid var(--line); padding: 6px 8px; text-align: left; }
th[align="center"], td[align="center"] { text-align: center; }
th[align="right"], td[align="right"] { text-align: right; }
th > p, td > p { margin: 0; }
hr { border: 0; border-top: 1px solid var(--line); margin: 2em 0; }
li:has(> input[type="checkbox"]) { list-style: none; margin-left: -1.3em; }
li > input[type="checkbox"] { margin-right: 0.5em; }
ul[data-type="taskList"] { list-style: none; padding-left: 0; }
li[data-type="taskItem"] > p:first-child::before { content: "☐ "; }
li[data-type="taskItem"][data-checked="true"] > p:first-child::before { content: "☑ "; }
"#;

/// The page as one standalone file.
pub async fn freeze_page(pool: &PgPool, drive: &DriveConfig, page_id: &str) -> Result<FrozenPage> {
    let row: Option<PageRow> = sqlx::query_as(
        "SELECT title, content, yjs_state, icon, cover_url, format FROM app_pages \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(page_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("read page: {e}")))?;
    let row = row.ok_or_else(|| Error::NotFound(format!("no page {page_id:?}")))?;

    // The saved document. Edits from the last few seconds may still be on
    // their way to the database; Update re-freezes.
    let Body { html: body, names, mut images_left_out, embeds_left_out } = match row.format {
        PageFormat::Markdown => markdown_body(pool, drive, page_id, &row).await,
        PageFormat::Tree => tree_body(pool, drive, page_id, &row).await?,
    };

    let cover = match row.cover_url.as_deref() {
        Some(url) => match inline_image(pool, drive, url).await {
            Some(data) => format!(r#"<img class="cover" src="{data}" alt="">"#),
            None => {
                images_left_out += 1;
                String::new()
            }
        },
        None => String::new(),
    };
    // An emoji icon reads as part of the title; an icon name does not.
    let icon = row
        .icon
        .filter(|i| !i.is_empty() && !i.contains(':') && i.chars().count() <= 4)
        .map(|i| format!("{} ", escape(&i)))
        .unwrap_or_default();
    let title = escape(&row.title);
    let html = format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{title}</title>\n<style>{PAGE_CSS}</style>\n</head>\n<body>\n<main>\n{cover}\
         <h1>{icon}{title}</h1>\n{body}</main>\n</body>\n</html>\n"
    );
    Ok(FrozenPage { title: row.title, html, names, images_left_out, embeds_left_out })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refs_leave_as_names_only() {
        let md = "Dinner with [⟦Nick⟧](/person/person_devnick) at [⟦Roscioli⟧](/place/place_1), \
                  see [the plan](/page/page_9) and [a site](https://example.com).";
        let (text, names) = strip_internal_links(md);
        assert_eq!(
            text,
            "Dinner with Nick at Roscioli, see the plan and [a site](https://example.com)."
        );
        assert_eq!(names, vec!["Nick".to_string(), "Roscioli".to_string()]);
    }

    #[test]
    fn drive_image_urls_are_recognised() {
        assert_eq!(drive_file_id("/api/drive/files/df_1/download"), Some("df_1"));
        assert_eq!(drive_file_id("/drive/df_2"), Some("df_2"));
        assert_eq!(drive_file_id("https://example.com/a.png"), None);
        assert_eq!(drive_file_id("/api/drive/files/../x/download"), None);
    }

    #[test]
    fn markdown_renders_and_images_match() {
        let html = render("# Day 1\n\n- [x] Pantheon\n- [ ] Gelato\n\n| a | b |\n|---|---|\n| 1 | 2 |");
        assert!(html.contains("<h1>Day 1</h1>"));
        assert!(html.contains("<table>"));
        assert!(html.contains("checkbox"));
        let c = image_re().captures("![Colosseum](/api/drive/files/df_1/download \"t\")").unwrap();
        assert_eq!(&c[1], "Colosseum");
        assert_eq!(&c[2], "/api/drive/files/df_1/download");
    }

    /// A block page's tree from HTML the model's write path takes.
    fn tree(html: &str) -> Vec<Node> {
        let o = virtues_document::parse_html(html, "doc");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        o.nodes
    }

    fn shared(html: &str) -> (String, Collected) {
        let (nodes, collected) = publishable(tree(html));
        (virtues_document::to_html(&nodes, false), collected)
    }

    #[test]
    fn mentions_leave_as_their_labels_and_only_names_are_listed() {
        let (html, got) = shared(
            "<p>Lunch with <virtues-mention to=\"/person/person_1\" label=\"Nick\"></virtues-mention> and \
             <virtues-mention to=\"/person/person_2\" label=\"⟦David Okafor⟧\"></virtues-mention> at \
             <virtues-mention to=\"/place/place_1\" label=\"Roscioli\"></virtues-mention> for \
             <virtues-mention to=\"/org/org_1\" label=\"Example Co\"></virtues-mention>; see \
             <virtues-mention to=\"/page/page_abc\" label=\"Q3 plan\"></virtues-mention> and \
             <virtues-mention to=\"/person/person_1\" label=\"Nick\"></virtues-mention> again.</p>",
        );
        assert_eq!(
            html,
            "<p>Lunch with Nick and David Okafor at Roscioli for Example Co; see Q3 plan and Nick again.</p>"
        );
        assert_eq!(got.names, ["David Okafor", "Example Co", "Nick", "Roscioli"]);
        assert_eq!(got.embeds_left_out, 0);
    }

    #[test]
    fn links_on_the_box_lose_the_link_and_keep_their_words() {
        let (html, got) = shared(
            "<p>Ask <a href=\"/person/person_1\">⟦Nick⟧</a> about \
             <a href=\"/person/person_2\"><strong>David</strong> Okafor</a>, read \
             <a href=\"/page/page_9\">the plan</a>, open <a href=\"#notes\">notes</a>, and see \
             <a href=\"https://example.com/rome\">a <em>site</em></a> or \
             <a href=\"mailto:nick@example.com\">mail</a>.</p>",
        );
        assert_eq!(
            html,
            "<p>Ask Nick about <strong>David</strong> Okafor, read the plan, open notes, and see \
             <a href=\"https://example.com/rome\">a <em>site</em></a> or \
             <a href=\"mailto:nick@example.com\">mail</a>.</p>"
        );
        assert_eq!(got.names, ["David Okafor", "Nick"]);
    }

    #[test]
    fn a_pending_proposal_leaves_as_the_text_it_would_replace() {
        let (html, _) = shared(
            "<p>Dinner on <virtues-del proposal=\"p1\"><strong>Friday</strong></virtues-del>\
             <virtues-ins proposal=\"p1\"><strong>Saturday</strong></virtues-ins> at eight\
             <virtues-ins proposal=\"p2\">, with dessert</virtues-ins>.</p>\
             <p><virtues-ins proposal=\"p3\">A whole new line.</virtues-ins></p>",
        );
        assert_eq!(html, "<p>Dinner on <strong>Friday</strong> at eight.</p><p></p>");
    }

    #[test]
    fn applets_and_media_stay_on_the_box_and_are_counted() {
        let (html, got) = shared(
            "<p>Before</p>\
             <virtues-applet ref=\"applet_user__rome\" height=\"300\"></virtues-applet>\
             <audio src=\"/drive/df_3\" data-name=\"memo.m4a\"></audio>\
             <video src=\"/drive/df_2\"></video>\
             <ul><li><p>Item</p><virtues-file src=\"/drive/df_1\" data-name=\"report.pdf\"></virtues-file></li></ul>\
             <img src=\"/drive/df_4\" alt=\"Harbour\">\
             <img src=\"https://images.example.com/photo-1\">\
             <img src=\"/drive/df_4\" alt=\"Harbour again\">",
        );
        assert_eq!(got.embeds_left_out, 4, "an applet, audio, video and a file");
        assert_eq!(
            got.images.iter().map(String::as_str).collect::<Vec<_>>(),
            ["/drive/df_4", "https://images.example.com/photo-1"],
            "each image once, for fetching"
        );
        assert_eq!(
            html,
            "<p>Before</p><p><em>memo.m4a</em></p>\
             <ul><li><p>Item</p><p><em>report.pdf</em></p></li></ul>\
             <img src=\"/drive/df_4\" alt=\"Harbour\">\
             <img src=\"https://images.example.com/photo-1\">\
             <img src=\"/drive/df_4\" alt=\"Harbour again\">"
        );
    }

    #[test]
    fn drive_images_are_inlined_and_the_rest_left_out_with_their_alt() {
        let (nodes, _) = publishable(tree(
            "<img src=\"/drive/df_4\" alt=\"Harbour\" width=\"600\">\
             <blockquote><img src=\"https://images.example.com/photo-1\" alt=\" Harbour at dusk \"></blockquote>\
             <img src=\"https://images.example.com/photo-2\">",
        ));
        let inlined = HashMap::from([("/drive/df_4".to_string(), "data:image/png;base64,AAAA".to_string())]);
        let mut left_out = 0;
        let nodes = place_images(nodes, &inlined, &mut left_out);
        assert_eq!(left_out, 2);
        assert_eq!(
            virtues_document::to_html(&nodes, false),
            "<img src=\"data:image/png;base64,AAAA\" alt=\"Harbour\" width=\"600\">\
             <blockquote><p><em>Harbour at dusk</em></p></blockquote>"
        );
    }

    #[test]
    fn blocks_keep_their_shape_and_lose_their_ids() {
        let (html, got) = shared(
            "<h2 data-id=\"k3n1x0aa\">Plan</h2>\
             <ul data-type=\"taskList\"><li data-type=\"taskItem\" data-checked=\"true\"><p>Book seats</p></li>\
             <li data-type=\"taskItem\"><p>Pack</p></li></ul>\
             <aside data-tone=\"tip\"><p>Bring <mark>cash</mark> and <u>tickets</u>.</p></aside>\
             <pre data-language=\"rust\">fn main() {}</pre>\
             <table><tr><th align=\"right\"><p>Cost</p></th></tr><tr><td align=\"right\"><p>12</p></td></tr></table>",
        );
        assert_eq!(got, Collected::default());
        assert!(!html.contains("data-id"), "{html}");
        assert!(html.contains("<ul data-type=\"taskList\"><li data-type=\"taskItem\" data-checked=\"true\">"));
        assert!(html.contains("<aside data-tone=\"tip\">"));
        assert!(html.contains("<pre data-language=\"rust\">"));
        assert!(html.contains("<th align=\"right\">"));
        for rule in [
            "ul[data-type=\"taskList\"]",
            "li[data-type=\"taskItem\"][data-checked=\"true\"]",
            "aside[data-tone=\"tip\"]",
            "pre[data-language]",
            "td[align=\"right\"]",
            "mark {",
            "u {",
        ] {
            assert!(PAGE_CSS.contains(rule), "the shared page styles {rule}");
        }
    }

    // ---------------------------------------------------------- freeze_page

    /// A 1x1 PNG, so a Drive image is read end to end.
    const PNG_1X1: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
        0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
        0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
        0x42, 0x60, 0x82,
    ];

    /// A Drive in a scratch directory holding one image, `df_photo`.
    async fn drive_with_a_photo(pool: &PgPool) -> (tempfile::TempDir, DriveConfig) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("photo.png"), PNG_1X1).unwrap();
        sqlx::query(
            "INSERT INTO app_drive_files (id, path, filename, mime_type, size_bytes) \
             VALUES ('df_photo', 'photo.png', 'photo.png', 'image/png', $1)",
        )
        .bind(PNG_1X1.len() as i64)
        .execute(pool)
        .await
        .unwrap();
        let storage = crate::storage::Storage::file(dir.path().to_string_lossy().into_owned()).unwrap();
        (dir, DriveConfig::new(std::sync::Arc::new(storage)))
    }

    async fn markdown_page(pool: &PgPool, title: &str, content: &str) -> String {
        let req = crate::api::pages::CreatePageRequest {
            title: title.into(),
            content: content.into(),
            project_id: None,
            icon: None,
            icon_color: None,
            cover_url: None,
            tags: None,
            format: None,
        };
        crate::api::pages::create_page_as(pool, req, PageFormat::Markdown).await.unwrap().page.id
    }

    /// A block page holding `html`, saved as the server saves one.
    async fn block_page(pool: &PgPool, html: &str) -> String {
        use yrs::Transact;
        let page_id = markdown_page(pool, "Trip", "").await;
        let doc = virtues_document::doc_from_nodes(tree(html));
        let txn = doc.transact();
        let state = virtues_document::encode_state(&txn, &yrs::StateVector::default());
        let export = virtues_document::to_markdown(&virtues_document::read_doc(&txn));
        sqlx::query("UPDATE app_pages SET yjs_state = $1, content = $2, format = 'tree' WHERE id = $3")
            .bind(&state)
            .bind(&export)
            .bind(&page_id)
            .execute(pool)
            .await
            .unwrap();
        page_id
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_block_page_shares_its_words_names_and_own_images(pool: PgPool) {
        let (_dir, drive) = drive_with_a_photo(&pool).await;
        let page_id = block_page(
            &pool,
            "<h2>Plan</h2>\
             <p>Lunch with <virtues-mention to=\"/person/person_1\" label=\"Nick\"></virtues-mention> at \
             <a href=\"/place/place_1\">Roscioli</a> on <virtues-del proposal=\"p1\">Friday</virtues-del>\
             <virtues-ins proposal=\"p1\">Saturday</virtues-ins>; see \
             <virtues-mention to=\"/page/page_abc\" label=\"Q3 plan\"></virtues-mention> and \
             <a href=\"https://maps.example.com/rome\">the map</a>.</p>\
             <img src=\"/drive/df_photo\" alt=\"Colosseum\">\
             <img src=\"https://images.example.com/photo-1\" alt=\"Harbour at dusk\">\
             <audio src=\"/drive/df_3\" data-name=\"memo.m4a\"></audio>\
             <virtues-file src=\"/drive/df_1\"></virtues-file>\
             <virtues-applet ref=\"applet_user__rome\"></virtues-applet>\
             <p>Tickets: <code>&lt;script&gt;</code></p>",
        )
        .await;

        let frozen = freeze_page(&pool, &drive, &page_id).await.unwrap();
        assert_eq!(frozen.names, ["Nick", "Roscioli"]);
        assert_eq!(frozen.images_left_out, 1);
        assert_eq!(frozen.embeds_left_out, 3, "audio, a file and an applet");
        let html = &frozen.html;
        assert!(
            html.contains(
                "<p>Lunch with Nick at Roscioli on Friday; see Q3 plan and \
                 <a href=\"https://maps.example.com/rome\">the map</a>.</p>"
            ),
            "{html}"
        );
        assert!(html.contains("<img src=\"data:image/png;base64,"), "the Drive image is inlined");
        assert!(html.contains("<p><em>Harbour at dusk</em></p>"));
        assert!(html.contains("<p><em>memo.m4a</em></p>"));
        assert!(html.contains("<code>&lt;script&gt;</code>"), "typed text stays text");
        for gone in ["virtues-", "data-id", "Saturday", "/drive/", "/person/", "/place/", "/page/"] {
            assert!(!html.contains(gone), "{gone} stays on the box: {html}");
        }

        // The Share modal says what stayed behind.
        let preview = crate::api::publications::preview(
            &pool,
            &drive,
            &crate::api::publications::Producer::Page(page_id.clone()),
        )
        .await
        .unwrap();
        assert_eq!(preview.embeds_left_out, 3);
        assert_eq!(preview.images_left_out, 1);
        assert_eq!(preview.names, ["Nick", "Roscioli"]);
        assert_eq!(preview.links, ["https://maps.example.com/rome"]);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_block_page_converted_from_markdown_shares_like_one(pool: PgPool) {
        let (_dir, drive) = drive_with_a_photo(&pool).await;
        let req = crate::api::pages::CreatePageRequest {
            title: "Trip".into(),
            content: "Ask [@Nick](/person/person_1) about [⟦David Okafor⟧](/person/person_2).\n\n\
                      - [x] Book seats\n- [ ] Pack\n\n![photo.png](/drive/df_photo)\n\n![voice.m4a](/drive/df_3)\n"
                .into(),
            project_id: None,
            icon: None,
            icon_color: None,
            cover_url: None,
            tags: None,
            format: None,
        };
        let page = crate::api::pages::create_page_as(&pool, req, PageFormat::Tree).await.unwrap().page;
        assert_eq!(page.format, PageFormat::Tree);

        let frozen = freeze_page(&pool, &drive, &page.id).await.unwrap();
        assert_eq!(frozen.names, ["David Okafor", "Nick"]);
        assert_eq!((frozen.images_left_out, frozen.embeds_left_out), (0, 1));
        assert!(frozen.html.contains("<p>Ask Nick about David Okafor.</p>"), "{}", frozen.html);
        assert!(frozen.html.contains("data-checked=\"true\""));
        assert!(frozen.html.contains("<img src=\"data:image/png;base64,"));
        assert!(frozen.html.contains("<p><em>voice.m4a</em></p>"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_block_page_whose_document_does_not_read_is_not_shared(pool: PgPool) {
        let (_dir, drive) = drive_with_a_photo(&pool).await;
        let page_id = block_page(&pool, "<p>Plan</p>").await;
        let markdown_state = crate::server::yjs::state_from_text("Plan");
        for state in [&b"not a state"[..], &markdown_state[..]] {
            sqlx::query("UPDATE app_pages SET yjs_state = $1 WHERE id = $2")
                .bind(state)
                .bind(&page_id)
                .execute(&pool)
                .await
                .unwrap();
            let err = freeze_page(&pool, &drive, &page_id).await.err().expect("refused");
            assert!(matches!(&err, Error::Other(m) if m == UNREADABLE), "{err}");
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_block_page_a_newer_version_wrote_is_not_shared(pool: PgPool) {
        use yrs::Transact;
        let (_dir, drive) = drive_with_a_photo(&pool).await;
        let page_id = block_page(&pool, "<p>Plan</p>").await;
        let mut nodes = tree("<p>Plan</p>");
        let mut later = Node::text("later", vec![]);
        later.marks.push(Mark { kind: "comment".into(), attrs: Default::default() });
        nodes[0].content.push(later);
        nodes.push(Node::element("poll", Default::default(), vec![]));
        assert!(!all_known(&nodes[..1]) && !all_known(&nodes[1..]) && all_known(&tree("<p>Plan</p>")));

        let doc = virtues_document::doc_from_nodes(nodes);
        let state = virtues_document::encode_state(&doc.transact(), &yrs::StateVector::default());
        sqlx::query("UPDATE app_pages SET yjs_state = $1 WHERE id = $2")
            .bind(&state)
            .bind(&page_id)
            .execute(&pool)
            .await
            .unwrap();
        let err = freeze_page(&pool, &drive, &page_id).await.err().expect("refused");
        assert!(matches!(&err, Error::InvalidInput(m) if m == NEWER), "{err}");

        // The Share modal says why, instead of failing.
        let preview = crate::api::publications::preview(
            &pool,
            &drive,
            &crate::api::publications::Producer::Page(page_id),
        )
        .await
        .unwrap();
        assert_eq!(preview.title, "Trip");
        assert_eq!(preview.problem.as_deref(), Some(NEWER));
        assert!(preview.html.is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn a_markdown_page_shares_its_saved_text(pool: PgPool) {
        let (_dir, drive) = drive_with_a_photo(&pool).await;
        let page_id = markdown_page(&pool, "Rome", "stale").await;
        let state = crate::server::yjs::state_from_text(
            "Dinner with [⟦Nick⟧](/person/person_1). ![Colosseum](/drive/df_photo) ![Harbour](https://images.example.com/a.png)",
        );
        sqlx::query("UPDATE app_pages SET yjs_state = $1 WHERE id = $2")
            .bind(&state)
            .bind(&page_id)
            .execute(&pool)
            .await
            .unwrap();
        let frozen = freeze_page(&pool, &drive, &page_id).await.unwrap();
        assert!(frozen.html.contains("Dinner with Nick."), "{}", frozen.html);
        assert!(frozen.html.contains("data:image/png;base64,"));
        assert!(frozen.html.contains("<em>Harbour</em>"));
        assert!(!frozen.html.contains("stale"));
        assert_eq!(frozen.names, ["Nick"]);
        assert_eq!((frozen.images_left_out, frozen.embeds_left_out), (1, 0));

        // A saved document that does not read shares the page's `content`.
        sqlx::query("UPDATE app_pages SET yjs_state = $1 WHERE id = $2")
            .bind(&b"not a state"[..])
            .bind(&page_id)
            .execute(&pool)
            .await
            .unwrap();
        let frozen = freeze_page(&pool, &drive, &page_id).await.unwrap();
        assert!(frozen.html.contains("<p>stale</p>"), "{}", frozen.html);
    }
}
