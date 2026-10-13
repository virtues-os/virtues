//! Articles read as articles (`Dialect::Article`): their footnotes, figures
//! and veils become the contract's own nodes, and the export writes them
//! back in the shape the day page's readers read. `corpus/articles.json` is
//! shared with the web's reader tests, which hold the readers to reading an
//! export as they read the markdown it came from.

use serde_json::{json, Map, Value};
use virtues_document::migrate::{self, ArticleCounts, Dialect};
use virtues_document::{contract, edition_of, parse_article_markdown, to_html, to_markdown, Node};

const ARTICLES: &str = include_str!("corpus/articles.json");

struct Article {
    name: String,
    markdown: String,
    export: String,
    writer: bool,
}

fn articles() -> Vec<Article> {
    let v: Vec<Value> = serde_json::from_str(ARTICLES).expect("articles.json parses");
    v.into_iter()
        .map(|a| Article {
            name: a["name"].as_str().unwrap().to_string(),
            markdown: a["markdown"].as_str().unwrap().to_string(),
            export: a["export"].as_str().unwrap().to_string(),
            writer: a["writer"].as_bool().unwrap(),
        })
        .collect()
}

fn read(md: &str) -> Vec<Node> {
    let o = parse_article_markdown(md);
    assert!(o.errors.is_empty(), "{md:?}: {:?}", o.errors);
    o.nodes
}

fn measured(md: &str) -> (migrate::Measured, ArticleCounts) {
    let m = migrate::measure_as(contract(), md, Dialect::Article);
    let counts = m.article.clone().expect("an article's counts");
    (m, counts)
}

/// Each footnote in `nodes`: its label and note.
fn footnotes(nodes: &[Node]) -> Vec<(String, Option<String>)> {
    let mut out = vec![];
    for n in nodes {
        n.walk(&mut |n| {
            if n.kind == "footnote" {
                let s = |k: &str| n.attrs.get(k).and_then(Value::as_str).map(str::to_string);
                out.push((s("label").unwrap_or_default(), s("note")));
            }
        });
    }
    out
}

/// Each article in the corpus converts without losing a word or an address,
/// and exports as the corpus says: a page as its writer wrote it, byte for
/// byte, and the rest (a table's rule, an escape, a source no sentence
/// cites, a definition's second line) as the readers read it the same.
#[test]
fn a_writers_page_exports_as_written() {
    for a in articles() {
        let nodes = read(&a.markdown);
        let export = to_markdown(&nodes);
        assert_eq!(export, a.export, "{}", a.name);
        if a.writer {
            assert_eq!(export, a.markdown, "{}: a writer's page exports as written", a.name);
        }
        assert_eq!(edition_of(&a.markdown).unwrap(), a.export, "{}", a.name);
        // The export is its own edition: a page written from it reads back.
        assert_eq!(edition_of(&export).unwrap(), export, "{}", a.name);
        let (m, counts) = measured(&a.markdown);
        assert!(m.text_lost.is_none(), "{}: {:?}", a.name, m.text_lost);
        assert_eq!(m.addresses_lost, Vec::<String>::new(), "{}", a.name);
        assert_eq!(m.export_diff, None, "{}", a.name);
        let continued = a.name.contains("continuation");
        assert_eq!(counts.holds(), !continued, "{}: {counts:?}", a.name);
    }
}

/// The full day page: each kind of footnote, figure and veil it holds is a
/// node of the contract, and what it says of itself is counted.
#[test]
fn a_day_page_reads_as_the_articles_nodes() {
    let a = articles().into_iter().find(|a| a.name.starts_with("day page with a table")).unwrap();
    let nodes = read(&a.markdown);
    let html = to_html(&nodes, false);
    for held in [
        "<h2>Morning at the clinic<virtues-footnote label=\"cx-1\" note=\"9:05 AM–10:50 AM\"></virtues-footnote></h2>",
        ".<virtues-footnote label=\"ev-2\" note=\"Message · 9:44 AM · data_communication_message:msg_7\"></virtues-footnote>\
         <virtues-footnote label=\"ev-3\" note=\"Chat · 10:12 AM\"></virtues-footnote></p>",
        "<p><virtues-footnote label=\"ev-4\" note=\"Message · 12:10 PM · data_communication_message:msg_5\"></virtues-footnote></p>",
        "<td><p><virtues-veil>the Blue Kettle</virtues-veil></p></td>",
        "<a href=\"/person/person_n\"><virtues-veil>Nick</virtues-veil></a>",
        "<strong><virtues-veil>Nick</virtues-veil></strong>",
        "<virtues-figure kind=\"quote\" text=\"The waiting room had one window.\" who=\"Nick\" \
         ref=\"data_communication_message:msg_7\" time=\"9:44 AM\"></virtues-figure>",
        "<virtues-figure kind=\"thread\" refs=\"data_communication_message:msg_8, data_communication_message:msg_9\"></virtues-figure>",
        "<virtues-figure kind=\"route\"></virtues-figure>",
        "<virtues-figure kind=\"picture\" src=\"/api/drive/files/f_1/download\" \
         caption=\"A wet morning at ⟦the Larkspur Clinic⟧.\" data-style=\"watercolor\" time=\"10:40 AM\"></virtues-figure>",
    ] {
        assert!(html.contains(held), "{held}\nin\n{html}");
    }
    assert!(!html.contains("[^") && !html.contains("```"), "{html}");
    let (m, counts) = measured(&a.markdown);
    assert_eq!(
        counts,
        ArticleCounts {
            refs_in: 9,
            refs_out: 9,
            notes_in: 10,
            notes_out: 9,
            orphan_notes: vec!["cx-4".into()],
            notes_continued: 0,
            notes_redefined: 0,
            figures_in: 4,
            figures_out: 4,
            figures_as_code: 0,
            veils_in: 6,
            veils_out: 6,
            suggestions_over_nodes: 0,
        }
    );
    assert!(counts.holds());
    let categories: Vec<String> = m.outcome.notes.iter().map(|n| migrate::category(&n.message)).collect();
    assert_eq!(categories, ["orphan footnote left out"]);
}

/// A marker no definition names is a footnote with no note: the readers
/// draw none for it, and its export is the marker alone. So is the first of
/// two markers side by side, which pulldown-cmark leaves as text, and a
/// marker after a `!`.
#[test]
fn a_dangling_reference_stays_a_reference() {
    let md = "Wow![^ev-9] Then lunch.[^ev-1][^ev-2] After![^ev-1]\n\n[^ev-1]: Message · 12:10 PM\n[^ev-2]: Chat · 12:15 PM\n";
    let nodes = read(md);
    assert_eq!(
        footnotes(&nodes),
        [
            ("ev-9".to_string(), None),
            ("ev-1".to_string(), Some("Message · 12:10 PM".to_string())),
            ("ev-2".to_string(), Some("Chat · 12:15 PM".to_string())),
            ("ev-1".to_string(), Some("Message · 12:10 PM".to_string())),
        ]
    );
    assert_eq!(nodes[0].text_content(), "Wow! Then lunch. After!");
    assert_eq!(to_markdown(&nodes), md);
    let (m, counts) = measured(md);
    assert!(m.text_lost.is_none() && m.export_diff.is_none(), "{m:?}");
    assert_eq!((counts.refs_in, counts.refs_out, counts.notes_in, counts.notes_out), (4, 4, 2, 2));
    assert!(counts.holds());

    // In a heading, a table's cell, a list and alone after a table.
    let md = "## Morning[^cx-1]\n\n| A | B |\n| --- | --- |\n| x[^ev-1] | y |\n\n[^ev-2]\n\n- one[^ev-3]\n\n[^cx-1]: 9:05 AM\n[^ev-1]: Recording · 9:10 AM\n[^ev-2]: Recording · 9:20 AM\n[^ev-3]: Recording · 9:30 AM\n";
    let nodes = read(md);
    assert_eq!(footnotes(&nodes).len(), 4);
    assert!(footnotes(&nodes).iter().all(|(_, note)| note.is_some()));
    assert_eq!(to_markdown(&nodes), md);
}

/// A definition no reference cites under a label the day page lifts as a
/// note (`ev-`, `cx-`) is left out of the blocks, with a note naming its
/// label, and counted: no reader drew one.
#[test]
fn an_orphan_definition_is_noted_and_counted() {
    let md = "## Afternoon\n\nYou read.[^ev-1]\n\n[^cx-1]: 1:00 PM\n[^ev-1]: Recording · 1:10 PM\n[^cx-2]: 2:00 PM\n";
    let nodes = read(md);
    assert_eq!(to_markdown(&nodes), "## Afternoon\n\nYou read.[^ev-1]\n\n[^ev-1]: Recording · 1:10 PM\n");
    let (m, counts) = measured(md);
    assert_eq!(counts.orphan_notes, ["cx-1", "cx-2"]);
    assert_eq!((counts.notes_in, counts.notes_out), (3, 1));
    assert!(counts.holds(), "{counts:?}");
    assert!(m.text_lost.is_none());
    let notes: Vec<&str> = m.outcome.notes.iter().map(|n| n.message.as_str()).collect();
    assert_eq!(notes, ["left out 2 sources no sentence cites: [^cx-1], [^cx-2]"]);
}

/// A definition no reference cites under any other label is a paragraph to
/// the day page, which lifts only `ev-` and `cx-` definitions: it stays the
/// text it was, where it stood, and its export reads back. A cited one is
/// a note as ever.
#[test]
fn an_uncited_definition_the_day_page_draws_stays_its_text() {
    let md = "A day.\n\nYou walked.[^ev-1]\n\n[^note]: Remember to call the bank about it.\n\n[^ev-1]: Walk · 7:40 AM\n";
    let nodes = read(md);
    let text: Vec<String> = nodes.iter().map(Node::text_content).collect();
    assert_eq!(text, ["A day.", "You walked.", "[^note]: Remember to call the bank about it."]);
    let (m, counts) = measured(md);
    assert!(m.text_lost.is_none() && m.export_diff.is_none(), "{m:?}");
    assert_eq!((counts.notes_in, counts.notes_out), (1, 1));
    assert!(counts.orphan_notes.is_empty(), "{counts:?}");
    assert!(counts.holds(), "{counts:?}");
    let categories: Vec<String> = m.outcome.notes.iter().map(|n| migrate::category(&n.message)).collect();
    assert_eq!(categories, ["uncited definition kept as text"]);
    let export = to_markdown(&nodes);
    assert_eq!(
        export,
        "A day.\n\nYou walked.[^ev-1]\n\n\\[^note\\]: Remember to call the bank about it.\n\n[^ev-1]: Walk · 7:40 AM\n"
    );
    assert_eq!(edition_of(&export).unwrap(), export);

    let cited = "You walked.[^note]\n\n[^note]: Remember the bank.\n";
    assert_eq!(footnotes(&read(cited)), [("note".to_string(), Some("Remember the bank.".to_string()))]);
}

/// What an article's readers drew as text stays its text: a raw tag that
/// is no markup (the day page draws `<unk>` as it is written), and an
/// opening `---`, a line and `---`, which no reader read as front matter.
/// HTML the export writes (`<u>`, `<br>`) is markup, so an export reads
/// back.
#[test]
fn what_the_readers_drew_as_text_stays_text() {
    let md = "A day.\n\nThe log said <unk> twice.[^ev-1] A Vec<String> broke.[^ev-2]\n\n[^ev-1]: Message · 9:00 AM\n[^ev-2]: Chat · 9:30 AM\n";
    let nodes = read(md);
    assert_eq!(nodes[1].text_content(), "The log said <unk> twice. A Vec<String> broke.");
    let (m, counts) = measured(md);
    assert!(m.text_lost.is_none() && m.export_diff.is_none(), "{m:?}");
    assert!(counts.holds(), "{counts:?}");
    assert!(m.outcome.notes.is_empty(), "{:?}", m.outcome.notes);
    let export = to_markdown(&nodes);
    assert_eq!(edition_of(&export).unwrap(), export);

    let md = "---\nA quiet Sunday at home.\n---\n\nYou read all morning.[^ev-1]\n\n[^ev-1]: Recording · 9:00 AM\n";
    let nodes = read(md);
    let kinds: Vec<&str> = nodes.iter().map(|n| n.kind.as_str()).collect();
    assert_eq!(kinds, ["horizontalRule", "heading", "paragraph"]);
    assert_eq!(nodes[1].text_content(), "A quiet Sunday at home.");
    let (m, _) = measured(md);
    assert!(m.text_lost.is_none() && m.export_diff.is_none(), "{m:?}");
    assert!(!m.outcome.notes.iter().any(|n| n.message.contains("front matter")), "{:?}", m.outcome.notes);

    let md = "You <u>underlined</u> it.\n";
    let nodes = read(md);
    assert!(to_html(&nodes, false).contains("<u>underlined</u>"), "{}", to_html(&nodes, false));
    assert_eq!(to_markdown(&nodes), md);
}

/// A label defined twice takes its last definition, as every reader does
/// (`dayArticle.ts` sets each definition over the one before, and
/// `day_picture::cx_spans` collects them into a map), and it is noted and
/// keeps the article markdown at a move.
#[test]
fn a_relabelled_definition_takes_the_last() {
    let md = "You ran.[^ev-1]\n\n[^ev-1]: Recording · 6:00 AM\n[^ev-1]: Location · 6:30 AM\n";
    let nodes = read(md);
    assert_eq!(footnotes(&nodes), [("ev-1".to_string(), Some("Location · 6:30 AM".to_string()))]);
    let (m, counts) = measured(md);
    assert_eq!((counts.notes_in, counts.notes_redefined), (1, 1));
    assert!(!counts.holds());
    assert!(m.outcome.notes.iter().any(|n| migrate::category(&n.message) == "footnote defined twice"));
}

/// A figure fence the article's figure cannot hold stays the code block it
/// was, which exports the same fence, so the readers draw it as before.
#[test]
fn a_figure_with_an_unknown_key_stays_its_fence() {
    for (body, why) in [
        ("```figure\nkind: quote\ncolor: red\n```", "a figure has no `color`"),
        ("```figure\nkind: quote\nkind: route\n```", "it gives `kind` twice"),
        ("```figure\nkind: chart\n```", "`kind` is one of"),
        ("```figure\nkind: picture\nsrc: javascript:alert(1)\n```", "cannot use `javascript:`"),
        ("```figure\ntext: no kind\n```", "it gives no `kind`"),
        ("```figure\nkind: quote\njust words\n```", "is not `key: value`"),
        ("````figure\nkind: route\n````", "three backticks"),
        ("~~~figure\nkind: route\n~~~", "three backticks"),
    ] {
        let md = format!("A day.\n\n{body}\n");
        let nodes = read(&md);
        assert_eq!(nodes[1].kind, "codeBlock", "{body}");
        assert_eq!(nodes[1].attrs["language"], json!("figure"), "{body}");
        let (m, counts) = measured(&md);
        assert_eq!((counts.figures_in, counts.figures_out, counts.figures_as_code), (1, 0, 1), "{body}");
        assert!(counts.holds());
        assert!(m.outcome.notes.iter().any(|n| n.message.contains(why)), "{body}: {:?}", m.outcome.notes);
        // The same fence, or, for code that would read as a figure, one of
        // four backticks: never one an article reads as a figure.
        let export = to_markdown(&nodes);
        if body.starts_with("```figure") {
            assert_eq!(export, md, "{body}");
        }
        assert_eq!(read(&export)[1].kind, "codeBlock", "{body}: {export}");
    }
    // Keys in any case and order, values' whitespace collapsed: a figure,
    // written back in the contract's order.
    let nodes = read("```figure\nTime:  9:44 AM\nkind: quote\ntext:   Did you get   home?\nwho: \n```\n");
    assert_eq!(nodes[0].kind, "figure");
    assert_eq!(
        to_markdown(&nodes),
        "```figure\nkind: quote\ntext: Did you get home?\nwho: \ntime: 9:44 AM\n```\n"
    );
}

/// A bare `⟦` and the next bare `⟧` in one run of text are a veil; across
/// other syntax, unpaired, empty, written as a character reference, or in
/// code, the brackets stay the text they are.
#[test]
fn a_veil_is_one_run_or_it_is_text() {
    let veiled = |md: &str| -> Vec<String> {
        let mut out = vec![];
        for n in read(md) {
            n.walk(&mut |n| {
                if n.marks.iter().any(|m| m.kind == "veil") {
                    out.push(n.text.clone().unwrap_or_default());
                }
            });
        }
        out
    };
    for (md, veils) in [
        ("At ⟦the clinic⟧ with ⟦Nick⟧.\n", vec!["the clinic", "Nick"]),
        ("[⟦Nick⟧](/person/person_1) and **⟦the clinic⟧**.\n", vec!["Nick", "the clinic"]),
        ("A ==⟦marked⟧== word.\n", vec!["marked"]),
        ("A line ⟦running\nover⟧ two.\n", vec!["running over"]),
        ("⟦**bold**⟧ crosses syntax.\n", vec![]),
        ("⟦open and never closed.\n", vec![]),
        ("Empty ⟦⟧ brackets.\n", vec![]),
        ("Two ⟦opens ⟦then⟧ one.\n", vec!["then"]),
        ("A reference &#10214;x⟧ is not bare.\n", vec![]),
        ("In `⟦code⟧` it is code.\n", vec![]),
        ("| A |\n| --- |\n| ⟦cell⟧ |\n", vec!["cell"]),
    ] {
        assert_eq!(veiled(md), veils, "{md:?}");
        let (m, _) = measured(md);
        assert!(m.text_lost.is_none(), "{md:?}: {:?}", m.text_lost);
    }
    // Brackets left as text export bare, as they were written; the readers
    // read them as they always did.
    let nodes = read("⟦open and never closed.\n");
    assert_eq!(to_markdown(&nodes), "⟦open and never closed.\n");
}

/// A note is the first line of its definition as written: never parsed, so
/// a record's `_` and `*` stay, and written back as it is.
#[test]
fn a_note_is_written_verbatim() {
    let md = "You read.[^ev-1] You ate.[^ev-2]\n\n[^ev-1]: Message · 7:40 PM · data_communication_message:m_1\n[^ev-2]: *starred* <b>&amp; a_b\n";
    let nodes = read(md);
    assert_eq!(
        footnotes(&nodes),
        [
            ("ev-1".to_string(), Some("Message · 7:40 PM · data_communication_message:m_1".to_string())),
            ("ev-2".to_string(), Some("*starred* <b>&amp; a_b".to_string())),
        ]
    );
    assert_eq!(to_markdown(&nodes), md);
    // A note's line break, which no definition holds, is written as a space.
    let mut nodes = read("You read.[^ev-1]\n\n[^ev-1]: one\n");
    nodes[0].content[1].attrs.insert("note".into(), json!("one\ntwo"));
    assert_eq!(to_markdown(&nodes), "You read.[^ev-1]\n\n[^ev-1]: one two\n");
}

/// A definition running past its first line is noted and counted: the day
/// page's readers read its first line alone, so the note holds that line,
/// and a move to blocks waits for a person to look.
#[test]
fn a_continued_definition_is_noted() {
    for md in [
        "You ran.[^ev-1]\n\n[^ev-1]: Recording · 6:00 AM\nby the river\n",
        "You ran.[^ev-1]\n\n[^ev-1]: Recording · 6:00 AM\n\n    A second paragraph.\n",
    ] {
        let nodes = read(md);
        assert_eq!(footnotes(&nodes), [("ev-1".to_string(), Some("Recording · 6:00 AM".to_string()))], "{md:?}");
        let (m, counts) = measured(md);
        assert_eq!(counts.notes_continued, 1, "{md:?}");
        assert!(!counts.holds());
        assert!(m.outcome.notes.iter().any(|n| migrate::category(&n.message) == "footnote definition continued"));
    }
}

/// What reads as a footnote anywhere else stays text: a label outside the
/// label type, and a definition inside a quote, which the readers never
/// read as one. On a page that is not an article, `[^1]` and `⟦` are text.
#[test]
fn what_is_not_a_footnote_stays_text() {
    let md = "Smile[^_^] and x[^a b].\n\n> [^q]: in a quote\n\n[^_^]: one\n[^a b]: two\n";
    let nodes = read(md);
    assert!(footnotes(&nodes).is_empty(), "{nodes:?}");
    let (m, counts) = measured(md);
    assert!(m.text_lost.is_none(), "{:?}", m.text_lost);
    assert_eq!(counts.refs_in, 0);
    let text: String = nodes.iter().map(Node::text_content).collect::<Vec<_>>().join("|");
    assert!(text.contains("[^_^]") && text.contains("[^q]: in a quote") && text.contains("[^a b]: two"), "{text}");

    let page = virtues_document::parse_written_markdown("Lunch[^1] at ⟦noon⟧.\n\n[^1]: a note\n");
    assert!(page.errors.is_empty());
    assert!(footnotes(&page.nodes).is_empty());
    assert!(!to_html(&page.nodes, false).contains("virtues-veil"));
}

// ------------------------------------------------------------ generated trees

/// Words a generated article is made of: fictional, and plain letters, so
/// none of them is markdown.
const WORDS: &[&str] = &[
    "Nick", "lunch", "canal", "rain", "porch", "Okafor", "bakery", "morning", "the", "window", "bridge",
    "kettle", "harbor", "dog", "walked", "read", "quietly",
];

fn text(t: &str, marks: &[&str], href: Option<&str>) -> Value {
    let mut ms: Vec<Value> = vec![];
    if let Some(h) = href {
        ms.push(json!({ "type": "link", "attrs": { "href": h } }));
    }
    ms.extend(marks.iter().map(|m| json!({ "type": m })));
    if ms.is_empty() {
        json!({ "type": "text", "text": t })
    } else {
        json!({ "type": "text", "text": t, "marks": ms })
    }
}

struct Gen {
    rng: fastrand::Rng,
    labels: usize,
}

impl Gen {
    fn words(&mut self, at_least: usize) -> String {
        let n = at_least + self.rng.usize(..3);
        (0..n).map(|_| WORDS[self.rng.usize(..WORDS.len())]).collect::<Vec<_>>().join(" ")
    }

    fn note(&mut self) -> Option<String> {
        match self.rng.usize(..4) {
            0 => None,
            1 => Some(format!("Chat · {}:{:02} PM", 1 + self.rng.usize(..12), self.rng.usize(..60))),
            2 => Some(format!("Recording · 9:05 AM · data_communication_transcription:tr_{}", self.rng.usize(..99))),
            _ => Some(format!("{} · *{}* · a_b", self.words(1), self.words(1))),
        }
    }

    fn footnote(&mut self) -> Value {
        self.labels += 1;
        let label = format!("{}-{}", if self.rng.bool() { "ev" } else { "cx" }, self.labels);
        match self.note() {
            Some(note) => json!({ "type": "footnote", "attrs": { "label": label, "note": note } }),
            None => json!({ "type": "footnote", "attrs": { "label": label } }),
        }
    }

    /// Inline content: runs of words, a marked run always set apart by a
    /// space (as flanking needs; the export moves a space at a mark's edge),
    /// footnotes closing a sentence, mentions, veils alone or under bold,
    /// italic or a link.
    fn inline(&mut self) -> Vec<Value> {
        let mut out = vec![text(&self.words(1), &[], None)];
        for _ in 0..1 + self.rng.usize(..4) {
            match self.rng.usize(..6) {
                0 => {
                    out.push(text(".", &[], None));
                    out.push(self.footnote());
                    if self.rng.bool() {
                        out.push(self.footnote());
                    }
                }
                1 => out.push(json!({ "type": "mention", "attrs": { "to": "/person/person_1", "label": "Nick" } })),
                2 => out.push(text(&self.words(1), &["veil"], None)),
                3 => {
                    let marks: &[&str] = [&["bold", "veil"][..], &["italic"], &["bold"], &["italic", "veil"]][self.rng.usize(..4)];
                    out.push(text(" ", &[], None));
                    out.push(text(&self.words(1), marks, None));
                }
                4 => {
                    out.push(text(" ", &[], None));
                    let veil: &[&str] = if self.rng.bool() { &["veil"] } else { &[] };
                    out.push(text(&self.words(1), veil, Some("/place/place_2")));
                }
                _ => {}
            }
            out.push(text(&format!(" {}", self.words(1)), &[], None));
        }
        merge(out)
    }

    fn figure(&mut self) -> Value {
        let kinds = ["quote", "thread", "route", "picture"];
        let mut attrs = Map::new();
        attrs.insert("kind".into(), json!(kinds[self.rng.usize(..4)]));
        for key in ["text", "who", "ref", "refs", "caption", "style", "time"] {
            if self.rng.usize(..3) == 0 {
                let value = match self.rng.usize(..4) {
                    0 => String::new(),
                    1 => format!("{} ⟦{}⟧", self.words(1), self.words(1)),
                    _ => self.words(1),
                };
                attrs.insert(key.into(), json!(value));
            }
        }
        if self.rng.bool() {
            attrs.insert("src".into(), json!("/api/drive/files/f_1/download"));
        }
        json!({ "type": "figure", "attrs": attrs })
    }

    /// A block. Never a list straight after a list, which markdown reads
    /// as one list.
    fn block(&mut self, after_list: bool) -> Value {
        let para = |g: &mut Gen| json!({ "type": "paragraph", "content": g.inline() });
        match self.rng.usize(..6) {
            2 if after_list => para(self),
            0 => {
                let mut content = vec![text(&self.words(1), &[], None)];
                if self.rng.bool() {
                    content.push(text(" ", &[], None));
                    content.push(text(&self.words(1), &["veil"], None));
                }
                if self.rng.bool() {
                    content.push(self.footnote());
                }
                json!({ "type": "heading", "attrs": { "level": 2 }, "content": merge(content) })
            }
            1 => self.figure(),
            2 => {
                let items: Vec<Value> = (0..1 + self.rng.usize(..3))
                    .map(|_| json!({ "type": "listItem", "content": [para(self)] }))
                    .collect();
                json!({ "type": "bulletList", "content": items })
            }
            3 => {
                let cell = |g: &mut Gen, kind: &str| json!({ "type": kind, "content": [para(g)] });
                json!({ "type": "table", "content": [
                    { "type": "tableRow", "content": [cell(self, "tableHeader"), cell(self, "tableHeader")] },
                    { "type": "tableRow", "content": [cell(self, "tableCell"), cell(self, "tableCell")] },
                ] })
            }
            _ => para(self),
        }
    }
}

/// A tree as a document holds it: each node's attributes filled with the
/// contract's defaults, marks in the contract's order (`ydoc::canonicalize`,
/// which the converter without its Yjs half lacks).
fn canonicalize(nodes: &mut [Node]) {
    let c = contract();
    for n in nodes {
        n.walk_mut(&mut |n| {
            n.marks.sort_by_key(|m| c.mark_rank(&m.kind));
            if !n.is_text() {
                for (k, d) in c.default_attrs(&n.kind) {
                    n.attrs.entry(k).or_insert(d);
                }
            }
        });
    }
}

/// Text runs side by side with the same marks as one, as a tree holds them.
fn merge(runs: Vec<Value>) -> Vec<Value> {
    let mut out: Vec<Value> = vec![];
    for r in runs {
        if let (Some(last), Some(t)) = (out.last_mut(), r.get("text")) {
            if last.get("text").is_some() && last.get("marks") == r.get("marks") {
                let joined = format!("{}{}", last["text"].as_str().unwrap(), t.as_str().unwrap());
                last["text"] = json!(joined);
                continue;
            }
        }
        out.push(r);
    }
    out
}

/// An article's export reads back as the tree it was written from, whatever
/// mix of footnotes, figures, veils, marks, mentions, lists and tables the
/// tree holds: generated trees, a fixed seed each, with no veil brackets in
/// their text (a bracket in unveiled text is a veil to every reader, and
/// exports bare) and no whitespace at a mark's edge (which the export moves
/// out of bold and italic, as CommonMark's flanking needs).
#[test]
fn an_articles_export_reads_back_to_the_same_tree() {
    for seed in 0..400 {
        let mut g = Gen { rng: fastrand::Rng::with_seed(seed), labels: 0 };
        let mut blocks: Vec<Value> = vec![];
        for _ in 0..1 + g.rng.usize(..6) {
            let after_list = blocks.last().is_some_and(|b| b["type"] == "bulletList");
            blocks.push(g.block(after_list));
        }
        let mut tree: Vec<Node> = serde_json::from_value(Value::Array(blocks)).unwrap();
        canonicalize(&mut tree);
        assert_eq!(virtues_document::validate(&tree), [], "seed {seed}");
        let md = to_markdown(&tree);
        let mut back = read(&md);
        canonicalize(&mut back);
        assert_eq!(to_html(&back, false), to_html(&tree, false), "seed {seed}: {md}");
        assert_eq!(to_markdown(&back), md, "seed {seed}");
        let (m, counts) = measured(&md);
        assert!(m.text_lost.is_none() && m.export_diff.is_none(), "seed {seed}: {md}\n{m:?}");
        assert_eq!(counts.refs_in, counts.refs_out, "seed {seed}");
        assert_eq!(counts.figures_in, counts.figures_out, "seed {seed}");
    }
}
