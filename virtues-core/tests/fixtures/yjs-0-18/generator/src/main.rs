//! Writes yrs 0.18.8 page states, the way virtues-core wrote them before the
//! upgrade to yrs 0.28, into virtues-core/tests/fixtures/yjs-0-18/.
//!
//! Every doc is a Y.Text named "content", edited with byte offsets (0.18's
//! default `OffsetKind::Bytes`), and saved as `encode_state_as_update_v1` of
//! the whole doc (what `app_pages.yjs_state` holds). Client ids are fixed so
//! the output is reproducible.
//!
//! Usage: cargo run -- <output dir>

use std::path::{Path, PathBuf};
use yrs::updates::decoder::Decode;
use yrs::{Doc, GetString, ReadTxn, StateVector, Text, Transact, Update, WriteTxn};

const SERVER: u64 = 1;
const EDITOR: u64 = 2;

/// `doc_from_text` in the core.
fn doc_from_text(client: u64, text: &str) -> Doc {
    let doc = Doc::with_client_id(client);
    {
        let mut txn = doc.transact_mut();
        let content = txn.get_or_insert_text("content");
        if !text.is_empty() {
            content.insert(&mut txn, 0, text);
        }
    }
    doc
}

fn text_of(doc: &Doc) -> String {
    let txn = doc.transact();
    txn.get_text("content").map(|t| t.get_string(&txn)).unwrap_or_default()
}

fn state(doc: &Doc) -> Vec<u8> {
    doc.transact().encode_state_as_update_v1(&StateVector::default())
}

/// `apply_text_edit` in the core: find by `str::find`, replace by bytes.
fn find_replace(doc: &Doc, find: &str, replace: &str) {
    let mut txn = doc.transact_mut();
    let text = txn.get_or_insert_text("content");
    let current = text.get_string(&txn);
    let at = current.find(find).unwrap_or_else(|| panic!("{find:?} not in {current:?}")) as u32;
    text.remove_range(&mut txn, at, find.len() as u32);
    text.insert(&mut txn, at, replace);
}

/// An editor's edit: delete `del` bytes at the byte offset of `anchor`
/// (plus `skip`), then insert `ins` there.
fn edit_at(doc: &Doc, anchor: &str, skip: usize, del: usize, ins: &str) {
    let mut txn = doc.transact_mut();
    let text = txn.get_or_insert_text("content");
    let current = text.get_string(&txn);
    let at = current.find(anchor).unwrap_or_else(|| panic!("{anchor:?} not in {current:?}")) + skip;
    assert!(current.is_char_boundary(at) && current.is_char_boundary(at + del));
    if del > 0 {
        text.remove_range(&mut txn, at as u32, del as u32);
    }
    if !ins.is_empty() {
        text.insert(&mut txn, at as u32, ins);
    }
}

fn append(doc: &Doc, s: &str) {
    let mut txn = doc.transact_mut();
    let text = txn.get_or_insert_text("content");
    let len = text.len(&txn);
    text.insert(&mut txn, len, s);
}

/// Send `from` everything `to` lacks, as the socket does.
fn sync(from: &Doc, to: &Doc) {
    let sv = to.transact().state_vector();
    let update = from.transact().encode_state_as_update_v1(&sv);
    to.transact_mut().apply_update(Update::decode_v1(&update).unwrap());
}

fn sync_both(a: &Doc, b: &Doc) {
    sync(a, b);
    sync(b, a);
    assert_eq!(text_of(a), text_of(b), "replicas diverged");
}

/// Decode a state into a fresh 0.18 doc and read it back: the fixture must
/// round-trip in the version that wrote it.
fn reread(bytes: &[u8]) -> String {
    let doc = Doc::new();
    doc.transact_mut().apply_update(Update::decode_v1(bytes).unwrap());
    text_of(&doc)
}

fn write(dir: &Path, name: &str, bytes: &[u8], text: &str) {
    std::fs::write(dir.join(format!("{name}.bin")), bytes).unwrap();
    std::fs::write(dir.join(format!("{name}.txt")), text).unwrap();
    println!("{name}: {} bytes of state, {} bytes of text", bytes.len(), text.len());
}

fn fixture(dir: &Path, name: &str, doc: &Doc) {
    let bytes = state(doc);
    let text = text_of(doc);
    assert_eq!(reread(&bytes), text, "{name} does not round-trip in 0.18");
    write(dir, name, &bytes, &text);
}

const ASCII: &str = "# Groceries\n\n- Coffee beans\n- Oat milk\n\nAsk Nick about the **Thursday** plan. \
See [the list](/page/page_abc123) and ((David Okafor))[[person_abc123]].\n";

const NON_ASCII: &str = "# Café notes\n\n\
Crème brûlée at the café with Zoë; naïve façade, Ångström, São Paulo, Łódź.\n\n\
東京の朝は静かだ。日本語のテキスト。\n\n\
中文测试：你好，世界！한국어도 있어요.\n\n\
Wave 👋🏽 and thumbs 👍🏿 up.\n\n\
Family 👨‍👩‍👧‍👦 and the flag 🏳️‍🌈 and 🧑🏽‍💻 at work.\n";

fn main() {
    let dir: PathBuf = std::env::args().nth(1).expect("output dir").into();
    std::fs::create_dir_all(&dir).unwrap();

    // Plain ASCII, as a page is first saved.
    fixture(&dir, "ascii", &doc_from_text(SERVER, ASCII));

    // Non-ASCII, then edited by the server (byte offsets from `str::find`)
    // and by an editor, around and inside the multi-byte runs.
    {
        let server = doc_from_text(SERVER, NON_ASCII);
        let editor = Doc::with_client_id(EDITOR);
        sync_both(&server, &editor);
        find_replace(&server, "静か", "賑やか");
        edit_at(&editor, "and the flag", 0, 0, "🎉 ");
        edit_at(&editor, "👍🏿 ", 0, "👍🏿 ".len(), "");
        sync_both(&server, &editor);
        find_replace(&server, "Zoë", "Zoë and Nick");
        edit_at(&editor, "世界", "世".len(), 0, "の");
        append(&server, "\nÉpilogue — fin. 👩‍👩‍👧‍👦\n");
        sync_both(&server, &editor);
        fixture(&dir, "non-ascii", &server);
    }

    // A long history: two replicas, concurrent edits, deletions, inserts in
    // the middle, a server rewrite of one line, synced every few steps.
    {
        let server = doc_from_text(SERVER, ASCII);
        let editor = Doc::with_client_id(EDITOR);
        sync_both(&server, &editor);
        let mut seed: u64 = 0x5eed;
        let mut next = move |n: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 33) as usize) % n.max(1)
        };
        for step in 0..300 {
            let who = if next(3) == 0 { &server } else { &editor };
            let current = text_of(who);
            // A char boundary somewhere in the text.
            let boundaries: Vec<usize> = (0..=current.len()).filter(|&i| current.is_char_boundary(i)).collect();
            let at = boundaries[next(boundaries.len())];
            match next(4) {
                0 | 1 => {
                    let ins = match next(4) {
                        0 => format!("word{step} "),
                        1 => format!("\n- item {step}\n"),
                        2 => "é ".to_string(),
                        _ => "日本 ".to_string(),
                    };
                    let mut txn = who.transact_mut();
                    let text = txn.get_or_insert_text("content");
                    text.insert(&mut txn, at as u32, &ins);
                }
                2 => {
                    // Delete up to 12 bytes, ending on a char boundary.
                    let end = boundaries.iter().copied().filter(|&b| b > at && b <= at + 12).last();
                    if let Some(end) = end {
                        let mut txn = who.transact_mut();
                        let text = txn.get_or_insert_text("content");
                        text.remove_range(&mut txn, at as u32, (end - at) as u32);
                    }
                }
                _ => append(who, &format!("Line {step}.\n")),
            }
            if step % 7 == 6 {
                sync_both(&server, &editor);
            }
        }
        // An insert in the middle and a deletion, last, so the final text
        // depends on both.
        sync_both(&server, &editor);
        let middle = {
            let t = text_of(&server);
            let mut m = t.len() / 2;
            while !t.is_char_boundary(m) {
                m += 1;
            }
            m
        };
        {
            let mut txn = server.transact_mut();
            let text = txn.get_or_insert_text("content");
            text.insert(&mut txn, middle as u32, "[inserted in the middle]");
        }
        {
            let t = text_of(&editor);
            let end = (1..=6).rev().find(|&i| t.is_char_boundary(i)).unwrap();
            let mut txn = editor.transact_mut();
            let text = txn.get_or_insert_text("content");
            text.remove_range(&mut txn, 0, end as u32);
        }
        sync_both(&server, &editor);
        fixture(&dir, "history", &server);
    }

    // A page saved with nothing on it, and one emptied by deleting all of it.
    fixture(&dir, "empty", &doc_from_text(SERVER, ""));
    {
        let doc = doc_from_text(SERVER, NON_ASCII);
        let mut txn = doc.transact_mut();
        let text = txn.get_or_insert_text("content");
        let len = text.len(&txn);
        text.remove_range(&mut txn, 0, len);
        drop(txn);
        fixture(&dir, "emptied", &doc);
    }

    // An incremental update: what an editor sends the server after editing a
    // page it synced (sync step 2 / an update message): an insert in the
    // middle, a deletion, and a non-ASCII append. `incremental.txt` is the
    // text once it is applied to `ascii.bin`.
    {
        let server = doc_from_text(SERVER, ASCII);
        let base = state(&server);
        let editor = Doc::with_client_id(EDITOR);
        sync(&server, &editor);
        let base_sv = editor.transact().state_vector();
        edit_at(&editor, "- Oat milk", 0, 0, "- Bread\n");
        edit_at(&editor, "**Thursday** ", 0, "**Thursday** ".len(), "");
        append(&editor, "Merci, Zoë 👋🏽 東京\n");
        let update = editor.transact().encode_state_as_update_v1(&base_sv);
        sync(&editor, &server);
        let after = text_of(&server);
        assert_eq!(after, text_of(&editor));
        // The base is the "ascii" fixture, byte for byte.
        assert_eq!(base, std::fs::read(dir.join("ascii.bin")).unwrap());
        std::fs::write(dir.join("incremental.bin"), &update).unwrap();
        std::fs::write(dir.join("incremental.txt"), &after).unwrap();
        println!("incremental: update {} bytes on ascii.bin", update.len());
    }
}
