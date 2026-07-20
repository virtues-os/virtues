//! Paragraph-aware chunking (researcher-plan D1, locked decision 7).
//!
//! ~400–600 tokens per chunk approximated as a word budget (~1.3 words/token →
//! 300–450 words), 10–15% overlap, chunks may cross pages. Every chunk anchors:
//! - `page_num`: the page where the chunk STARTS (None for unpaged formats)
//! - `char_start/char_end`: offsets into the extractor's canonical full text
//!   (bookkeeping only — viewer landing is quote-based, never offset-based)
//! - `quote_head`: leading snippet for self-contained citation links.

use super::ExtractedPage;

/// Target chunk size in words (≈ 400–600 tokens).
const TARGET_WORDS: usize = 380;
/// Hard maximum before a paragraph is force-split.
const MAX_WORDS: usize = 550;
/// Overlap carried from the previous chunk, in words (~12%).
const OVERLAP_WORDS: usize = 45;
/// Words in the quote_head snippet used for `?q=` citation landing.
const QUOTE_HEAD_WORDS: usize = 8;

#[derive(Debug, Clone)]
pub struct Chunk {
    pub page_num: Option<i32>,
    pub char_start: usize,
    pub char_end: usize,
    pub quote_head: String,
    pub text: String,
}

/// A paragraph with provenance into the canonical text.
struct Para {
    page_num: Option<i32>,
    char_start: usize,
    char_end: usize,
    text: String,
}

/// Split extracted pages into anchored chunks.
///
/// The canonical text is the concatenation of page texts joined by "\n\n";
/// char offsets index into that string (byte offsets on UTF-8 boundaries).
pub fn chunk_pages(pages: &[ExtractedPage]) -> Vec<Chunk> {
    // 1. Collect paragraphs with canonical offsets.
    let mut paras: Vec<Para> = Vec::new();
    let mut offset = 0usize;
    for (i, page) in pages.iter().enumerate() {
        if i > 0 {
            offset += 2; // the "\n\n" page joiner in canonical text
        }
        let page_base = offset;
        let mut local = 0usize;
        for raw in page.text.split("\n\n") {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                // Locate trimmed slice within raw for exact offsets.
                let lead = raw.len() - raw.trim_start().len();
                let start = page_base + local + lead;
                paras.push(Para {
                    page_num: page.page_num,
                    char_start: start,
                    char_end: start + trimmed.len(),
                    text: trimmed.to_string(),
                });
            }
            local += raw.len() + 2; // the "\n\n" split separator
        }
        offset += page.text.len();
    }

    // 2. Pack paragraphs into chunks with a word budget; force-split giants.
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut cur: Vec<&Para> = Vec::new();
    let mut cur_words = 0usize;

    let flush = |cur: &mut Vec<&Para>, cur_words: &mut usize, chunks: &mut Vec<Chunk>| {
        if cur.is_empty() {
            return;
        }
        let first = cur.first().unwrap();
        let last = cur.last().unwrap();
        let text = cur
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        chunks.push(Chunk {
            page_num: first.page_num,
            char_start: first.char_start,
            char_end: last.char_end,
            quote_head: quote_head(&text),
            text,
        });
        cur.clear();
        *cur_words = 0;
    };

    let mut i = 0usize;
    while i < paras.len() {
        let para = &paras[i];
        let words = para.text.split_whitespace().count();

        if words > MAX_WORDS {
            // Giant paragraph: flush current, then split it by word windows.
            flush(&mut cur, &mut cur_words, &mut chunks);
            for piece in split_giant(para) {
                chunks.push(piece);
            }
            i += 1;
            continue;
        }

        if cur_words + words > MAX_WORDS && cur_words >= TARGET_WORDS / 2 {
            flush(&mut cur, &mut cur_words, &mut chunks);
            // Overlap: re-open with the tail paragraph of the previous chunk
            // when it's small enough to serve as context.
            if let Some(prev) = paras[..i].last() {
                let prev_words = prev.text.split_whitespace().count();
                if prev_words <= OVERLAP_WORDS * 2 {
                    cur.push(prev);
                    cur_words = prev_words;
                }
            }
        }

        cur.push(para);
        cur_words += words;
        if cur_words >= TARGET_WORDS {
            flush(&mut cur, &mut cur_words, &mut chunks);
        }
        i += 1;
    }
    flush(&mut cur, &mut cur_words, &mut chunks);

    chunks
}

/// Word-window split for a paragraph beyond MAX_WORDS (dense OCR-less PDFs
/// sometimes yield page-sized paragraphs). Offsets stay within the paragraph.
fn split_giant(para: &Para) -> Vec<Chunk> {
    let words: Vec<(usize, &str)> = para
        .text
        .split_whitespace()
        .map(|w| {
            let off = w.as_ptr() as usize - para.text.as_ptr() as usize;
            (off, w)
        })
        .collect();
    let mut out = Vec::new();
    let step = TARGET_WORDS.saturating_sub(OVERLAP_WORDS).max(1);
    let mut start_w = 0usize;
    while start_w < words.len() {
        let end_w = (start_w + TARGET_WORDS).min(words.len());
        let (first_off, _) = words[start_w];
        let (last_off, last_word) = words[end_w - 1];
        let slice = &para.text[first_off..last_off + last_word.len()];
        out.push(Chunk {
            page_num: para.page_num,
            char_start: para.char_start + first_off,
            char_end: para.char_start + last_off + last_word.len(),
            quote_head: quote_head(slice),
            text: slice.to_string(),
        });
        if end_w == words.len() {
            break;
        }
        start_w += step;
    }
    out
}

/// Leading snippet for `?q=` links: first N words, whitespace-normalized.
fn quote_head(text: &str) -> String {
    text.split_whitespace()
        .take(QUOTE_HEAD_WORDS)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(n: i32, text: &str) -> ExtractedPage {
        ExtractedPage {
            page_num: Some(n),
            text: text.to_string(),
        }
    }

    #[test]
    fn chunks_carry_page_and_offsets() {
        let long_para = "word ".repeat(200);
        let pages = vec![page(1, &long_para), page(2, &long_para)];
        let chunks = chunk_pages(&pages);
        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].page_num, Some(1));
        assert_eq!(chunks[0].char_start, 0);
        assert!(chunks.iter().all(|c| c.char_end > c.char_start));
        assert!(chunks.iter().all(|c| !c.quote_head.is_empty()));
    }

    #[test]
    fn giant_paragraph_is_window_split() {
        let giant = "term ".repeat(2000);
        let pages = vec![page(1, &giant)];
        let chunks = chunk_pages(&pages);
        assert!(chunks.len() > 2);
        for c in &chunks {
            assert!(c.text.split_whitespace().count() <= TARGET_WORDS);
        }
    }

    #[test]
    fn cross_page_chunk_anchors_to_starting_page() {
        // Small paragraphs across pages pack into one chunk anchored to page 1.
        let pages = vec![page(1, "alpha beta gamma."), page(2, "delta epsilon zeta.")];
        let chunks = chunk_pages(&pages);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].page_num, Some(1));
        assert!(chunks[0].text.contains("delta"));
    }

    #[test]
    fn empty_pages_produce_no_chunks() {
        let pages = vec![page(1, "   \n\n  ")];
        assert!(chunk_pages(&pages).is_empty());
    }
}
