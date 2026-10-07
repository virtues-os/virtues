//! Freezing a page for sharing: its text, rendered once to a standalone HTML
//! file the door serves (`api::publications`).
//!
//! What a page carries out, and only that:
//!
//! - **Its text**, rendered from the page's Markdown.
//! - **Names, not records.** A person, place or other ref in the page
//!   (`[⟦Nick⟧](/person/…)`) becomes its label alone: no link, nothing looked
//!   up. A page that mentions someone must not quietly carry their number.
//!   The names are listed so the Share modal can show them.
//! - **Its own images**, from Drive, inlined. An image from another site is
//!   left out (the loader's frame may not fetch anything), and counted, so the
//!   modal can say so.

use std::sync::OnceLock;

use base64::Engine;
use regex::Regex;
use sqlx::PgPool;

use crate::api::drive::DriveConfig;
use crate::error::{Error, Result};

pub struct FrozenPage {
    pub title: String,
    pub html: String,
    /// The people, places and other things the page names, as their labels.
    pub names: Vec<String>,
    /// Images that are not in Drive and so are not in the shared page.
    pub images_left_out: usize,
}

#[derive(sqlx::FromRow)]
struct PageRow {
    title: String,
    content: String,
    yjs_state: Option<Vec<u8>>,
    icon: Option<String>,
    cover_url: Option<String>,
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
        let kind = c[2].trim_start_matches('/').split('/').next().unwrap_or("");
        if matches!(kind, "person" | "place" | "org" | "organization") && !label.is_empty() {
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

const PAGE_CSS: &str = r#"
:root { color-scheme: light dark; --fg: #1d1f1e; --muted: #6a706d; --bg: #fdfcfa; --line: #e6e3dd; --code: #f2f0ec; }
@media (prefers-color-scheme: dark) { :root { --fg: #e9ebe9; --muted: #9aa19e; --bg: #151716; --line: #2c302e; --code: #1f2321; } }
html, body { margin: 0; background: var(--bg); color: var(--fg); }
body { font: 18px/1.65 Georgia, "Iowan Old Style", "Times New Roman", serif; }
main { max-width: 680px; margin: 0 auto; padding: 40px 20px 80px; }
.cover { width: 100%; max-height: 320px; object-fit: cover; border-radius: 10px; margin-bottom: 28px; }
h1 { font-size: 2.1em; line-height: 1.2; font-weight: normal; margin: 0 0 24px; }
h2, h3, h4 { font-weight: normal; line-height: 1.3; margin: 1.6em 0 0.5em; }
p, ul, ol, blockquote, table, pre { margin: 0 0 1em; }
a { color: inherit; }
img { max-width: 100%; height: auto; border-radius: 6px; }
blockquote { border-left: 3px solid var(--line); padding-left: 16px; color: var(--muted); }
code { font: 0.85em ui-monospace, Menlo, monospace; background: var(--code); padding: 0.1em 0.3em; border-radius: 4px; }
pre { background: var(--code); padding: 12px 14px; border-radius: 8px; overflow-x: auto; }
pre code { background: none; padding: 0; }
table { border-collapse: collapse; width: 100%; font-size: 0.9em; }
th, td { border-bottom: 1px solid var(--line); padding: 6px 8px; text-align: left; }
hr { border: 0; border-top: 1px solid var(--line); margin: 2em 0; }
li:has(> input[type="checkbox"]) { list-style: none; margin-left: -1.3em; }
li > input[type="checkbox"] { margin-right: 0.5em; }
"#;

/// The page as one standalone file.
pub async fn freeze_page(pool: &PgPool, drive: &DriveConfig, page_id: &str) -> Result<FrozenPage> {
    let row: Option<PageRow> = sqlx::query_as(
        "SELECT title, content, yjs_state, icon, cover_url FROM app_pages \
         WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(page_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| Error::Database(format!("read page: {e}")))?;
    let row = row.ok_or_else(|| Error::NotFound(format!("no page {page_id:?}")))?;

    // The saved document. Edits from the last few seconds may still be on
    // their way to the database; Update re-freezes.
    let markdown = match &row.yjs_state {
        Some(state) => std::panic::catch_unwind(|| crate::api::pages::yjs_state_to_markdown(state))
            .unwrap_or_else(|_| row.content.clone()),
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
    let body = render(&text);

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
    Ok(FrozenPage { title: row.title, html, names, images_left_out })
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
}
