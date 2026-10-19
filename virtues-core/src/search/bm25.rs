//! Shared BM25 helpers.
//!
//! The lexical retrieval arm is real BM25 (not Postgres `ts_rank`, which has no
//! IDF): term frequencies live in `search_bm25_postings`, document lengths in
//! `search_embeddings.bm25_len`, and the corpus stats (N, Σlen) in the
//! single-row `search_index_meta`. Document-frequency is derived inline per
//! query — no stale global df table.
//!
//! This module owns the one thing that MUST be byte-identical on both sides of
//! the index: tokenization. [`indexer`](super::indexer) tokenizes chunks at
//! ingest; [`query`](super::query) tokenizes the query the same way, or their
//! terms won't line up.

/// Tokenize into BM25 terms: maximal runs of ASCII alphanumerics, lowercased.
/// Mirrors the reference `re.findall(r"[a-z0-9]+", text.lower())` the on-device
/// retrieval was measured with. Non-ASCII letters act as separators (they fall
/// outside `[a-z0-9]` after lowercasing), matching that reference for the
/// personal-log common case (ASCII + accented Latin); exotic Unicode casing
/// (e.g. Turkish İ) can diverge, which is immaterial here.
pub fn tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            cur.push(ch.to_ascii_lowercase());
        } else if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// BM25 saturation + length-normalization parameters — the Lucene/BM25+ variant
/// (`k1=1.5`, `b=0.75`) the on-device SQL used. Kept here so the ingest side and
/// the scoring SQL in `query.rs` can't drift.
pub const K1: f64 = 1.5;
pub const B: f64 = 0.75;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_like_the_reference() {
        assert_eq!(tokens("Invoice #48213 — Zürich trip!"), ["invoice", "48213", "z", "rich", "trip"]);
        assert_eq!(tokens("  MixedCASE and_underscores 007 "), ["mixedcase", "and", "underscores", "007"]);
        assert!(tokens("—/—").is_empty());
    }
}
