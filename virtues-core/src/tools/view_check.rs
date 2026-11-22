//! Checks setup runs on a site's view before it saves it.
//!
//! A view is drawn in a sandboxed frame whose CSP allows only the server
//! itself (`server/faces.rs`), so two mistakes leave the page empty while the
//! chat says the site is up: loading anything from another host, and a plain
//! script that awaits outside an async function. These find both.
//!
//! The view is read by part: the attributes of each tag, each stylesheet and
//! each script. Text between tags and HTML comments are not read, so a page
//! that only mentions an address ("Weather data from https://…") passes.
//! Inside a script, comments are not read, and an address counts only where
//! the code loads it.

/// What a view does with an address on another host.
#[derive(Debug, PartialEq)]
pub(crate) enum OtherHost {
    /// It loads from there: a script, stylesheet, font, image, module import,
    /// `fetch` or socket. It never arrives. A module that imports from
    /// another host does not run at all.
    Loads(String),
    /// It links there. Nothing fails to load, but the link opens inside the
    /// view's own frame, where most sites refuse to load.
    Links(String),
}

/// The first address in a view that names another host: a load when there is
/// one, since that breaks the page, and otherwise a link.
pub(crate) fn other_host(view: &str) -> Option<OtherHost> {
    let mut loads: Vec<(usize, String)> = Vec::new();
    let mut links: Vec<(usize, String)> = Vec::new();
    for part in parts(view) {
        match part {
            Part::Tag { name, attrs } => {
                for attr in attrs {
                    let at = attr.at;
                    match attr.name.as_str() {
                        "style" => loads.extend(css_loads(attr.value, at)),
                        n if n.starts_with("on") => {
                            let (l, k) = script_hosts(attr.value, at);
                            loads.extend(l);
                            links.extend(k);
                        }
                        "src" | "poster" | "data" | "background" | "action" | "formaction" => {
                            if let Some(a) = elsewhere(attr.value) {
                                loads.push((at, a));
                            }
                        }
                        "srcset" => {
                            for candidate in attr.value.split(',') {
                                if let Some(a) = elsewhere(candidate) {
                                    loads.push((at, a));
                                    break;
                                }
                            }
                        }
                        "href" | "xlink:href" => {
                            if let Some(a) = elsewhere(attr.value) {
                                if name == "a" {
                                    links.push((at, a));
                                } else {
                                    loads.push((at, a));
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Part::Style { css, at } => loads.extend(css_loads(css, at)),
            Part::Script { code, at, .. } => {
                let (l, k) = script_hosts(code, at);
                loads.extend(l);
                links.extend(k);
            }
        }
    }
    let first = |v: Vec<(usize, String)>| v.into_iter().min_by_key(|(at, _)| *at).map(|(_, a)| a);
    first(loads).map(OtherHost::Loads).or_else(|| first(links).map(OtherHost::Links))
}

/// Whether a view has a plain inline script that awaits with nothing to make
/// it legal. `await` outside a module and outside an async function is a
/// syntax error, so the browser runs none of the script and the page stays
/// empty. Only the certain case counts: a script that is not a module, has
/// no `src`, uses `await` and has no `async` anywhere in it. Words in
/// comments and strings are not code.
pub(crate) fn awaits_outside_async(view: &str) -> bool {
    parts(view).into_iter().any(|part| match part {
        Part::Script { code, module: false, .. } => {
            let bare = lex(code).bare;
            has_word(&bare, "await") && !has_word(&bare, "async")
        }
        _ => false,
    })
}

/// One part of a view.
enum Part<'a> {
    /// A tag, with its name lowercased and its attributes.
    Tag { name: String, attrs: Vec<Attr<'a>> },
    /// The text of a `<style>` element, and where it starts in the view.
    Style { css: &'a str, at: usize },
    /// The code of an inline JavaScript `<script>`, and where it starts. A
    /// script with a `src` is a tag only, and a data block (`type` that is
    /// not JavaScript) is skipped.
    Script { code: &'a str, at: usize, module: bool },
}

struct Attr<'a> {
    /// Lowercased.
    name: String,
    value: &'a str,
    /// Where the value starts in the view.
    at: usize,
}

/// The tags, stylesheets and scripts of a view, in order. Offsets come from
/// an ASCII-lowercased copy, which has the same bytes at the same places, and
/// every cut falls on an ASCII byte, so slicing the view with them is safe.
fn parts(view: &str) -> Vec<Part<'_>> {
    let lower = view.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(off) = lower[i..].find('<') {
        let at = i + off;
        if lower[at..].starts_with("<!--") {
            i = lower[at + 4..].find("-->").map_or(lower.len(), |e| at + 4 + e + 3);
            continue;
        }
        let name_start = at + 1;
        let mut name_end = name_start;
        while name_end < bytes.len() && (bytes[name_end].is_ascii_alphanumeric() || matches!(bytes[name_end], b'-' | b':')) {
            name_end += 1;
        }
        if name_end == name_start {
            // A closing tag, a doctype, or a `<` in text.
            i = at + 1;
            continue;
        }
        let name = lower[name_start..name_end].to_string();
        let (attrs, tag_end) = read_attrs(view, &lower, name_end);
        match name.as_str() {
            "script" | "style" => {
                let close = if name == "script" { "</script" } else { "</style" };
                let body_end = lower[tag_end..].find(close).map_or(lower.len(), |e| tag_end + e);
                let body = &view[tag_end..body_end];
                if name == "style" {
                    out.push(Part::Style { css: body, at: tag_end });
                } else {
                    let kind = attrs.iter().find(|a| a.name == "type").map(|a| a.value.trim().to_ascii_lowercase());
                    let module = kind.as_deref() == Some("module");
                    let javascript = kind.as_deref().is_none_or(|k| k.is_empty() || module || k.contains("javascript") || k.contains("ecmascript"));
                    let has_src = attrs.iter().any(|a| a.name == "src");
                    if javascript && !has_src {
                        out.push(Part::Script { code: body, at: tag_end, module });
                    }
                }
                out.push(Part::Tag { name, attrs });
                i = body_end;
            }
            _ => {
                out.push(Part::Tag { name, attrs });
                i = tag_end;
            }
        }
    }
    out
}

/// The attributes of a tag, from just after its name, and where the tag ends
/// (after its `>`).
fn read_attrs<'a>(view: &'a str, lower: &str, from: usize) -> (Vec<Attr<'a>>, usize) {
    let bytes = lower.as_bytes();
    let n = bytes.len();
    let mut attrs = Vec::new();
    let mut i = from;
    loop {
        while i < n && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        if i >= n {
            return (attrs, n);
        }
        if bytes[i] == b'>' {
            return (attrs, i + 1);
        }
        let name_start = i;
        while i < n && !bytes[i].is_ascii_whitespace() && !matches!(bytes[i], b'=' | b'>' | b'/') {
            i += 1;
        }
        let name = lower[name_start..i].to_string();
        let mut j = i;
        while j < n && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j < n && bytes[j] == b'=' {
            j += 1;
            while j < n && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            let (start, end, next) = if j < n && matches!(bytes[j], b'"' | b'\'') {
                let quote = bytes[j];
                let end = lower[j + 1..].find(quote as char).map_or(n, |e| j + 1 + e);
                (j + 1, end, (end + 1).min(n))
            } else {
                let mut end = j;
                while end < n && !bytes[end].is_ascii_whitespace() && bytes[end] != b'>' {
                    end += 1;
                }
                (j, end, end)
            };
            attrs.push(Attr { name, value: &view[start..end], at: start });
            i = next;
        } else if i == name_start {
            // A stray character: step past it so the scan always moves.
            i += 1;
        } else {
            attrs.push(Attr { name, value: "", at: i });
        }
    }
}

/// The address when `text` starts with one on another host, after spaces and
/// quotes: `http://`, `https://`, `//`, `ws://` or `wss://`.
fn elsewhere(text: &str) -> Option<String> {
    let t = text.trim_start_matches(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '('));
    let lower = t.get(..6).unwrap_or(t).to_ascii_lowercase();
    let lower = lower.as_str();
    let other = lower.starts_with("http:/") || lower.starts_with("https:") || lower.starts_with("//") || lower.starts_with("ws://") || lower.starts_with("wss://");
    other.then(|| {
        t.chars()
            .take_while(|c| !c.is_whitespace() && !matches!(c, '"' | '\'' | '`' | ')' | '>' | '$'))
            .take(80)
            .collect()
    })
}

/// Addresses found, each with where it is in the view.
type Found = Vec<(usize, String)>;

/// What a stylesheet loads from another host: `url(…)` and `@import`, outside
/// comments.
fn css_loads(css: &str, at: usize) -> Vec<(usize, String)> {
    let mut bare = css.as_bytes().to_vec();
    let mut i = 0;
    while let Some(off) = css[i..].find("/*") {
        let start = i + off;
        let end = css[start + 2..].find("*/").map_or(css.len(), |e| start + 2 + e + 2);
        bare[start..end].iter_mut().for_each(|b| *b = b' ');
        i = end;
    }
    let bare = String::from_utf8_lossy(&bare).to_ascii_lowercase();
    let mut out = Vec::new();
    for lead in ["url(", "@import"] {
        for (pos, _) in bare.match_indices(lead) {
            let rest = &css[pos + lead.len()..];
            let rest = rest.trim_start();
            let rest = rest.strip_prefix("url(").or_else(|| rest.strip_prefix("URL(")).unwrap_or(rest);
            if let Some(a) = elsewhere(rest) {
                out.push((at + pos, a));
            }
        }
    }
    out
}

/// A script read for what it reaches on another host: (loads, links), each
/// with where it is in the view.
fn script_hosts(code: &str, at: usize) -> (Found, Found) {
    let lexed = lex(code);
    let bare = lexed.bare.to_ascii_lowercase();
    let mut loads = Vec::new();
    let mut links = Vec::new();
    // Addresses in strings that the code does not load where they are
    // written, kept in case the code loads one through a name.
    let mut held: Vec<(usize, String)> = Vec::new();
    for (k, lit) in lexed.literals.iter().enumerate() {
        let Some(address) = elsewhere(lit.text(code)) else {
            continue;
        };
        let place = at + lit.start;
        let before = bare[..lit.start].trim_end();
        let loaded_by_call = ["fetch(", "import(", "websocket(", "eventsource(", "sendbeacon(", "importscripts("]
            .iter()
            .any(|call| before.ends_with(call));
        let by_keyword = ends_with_word(before, "from") || ends_with_word(before, "import");
        let assigned = before.strip_suffix('=').map(str::trim_end);
        let set_attr = before
            .strip_suffix(',')
            .map(str::trim_end)
            .and_then(|b| k.checked_sub(1).map(|p| (b, &lexed.literals[p])))
            .filter(|(b, p)| b.len() == p.end && bare[..p.start].trim_end().ends_with("setattribute("))
            .map(|(_, p)| p.text(code).to_ascii_lowercase());
        if loaded_by_call || by_keyword {
            loads.push((place, address));
        } else if let Some(target) = assigned {
            if [".src", ".srcset", ".poster", ".data", ".action"].iter().any(|p| target.ends_with(p)) {
                loads.push((place, address));
            } else if target.ends_with(".href") || target.ends_with("location") {
                links.push((place, address));
            } else {
                held.push((place, address));
            }
        } else if let Some(name) = set_attr {
            match name.as_str() {
                "src" | "srcset" | "poster" | "data" | "action" => loads.push((place, address)),
                "href" => links.push((place, address)),
                _ => held.push((place, address)),
            }
        } else if !before.ends_with("url(") {
            // `new URL("https://…")` only parses an address. Anything else is
            // held: a constant the code may fetch later.
            held.push((place, address));
        }
    }
    // A call that loads through a name or a template (`fetch(API)`,
    // `fetch(`${API}/x`)`) reaches whatever address the script holds.
    if let Some(first) = held.into_iter().next() {
        let indirect = ["fetch(", "import(", "websocket(", "eventsource("].iter().any(|call| {
            bare.match_indices(call).any(|(pos, _)| {
                let arg_at = pos + call.len();
                let arg = bare[arg_at..].trim_start();
                let arg_start = bare.len() - arg.len();
                match lexed.literals.iter().find(|l| l.start == arg_start) {
                    Some(l) => code[l.start..].starts_with("`${"),
                    None => !arg.starts_with(')'),
                }
            })
        });
        if indirect {
            loads.push(first);
        }
    }
    (loads, links)
}

/// Whether `text` ends with `word` as a word of its own.
fn ends_with_word(text: &str, word: &str) -> bool {
    text.strip_suffix(word)
        .is_some_and(|rest| !rest.chars().next_back().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$'))
}

/// Whether `word` appears in code as a word of its own.
fn has_word(code: &str, word: &str) -> bool {
    let name_char = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$';
    code.match_indices(word).any(|(at, _)| {
        let before = code[..at].chars().next_back();
        let after = code[at + word.len()..].chars().next();
        !before.is_some_and(name_char) && !after.is_some_and(name_char)
    })
}

/// A string literal: from its opening quote to just past its closing one.
/// `inner_end` is where its text ends: at the closing quote, or where an
/// unclosed string stops.
struct Literal {
    start: usize,
    inner_end: usize,
    end: usize,
}

impl Literal {
    fn text<'a>(&self, code: &'a str) -> &'a str {
        &code[self.start + 1..self.inner_end.max(self.start + 1)]
    }
}

struct Lexed {
    /// The code with its comments and the insides of its strings blanked to
    /// spaces, at the same offsets. Quotes stay, so a string's place shows.
    bare: String,
    literals: Vec<Literal>,
}

/// Split a script into code, comments and strings. A `//` or `/*` inside a
/// string is part of the string, and a quote inside a comment is part of the
/// comment. Regular expressions are read as code.
fn lex(code: &str) -> Lexed {
    let bytes = code.as_bytes();
    let n = bytes.len();
    let mut bare = bytes.to_vec();
    let mut literals = Vec::new();
    let blank = |from: usize, to: usize, bare: &mut Vec<u8>| bare[from..to].iter_mut().for_each(|b| *b = b' ');
    let mut i = 0;
    while i < n {
        match bytes[i] {
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                let end = code[i..].find('\n').map_or(n, |e| i + e);
                blank(i, end, &mut bare);
                i = end;
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                let end = code[i + 2..].find("*/").map_or(n, |e| i + 2 + e + 2);
                blank(i, end, &mut bare);
                i = end;
            }
            q @ (b'"' | b'\'' | b'`') => {
                let mut j = i + 1;
                while j < n && bytes[j] != q {
                    if bytes[j] == b'\\' {
                        j += 1;
                    } else if bytes[j] == b'\n' && q != b'`' {
                        break;
                    }
                    j += 1;
                }
                let (inner_end, end) = if j < n && bytes[j] == q { (j, j + 1) } else { (j.min(n), j.min(n)) };
                if inner_end > i + 1 {
                    blank(i + 1, inner_end, &mut bare);
                }
                literals.push(Literal { start: i, inner_end, end });
                i = end.max(i + 1);
            }
            _ => i += 1,
        }
    }
    Lexed { bare: String::from_utf8_lossy(&bare).into_owned(), literals }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loads(a: &str) -> Option<OtherHost> {
        Some(OtherHost::Loads(a.to_string()))
    }
    fn links(a: &str) -> Option<OtherHost> {
        Some(OtherHost::Links(a.to_string()))
    }

    /// What a page loads from another host never arrives: a script, a
    /// stylesheet, a font, an image or a form's target.
    #[test]
    fn a_tag_or_stylesheet_that_loads_from_another_host_is_found() {
        let cdn = "<link rel=\"stylesheet\" href=\"virtues.css\"><script src=\"https://cdn.example.com/chart.js\"></script>";
        assert_eq!(other_host(cdn), loads("https://cdn.example.com/chart.js"));
        assert_eq!(other_host("<style>@import url(//fonts.example.com/a.css);</style>"), loads("//fonts.example.com/a.css"));
        assert_eq!(other_host("<link rel=\"stylesheet\" href=\"https://fonts.example.com/a.css\">"), loads("https://fonts.example.com/a.css"));
        assert_eq!(other_host("<img srcset=\"a.png 1x, https://img.example.com/b.png 2x\">"), loads("https://img.example.com/b.png"));
        assert_eq!(other_host("<div style=\"background: url('https://img.example.com/c.png')\"></div>"), loads("https://img.example.com/c.png"));
        assert_eq!(other_host("<form action=\"https://forms.example.com/send\"></form>"), loads("https://forms.example.com/send"));
    }

    /// A module that imports from another host never runs, so the page stays
    /// empty. A `fetch` or a socket fails the same way, however the code is
    /// laid out.
    #[test]
    fn a_script_that_loads_from_another_host_is_found_in_any_layout() {
        let module = "<script type=\"module\">import { chart } from 'https://cdn.example.com/chart.mjs';</script>";
        assert_eq!(other_host(module), loads("https://cdn.example.com/chart.mjs"));
        let minified = "<script type=module>import{a}from\"https://cdn.example.com/a.mjs\";</script>";
        assert_eq!(other_host(minified), loads("https://cdn.example.com/a.mjs"));
        let split = "<script type=module>import x from\n  'https://cdn.example.com/x.mjs';</script>";
        assert_eq!(other_host(split), loads("https://cdn.example.com/x.mjs"));
        let bare = "<script type=module>import 'https://cdn.example.com/side.js';</script>";
        assert_eq!(other_host(bare), loads("https://cdn.example.com/side.js"));
        let dynamic = "<script type=module>const m = await import(\"https://cdn.example.com/m.js\");</script>";
        assert_eq!(other_host(dynamic), loads("https://cdn.example.com/m.js"));
        let formatted = "<script>\nfetch(\n  \"https://api.example.com/weather\",\n  { cache: 'no-store' }\n).then(draw);\n</script>";
        assert_eq!(other_host(formatted), loads("https://api.example.com/weather"));
        let commented = "<script>fetch( // the forecast\n 'https://api.example.com/f')</script>";
        assert_eq!(other_host(commented), loads("https://api.example.com/f"));
        let held = "<script>const API = 'https://api.example.com'; fetch(`${API}/x`).then(draw);</script>";
        assert_eq!(other_host(held), loads("https://api.example.com"));
        let set = "<script>img.src = 'https://img.example.com/a.png';</script>";
        assert_eq!(other_host(set), loads("https://img.example.com/a.png"));
        let attr = "<script>img.setAttribute('src', 'https://img.example.com/b.png');</script>";
        assert_eq!(other_host(attr), loads("https://img.example.com/b.png"));
        let socket = "<script>const s = new WebSocket('wss://live.example.com/feed');</script>";
        assert_eq!(other_host(socket), loads("wss://live.example.com/feed"));
        let handler = "<button onclick=\"fetch('https://api.example.com/x')\">Go</button>";
        assert_eq!(other_host(handler), loads("https://api.example.com/x"));
    }

    /// A link out is told apart from a load, in markup and in a script, and a
    /// load is the one reported when a page has both.
    #[test]
    fn a_link_to_another_site_is_a_link() {
        let link = "<p>From <a href=\"https://example.com/guide\">the guide</a>.</p>";
        assert_eq!(other_host(link), links("https://example.com/guide"));
        let crlf = "<a\r\nhref=\"https://example.com/a\">A</a>";
        assert_eq!(other_host(crlf), links("https://example.com/a"));
        let scripted = "<script>a.href = `https://maps.example.com/?q=${q}`;</script>";
        assert_eq!(other_host(scripted), links("https://maps.example.com/?q="));
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\"><a xlink:href=\"https://example.com/s\"><text>s</text></a></svg>";
        assert_eq!(other_host(svg), links("https://example.com/s"));
        let both = format!("{link}<script type=module>import 'https://cdn.example.com/side.js';</script>");
        assert_eq!(other_host(&both), loads("https://cdn.example.com/side.js"));
    }

    /// A page that only mentions an address, in its text, a comment, a data
    /// block or a string the code never loads, passes.
    #[test]
    fn an_address_that_is_only_mentioned_passes() {
        let cases = [
            "<p>Weather data from https://weather.example.com, updated hourly.</p>",
            "<!-- icons from https://icons.example.com -->",
            "<p>See https://example.com in the text.</p>",
            "<script>// adapted from https://blog.example.com/post\ndraw();</script>",
            "<script>/* from 'https://x.example.com' */ draw();</script>",
            "<script type=\"application/json\">{\"note\": \"from https://example.com\"}</script>",
            "<script>const u = new URL(\"https://example.com/x\"); show(u.pathname);</script>",
            "<script>const imgSrc = 'https://example.com/a.png'; label(imgSrc);</script>",
            "<script>openUrl('https://example.com/b');</script>",
            "<script>const label = \"Data from 'https://example.com' and more\"; show(label);</script>",
            "<script type=module>import { rows } from './rows.js'; const r = await virtues.query('select 1');</script>",
            "<svg xmlns=\"http://www.w3.org/2000/svg\"><circle r=\"4\"/></svg>",
            "<link rel=\"stylesheet\" href=\"virtues.css\"><script src=\"virtues.js\"></script>",
        ];
        for view in cases {
            assert_eq!(other_host(view), None, "{view}");
        }
    }

    /// The working view the authoring guide gives the assistant passes both
    /// checks, so the example it copies is one setup accepts.
    #[test]
    fn the_guides_example_view_passes() {
        let guide = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../applets/AGENTS.md")).unwrap();
        let start = guide.find("```html\n").expect("the guide has a view") + "```html\n".len();
        let view = &guide[start..start + guide[start..].find("```").unwrap()];
        assert!(view.contains("<script type=\"module\">") && view.contains("virtues.query"));
        assert_eq!(other_host(view), None);
        assert!(!awaits_outside_async(view));
    }

    /// Odd input does not panic: letters outside ASCII, an unterminated tag,
    /// an unterminated string, an empty page.
    #[test]
    fn odd_input_does_not_panic() {
        for view in [
            "",
            "<",
            "<script",
            "<a href=",
            "<p>Caf\u{e9} \u{1f600} \u{65e5}\u{672c}</p><script>const s = '\u{e9}",
            "<script>fetch('",
            "<style>url(",
        ] {
            let _ = other_host(view);
            let _ = awaits_outside_async(view);
        }
    }

    /// A plain script that awaits with no async function never runs, so the
    /// view stays empty. A module, an async function, a comment, a string and
    /// a longer word are all left alone.
    #[test]
    fn a_view_whose_script_awaits_outside_async_is_found() {
        let head = "<script src=\"virtues.js\"></script>";
        let plain = format!("{head}<script>const rows = await virtues.query('select 1');</script>");
        assert!(awaits_outside_async(&plain));
        let upper = format!("{head}<SCRIPT>\n  const rows = await virtues.query('select 1');\n</SCRIPT>");
        assert!(awaits_outside_async(&upper));

        let module = format!("{head}<script type=\"module\">const rows = await virtues.query('select 1');</script>");
        assert!(!awaits_outside_async(&module));
        let unquoted = format!("{head}<script type=module>const rows = await virtues.query('select 1');</script>");
        assert!(!awaits_outside_async(&unquoted));
        let wrapped = format!("{head}<script>(async () => {{ const rows = await virtues.query('select 1'); }})();</script>");
        assert!(!awaits_outside_async(&wrapped));
        let comment = format!("{head}<script>// await the rows later\n/* await */ draw();</script>");
        assert!(!awaits_outside_async(&comment));
        let string = format!("{head}<script>el.textContent = 'Results await you';</script>");
        assert!(!awaits_outside_async(&string));
        let slashes = format!("{head}<script>const glob = '*/*'; const p = \"//x\"; draw(glob, p);</script>");
        assert!(!awaits_outside_async(&slashes));
        let longer = format!("{head}<script>const awaiting = 1; show(awaiting);</script>");
        assert!(!awaits_outside_async(&longer));
        let data = format!("{head}<script type=\"application/json\">{{\"note\": \"await this\"}}</script>");
        assert!(!awaits_outside_async(&data));
        assert!(!awaits_outside_async("<p>No script at all, and an open <script"));
    }
}
