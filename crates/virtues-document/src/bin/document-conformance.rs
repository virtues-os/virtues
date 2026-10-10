//! The Rust half of the document conformance check, and the migration
//! measure. The web conformance test (vitest) and CI call it:
//!
//! ```text
//! document-conformance batch <cases.json> <out.json>
//!     each {name, html} → errors, notes, and for an accepted case the tree,
//!     canonical HTML, markdown export and the Yjs update Rust writes (hex)
//! document-conformance read-updates <in.json> <out.json>
//!     each {name, update} (hex, written by the browser binding) → the tree
//!     and canonical HTML Rust reads from it, the problems the server's
//!     check of a whole document (`validate_doc`) finds in it, and those of
//!     them the server would refuse the update for (`check_update`)
//! document-conformance migrate files <path>... [--report <out.json>]
//! document-conformance migrate json <pages.json> [--report <out.json>]
//!     convert markdown documents ({name, markdown} for json) and report
//!     notes, lost text and whether the export reads back to the same tree
//! ```

use anyhow::{bail, Context};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use virtues_document::{contract, decode_update, ingest, migrate, render, validate, ydoc};
use yrs::{ReadTxn, StateVector, Transact};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> anyhow::Result<Vec<u8>> {
    s.as_bytes()
        .chunks(2)
        .enumerate()
        .map(|(i, pair)| {
            std::str::from_utf8(pair)
                .ok()
                .filter(|p| p.len() == 2)
                .and_then(|p| u8::from_str_radix(p, 16).ok())
                .with_context(|| format!("bad hex at {}", i * 2))
        })
        .collect()
}

fn read_cases(path: &str) -> anyhow::Result<Vec<Value>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {path}"))
}

fn write_json(path: &str, value: &impl serde::Serialize) -> anyhow::Result<()> {
    std::fs::write(path, serde_json::to_string_pretty(value)?)
        .with_context(|| format!("writing {path}"))
}

fn batch(input: &str, output: &str) -> anyhow::Result<()> {
    let c = contract();
    let mut out = vec![];
    for case in read_cases(input)? {
        let html = case["html"].as_str().unwrap_or("");
        let o = ingest::parse(c, html, &c.fragment, ingest::Mode::Strict);
        let mut r = json!({ "name": case["name"], "errors": o.errors, "notes": o.notes });
        if o.errors.is_empty() {
            let mut tree = o.nodes.clone();
            ydoc::canonicalize(c, &mut tree);
            let doc = ydoc::new_doc();
            {
                let mut txn = doc.transact_mut();
                let frag = ydoc::fragment(c, &mut txn);
                ydoc::write_nodes(c, &mut txn, &frag, 0, &o.nodes);
            }
            let update = doc
                .transact()
                .encode_state_as_update_v1(&StateVector::default());
            r["tree"] = serde_json::to_value(&tree)?;
            r["html"] = json!(render::html(c, &tree, &render::Options { ids: true }));
            r["markdown"] = json!(render::markdown(c, &tree));
            r["update"] = json!(hex(&update));
        }
        out.push(r);
    }
    write_json(output, &out)
}

fn read_updates(input: &str, output: &str) -> anyhow::Result<()> {
    let c = contract();
    let mut out = vec![];
    for case in read_cases(input)? {
        let name = case["name"].as_str().unwrap_or("");
        let doc = ydoc::new_doc();
        let bytes =
            unhex(case["update"].as_str().unwrap_or("")).with_context(|| format!("case {name}"))?;
        doc.transact_mut()
            .apply_update(decode_update(&bytes).with_context(|| format!("case {name}"))?)
            .with_context(|| format!("case {name}"))?;
        // What the server's check refuses: the update brought into an empty
        // page, so every problem it refuses is the update's.
        let refused = validate::check_update(c, &ydoc::new_doc(), &bytes)
            .with_context(|| format!("case {name}"))?;
        let txn = doc.transact();
        let tree = ydoc::read_doc(c, &txn);
        out.push(json!({
            "name": case["name"],
            "tree": tree,
            "html": render::html(c, &tree, &render::Options { ids: true }),
            "problems": validate::validate_doc(c, &txn),
            "refused": refused,
        }));
    }
    write_json(output, &out)
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    if root.is_file() {
        out.push(root.to_path_buf());
        return out;
    }
    if let Ok(rd) = std::fs::read_dir(root) {
        let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                out.extend(walk(&p));
            } else if p.extension().map(|e| e == "md").unwrap_or(false) {
                out.push(p);
            }
        }
    }
    out
}

fn migrate_report(args: &[String]) -> anyhow::Result<()> {
    let c = contract();
    let Some(mode) = args.first() else {
        bail!(USAGE)
    };
    let report_at = args.iter().position(|a| a == "--report");
    let sources = &args[1..report_at.unwrap_or(args.len()).max(1)];
    let report_path = report_at.and_then(|i| args.get(i + 1));
    let mut docs: Vec<(String, String)> = vec![];
    match mode.as_str() {
        "files" => {
            for root in sources {
                for entry in walk(Path::new(root)) {
                    let text = std::fs::read_to_string(&entry)
                        .with_context(|| format!("reading {}", entry.display()))?;
                    docs.push((entry.display().to_string(), text));
                }
            }
        }
        "json" => {
            let Some(path) = sources.first() else {
                bail!("migrate json <pages.json>")
            };
            for d in read_cases(path)? {
                docs.push((
                    d["name"].as_str().unwrap_or("").into(),
                    d["markdown"].as_str().unwrap_or("").into(),
                ));
            }
        }
        _ => bail!(USAGE),
    }

    let mut untouched = 0;
    let mut rejected = vec![];
    let mut categories: BTreeMap<String, (usize, usize)> = BTreeMap::new(); // (documents, occurrences)
    let mut text_lost = vec![];
    let mut export_same = 0;
    let mut export_diffs: BTreeMap<String, usize> = BTreeMap::new();
    let mut per_doc = vec![];

    for (name, md) in &docs {
        let m = migrate::measure(c, md);
        if !m.outcome.errors.is_empty() {
            rejected.push(json!({ "doc": name, "errors": m.outcome.errors }));
            continue;
        }
        if m.outcome.notes.is_empty() {
            untouched += 1;
        }
        let mut seen = HashSet::new();
        for n in &m.outcome.notes {
            let cat = migrate::category(&n.message);
            let e = categories.entry(cat.clone()).or_default();
            e.1 += 1;
            if seen.insert(cat) {
                e.0 += 1;
            }
        }
        if let Some(loss) = &m.text_lost {
            let mut v = serde_json::to_value(loss)?;
            v["doc"] = json!(name);
            text_lost.push(v);
        }
        match &m.export_diff {
            None => export_same += 1,
            Some(d) => {
                let key = d
                    .split(" text ")
                    .next()
                    .unwrap_or(d)
                    .split(" attrs ")
                    .next()
                    .unwrap_or(d)
                    .to_string();
                let last = key
                    .split('/')
                    .rfind(|s| !s.is_empty())
                    .unwrap_or("")
                    .to_string();
                let suffix = if d.contains(" text ") {
                    " text"
                } else if d.contains(" attrs ") {
                    " attrs"
                } else if d.contains("became") {
                    " kind"
                } else {
                    ""
                };
                *export_diffs.entry(last + suffix).or_default() += 1;
            }
        }
        per_doc.push(json!({
            "doc": name,
            "notes": m.outcome.notes.iter().map(|n| migrate::category(&n.message)).collect::<Vec<_>>(),
            "text_lost": m.text_lost.is_some(),
            "export_diff": m.export_diff,
        }));
    }

    let accepted = docs.len() - rejected.len();
    println!("documents:            {}", docs.len());
    println!("converted:            {accepted}");
    println!("  untouched:          {untouched}");
    println!("  with notes:         {}", accepted - untouched);
    println!("refused:              {}", rejected.len());
    println!("text lost:            {}", text_lost.len());
    println!("export reads back:    {export_same}/{accepted}");
    println!("\nnotes (documents, occurrences):");
    let mut cats: Vec<_> = categories.iter().collect();
    cats.sort_by_key(|(_, (docs, _))| std::cmp::Reverse(*docs));
    for (k, (d, n)) in cats {
        println!("  {d:>4} {n:>6}  {k}");
    }
    if !export_diffs.is_empty() {
        println!("\nexport differences (first per document):");
        let mut v: Vec<_> = export_diffs.iter().collect();
        v.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
        for (k, n) in v {
            println!("  {n:>4}  {k}");
        }
    }
    if let Some(p) = report_path {
        write_json(
            p,
            &json!({ "rejected": rejected, "text_lost": text_lost, "docs": per_doc }),
        )?;
    }
    Ok(())
}

const USAGE: &str = "usage: document-conformance batch <in> <out> | read-updates <in> <out> | \
                     migrate files <path>... [--report <out>] | migrate json <pages.json> [--report <out>]";

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match (args.first().map(String::as_str), args.get(1), args.get(2)) {
        (Some("batch"), Some(input), Some(output)) => batch(input, output),
        (Some("read-updates"), Some(input), Some(output)) => read_updates(input, output),
        (Some("migrate"), _, _) => migrate_report(&args[1..]),
        _ => bail!(USAGE),
    }
}
