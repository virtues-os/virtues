//! What one input can cost. Every way a tree comes in (the model's HTML,
//! markdown, a Yjs update from a socket) is refused past `MAX_DEPTH` instead
//! of overflowing the stack, which would abort the whole server, and so is a
//! value nested past `MAX_VALUE_DEPTH` inside an update; and reading,
//! checking, building and editing a page costs time in proportion to its
//! size, since the server does it under the page's lock on every update.

use std::time::{Duration, Instant};
use virtues_document::{
    apply_ops, check_update, decode_update, doc_from_nodes, new_doc, parse_html, parse_markdown,
    parse_pasted_markdown, read_doc, to_html, to_markdown, validate, validate_doc, Op, MAX_DEPTH, MAX_VALUE_DEPTH,
};
use yrs::updates::decoder::Decode;
use yrs::{
    GetString, ReadTxn, StateVector, Text, Transact, Update, WriteTxn, Xml, XmlElementPrelim,
    XmlFragment, XmlOut,
};

/// Run `f` on a thread with a tokio worker's stack, 2 MiB, which is what the
/// server runs these on.
fn on_worker_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("no panic")
}

fn nested(open: &str, close: &str, inner: &str, n: usize) -> String {
    format!("{}{inner}{}", open.repeat(n), close.repeat(n))
}

/// A peer that synced `doc`, and the update it sends after `edit`.
fn peer_update(doc: &yrs::Doc, edit: impl FnOnce(&mut yrs::TransactionMut)) -> Vec<u8> {
    let peer = new_doc();
    let state = doc
        .transact()
        .encode_state_as_update_v1(&StateVector::default());
    peer.transact_mut()
        .apply_update(Update::decode_v1(&state).unwrap())
        .unwrap();
    let before = peer.transact().state_vector();
    edit(&mut peer.transact_mut());
    let update = peer.transact().encode_diff_v1(&before);
    update
}

fn refused_as_too_deep(errors: &[virtues_document::Problem]) -> bool {
    errors
        .iter()
        .any(|e| e.message.contains(&format!("{MAX_DEPTH} levels deep")))
}

#[test]
fn the_deepest_page_allowed_goes_through_every_walk() {
    on_worker_stack(|| {
        // MAX_DEPTH levels: the blockquotes and the paragraph inside them.
        let html = nested(
            "<blockquote>",
            "</blockquote>",
            "<p>deep <strong>text</strong></p>",
            MAX_DEPTH - 1,
        );
        let o = parse_html(&html, "doc");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        let doc = doc_from_nodes(o.nodes);
        let tree = read_doc(&doc.transact());
        assert_eq!(validate(&tree), []);
        assert_eq!(validate_doc(&doc.transact()), []);
        assert!(to_html(&tree, true).contains("deep <strong>text</strong>"));

        let md = to_markdown(&tree);
        let back = parse_markdown(&md);
        assert!(back.errors.is_empty(), "{:?}", back.errors);

        // A keystroke at the bottom, then the model replacing that block.
        let update = peer_update(&doc, |txn| {
            let mut el = txn.get_or_insert_xml_fragment("doc").get(txn, 0);
            while let Some(XmlOut::Element(e)) = el {
                match e.get(txn, 0) {
                    Some(XmlOut::Text(t)) => {
                        t.insert(txn, 0, "very ");
                        break;
                    }
                    next => el = next,
                }
            }
        });
        assert_eq!(check_update(&doc, &update).unwrap(), []);
        let mut id = None;
        tree[0].walk(&mut |n| {
            if n.kind == "paragraph" {
                id = n.id().map(str::to_string);
            }
        });
        let id = id.unwrap();
        // A block that would take the page one level past the limit.
        let deeper = apply_ops(
            &doc,
            Some(&tree),
            &[Op::Replace {
                id: id.clone(),
                html: "<blockquote><p>one more</p></blockquote>".into(),
            }],
        );
        assert!(
            deeper.as_ref().is_err_and(|e| refused_as_too_deep(e)),
            "{deeper:?}"
        );
        let applied = apply_ops(
            &doc,
            Some(&tree),
            &[Op::Replace {
                id,
                html: "<p>replaced</p>".into(),
            }],
        );
        assert!(applied.is_ok(), "{applied:?}");
    });
}

#[test]
fn deeper_html_is_refused_not_a_crash() {
    on_worker_stack(|| {
        // Past the tree's limit; just inside the DOM's looser one, so every
        // walk of ingest runs to its deepest; and far past both.
        for n in [MAX_DEPTH, 2 * MAX_DEPTH - 2, 2_000, 100_000] {
            let o = parse_html(
                &nested("<blockquote>", "</blockquote>", "<p>x</p>", n),
                "doc",
            );
            assert!(refused_as_too_deep(&o.errors), "{n}: {:?}", o.errors);
        }
        // A mark nested in itself is one mark, so the tree stays shallow; the
        // walk over the DOM still recurses for each.
        let strong = |n| format!("<p>{}</p>", nested("<strong>", "</strong>", "x", n));
        let o = parse_html(&strong(2 * MAX_DEPTH - 2), "doc");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        for n in [2_000, 20_000] {
            let o = parse_html(&strong(n), "doc");
            assert!(refused_as_too_deep(&o.errors), "{n}: {:?}", o.errors);
        }
        // Formatting left open nests, as HTML parses it.
        let o = parse_html(&format!("<p>{}x</p>", "<b>".repeat(5_000)), "doc");
        assert!(refused_as_too_deep(&o.errors), "{:?}", o.errors);
    });
}

#[test]
fn deeper_markdown_is_refused_not_a_crash() {
    on_worker_stack(|| {
        for md in [
            format!("{}x", "> ".repeat(2_000)),
            format!("{}x", "> ".repeat(100_000)),
            format!("{}x", "- ".repeat(2_000)),
        ] {
            let o = parse_markdown(&md);
            assert!(refused_as_too_deep(&o.errors), "{:?}", o.errors);
        }
    });
}

#[test]
fn a_deeper_yjs_update_is_refused_not_a_crash() {
    on_worker_stack(|| {
        let doc = doc_from_nodes(parse_html("<p>x</p>", "doc").nodes);
        for n in [MAX_DEPTH + 1, 5_000] {
            let update = peer_update(&doc, |txn| {
                let frag = txn.get_or_insert_xml_fragment("doc");
                let mut el = frag.push_back(txn, XmlElementPrelim::empty("blockquote"));
                for _ in 1..n {
                    el = el.push_back(txn, XmlElementPrelim::empty("blockquote"));
                }
            });
            let problems = check_update(&doc, &update).unwrap();
            assert!(refused_as_too_deep(&problems), "{n}: {problems:?}");
        }
        assert_eq!(validate_doc(&doc.transact()), []);
    });
}

/// `update` with the one occurrence of `from` replaced by `to`.
fn splice(update: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let at: Vec<usize> = (0..=update.len() - from.len())
        .filter(|&i| update[i..].starts_with(from))
        .collect();
    assert_eq!(at.len(), 1, "the placeholder occurs once");
    [&update[..at[0]], to, &update[at[0] + from.len()..]].concat()
}

/// lib0's unsigned varint.
fn var(out: &mut Vec<u8>, mut n: usize) {
    while n >= 0x80 {
        out.push((n as u8 & 0x7f) | 0x80);
        n >>= 7;
    }
    out.push(n as u8);
}

/// A value in lib0's encoding, `depth` arrays around a number. yrs's own
/// encoder would recurse as deep, so it is written by hand.
fn nested_any(depth: usize) -> Vec<u8> {
    let mut v = [117u8, 1].repeat(depth);
    v.extend([125, 0]);
    v
}

/// A string value, as lib0 encodes one.
fn any_string(s: &str) -> Vec<u8> {
    let mut v = vec![119];
    var(&mut v, s.len());
    v.extend_from_slice(s.as_bytes());
    v
}

#[test]
fn values_nested_past_the_limit_are_refused_not_a_crash() {
    on_worker_stack(|| {
        const MARKER: &str = "nested-value-goes-here";
        let doc = doc_from_nodes(parse_html("<h2>Plan</h2><p>x</p>", "doc").nodes);
        // A heading's attribute, `meta`'s entry, and a mark's value: what an
        // update can carry that yrs decodes by recursing.
        let attribute = peer_update(&doc, |txn| {
            let frag = txn.get_or_insert_xml_fragment("doc");
            let Some(XmlOut::Element(h)) = frag.get(txn, 0) else {
                panic!("the heading")
            };
            h.insert_attribute(txn, "level", MARKER);
        });
        let meta = peer_update(&doc, |txn| {
            let meta = txn.get_or_insert_map("meta");
            yrs::Map::insert(&meta, txn, "contract", MARKER);
        });
        let mark = peer_update(&doc, |txn| {
            let frag = txn.get_or_insert_xml_fragment("doc");
            let Some(XmlOut::Element(p)) = frag.get(txn, 1) else {
                panic!("the paragraph")
            };
            let Some(XmlOut::Text(t)) = p.get(txn, 0) else {
                panic!("its text")
            };
            let attrs = yrs::types::Attrs::from([(
                std::sync::Arc::from("bold"),
                yrs::Any::from(std::collections::HashMap::from([(
                    "x".to_string(),
                    yrs::Any::from(MARKER),
                )])),
            )]);
            t.format(txn, 0, 1, attrs);
        });
        for depth in [MAX_VALUE_DEPTH + 1, 1_000, 100_000] {
            let deep = nested_any(depth);
            let mut json = vec![];
            let text = format!("{}{}", "[".repeat(depth), "]".repeat(depth));
            var(&mut json, text.len());
            json.extend_from_slice(text.as_bytes());
            let short_json = format!("{{\"x\":\"{MARKER}\"}}");
            let mut placeholder = vec![short_json.len() as u8];
            placeholder.extend_from_slice(short_json.as_bytes());
            for (what, update) in [
                ("attribute", splice(&attribute, &any_string(MARKER), &deep)),
                ("meta", splice(&meta, &any_string(MARKER), &deep)),
                ("mark", splice(&mark, &placeholder, &json)),
            ] {
                let err = check_update(&doc, &update).unwrap_err().to_string();
                assert!(err.contains("nests more than"), "{what} {depth}: {err}");
                let err = decode_update(&update).unwrap_err().to_string();
                assert!(err.contains("nests more than"), "{what} {depth}: {err}");
            }
        }
        // Up to the limit, the update decodes and the contract answers.
        let ok = splice(&attribute, &any_string(MARKER), &nested_any(MAX_VALUE_DEPTH));
        let problems = check_update(&doc, &ok).unwrap();
        assert!(
            problems.iter().any(|p| p.message.contains("`level`")),
            "{problems:?}"
        );
        assert_eq!(validate_doc(&doc.transact()), []);
    });
}

/// Source the scan must read the way HTML's tokenizer does, or the HTML
/// after it passes unseen by the guards on what parsing costs: comments the
/// tokenizer ends at once or with `--!>`, its bogus comments, an end tag's
/// attributes, and what HTML reads as text in one place and as tags in
/// another (inside `<svg>`, a `<style>` is an ordinary element; a CDATA
/// section runs to `]]>` there and to the first `>` elsewhere).
const PREFIXES: &[&str] = &[
    "",
    "<!-->",
    "<!--->",
    "<!-- x --!>",
    "<!x <p title=\">",
    "<? <p title=\">",
    "</# <p title=\">",
    "</p title=\"<!--\">",
    "<style><!--</style>",
    "<textarea><p title=\"</textarea>",
    "<script><!--<script></script>",
    "<svg><style>",
    "<svg><![CDATA[><!--]]>",
    "<![CDATA[<!--]]>",
];

/// HTML's parser builds a formatting element left open again inside every
/// block after it, and checks each attribute of a tag against the ones
/// before it: either turns a few kilobytes into gigabytes or minutes. Both
/// are refused before the parse, on every path that parses HTML, whatever
/// comes first.
#[test]
fn html_that_costs_more_than_its_size_is_refused_before_it_is_parsed() {
    on_worker_stack(|| {
        // Distinct attributes keep the parser from capping the copies at
        // three of each.
        let open: String = (0..150).map(|i| format!("<b title=\"{i}\">")).collect();
        let attrs: String = (0..100_000).map(|i| format!(" data-a{i}=\"x\"")).collect();
        let cases = [
            ("formatting", format!("<p>{open}x{}", "<p>x".repeat(40_000))),
            ("attributes", format!("<p{attrs}>x</p>")),
            ("depth", "<blockquote>".repeat(30_000)),
        ];
        let refused = |errors: &[virtues_document::Problem]| {
            errors.iter().any(|e| {
                e.message.contains("open across blocks")
                    || e.message.contains("more than 100 attributes")
                    || e.message.contains("elements open at once")
                    || e.message.contains("reads two ways")
            })
        };
        for prefix in PREFIXES {
            for (what, html) in &cases {
                let html = format!("{prefix}{html}");
                let doc = doc_from_nodes(parse_html("<p>x</p>", "doc").nodes);
                let start = Instant::now();
                let parsed = parse_html(&html, "doc");
                let applied = apply_ops(&doc, None, &[Op::Append { html: html.clone() }]);
                // Inline HTML inside one markdown paragraph reaches the same
                // parse.
                let converted = parse_markdown(&format!("Intro {html}\n"));
                assert!(
                    start.elapsed() < Duration::from_secs(2),
                    "{prefix:?} {what}: {:?}",
                    start.elapsed()
                );
                assert!(refused(&parsed.errors), "{prefix:?} {what}: {:?}", parsed.errors);
                assert!(applied.as_ref().is_err_and(|e| refused(e)), "{prefix:?} {what}");
                assert!(
                    refused(&converted.errors),
                    "{prefix:?} {what}: {:?}",
                    converted.errors
                );
            }
            // Trap 8: a self-closed widget swallows what follows it.
            let widget = format!("{prefix}<p>Chart:</p><virtues-applet ref=\"sleep-week\"/>");
            let o = parse_html(&widget, "doc");
            assert!(!o.errors.is_empty(), "{prefix:?}: a self-closed widget was taken");
        }
        // A comment is not read for tags: a self-closed widget inside one
        // is not refused.
        let o = parse_html(
            "<!-- <virtues-applet ref=\"x\"/> --><p>x</p><!--><p>y</p>",
            "doc",
        );
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        assert_eq!(to_html(&o.nodes, false), "<p>x</p><p>y</p>");
        // Formatting closed where its text ends costs nothing, however long
        // the page.
        let fine: String = (0..20_000)
            .map(|i| format!("<p><b title=\"{i}\">x</b> y</p>"))
            .collect();
        let o = parse_html(&fine, "doc");
        assert!(
            !o.errors.iter().any(|e| e.message.contains("open across blocks")),
            "{:?}",
            o.errors.first()
        );
    });
}

/// Comments and CDATA sections each end at a fixed string; input full of
/// them that lacks it once read on to the end from every one, so a few MiB
/// pasted or written took minutes, under the page's lock for an edit and
/// holding the box's one conversion turn for a paste. Every way in costs
/// about four times as long for four times the input, as it does for
/// paragraphs; so does formatting left open and closed by end tags that
/// close none of it.
/// A named input of about `n` bytes.
type Shape = (&'static str, fn(usize) -> String);
/// A named way into a parse.
type Way = (&'static str, fn(&str));

#[test]
fn comments_and_cdata_cost_what_they_hold() {
    let shapes: [Shape; 4] = [
        ("<!--x-->", |n| "<!--x-->".repeat(n / 8)),
        ("<!--x--!>", |n| "<!--x--!>".repeat(n / 9)),
        ("<![CDATA[x>", |n| "<![CDATA[x>".repeat(n / 11)),
        ("<b> then </i>", |n| format!("{}{}", "<b>".repeat(n / 7), "</i>".repeat(n / 7))),
    ];
    for (what, make) in shapes {
        let (small, large) = (make(128 << 10), make(512 << 10));
        let ways: [Way; 3] = [
            ("html", |h| {
                parse_html(h, "doc");
            }),
            ("pasted markdown", |h| {
                parse_pasted_markdown(&format!("# Notes\n\n{h}\n"));
            }),
            ("an edit", |h| {
                let doc = doc_from_nodes(parse_html("<p>x</p>", "doc").nodes);
                let _ = apply_ops(&doc, None, &[Op::Append { html: h.to_string() }]);
            }),
        ];
        for (way, f) in ways {
            let s = best(|| f(&small));
            let l = best(|| f(&large));
            let ratio = l.as_secs_f64() / s.as_secs_f64().max(1e-6);
            assert!(
                ratio < 9.0 && l < Duration::from_secs(5),
                "{what} as {way}: 128 KiB {s:?}, 512 KiB {l:?} ({ratio:.1}x)"
            );
        }
    }
}

#[test]
fn input_past_the_size_limit_is_refused() {
    let big = "x".repeat(virtues_document::MAX_INPUT_BYTES + 1);
    assert!(!parse_html(&big, "doc").errors.is_empty());
    assert!(!parse_markdown(&big).errors.is_empty());
    // A batch of ops is one call: two halves past the limit together are
    // refused, though each would pass alone.
    let half = format!("<p>{}</p>", "x".repeat(virtues_document::MAX_INPUT_BYTES / 2));
    let doc = doc_from_nodes(parse_html("<p>x</p>", "doc").nodes);
    let ops = [
        Op::Append { html: half.clone() },
        Op::Append { html: half },
    ];
    let refused = apply_ops(&doc, None, &ops).unwrap_err();
    assert!(refused[0].message.contains("at most"), "{refused:?}");
}

/// The best of five, to keep a loaded machine's noise out of a ratio.
fn best(mut f: impl FnMut()) -> Duration {
    (0..5)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed()
        })
        .min()
        .unwrap()
}

/// Times for a page of `n` paragraphs and a table of `n` rows, one cell of
/// which spans two: build, read, check a keystroke, and replace the last
/// block.
fn costs(n: usize) -> [Duration; 4] {
    let mut html: String = (0..n)
        .map(|i| format!("<p>Paragraph {i} with a few words.</p>"))
        .collect();
    html.push_str("<table><tr><td rowspan=\"2\">a</td><td>b</td><td>c</td></tr>");
    html.push_str("<tr><td>d</td><td>e</td></tr>");
    html.push_str(&"<tr><td>1</td><td>2</td><td>3</td></tr>".repeat(n));
    html.push_str("</table><p>Last.</p>");
    let parsed = parse_html(&html, "doc");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors.first());
    let nodes = parsed.nodes;
    let build = best(|| {
        doc_from_nodes(nodes.clone());
    });
    let doc = doc_from_nodes(nodes);
    let read = best(|| {
        read_doc(&doc.transact());
    });
    let update = peer_update(&doc, |txn| {
        let frag = txn.get_or_insert_xml_fragment("doc");
        if let Some(XmlOut::Element(p)) = frag.get(txn, frag.len(txn) - 1) {
            if let Some(XmlOut::Text(t)) = p.get(txn, 0) {
                t.insert(txn, 0, "!");
            }
        }
    });
    let check = best(|| {
        assert_eq!(check_update(&doc, &update).unwrap(), []);
    });
    let id = read_doc(&doc.transact())
        .last()
        .and_then(|n| n.id().map(str::to_string))
        .unwrap();
    let replace = best(|| {
        apply_ops(
            &doc,
            None,
            &[Op::Replace {
                id: id.clone(),
                html: "<p>Changed.</p>".into(),
            }],
        )
        .unwrap();
    });
    let frag = doc.transact().get_xml_fragment("doc").unwrap();
    assert!(frag
        .get_string(&doc.transact())
        .ends_with("Changed.</paragraph>"));
    [build, read, check, replace]
}

/// A table's cell spans are bounded by the contract, and its grid by the
/// check: a span past any table's size is refused, on every way a table
/// comes in, without the check allocating a cell for every column it says it
/// covers, which for the largest spans aborts the process.
#[test]
fn spans_past_any_tables_size_are_refused_not_a_crash() {
    on_worker_stack(|| {
        let start = Instant::now();
        for span in ["9223372036854775807", "8000000000000000", "1000000000", "1001"] {
            let html = format!("<table><tr><td colspan=\"{span}\">x</td></tr></table>");
            let o = parse_html(&html, "doc");
            assert!(
                o.errors.iter().any(|e| e.message.contains("at most 1000")),
                "{span}: {:?}",
                o.errors
            );
            let o = parse_markdown(&format!("Notes\n\n{html}\n"));
            assert!(!o.notes.is_empty() || !o.errors.is_empty(), "{span}");
        }
        let o = parse_html(
            "<table><tr><td rowspan=\"65535\">x</td></tr></table>",
            "doc",
        );
        assert!(o.errors.iter().any(|e| e.message.contains("at most 65534")), "{:?}", o.errors);
        // A browser's update, numbers as doubles, as Yjs sends them.
        let doc = doc_from_nodes(parse_html("<table><tr><td>x</td></tr></table>", "doc").nodes);
        for colspan in [8e15, 1e9, 1001.0] {
            let update = peer_update(&doc, |txn| {
                let frag = txn.get_or_insert_xml_fragment("doc");
                let Some(XmlOut::Element(table)) = frag.get(txn, 0) else { panic!("table") };
                let Some(XmlOut::Element(row)) = table.get(txn, 0) else { panic!("row") };
                let Some(XmlOut::Element(cell)) = row.get(txn, 0) else { panic!("cell") };
                cell.insert_attribute(txn, "colspan", yrs::Any::from(colspan));
            });
            let problems = check_update(&doc, &update).unwrap();
            assert!(
                problems.iter().any(|p| p.message.contains("at most 1000")),
                "{colspan}: {problems:?}"
            );
        }
        // Spans inside the contract, in a table too large to map: named,
        // not mapped.
        let wide = format!(
            "<table><tr>{}</tr></table>",
            "<td colspan=\"1000\">x</td>".repeat(2_000)
        );
        let o = parse_html(&wide, "doc");
        assert!(
            o.errors.iter().any(|e| e.message.contains("cells")),
            "{:?}",
            o.errors.first()
        );
        assert!(start.elapsed() < Duration::from_secs(2), "{:?}", start.elapsed());
    });
}

/// The markdown export writes each line once: a run of line breaks, and a
/// page nested deep, cost time in proportion to what is written.
#[test]
fn the_export_costs_what_it_writes() {
    let breaks = |n: usize| parse_html(&format!("<p>a{}b</p>", "<br>".repeat(n)), "doc").nodes;
    let (small, large) = (breaks(25_000), breaks(100_000));
    let s = best(|| {
        to_markdown(&small);
    });
    let l = best(|| {
        to_markdown(&large);
    });
    let ratio = l.as_secs_f64() / s.as_secs_f64().max(1e-6);
    assert!(ratio < 9.0, "breaks: 25,000 {s:?}, 100,000 {l:?} ({ratio:.1}x)");

    // Four times as deep writes four times the prefixes; prefixing each
    // level's markdown again would cost sixteen.
    let deep = |depth: usize| {
        let inner = "<p>x</p>".repeat(2_000);
        let html = format!(
            "{}{inner}{}",
            "<blockquote>".repeat(depth),
            "</blockquote>".repeat(depth)
        );
        let o = parse_html(&html, "doc");
        assert!(o.errors.is_empty(), "{:?}", o.errors.first());
        o.nodes
    };
    let (shallow, deeper) = (deep(24), deep(96));
    on_worker_stack(move || {
        let s = best(|| {
            to_markdown(&shallow);
        });
        let l = best(|| {
            to_markdown(&deeper);
        });
        let ratio = l.as_secs_f64() / s.as_secs_f64().max(1e-6);
        assert!(ratio < 9.0, "depth: 24 {s:?}, 96 {l:?} ({ratio:.1}x)");
    });
}

#[test]
fn page_costs_grow_with_the_page_not_its_square() {
    // Four times the blocks: about four times the time when linear, sixteen
    // when quadratic, which is what indexing children one by one costs.
    let small = costs(2_000);
    let large = costs(8_000);
    for (what, (s, l)) in ["build", "read", "check", "replace"]
        .iter()
        .zip(small.iter().zip(large.iter()))
    {
        let ratio = l.as_secs_f64() / s.as_secs_f64().max(1e-6);
        assert!(
            ratio < 9.0,
            "{what}: 2,000 blocks {s:?}, 8,000 blocks {l:?} ({ratio:.1}x)"
        );
    }
}

/// CriticMarkup is read from every character of the text, so reading it
/// costs time in proportion to the markdown, proposals or none, and a line
/// of nothing but delimiter characters is read, not refused or crashed on.
#[test]
fn proposals_cost_what_the_markdown_holds() {
    let page = |n: usize| "Lunch {--at noon--}{++on Friday++} with Nick. ".repeat(n) + "\n";
    let (small, large) = (page(2_000), page(8_000));
    let o = parse_markdown(&small);
    assert!(o.errors.is_empty(), "{:?}", o.errors.first());
    let marked = o.nodes[0]
        .content
        .iter()
        .filter(|n| n.marks.iter().any(|m| m.kind.starts_with("proposed")))
        .count();
    assert_eq!(marked, 4_000);
    let s = best(|| {
        parse_markdown(&small);
    });
    let l = best(|| {
        parse_markdown(&large);
    });
    let ratio = l.as_secs_f64() / s.as_secs_f64().max(1e-6);
    assert!(ratio < 9.0, "proposals: 2,000 {s:?}, 8,000 {l:?} ({ratio:.1}x)");

    // As many as one conversion holds (each makes about three parser
    // events, `~` pairing into strikethroughs), and then past that, where
    // it is refused as too many parts.
    for (n, read) in [(60_000, true), (600_000, false)] {
        let start = Instant::now();
        let o = parse_markdown(&format!("{}\n", "{-+<>~}".repeat(n)));
        assert_eq!(o.errors.is_empty(), read, "{n}: {:?}", o.errors.first());
        assert!(start.elapsed() < Duration::from_secs(10), "{n}: {:?}", start.elapsed());
    }
}

/// Markdown that only looks like front matter (an indented `---` with a
/// `---` line after it) converts, and quickly: the parser's own reading of
/// front matter never returned on it. Front matter at the very start is
/// still left out, with a note.
#[test]
fn markdown_that_looks_like_front_matter_converts() {
    for md in [
        " ---\nx\n---",
        "Notes:\n\n ---\nLunch with **Nick** on Friday.\n---\n",
        "- [ ] ---\nx\n---",
        "Notes.\n\n---\ntitle: x\n---\n",
    ] {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || tx.send(parse_markdown(md)).ok());
        let o = rx
            .recv_timeout(Duration::from_secs(10))
            .unwrap_or_else(|_| panic!("{md:?}: no answer in ten seconds"));
        assert!(o.errors.is_empty(), "{md:?}: {:?}", o.errors);
        assert!(!o.nodes.is_empty(), "{md:?}");
        assert!(
            o.notes.iter().all(|n| !n.message.contains("front matter")),
            "{md:?}: {:?}",
            o.notes
        );
    }
    let o = parse_markdown("---\ntitle: Trip\n---\n\nLunch.\n");
    assert_eq!(to_markdown(&o.nodes), "Lunch.\n");
    assert!(o.notes.iter().any(|n| n.message.contains("front matter")), "{:?}", o.notes);
}

/// What a conversion holds is bounded by its parts, not only its bytes:
/// markdown under the size limit that makes more parts than one conversion
/// holds, or more HTML than one write takes, is refused before the whole of
/// it is rendered.
#[test]
fn markdown_past_what_one_conversion_holds_is_refused() {
    let cap = virtues_document::MAX_INPUT_BYTES - 64;
    let fill = |unit: &str| unit.repeat(cap / unit.len());
    let mut table = format!("{}|\n{}|\n", "|h".repeat(1000), "|-".repeat(1000));
    while table.len() < cap {
        table.push_str("|a\n");
    }
    for md in [table, fill("*a "), fill("{++a++}"), fill("a\n\n")] {
        let o = parse_markdown(&md);
        assert!(
            o.errors.iter().any(|e| e.message.contains("parts to convert")),
            "{:?}",
            o.errors
        );
    }
    let prose = fill(
        "Lunch with **Nick** on *Friday* at [the cafe](https://example.com/cafe), then a walk by \
         the river and a long talk about the trip.\n\n",
    );
    let o = parse_markdown(&prose);
    assert!(
        o.errors.iter().any(|e| e.message.contains("MiB of HTML")),
        "{:?}",
        o.errors
    );
}
