//! HTML text extraction: tag-strip with block-level paragraph boundaries.
//! Deliberately simple — snapshots/readability are a separate future lane;
//! this handles .html files a user drops into Drive.

use quick_xml::events::Event;
use quick_xml::Reader;

use super::{Extraction, ExtractedPage, TextExtractor};
use crate::error::Result;

pub struct HtmlExtractor;

/// Elements whose content is never text.
const SKIP: &[&[u8]] = &[b"script", b"style", b"noscript", b"head", b"template"];
/// Elements that end a paragraph.
const BLOCK: &[&[u8]] = &[
    b"p", b"div", b"li", b"h1", b"h2", b"h3", b"h4", b"h5", b"h6", b"tr", b"section", b"article",
    b"blockquote", b"pre", b"br",
];

impl TextExtractor for HtmlExtractor {
    fn extract(&self, bytes: &[u8]) -> Result<Extraction> {
        let html = String::from_utf8_lossy(bytes);
        let mut reader = Reader::from_str(&html);
        reader.config_mut().check_end_names = false;

        let mut out = String::new();
        let mut skip_depth = 0usize;
        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let name = e.name().local_name().as_ref().to_ascii_lowercase();
                    if SKIP.contains(&name.as_slice()) {
                        skip_depth += 1;
                    }
                }
                Ok(Event::End(e)) => {
                    let name = e.name().local_name().as_ref().to_ascii_lowercase();
                    if SKIP.contains(&name.as_slice()) {
                        skip_depth = skip_depth.saturating_sub(1);
                    } else if BLOCK.contains(&name.as_slice()) {
                        out.push_str("\n\n");
                    }
                }
                Ok(Event::Empty(e)) => {
                    let name = e.name().local_name().as_ref().to_ascii_lowercase();
                    if BLOCK.contains(&name.as_slice()) {
                        out.push_str("\n\n");
                    }
                }
                Ok(Event::Text(t)) if skip_depth == 0 => {
                    if let Ok(text) = t.unescape() {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            if !out.is_empty() && !out.ends_with('\n') && !out.ends_with(' ') {
                                out.push(' ');
                            }
                            out.push_str(trimmed);
                        }
                    }
                }
                Ok(Event::Eof) => break,
                // Real-world HTML is sloppy; quick-xml errors on malformed
                // markup end the scan with whatever we got.
                Err(_) => break,
                _ => {}
            }
            buf.clear();
        }

        if out.trim().is_empty() {
            return Ok(Extraction::NoText);
        }
        Ok(Extraction::Pages(vec![ExtractedPage {
            page_num: None,
            text: out,
        }]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tags_and_scripts() {
        let html = b"<html><head><script>var x=1;</script></head><body><p>Real text.</p><p>More text.</p></body></html>";
        match HtmlExtractor.extract(html).unwrap() {
            Extraction::Pages(p) => {
                assert!(p[0].text.contains("Real text."));
                assert!(p[0].text.contains("More text."));
                assert!(!p[0].text.contains("var x"));
            }
            _ => panic!("expected pages"),
        }
    }
}
