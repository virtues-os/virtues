//! The conformance corpus, Rust's side alone. The browser's side (ProseMirror
//! parsing the same cases, Tiptap's binding reading Rust's Yjs) is the web
//! conformance test, which drives the `document-conformance` binary.
//!
//! A case named `REFUSE …` must be refused; every other case must be accepted
//! and hold the guarantees the model's edits rely on.

use serde_json::{json, Value};
use std::collections::HashSet;
use virtues_document::{contract, ingest, migrate, render, validate, ydoc, Node};
use yrs::Transact;

const CASES: &str = include_str!("corpus/cases.json");

fn cases() -> Vec<(String, String)> {
    let v: Vec<Value> = serde_json::from_str(CASES).expect("cases.json parses");
    v.into_iter()
        .map(|c| {
            (
                c["name"].as_str().unwrap().to_string(),
                c["html"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn strict(html: &str) -> ingest::Outcome {
    let c = contract();
    ingest::parse(c, html, &c.fragment, ingest::Mode::Strict)
}

fn canonical(mut nodes: Vec<Node>) -> Vec<Node> {
    ydoc::canonicalize(contract(), &mut nodes);
    nodes
}

#[test]
fn refusals_are_refused_and_the_rest_accepted() {
    for (name, html) in cases() {
        let o = strict(&html);
        if name.starts_with("REFUSE") {
            assert!(!o.errors.is_empty(), "{name}: accepted, should be refused");
        } else {
            assert!(o.errors.is_empty(), "{name}: {:?}", o.errors);
        }
    }
}

#[test]
fn canonical_html_is_a_fixed_point() {
    let c = contract();
    for (name, html) in cases()
        .into_iter()
        .filter(|(n, _)| !n.starts_with("REFUSE"))
    {
        let tree = canonical(strict(&html).nodes);
        let canon = render::html(c, &tree, &render::Options { ids: true });
        let again = strict(&canon);
        assert!(
            again.errors.is_empty(),
            "{name}: canonical html refused: {:?}",
            again.errors
        );
        let again = canonical(again.nodes);
        assert_eq!(again, tree, "{name}: canonical html parses to another tree");
        assert_eq!(
            render::html(c, &again, &render::Options { ids: true }),
            canon,
            "{name}"
        );
    }
}

#[test]
fn every_accepted_tree_validates_and_round_trips_through_yjs() {
    let c = contract();
    for (name, html) in cases()
        .into_iter()
        .filter(|(n, _)| !n.starts_with("REFUSE"))
    {
        let o = strict(&html);
        let tree = canonical(o.nodes.clone());
        assert_eq!(validate::validate(c, &tree, &c.fragment), [], "{name}");
        let doc = ydoc::new_doc();
        {
            let mut txn = doc.transact_mut();
            let frag = ydoc::fragment(c, &mut txn);
            ydoc::write_nodes(c, &mut txn, &frag, 0, &o.nodes);
        }
        assert_eq!(ydoc::read_doc(c, &doc.transact()), tree, "{name}");
        assert_eq!(validate::validate_doc(c, &doc.transact()), [], "{name}");
    }
}

/// Cases whose markdown export reads back to a different tree, because
/// markdown cannot say what the tree does. Each must still read back without
/// error, and the tree it reads back as must then hold steady: its own
/// export reads back to it.
const EXPORT_DIFFERS: &[(&str, &str)] = &[
    (
        "whitespace around marks",
        "a space at a mark's edge moves outside it: CommonMark's delimiters must touch text",
    ),
    (
        "table thead tbody",
        "a GFM cell holds one line: a cell's paragraphs come back as one with line breaks",
    ),
    ("table spans", "GFM has no column or row spans"),
    ("cell spanning the most columns", "as `table spans`"),
    (
        "image width past 2^53 / 1000",
        "the dialect's `![alt|600](…)` reads a width up to 4294967295; a larger one stays in the alt",
    ),
    ("image width at the largest int", "as `image width past 2^53 / 1000`"),
    (
        "ordered list starting past 2^53 / 1000",
        "CommonMark reads a list's start number up to nine digits",
    ),
    (
        "table align on one cell",
        "GFM aligns a column, not a cell: the column takes the cell's alignment",
    ),
    (
        "image block",
        "an image at a path with no extension, its alt naming no image file, reads back as a \
         file, as both page editors read `![alt](src)` (`kindOfLink`)",
    ),
    (
        "file without a name",
        "an embed with no name exports its file name, which reads back as its name",
    ),
];

#[test]
fn every_markdown_export_reads_back_in() {
    let c = contract();
    // CriticMarkup carries no proposal id: the converter makes new ones, and
    // which runs share one is what reads back.
    let plain = |nodes: &[Node]| {
        let mut nodes = nodes.to_vec();
        migrate::number_proposals(&mut nodes);
        render::html(c, &nodes, &render::Options { ids: false })
    };
    for (name, html) in cases()
        .into_iter()
        .filter(|(n, _)| !n.starts_with("REFUSE"))
    {
        let tree = canonical(strict(&html).nodes);
        let md = render::markdown(c, &tree);
        let back = migrate::from_markdown(c, &md);
        assert!(
            back.errors.is_empty(),
            "{name}: export {md:?} refused: {:?}",
            back.errors
        );
        let back = canonical(back.nodes);
        if !EXPORT_DIFFERS.iter().any(|(n, _)| *n == name) {
            assert_eq!(
                plain(&back),
                plain(&tree),
                "{name}: {md:?} reads back as another tree"
            );
            continue;
        }
        assert_ne!(
            plain(&back),
            plain(&tree),
            "{name} now reads back the same: take it off EXPORT_DIFFERS"
        );
        let again = render::markdown(c, &back);
        let steady = canonical(migrate::from_markdown(c, &again).nodes);
        assert_eq!(
            plain(&steady),
            plain(&back),
            "{name}: {again:?} does not hold steady"
        );
    }
}

#[test]
fn the_binary_writes_and_reads_the_shapes_the_web_test_expects() {
    let bin = env!("CARGO_BIN_EXE_document-conformance");
    let dir = std::env::temp_dir().join(format!("virtues-document-corpus-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cases_path = dir.join("cases.json");
    let rust_path = dir.join("rust.json");
    std::fs::write(&cases_path, CASES).unwrap();

    let status = std::process::Command::new(bin)
        .arg("batch")
        .arg(&cases_path)
        .arg(&rust_path)
        .status()
        .unwrap();
    assert!(status.success());
    let rust: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(&rust_path).unwrap()).unwrap();
    assert_eq!(rust.len(), cases().len());

    // Feed Rust's own updates back through read-updates: the same HTML.
    let updates: Vec<Value> = rust
        .iter()
        .filter(|r| r["errors"].as_array().unwrap().is_empty())
        .map(|r| json!({ "name": r["name"], "update": r["update"] }))
        .collect();
    let updates_path = dir.join("updates.json");
    let back_path = dir.join("back.json");
    std::fs::write(&updates_path, serde_json::to_string(&updates).unwrap()).unwrap();
    let status = std::process::Command::new(bin)
        .arg("read-updates")
        .arg(&updates_path)
        .arg(&back_path)
        .status()
        .unwrap();
    assert!(status.success());
    let back: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(&back_path).unwrap()).unwrap();
    for b in &back {
        let r = rust.iter().find(|r| r["name"] == b["name"]).unwrap();
        assert_eq!(b["html"], r["html"], "{}", b["name"]);
        assert_eq!(b["tree"], r["tree"], "{}", b["name"]);
        assert_eq!(b["problems"], json!([]), "{}", b["name"]);
        assert_eq!(b["refused"], json!([]), "{}", b["name"]);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The two readers of `content` that find refs by their text: the backlinks
/// query (`api::pages::get_page_backlinks`, a `LIKE` on `/page/{id})`) and
/// the project graph (`api::projects::extract_entity_urls`, which scans for
/// `](/person/`, `](/place/` and `](/org/` up to the next `)`). Neither can
/// see a tree, so a mention's export must hold what each reads, byte for
/// byte as the ref picker writes it (`[@Label](/kind/id)`). These mirror the
/// two parsers; virtues-core tests the parsers themselves on a tree page.
fn backlink_targets(content: &str, page_id: &str) -> bool {
    content.contains(&format!("/page/{page_id})"))
}

fn graph_entities(content: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    for prefix in ["/person/", "/place/", "/org/"] {
        let needle = format!("]({prefix}");
        let mut from = 0usize;
        while let Some(hit) = content[from..].find(&needle) {
            let start = from + hit + 2;
            let rest = &content[start..];
            let Some(end) = rest.find(')') else { break };
            let url = &rest[..end];
            if !url.is_empty() && !url.contains(['#', '?', ' ']) {
                out.insert(url.to_string());
            }
            from = start + end;
        }
    }
    out
}

#[test]
fn mentions_export_as_the_ref_picker_writes_them() {
    let c = contract();
    let export = |name: &str| {
        let (_, html) = cases().into_iter().find(|(n, _)| n == name).unwrap();
        render::markdown(c, &canonical(strict(&html).nodes))
    };
    assert_eq!(
        export("mention of a person"),
        "Lunch with [@Nick](/person/person_1).\n"
    );
    assert_eq!(
        export("mention of a person by two names"),
        "Ask [@David Okafor](/person/person_2) first.\n"
    );
    assert_eq!(
        export("mention of a page"),
        "See [@Q3 plan](/page/page_abc).\n"
    );

    let people = export("mention of a person") + &export("mention of a person by two names");
    assert_eq!(
        graph_entities(&people),
        HashSet::from([
            "/person/person_1".to_string(),
            "/person/person_2".to_string()
        ])
    );
    assert!(backlink_targets(&export("mention of a page"), "page_abc"));
    assert!(!backlink_targets(&export("mention of a page"), "page_ab"));

    // Inside a proposal, and among the dialect's other syntax, the ref is
    // still read: a pending proposal names its refs as the page does.
    let o = strict(
        "<p><virtues-ins proposal=\"p1\">with <virtues-mention to=\"/org/org_1\" label=\"Acme [West]\"></virtues-mention></virtues-ins> \
         and <strong><virtues-mention to=\"/place/place_1\" label=\"Lisbon\"></virtues-mention></strong></p>",
    );
    assert!(o.errors.is_empty(), "{:?}", o.errors);
    let md = render::markdown(c, &canonical(o.nodes));
    assert_eq!(
        graph_entities(&md),
        HashSet::from(["/org/org_1".to_string(), "/place/place_1".to_string()]),
        "{md}"
    );
}
