//! Reading a Yjs update that came from outside: a socket, a saved state.
//!
//! yrs decodes a value (`Any`: what a map entry, an XML attribute or a
//! subdocument's options hold) by calling itself once per level of arrays
//! and maps inside it, and parses a mark's value (a JSON string) the same
//! way. Nesting costs two bytes a level, so a few kilobytes nested a
//! thousand deep overflow a 2 MiB thread's stack while decoding, and that
//! aborts the process rather than failing the call. [`decode_update`] walks
//! the bytes first, in a loop, and refuses an update whose values nest past
//! [`MAX_VALUE_DEPTH`] before yrs sees it.
//!
//! The walk follows yrs's v1 decoder (`Update::decode`, `ItemContent::decode`,
//! `Any::decode` in yrs 0.28) step for step, so it reaches every value yrs
//! would decode. Where yrs would fail, so may the walk; the error is the
//! same kind either way: the bytes are not an update this server reads.
//!
//! One kind of content yrs decodes is refused outright: JSON content
//! (`ContentJSON`, content kind 2). Yjs does not write it (it writes values
//! as `ContentAny`), and yrs reads one string more than the count it is
//! given and writes as many as the count, so what it re-encodes does not
//! decode.
//! A pending item of that kind is merged into every state yrs encodes, and
//! the merge unwraps the failed decode: one such item would make every later
//! save and sync of its page panic.

use std::collections::HashSet;
use std::panic::AssertUnwindSafe;
use yrs::updates::decoder::Decode;
use yrs::{ReadTxn, StateVector, Update};

/// How deep a value inside an update may nest: arrays and maps inside one
/// another. A page holds flat values (an attribute is a number or a string;
/// a mark's value is one map of them), so this is far past anything a page
/// writes and far below what a thread's stack holds.
pub const MAX_VALUE_DEPTH: usize = 32;

/// A v1 update, decoded once its values are known to nest at most
/// [`MAX_VALUE_DEPTH`] deep and it holds no JSON content.
pub fn decode_update(bytes: &[u8]) -> anyhow::Result<Update> {
    walk(bytes)?;
    Ok(Update::decode_v1(bytes)?)
}

/// A document's state past `sv` as a v1 update, items yrs holds back for a
/// missing one included. yrs merges those items into the state it encodes
/// and unwraps the merge, so one it reads but cannot encode again panics
/// there, and on every later encode of the document: every save and every
/// sync of its page. [`decode_update`] refuses the one kind known to do
/// that; should another get in, the state leaves the held-back items out
/// (the client that sent them still has them, and sends them again when it
/// syncs) rather than take down whoever asked.
pub fn encode_state<T: ReadTxn>(txn: &T, sv: &StateVector) -> Vec<u8> {
    std::panic::catch_unwind(AssertUnwindSafe(|| txn.encode_state_as_update_v1(sv)))
        .unwrap_or_else(|_| txn.encode_diff_v1(sv))
}

/// The root types an update names as the parent of an item it carries, each
/// once, without decoding it. An item names its parent only when it has
/// neither neighbour to take the parent from: the first item written into a
/// root type, or under a key of a root map, names the root. Refused as
/// [`decode_update`] refuses.
pub fn named_roots(bytes: &[u8]) -> anyhow::Result<Vec<String>> {
    let roots = walk(bytes)?;
    Ok(roots
        .into_iter()
        .map(|r| String::from_utf8_lossy(r).into_owned())
        .collect())
}

fn walk(bytes: &[u8]) -> anyhow::Result<HashSet<&[u8]>> {
    let mut w = Walk::new(bytes);
    w.update()
        .map_err(|e| anyhow::anyhow!("not a v1 update this server reads: {e}"))?;
    Ok(w.roots)
}

const GC: u8 = 0;
const SKIP: u8 = 10;
const HAS_ORIGIN: u8 = 0b1000_0000;
const HAS_RIGHT_ORIGIN: u8 = 0b0100_0000;
const HAS_PARENT_SUB: u8 = 0b0010_0000;

struct Walk<'a> {
    bytes: &'a [u8],
    at: usize,
    /// Root types named as an item's parent.
    roots: HashSet<&'a [u8]>,
}

type Step<T = ()> = Result<T, String>;

impl<'a> Walk<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Walk {
            bytes,
            at: 0,
            roots: HashSet::new(),
        }
    }

    fn u8(&mut self) -> Step<u8> {
        let b = *self
            .bytes
            .get(self.at)
            .ok_or_else(|| format!("ends early, at byte {}", self.at))?;
        self.at += 1;
        Ok(b)
    }

    fn skip(&mut self, n: u64) -> Step {
        let end = usize::try_from(n)
            .ok()
            .and_then(|n| self.at.checked_add(n))
            .filter(|&end| end <= self.bytes.len())
            .ok_or_else(|| format!("a length at byte {} runs past the end", self.at))?;
        self.at = end;
        Ok(())
    }

    /// An unsigned varint, as yrs reads one into a `u64`.
    fn var_u64(&mut self) -> Step<u64> {
        let mut num = 0u64;
        let mut len = 0u32;
        loop {
            let b = self.u8()?;
            num |= u64::wrapping_shl(u64::from(b & 0x7f), len);
            len += 7;
            if b < 0x80 {
                return Ok(num);
            }
            if len > 70 {
                return Err(format!("a number at byte {} runs on", self.at));
            }
        }
    }

    /// An unsigned varint, as yrs reads one into a `u32` (wrapping, as it
    /// does).
    fn var_u32(&mut self) -> Step<u32> {
        let mut num = 0u32;
        let mut len = 0u32;
        loop {
            let b = self.u8()?;
            num |= u32::wrapping_shl(u32::from(b & 0x7f), len);
            len += 7;
            if b < 0x80 {
                return Ok(num);
            }
            if len > 70 {
                return Err(format!("a number at byte {} runs on", self.at));
            }
        }
    }

    /// A signed varint. yrs shifts each further byte into an `i64` and
    /// overflows the shift past ten bytes, so a longer one is refused.
    fn var_i64(&mut self) -> Step {
        let mut b = self.u8()?;
        let mut bytes = 1;
        while b >= 0x80 {
            if bytes == 10 {
                return Err(format!("a number at byte {} runs on", self.at));
            }
            b = self.u8()?;
            bytes += 1;
        }
        Ok(())
    }

    /// A length-prefixed byte string (`read_buf`, `read_string`).
    fn buf(&mut self) -> Step<&'a [u8]> {
        let len = self.var_u32()?;
        let start = self.at;
        self.skip(u64::from(len))?;
        Ok(&self.bytes[start..self.at])
    }

    fn id(&mut self) -> Step {
        self.var_u64()?;
        self.var_u32()?;
        Ok(())
    }

    /// The block section of an update, then its delete set. Returns where
    /// the update ends.
    fn update(&mut self) -> Step<usize> {
        let clients = self.var_u32()?;
        for _ in 0..clients {
            let blocks = self.var_u32()?;
            self.var_u64()?; // client
            self.var_u32()?; // clock
            for _ in 0..blocks {
                self.block()?;
            }
        }
        let clients = self.var_u32()?;
        for _ in 0..clients {
            self.var_u64()?; // client
            let ranges = self.var_u32()?;
            for _ in 0..ranges {
                self.var_u32()?; // clock
                self.var_u32()?; // length
            }
        }
        Ok(self.at)
    }

    fn block(&mut self) -> Step {
        let info = self.u8()?;
        if info == SKIP || info == GC {
            self.var_u32()?;
            return Ok(());
        }
        if info & HAS_ORIGIN != 0 {
            self.id()?;
        }
        if info & HAS_RIGHT_ORIGIN != 0 {
            self.id()?;
        }
        if info & (HAS_ORIGIN | HAS_RIGHT_ORIGIN) == 0 {
            if self.var_u32()? == 1 {
                let root = self.buf()?;
                self.roots.insert(root);
            } else {
                self.id()?; // the parent item
            }
            if info & HAS_PARENT_SUB != 0 {
                self.buf()?; // the key
            }
        }
        self.content(info & 0b1111)
    }

    fn content(&mut self, kind: u8) -> Step {
        match kind {
            // Deleted: a length.
            1 => {
                self.var_u32()?;
            }
            // JSON: refused (see the module's documentation).
            2 => return Err("JSON content, which Yjs does not write".into()),
            // Binary, string.
            3 | 4 => {
                self.buf()?;
            }
            // Embed: a JSON string.
            5 => self.json()?,
            // Format: a key, then its value as a JSON string.
            6 => {
                self.buf()?;
                self.json()?;
            }
            // A shared type: its kind, and an XML element's tag.
            7 => match self.u8()? {
                3 => {
                    self.buf()?;
                }
                0 | 1 | 2 | 4 | 5 | 6 | 9 | 15 => {}
                other => return Err(format!("unknown shared type {other}")),
            },
            // Values.
            8 => {
                let count = self.var_u32()?;
                for _ in 0..count {
                    self.value()?;
                }
            }
            // A subdocument: its guid, then its options as a value.
            9 => {
                self.buf()?;
                self.value()?;
            }
            other => return Err(format!("unknown content {other}")),
        }
        Ok(())
    }

    /// One value (`Any::decode`), nested ones included, without recursing.
    fn value(&mut self) -> Step {
        // For each array or map open around the next value: how many values
        // it still holds, and whether each comes after a key.
        let mut open: Vec<(u64, bool)> = vec![];
        loop {
            if let Some(&(_, true)) = open.last() {
                self.buf()?;
            }
            let opened = match self.u8()? {
                127 | 126 | 121 | 120 => None,
                125 => {
                    self.var_i64()?;
                    None
                }
                124 => {
                    self.skip(4)?;
                    None
                }
                123 | 122 => {
                    self.skip(8)?;
                    None
                }
                119 | 116 => {
                    self.buf()?;
                    None
                }
                118 => Some((self.var_u64()?, true)),
                117 => Some((self.var_u64()?, false)),
                other => return Err(format!("unknown value tag {other}")),
            };
            if let Some((len, keyed)) = opened {
                if open.len() + 1 > MAX_VALUE_DEPTH {
                    return Err(too_deep());
                }
                if len > 0 {
                    open.push((len, keyed));
                    continue;
                }
            }
            // A value ended: it may end the arrays and maps it closes.
            loop {
                match open.last_mut() {
                    None => return Ok(()),
                    Some((left, _)) => {
                        *left -= 1;
                        if *left > 0 {
                            break;
                        }
                        open.pop();
                    }
                }
            }
        }
    }

    /// A JSON string, which yrs parses into a value: refused when its arrays
    /// and objects nest past the limit.
    fn json(&mut self) -> Step {
        let text = self.buf()?;
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        for &b in text {
            if in_string {
                match (escaped, b) {
                    (true, _) => escaped = false,
                    (false, b'\\') => escaped = true,
                    (false, b'"') => in_string = false,
                    _ => {}
                }
                continue;
            }
            match b {
                b'"' => in_string = true,
                b'[' | b'{' => {
                    depth += 1;
                    if depth > MAX_VALUE_DEPTH {
                        return Err(too_deep());
                    }
                }
                b']' | b'}' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        Ok(())
    }
}

fn too_deep() -> String {
    format!("a value nests more than {MAX_VALUE_DEPTH} levels deep")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use yrs::{
        Any, Array, Doc, Map, ReadTxn, StateVector, Text, Transact, WriteTxn, Xml, XmlElementPrelim,
        XmlFragment, XmlTextPrelim,
    };

    /// Where the walk ends, for an update it reads.
    fn walked(bytes: &[u8]) -> usize {
        Walk::new(bytes).update().expect("the walk reads it")
    }

    fn nested(depth: usize) -> Any {
        let mut v = Any::from(1i64);
        for i in 0..depth {
            v = if i % 2 == 0 {
                Any::from(vec![v])
            } else {
                Any::from(HashMap::from([("k".to_string(), v)]))
            };
        }
        v
    }

    /// Every kind of content and value yrs writes, in one document.
    fn every_kind() -> Doc {
        let doc = Doc::new();
        {
            let mut txn = doc.transact_mut();
            let frag = txn.get_or_insert_xml_fragment("doc");
            let el = frag.push_back(&mut txn, XmlElementPrelim::empty("heading"));
            el.insert_attribute(&mut txn, "level", Any::from(2i64));
            el.insert_attribute(&mut txn, "nested", nested(MAX_VALUE_DEPTH - 1));
            let t = el.push_back(&mut txn, XmlTextPrelim::new("Hello, wörld 👋"));
            let bold: HashMap<String, Any> = HashMap::from([("href".into(), Any::from("/x"))]);
            t.format(
                &mut txn,
                0,
                5,
                yrs::types::Attrs::from([(std::sync::Arc::from("link"), Any::from(bold))]),
            );
            t.remove_range(&mut txn, 1, 2);
            let map = txn.get_or_insert_map("meta");
            map.insert(&mut txn, "contract", Any::from(1i64));
            map.insert(&mut txn, "f32", Any::from(1.5f64));
            map.insert(&mut txn, "f64", Any::from(0.1f64));
            // Past a double's 53 bits: written as a 64-bit integer.
            map.insert(&mut txn, "big", Any::from(9_007_199_254_740_993i64));
            map.insert(&mut txn, "neg", Any::from(-123_456_789i64));
            map.insert(&mut txn, "null", Any::Null);
            map.insert(&mut txn, "undefined", Any::Undefined);
            map.insert(&mut txn, "yes", Any::from(true));
            map.insert(&mut txn, "no", Any::from(false));
            map.insert(&mut txn, "bytes", Any::from(vec![1u8, 2, 3]));
            map.insert(&mut txn, "empty", Any::from(Vec::<Any>::new()));
            map.insert(&mut txn, "sub", yrs::Doc::new());
            let arr = txn.get_or_insert_array("list");
            arr.push_back(&mut txn, Any::from("a"));
            arr.push_back(&mut txn, Any::from(HashMap::<String, Any>::new()));
            let text = txn.get_or_insert_text("content");
            text.insert(&mut txn, 0, "# notes");
            text.insert_embed(&mut txn, 2, Any::from(HashMap::from([("img".to_string(), Any::from("x"))])));
        }
        doc
    }

    #[test]
    fn the_walk_reads_every_update_to_its_end() {
        let doc = every_kind();
        let full = doc.transact().encode_state_as_update_v1(&StateVector::default());
        assert_eq!(walked(&full), full.len());
        assert!(decode_update(&full).is_ok());
        // An update after edits, carrying origins and deletions.
        let before = doc.transact().state_vector();
        {
            let mut txn = doc.transact_mut();
            let text = txn.get_or_insert_text("content");
            text.insert(&mut txn, 3, "more ");
            text.remove_range(&mut txn, 0, 1);
            let map = txn.get_or_insert_map("meta");
            map.insert(&mut txn, "contract", Any::from(2i64));
        }
        let diff = doc.transact().encode_diff_v1(&before);
        assert_eq!(walked(&diff), diff.len());
        let empty = Doc::new().transact().encode_state_as_update_v1(&StateVector::default());
        assert_eq!(walked(&empty), empty.len());
    }

    #[test]
    fn values_past_the_limit_are_refused_before_yrs_decodes_them() {
        let at_limit = Doc::new();
        let past = Doc::new();
        for (doc, depth) in [(&at_limit, MAX_VALUE_DEPTH), (&past, MAX_VALUE_DEPTH + 1)] {
            let mut txn = doc.transact_mut();
            let map = txn.get_or_insert_map("meta");
            map.insert(&mut txn, "v", nested(depth));
        }
        let ok = at_limit.transact().encode_state_as_update_v1(&StateVector::default());
        assert!(decode_update(&ok).is_ok());
        let deep = past.transact().encode_state_as_update_v1(&StateVector::default());
        let err = decode_update(&deep).unwrap_err().to_string();
        assert!(err.contains("nests more than"), "{err}");
    }

    /// lib0's unsigned varint.
    fn var(out: &mut Vec<u8>, mut n: u64) {
        while n >= 0x80 {
            out.push((n as u8 & 0x7f) | 0x80);
            n >>= 7;
        }
        out.push(n as u8);
    }

    fn string(out: &mut Vec<u8>, s: &str) {
        var(out, s.len() as u64);
        out.extend_from_slice(s.as_bytes());
    }

    /// One item of JSON content holding one string, written as yrs reads it
    /// (a count of 0, then one string). `pending`: placed after an item
    /// (client 9, clock 5) no document has, so yrs holds it back; otherwise
    /// straight under the root `content`.
    fn json_item(pending: bool) -> Vec<u8> {
        let mut u = vec![];
        var(&mut u, 1); // one client
        var(&mut u, 1); // one item
        var(&mut u, 7); // its client
        var(&mut u, 0); // its clock
        if pending {
            u.push(HAS_ORIGIN | 2);
            var(&mut u, 9);
            var(&mut u, 5);
        } else {
            u.push(2);
            var(&mut u, 1); // under a root, by name
            string(&mut u, "content");
        }
        var(&mut u, 0);
        string(&mut u, "1");
        var(&mut u, 0); // no deletions
        u
    }

    /// yrs would decode either update, and a document holding the item could
    /// not be saved again: a pending one makes every state encode panic,
    /// one in place makes a state that does not decode.
    #[test]
    fn json_content_is_refused() {
        for pending in [true, false] {
            let update = json_item(pending);
            assert!(Update::decode_v1(&update).is_ok(), "yrs reads it");
            let err = decode_update(&update).unwrap_err().to_string();
            assert!(err.contains("JSON content"), "{err}");
            assert!(named_roots(&update).is_err());
        }
    }

    /// A document already holding back an item yrs cannot encode again
    /// still encodes, without it.
    #[test]
    fn a_state_encodes_whatever_is_held_back() {
        let doc = Doc::new();
        {
            let mut txn = doc.transact_mut();
            let t = txn.get_or_insert_text("content");
            t.insert(&mut txn, 0, "Some notes.");
        }
        doc.transact_mut()
            .apply_update(Update::decode_v1(&json_item(true)).unwrap())
            .unwrap();
        let state = encode_state(&doc.transact(), &StateVector::default());
        let back = Doc::new();
        back.transact_mut()
            .apply_update(decode_update(&state).unwrap())
            .unwrap();
        let txn = back.transact();
        let text = txn.get_text("content").unwrap();
        assert_eq!(yrs::GetString::get_string(&text, &txn), "Some notes.");
    }

    #[test]
    fn an_update_names_the_roots_its_first_items_go_into() {
        let doc = every_kind();
        let full = doc.transact().encode_state_as_update_v1(&StateVector::default());
        let mut roots = named_roots(&full).unwrap();
        roots.sort();
        assert_eq!(roots, ["content", "doc", "list", "meta"]);
        // Edits after an item already there name no root: an entry written
        // over another takes its parent from the one before it.
        let before = doc.transact().state_vector();
        {
            let mut txn = doc.transact_mut();
            let text = txn.get_or_insert_text("content");
            text.insert(&mut txn, 3, "more ");
            let map = txn.get_or_insert_map("meta");
            map.insert(&mut txn, "contract", Any::from(2i64));
        }
        let diff = doc.transact().encode_diff_v1(&before);
        assert_eq!(named_roots(&diff).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn truncated_and_unknown_bytes_are_errors_not_panics() {
        let doc = every_kind();
        let full = doc.transact().encode_state_as_update_v1(&StateVector::default());
        for end in 0..full.len() {
            let _ = decode_update(&full[..end]);
        }
        assert!(decode_update(&[1, 1, 5, 0, 0x08, 1, 1, 0x63]).is_err());
        assert!(decode_update(&[0xff; 20]).is_err());
    }
}
