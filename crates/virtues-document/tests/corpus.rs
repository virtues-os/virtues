//! The conformance corpus, Rust's side alone. The browser's side (ProseMirror
//! parsing the same cases, Tiptap's binding reading Rust's Yjs) is the web
//! conformance test, which drives the `document-conformance` binary.
//!
//! A case named `REFUSE …` must be refused; every other case must be accepted
//! and hold the guarantees the model's edits rely on.

use serde_json::{json, Value};
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
        "callout tones",
        "a callout exports as `> [!TONE]`, which the converter does not read yet (plan slice 4)",
    ),
    ("callout in list item", "as `callout tones`"),
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
];

#[test]
fn every_markdown_export_reads_back_in() {
    let c = contract();
    let plain = |nodes: &[Node]| render::html(c, nodes, &render::Options { ids: false });
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
