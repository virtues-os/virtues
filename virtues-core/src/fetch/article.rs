//! HTML → the parts of a page worth keeping: title, description, lead image,
//! and body text with the furniture removed.
//!
//! **Why this is hand-rolled rather than a readability crate.** The obvious move
//! is a Rust port of Mozilla's Readability, and two exist — but `readability-rust`
//! is at 0.1.0, and `readable-readability` ships 0% documentation coverage on
//! top of `kuchiki`, which is unmaintained. Taking either into a codebase that
//! otherwise keeps its dependency surface small is a poor trade for what we
//! actually need here.
//!
//! And what we need is less than a reader view. The consumer of this text is a
//! model composing an extraction record, not a person reading an article. The
//! job is to stop nav menus and cookie banners from eating the token budget and
//! showing up as "subjects" — not to reconstruct the author's paragraphs
//! faithfully. Tag-level furniture removal gets most of that, and when it isn't
//! enough the answer is the Parallel Extract tier (docs/bookmarks-plan.md step
//! 2), not a fragile dependency.
//!
//! `extraction/html.rs` stays as it is: it serves .html files a user drops into
//! Drive, and its own docs already call readability "a separate future lane".
//! This is that lane.

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

/// What a fetched page yields.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Article {
    pub title: Option<String>,
    pub description: Option<String>,
    pub image_url: Option<String>,
    pub text: String,
}

/// Elements whose content is never body text — machinery, or page furniture
/// that would otherwise be indexed as if the user had saved it.
const SKIP: &[&[u8]] = &[
    b"script",
    b"style",
    b"noscript",
    b"template",
    b"svg",
    b"iframe",
    b"form",
    b"button",
    b"select",
    b"nav",
    b"footer",
    b"aside",
];

/// Elements that end a paragraph.
const BLOCK: &[&[u8]] = &[
    b"p", b"div", b"li", b"h1", b"h2", b"h3", b"h4", b"h5", b"h6", b"tr", b"section", b"article",
    b"blockquote", b"pre", b"br", b"figcaption",
];

/// HTML void elements: legal to write as `<br>` with no close tag.
///
/// quick-xml reports those as `Start` with no matching `End`, so without this
/// list the element-depth counter climbs on every image and never comes back
/// down — and a subtree skip opened at depth 4 would never close.
const VOID: &[&[u8]] = &[
    b"area", b"base", b"br", b"col", b"embed", b"hr", b"img", b"input", b"link", b"meta", b"param",
    b"source", b"track", b"wbr",
];

/// Class/id tokens that mark a subtree as page furniture.
///
/// Matched against *tokens* — `class` split on whitespace, hyphens, and
/// underscores — never as substrings. Substring matching is how "toc" silently
/// eats an article about a `protocol`, and how "ad" eats "readme".
const FURNITURE: &[&str] = &[
    "nav",
    "navigation",
    "navbar",
    "menu",
    "sidebar",
    "footer",
    "banner",
    "breadcrumb",
    "breadcrumbs",
    "cookie",
    "consent",
    "newsletter",
    "subscribe",
    "share",
    "social",
    "comments",
    "related",
    "recommended",
    "promo",
    "advert",
    "ads",
    "popup",
    "modal",
    "toc",
    "pagination",
    "pager",
    "masthead",
    "skiplink",
    "jump",
    "widget",
];

/// Elements the furniture heuristic must never fire on.
///
/// These are the document, not a part of it, so a class token that happens to
/// appear on them says nothing about their contents. Found the hard way against
/// a real page: Wikipedia ships
/// `<html class="… vector-feature-navigation-update-disabled …">`, whose
/// `navigation` token matched the root element and skipped the entire article —
/// silently, yielding zero characters of text.
const NEVER_FURNITURE: &[&[u8]] = &[b"html", b"body", b"main", b"article"];

/// ARIA landmark roles that are furniture by definition.
const FURNITURE_ROLES: &[&str] = &[
    "navigation",
    "banner",
    "contentinfo",
    "complementary",
    "search",
];

/// Upper bound on extracted body text.
///
/// Generous enough for a long article, small enough that one pathological page
/// cannot dominate an enrichment run's token spend. Truncation is silent in the
/// text itself but the caller can tell, because the string lands exactly at the
/// cap.
pub const MAX_TEXT_CHARS: usize = 32_000;

/// `<header>` is deliberately NOT skipped: on a great many sites the article
/// headline and byline live inside one, and dropping it loses the most useful
/// sentence on the page. Nav and footer are where the furniture actually is.
pub fn parse(html: &str) -> Article {
    let mut reader = Reader::from_str(html);
    reader.config_mut().check_end_names = false;

    let mut out = String::new();
    let mut title_tag: Option<String> = None;
    let mut og_title: Option<String> = None;
    let mut og_desc: Option<String> = None;
    let mut meta_desc: Option<String> = None;
    let mut image: Option<String> = None;

    // Element depth, and the depth at which the current skipped subtree opened.
    // A counter keyed on tag names cannot express "skip this <div> and
    // everything under it", because the closing </div> looks like every other
    // </div> — so skipping is anchored to a depth instead.
    let mut depth = 0usize;
    let mut skip_from: Option<usize> = None;
    let mut head_depth = 0usize;
    let mut in_title = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = e.name().local_name().as_ref().to_ascii_lowercase();
                // A void element written without a slash still arrives as
                // Start; treat it as childless so it cannot move the depth.
                if VOID.contains(&name.as_slice()) {
                    if name.as_slice() == b"meta" {
                        read_meta(&e, &mut og_title, &mut og_desc, &mut meta_desc, &mut image);
                    } else if BLOCK.contains(&name.as_slice()) {
                        out.push_str("\n\n");
                    }
                    buf.clear();
                    continue;
                }

                depth += 1;
                if skip_from.is_none()
                    && (SKIP.contains(&name.as_slice())
                        || (!NEVER_FURNITURE.contains(&name.as_slice()) && is_furniture(&e)))
                {
                    skip_from = Some(depth);
                }

                match name.as_slice() {
                    b"head" => head_depth += 1,
                    // `</head>` is optional in HTML and routinely omitted —
                    // browsers close it implicitly when <body> starts. quick-xml
                    // does not, so without this the head counter never returned
                    // to zero and EVERY text node in the document was suppressed:
                    // the page came back with a title and no body at all.
                    //
                    // It failed silently, which is what made it worth a test.
                    // The row still enriched, because `<title>` survives, so the
                    // only symptom was a thinner record than the page deserved.
                    b"body" => head_depth = 0,
                    b"title" => in_title = true,
                    n if BLOCK.contains(&n) => out.push_str("\n\n"),
                    _ => {}
                }
            }
            // A self-closing tag has no content and no End event, so it must
            // NOT touch the depth counters — `<iframe/>` incrementing skip_depth
            // with nothing to decrement it would silently swallow the entire
            // rest of the document.
            Ok(Event::Empty(e)) => {
                let name = e.name().local_name().as_ref().to_ascii_lowercase();
                match name.as_slice() {
                    b"meta" => {
                        read_meta(&e, &mut og_title, &mut og_desc, &mut meta_desc, &mut image)
                    }
                    n if BLOCK.contains(&n) => out.push_str("\n\n"),
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name().local_name().as_ref().to_ascii_lowercase();
                if VOID.contains(&name.as_slice()) {
                    buf.clear();
                    continue;
                }
                match name.as_slice() {
                    b"head" => head_depth = head_depth.saturating_sub(1),
                    b"title" => in_title = false,
                    n if BLOCK.contains(&n) => out.push_str("\n\n"),
                    _ => {}
                }
                // Leaving the element the skip opened on ends the skip.
                if skip_from == Some(depth) {
                    skip_from = None;
                }
                depth = depth.saturating_sub(1);
            }
            Ok(Event::Text(t)) => {
                let Ok(text) = t.unescape() else {
                    buf.clear();
                    continue;
                };
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    buf.clear();
                    continue;
                }
                if in_title {
                    let slot = title_tag.get_or_insert_with(String::new);
                    if !slot.is_empty() {
                        slot.push(' ');
                    }
                    slot.push_str(trimmed);
                } else if skip_from.is_none()
                    && head_depth == 0
                    && out.chars().count() < MAX_TEXT_CHARS
                {
                    if !out.is_empty() && !out.ends_with('\n') && !out.ends_with(' ') {
                        out.push(' ');
                    }
                    out.push_str(trimmed);
                }
            }
            Ok(Event::Eof) => break,
            // Real-world HTML is sloppy; a parse error ends the scan with
            // whatever was gathered rather than discarding the page.
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    Article {
        // og:title is usually the cleaner headline — <title> carries the site
        // name and separators ("Story — Publication").
        title: og_title.or(title_tag).map(|s| collapse(&s)).filter(|s| !s.is_empty()),
        description: og_desc
            .or(meta_desc)
            .map(|s| collapse(&s))
            .filter(|s| !s.is_empty()),
        image_url: image.filter(|s| !s.trim().is_empty()),
        text: tidy(&out),
    }
}

/// Does this element's `class`, `id`, or ARIA `role` mark it as furniture?
///
/// Tokenized rather than substring-matched, and kept to landmark-ish words. The
/// failure mode to fear is a false positive — silently dropping the article —
/// so when in doubt a token is left out of [`FURNITURE`].
fn is_furniture(e: &BytesStart) -> bool {
    if let Some(role) = attr(e, b"role") {
        if FURNITURE_ROLES.contains(&role.to_ascii_lowercase().as_str()) {
            return true;
        }
    }
    for key in [b"class".as_slice(), b"id".as_slice()] {
        let Some(value) = attr(e, key) else { continue };
        let hit = value
            .to_ascii_lowercase()
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|token| FURNITURE.contains(&token));
        if hit {
            return true;
        }
    }
    false
}

/// Pull the metadata tags worth having off a `<meta>` element.
fn read_meta(
    e: &BytesStart,
    og_title: &mut Option<String>,
    og_desc: &mut Option<String>,
    meta_desc: &mut Option<String>,
    image: &mut Option<String>,
) {
    // The key is `property` for OpenGraph and `name` for classic meta tags;
    // plenty of sites use the wrong one, so accept either.
    let key = attr(e, b"property")
        .or_else(|| attr(e, b"name"))
        .unwrap_or_default()
        .to_ascii_lowercase();
    let Some(content) = attr(e, b"content") else {
        return;
    };
    if content.trim().is_empty() {
        return;
    }

    match key.as_str() {
        "og:title" => set_once(og_title, content),
        "og:description" => set_once(og_desc, content),
        "description" => set_once(meta_desc, content),
        "og:image" | "og:image:url" | "twitter:image" => set_once(image, content),
        _ => {}
    }
}

/// First writer wins — a page that repeats `og:image` means the first one.
fn set_once(slot: &mut Option<String>, value: String) {
    if slot.is_none() {
        *slot = Some(value);
    }
}

fn attr(e: &BytesStart, key: &[u8]) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        if a.key.local_name().as_ref().eq_ignore_ascii_case(key) {
            a.unescape_value().ok().map(|v| v.trim().to_string())
        } else {
            None
        }
    })
}

/// Squash all runs of whitespace to single spaces.
fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Collapse the runs of blank lines that block boundaries leave behind, and
/// enforce the character cap.
fn tidy(s: &str) -> String {
    let mut out = String::with_capacity(s.len().min(MAX_TEXT_CHARS));
    let mut blank_run = 0usize;
    for line in s.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            blank_run += 1;
            continue;
        }
        if !out.is_empty() {
            out.push_str(if blank_run > 0 { "\n\n" } else { "\n" });
        }
        blank_run = 0;
        out.push_str(trimmed);
        if out.chars().count() >= MAX_TEXT_CHARS {
            break;
        }
    }
    if out.chars().count() > MAX_TEXT_CHARS {
        out = out.chars().take(MAX_TEXT_CHARS).collect();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_metadata_and_drops_furniture() {
        let html = r#"
            <html><head>
              <title>Story — Publication</title>
              <meta property="og:title" content="The Cream House">
              <meta name="description" content="A stucco cottage.">
              <meta property="og:image" content="https://cdn.example.com/house.jpg">
            </head><body>
              <nav><a href="/">Home</a><a href="/about">About</a></nav>
              <article><p>Brown shutters and a green door.</p></article>
              <footer>Copyright 2026 Publication</footer>
              <script>var tracking = 1;</script>
            </body></html>"#;
        let a = parse(html);

        assert_eq!(a.title.as_deref(), Some("The Cream House"));
        assert_eq!(a.description.as_deref(), Some("A stucco cottage."));
        assert_eq!(
            a.image_url.as_deref(),
            Some("https://cdn.example.com/house.jpg")
        );
        assert!(a.text.contains("Brown shutters and a green door."));
        // The furniture is the whole point.
        assert!(!a.text.contains("About"), "nav leaked: {}", a.text);
        assert!(!a.text.contains("Copyright"), "footer leaked: {}", a.text);
        assert!(!a.text.contains("tracking"), "script leaked: {}", a.text);
    }

    #[test]
    fn falls_back_to_the_title_tag() {
        let html = "<html><head><title>Just a title</title></head><body><p>Body.</p></body></html>";
        let a = parse(html);
        assert_eq!(a.title.as_deref(), Some("Just a title"));
        assert_eq!(a.text, "Body.");
    }

    #[test]
    fn unclosed_head_does_not_swallow_the_body() {
        // Regression, found by audit rather than by failure: `</head>` is
        // optional in HTML, and omitting it left head_depth pinned above zero
        // so every text node was suppressed. The page still had a title, so
        // nothing errored — it just quietly extracted nothing.
        let html = "<html><head><title>T</title><meta charset=\"utf-8\">\
                    <body><p>The article body.</p></body></html>";
        let a = parse(html);
        assert!(
            a.text.contains("The article body."),
            "unclosed </head> swallowed the document: {:?}",
            a.text
        );
        assert_eq!(a.title.as_deref(), Some("T"));
    }

    #[test]
    fn head_text_never_becomes_body_text() {
        // A <head> containing anything text-shaped must not pollute the body —
        // this is why head is depth-tracked rather than added to SKIP (SKIP
        // would also swallow the <title> we want).
        let html = "<html><head><title>T</title><style>.a{color:red}</style></head>\
                    <body><p>Only this.</p></body></html>";
        let a = parse(html);
        assert_eq!(a.text, "Only this.");
    }

    #[test]
    fn keeps_header_because_headlines_live_there() {
        let html = "<html><body><header><h1>The Headline</h1></header>\
                    <p>Prose.</p></body></html>";
        let a = parse(html);
        assert!(a.text.contains("The Headline"), "got: {}", a.text);
    }

    #[test]
    fn self_closing_skip_tag_does_not_swallow_the_document() {
        // Regression: a self-closing tag has no End event, so incrementing
        // skip_depth on Empty left it stuck above zero and every subsequent
        // paragraph vanished.
        let html = "<html><body><iframe src='x'/><p>Still here.</p></body></html>";
        let a = parse(html);
        assert!(a.text.contains("Still here."), "got: {:?}", a.text);
    }

    #[test]
    fn drops_furniture_by_class_id_and_role() {
        // Real pages mark navigation with classes, not <nav> — Wikipedia's
        // 51-language list and skip-links arrive as divs and were landing at
        // the top of every extracted article.
        let html = r##"<html><body>
            <div class="mw-jump-link"><a href="#c">Jump to content</a></div>
            <div class="cookie-consent-banner">Accept all cookies</div>
            <div class="site-navigation"><a href="/">Home</a></div>
            <div role="complementary">Related stories</div>
            <div class="article-body"><p>The actual prose.</p></div>
          </body></html>"##;
        let a = parse(html);

        assert!(a.text.contains("The actual prose."), "lost body: {}", a.text);
        for junk in [
            "Jump to content",
            "Accept all cookies",
            "Home",
            "Related stories",
        ] {
            assert!(!a.text.contains(junk), "furniture leaked ({junk}): {}", a.text);
        }
    }

    #[test]
    fn a_furniture_token_on_the_root_element_cannot_eat_the_document() {
        // Verbatim from Wikipedia, which is how this was found: the class on
        // <html> contains "navigation", and matching it skipped everything.
        let html = r#"<html class="client-nojs vector-feature-navigation-update-disabled">
            <body><div class="content"><p>The whole article.</p></div></body></html>"#;
        let a = parse(html);
        assert!(
            a.text.contains("The whole article."),
            "root class ate the document: {:?}",
            a.text
        );
    }

    #[test]
    fn body_and_main_are_also_immune() {
        let html = r#"<html><body class="page-with-sidebar">
            <main id="menu-anchor"><p>Still extracted.</p></main></body></html>"#;
        let a = parse(html);
        assert!(a.text.contains("Still extracted."), "got: {:?}", a.text);
    }

    /// The heuristic is best-effort, and saying so in a test keeps the next
    /// person from assuming it is exhaustive. A container named only
    /// `p-lang-btn` carries no furniture token, so it survives — the answer for
    /// pages where this matters is the Parallel Extract tier, not an
    /// ever-growing keyword list that eventually eats an article.
    #[test]
    fn furniture_heuristic_is_not_exhaustive() {
        let html = r#"<html><body>
            <div id="p-lang-btn"><span>51 languages</span></div>
            <div><p>Prose.</p></div>
          </body></html>"#;
        let a = parse(html);
        assert!(a.text.contains("51 languages"));
        assert!(a.text.contains("Prose."));
    }

    #[test]
    fn furniture_tokens_do_not_match_as_substrings() {
        // The trap this guards: "toc" inside "protocol", "ads" inside
        // "roadside". Substring matching would drop both of these articles
        // entirely, and silently.
        let html = r#"<html><body>
            <div class="protocol-spec"><p>Protocol prose.</p></div>
            <div id="roadside-diner"><p>Diner prose.</p></div>
          </body></html>"#;
        let a = parse(html);
        assert!(a.text.contains("Protocol prose."), "got: {}", a.text);
        assert!(a.text.contains("Diner prose."), "got: {}", a.text);
    }

    #[test]
    fn unclosed_void_elements_do_not_break_depth() {
        // `<img>` and `<br>` without closing tags arrive as Start events. If
        // they moved the depth counter, the skip opened on the nav below would
        // never close and everything after it would vanish.
        let html = r#"<html><body>
            <nav><a href="/">Home</a></nav>
            <p>Before<br>after<img src="x.jpg">end.</p>
            <p>Later paragraph.</p>
          </body></html>"#;
        let a = parse(html);
        assert!(!a.text.contains("Home"), "nav leaked: {}", a.text);
        assert!(a.text.contains("Later paragraph."), "lost text: {}", a.text);
    }

    #[test]
    fn survives_malformed_markup() {
        let html = "<html><body><p>Unclosed paragraph<div>and a stray div</body>";
        let a = parse(html);
        assert!(a.text.contains("Unclosed paragraph"));
    }

    #[test]
    fn caps_runaway_pages() {
        let body = "<p>word word word</p>".repeat(20_000);
        let a = parse(&format!("<html><body>{body}</body></html>"));
        assert!(
            a.text.chars().count() <= MAX_TEXT_CHARS,
            "cap not enforced: {}",
            a.text.chars().count()
        );
    }

    #[test]
    fn empty_metadata_does_not_become_empty_strings() {
        let html = r#"<html><head><meta property="og:title" content="  ">
                      <title>  </title></head><body><p>Text.</p></body></html>"#;
        let a = parse(html);
        assert_eq!(a.title, None);
        assert_eq!(a.description, None);
    }
}
