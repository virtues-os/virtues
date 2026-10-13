//! Markdown read as a person saw it: a paste as the text it was copied
//! from (`Dialect::Paste`), a page as its markdown editor showed it
//! (`Dialect::Page`). Neither loses a character it showed without a note.

use virtues_document::migrate::{measure_as, Dialect};
use virtues_document::{contract, parse_pasted_markdown, Node, Problem};

/// Each top-level block as `kind:text`, a hard break as a line end.
fn blocks(nodes: &[Node]) -> Vec<String> {
    nodes
        .iter()
        .map(|n| {
            let mut s = format!("{}:", n.kind);
            n.walk(&mut |n| {
                if let Some(t) = &n.text {
                    s.push_str(t);
                } else if n.kind == "hardBreak" {
                    s.push('\n');
                }
            });
            s
        })
        .collect()
}

fn blocks_of(nodes: &[Node]) -> Vec<String> {
    blocks(nodes)
}

fn pasted(md: &str) -> Vec<String> {
    let o = parse_pasted_markdown(md);
    assert!(o.errors.is_empty(), "{md}: {:?}", o.errors);
    blocks(&o.nodes)
}

/// A page's markdown as it moves to blocks: its blocks and its notes, with
/// no text or address lost.
fn page(md: &str) -> (Vec<String>, Vec<String>) {
    let m = measure_as(contract(), md, Dialect::Page);
    assert!(m.outcome.errors.is_empty(), "{md}: {:?}", m.outcome.errors);
    assert!(m.text_lost.is_none(), "{md}: {:?}", m.text_lost);
    assert!(m.addresses_lost.is_empty(), "{md}: {:?}", m.addresses_lost);
    assert!(m.export_diff.is_none(), "{md}: {:?}", m.export_diff);
    let notes = m.outcome.notes.iter().map(|n: &Problem| n.message.clone()).collect();
    (blocks(&m.outcome.nodes), notes)
}

/// HTML whose words the page would drop, or run together, stays the text
/// it was in a paste: the content of `<script>`, `<style>` and
/// `<template>`, a comment, a `<details>` block. HTML whose every tag keeps
/// its words is still read as markup.
#[test]
fn a_paste_keeps_html_whose_words_the_page_would_lose_as_text() {
    for (md, kept) in [
        (
            "# Setup\n\n- Add <script src=\"/app.js\"></script> before </body>\n",
            vec!["heading:Setup", "bulletList:Add <script src=\"/app.js\"></script> before </body>"],
        ),
        ("- <template>Hello {{name}}</template>\n", vec!["bulletList:<template>Hello {{name}}</template>"]),
        ("- use <style>.a{color:red}</style> here\n", vec!["bulletList:use <style>.a{color:red}</style> here"]),
        ("<!-- TODO: check this -->\n- item\n", vec!["paragraph:<!-- TODO: check this -->", "bulletList:item"]),
        ("Call <!-- not yet --> Nick\n", vec!["paragraph:Call <!-- not yet --> Nick"]),
        (
            "<details><summary>Why?</summary>Because it is.</details>\n",
            vec!["paragraph:<details><summary>Why?</summary>Because it is.</details>"],
        ),
        ("- Press <kbd>Enter</kbd>\n", vec!["bulletList:Press Enter"]),
        ("<p align=\"center\">Centred</p>\n", vec!["paragraph:Centred"]),
    ] {
        assert_eq!(pasted(md), kept, "{md}");
    }
}

/// A paste keeps its lines as the text it came from showed them: a line
/// is never joined to the one before, nor run on into a list item or a
/// quote it is not indented into.
#[test]
fn a_paste_keeps_each_line_a_line() {
    for (md, kept) in [
        (
            "# install\npip install foo_bar\nexport PATH=$HOME/.local/bin:$PATH\n",
            vec!["heading:install", "paragraph:pip install foo_bar\nexport PATH=$HOME/.local/bin:$PATH"],
        ),
        (
            "Shopping\n- milk\nCall Nick at 5\nBring receipts\n",
            vec!["paragraph:Shopping", "bulletList:milk", "paragraph:Call Nick at 5\nBring receipts"],
        ),
        ("> Quoted\nNot quoted\n", vec!["blockquote:Quoted", "paragraph:Not quoted"]),
        ("1. First\n   more of it\n2. Second\n", vec!["orderedList:First\nmore of itSecond"]),
        ("```\n- a\nb\n```\n", vec!["codeBlock:- a\nb"]),
    ] {
        assert_eq!(pasted(md), kept, "{md}");
    }
}

/// The addresses the links of `nodes` go to, in order.
fn links(nodes: &[Node]) -> Vec<String> {
    let mut out = vec![];
    for n in nodes {
        n.walk(&mut |n| {
            for m in n.marks.iter().filter(|m| m.kind == "link") {
                out.extend(m.attrs.get("href").and_then(|h| h.as_str()).map(str::to_string));
            }
        });
    }
    out
}

/// A paste's link reference definitions are the lines of the text it was
/// copied from. The parser takes each out of what it renders, so a list of
/// sources pasted came in as its heading alone, every address gone, with
/// nothing said. A link naming a label still goes where its definition
/// says.
#[test]
fn a_paste_keeps_its_link_definitions_as_lines() {
    assert_eq!(
        pasted("# Sources\n[1]: https://example.com/report\n"),
        ["heading:Sources", "paragraph:[1]: https://example.com/report"]
    );
    assert_eq!(
        pasted("## References\n[1]: https://example.com/a\n[2]: https://example.com/b\n"),
        ["heading:References", "paragraph:[1]: https://example.com/a\n[2]: https://example.com/b"]
    );
    let md = "See [the report][q3] and [x\\*y].\n\n[q3]: https://example.com/q3 \"Q3\"\n[x\\*y]: https://example.com/xy\n";
    let o = parse_pasted_markdown(md);
    assert!(o.errors.is_empty(), "{:?}", o.errors);
    assert_eq!(
        blocks(&o.nodes),
        [
            "paragraph:See the report and x*y.",
            "paragraph:[q3]: https://example.com/q3 \"Q3\"\n[x\\*y]: https://example.com/xy"
        ]
    );
    assert_eq!(links(&o.nodes), ["https://example.com/q3", "https://example.com/xy"]);
}

/// A row wider than its table's header keeps every cell: GFM drops the
/// cells past the header's, so `3`, or a cell after a pipe in inline code,
/// went without a word. The table is widened to its widest row.
#[test]
fn a_paste_keeps_every_cell_of_a_row_wider_than_its_header() {
    let rows = |md: &str| -> Vec<Vec<String>> {
        let o = parse_pasted_markdown(md);
        assert!(o.errors.is_empty(), "{md}: {:?}", o.errors);
        let mut rows = vec![];
        for n in &o.nodes {
            n.walk(&mut |n| {
                if n.kind == "tableRow" {
                    rows.push(n.content.iter().map(|cell| cell.text_content()).collect());
                }
            });
        }
        rows
    };
    let s = |cells: &[&str]| cells.iter().map(|c| c.to_string()).collect::<Vec<_>>();
    assert_eq!(rows("| a | b |\n|---|---|\n| 1 | 2 | 3 |\n"), [s(&["a", "b", ""]), s(&["1", "2", "3"])]);
    assert_eq!(rows("a | b\n--|--\n1 | 2 | 3\n4\n"), [s(&["a", "b", ""]), s(&["1", "2", "3"]), s(&["4", "", ""])]);
    assert_eq!(
        rows("| a | b |\n|---|---|\n| `x | y` | z |\n"),
        [s(&["a", "b", ""]), s(&["`x", "y`", "z"])]
    );
    assert_eq!(rows("> | a |\n> |:-:|\n> | 1 | 2 | 3 |\n"), [s(&["a", "", ""]), s(&["1", "2", "3"])]);
    // An escaped pipe is a cell's text, and a table as wide as its rows is left as it is.
    assert_eq!(rows("| a | b |\n|---|---|\n| 1 \\| 2 | 3 |\n"), [s(&["a", "b"]), s(&["1 | 2", "3"])]);
}

/// The page's editor read no character reference and no escape: what was
/// typed is what moves. A table's `\|` is the pipe its table showed.
#[test]
fn a_page_keeps_references_and_escapes_as_typed() {
    for (md, kept) in [
        (
            "Write &amp; for an ampersand, &lt;b&gt; for a tag, and &copy; 2026.\n",
            "paragraph:Write &amp; for an ampersand, &lt;b&gt; for a tag, and &copy; 2026.",
        ),
        ("In HTML, write &nbsp; for a space and &#169; or &#xA9;.\n", "paragraph:In HTML, write &nbsp; for a space and &#169; or &#xA9;."),
        ("Share is at \\\\server\\share\n", "paragraph:Share is at \\\\server\\share"),
        ("a\\*b\\* and \\# not a heading, \\_y\n", "paragraph:a\\*b\\* and \\# not a heading, \\_y"),
        ("Tom &amp; Jerry, AT&T\n", "paragraph:Tom &amp; Jerry, AT&T"),
        // A definition is the line it was, its own escapes included.
        ("[a]: x\\*y &amp; z\n", "paragraph:[a]: x\\*y &amp; z"),
    ] {
        let (blocks, notes) = page(md);
        assert_eq!(blocks, [kept], "{md}");
        assert_eq!(notes, Vec::<String>::new(), "{md}");
    }
    let (blocks, _) = page("| a | b |\n| - | - |\n| x \\| y | `c &amp;` |\n");
    assert_eq!(blocks, ["table:abx | yc &amp;"]);
}

/// A backslash at a line's end was typed, and the page's editor showed it:
/// the parser reads it as a line break and drops it, and the conversion was
/// judged against that same rendering, so a path's last `\` and a shell
/// line's continuation went with nothing said.
#[test]
fn a_page_keeps_a_backslash_at_a_lines_end() {
    let (blocks, notes) = page("Paths: C:\\Users\\\nD:\\Data\n");
    assert_eq!(blocks, ["paragraph:Paths: C:\\Users\\\nD:\\Data"]);
    assert_eq!(notes, Vec::<String>::new());

    let m = measure_as(contract(), "docker run \\\n  -p 80:80 \\\n  nginx\n", Dialect::Page);
    assert!(m.text_lost.is_none(), "{:?}", m.text_lost);
    assert!(m.export_diff.is_none(), "{:?}", m.export_diff);
    assert_eq!(blocks_of(&m.outcome.nodes), ["paragraph:docker run \\\n-p 80:80 \\\nnginx"]);

    // Two spaces end a line as they did: no backslash where none was typed.
    let (blocks, _) = page("One  \nTwo\n");
    assert_eq!(blocks, ["paragraph:One\nTwo"]);
}

/// A pending proposal holding a mention, an image or a line break is kept
/// as its text, noted and counted: a suggestion's marks cover text alone,
/// so in the tree rejecting one left its mention behind, accepting a change
/// kept both mentions, and one around an image was dropped without a word.
/// One of text alone is a suggestion.
#[test]
fn a_page_keeps_a_proposal_holding_a_node_as_its_text() {
    for (md, text) in [
        ("Plan: {++Lunch with [@Nick](/person/p_1) on Friday.++}\n", vec!["paragraph:Plan: {++Lunch with  on Friday.++}"]),
        ("{--![x](https://example.com/x.png)--}\n", vec!["paragraph:{--", "image:", "paragraph:--}"]),
        ("{++![x](https://example.com/x.png)++}\n", vec!["paragraph:{++", "image:", "paragraph:++}"]),
        ("{++Lunch on Friday\nby the water++}\n", vec!["paragraph:{++Lunch on Friday\nby the water++}"]),
    ] {
        let m = measure_as(contract(), md, Dialect::Page);
        assert!(m.outcome.errors.is_empty(), "{md}: {:?}", m.outcome.errors);
        assert!(m.text_lost.is_none(), "{md}: {:?}", m.text_lost);
        assert!(m.addresses_lost.is_empty(), "{md}: {:?}", m.addresses_lost);
        assert_eq!(m.proposals_as_text, 1, "{md}");
        assert_eq!(blocks_of(&m.outcome.nodes), text, "{md}");
        let mut marked = false;
        for n in &m.outcome.nodes {
            n.walk(&mut |n| marked |= n.marks.iter().any(|m| m.kind.starts_with("proposed")));
        }
        assert!(!marked, "{md}: no part of it is a suggestion");
        let notes: Vec<&str> = m.outcome.notes.iter().map(|n| n.message.as_str()).collect();
        assert!(notes.iter().any(|n| n.contains("holds a mention, an image or a line break")), "{md}: {notes:?}");
    }
    let m = measure_as(contract(), "Plan: {++Lunch on Friday++} with [@Nick](/person/p_1).\n", Dialect::Page);
    assert_eq!(m.proposals_as_text, 0);
    let suggested: Vec<String> = m.outcome.nodes[0]
        .content
        .iter()
        .filter(|n| n.marks.iter().any(|m| m.kind == "proposedInsertion"))
        .filter_map(|n| n.text.clone())
        .collect();
    assert_eq!(suggested, ["Lunch on Friday"]);
}

/// A single `~` strikes nothing in the page's editor, as on GitHub; `~~`
/// does.
#[test]
fn a_page_strikes_only_what_two_tildes_strike() {
    let m = measure_as(contract(), "~~gone~~ and ~single~\n", Dialect::Page);
    assert!(m.text_lost.is_none(), "{:?}", m.text_lost);
    let para = &m.outcome.nodes[0];
    let struck: Vec<&str> = para
        .content
        .iter()
        .filter(|n| n.marks.iter().any(|m| m.kind == "strike"))
        .filter_map(|n| n.text.as_deref())
        .collect();
    assert_eq!(struck, ["gone"]);
    assert_eq!(blocks(&m.outcome.nodes), ["paragraph:gone and ~single~"]);
}

/// What blocks cannot hold of what the editor showed is said: a code
/// block's info past its language, a NUL.
#[test]
fn a_page_notes_what_blocks_cannot_hold() {
    let (blocks, notes) = page("```rust title=main.rs\nfn main() {}\n```\n");
    assert_eq!(blocks, ["codeBlock:fn main() {}"]);
    assert!(notes.iter().any(|n| n.contains("title=main.rs")), "{notes:?}");

    let (blocks, notes) = page("A\0B\n");
    assert_eq!(blocks, ["paragraph:A\u{FFFD}B"]);
    assert!(notes.iter().any(|n| n.contains("NUL")), "{notes:?}");
}

/// The page's editor read no front matter: a page opening with `---`, a
/// line and `---` showed a rule and a heading, and moves as them.
#[test]
fn a_page_opening_with_dashes_keeps_its_lines() {
    let (blocks, notes) = page("---\nMeeting notes\n---\nDiscussed the budget.\n");
    assert_eq!(blocks, ["horizontalRule:", "heading:Meeting notes", "paragraph:Discussed the budget."]);
    assert!(!notes.iter().any(|n| n.contains("front matter")), "{notes:?}");
}
