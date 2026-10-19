//! DOCX text extraction: a .docx is a zip; body text lives in
//! `word/document.xml` as `<w:t>` runs, with `<w:p>` paragraph boundaries.

use std::io::{Cursor, Read};

use quick_xml::events::Event;
use quick_xml::Reader;

use super::{Extraction, ExtractedPage, TextExtractor};
use crate::error::{Error, Result};

pub struct DocxExtractor;

impl TextExtractor for DocxExtractor {
    fn extract(&self, bytes: &[u8]) -> Result<Extraction> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|e| Error::Other(format!("docx open: {e}")))?;
        let mut xml = String::new();
        archive
            .by_name("word/document.xml")
            .map_err(|e| Error::Other(format!("docx document.xml: {e}")))?
            .read_to_string(&mut xml)
            .map_err(|e| Error::Other(format!("docx read: {e}")))?;

        let mut reader = Reader::from_str(&xml);
        let mut out = String::new();
        let mut buf = Vec::new();
        let mut in_text = false;
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let name = e.name();
                    let local = name.local_name();
                    match local.as_ref() {
                        b"t" => in_text = true,
                        _ => {}
                    }
                }
                Ok(Event::End(e)) => {
                    let name = e.name();
                    let local = name.local_name();
                    match local.as_ref() {
                        b"t" => in_text = false,
                        // Paragraph boundary → blank line (chunker's paragraph split).
                        b"p" => out.push_str("\n\n"),
                        _ => {}
                    }
                }
                // Tabs and explicit breaks inside runs.
                Ok(Event::Empty(e)) => {
                    let name = e.name();
                    match name.local_name().as_ref() {
                        b"tab" => out.push('\t'),
                        b"br" => out.push('\n'),
                        _ => {}
                    }
                }
                Ok(Event::Text(t)) if in_text => {
                    out.push_str(
                        &t.unescape()
                            .map_err(|e| Error::Other(format!("docx text: {e}")))?,
                    );
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(Error::Other(format!("docx xml: {e}"))),
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
    use std::io::Write;

    #[test]
    fn extracts_docx_paragraph_text() {
        // Build a minimal docx in memory.
        let doc_xml = r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>Hello researcher</w:t></w:r></w:p>
    <w:p><w:r><w:t>Second paragraph</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
        let mut zip_bytes = Vec::new();
        {
            let mut zw = zip::ZipWriter::new(Cursor::new(&mut zip_bytes));
            zw.start_file::<_, ()>("word/document.xml", Default::default())
                .unwrap();
            zw.write_all(doc_xml.as_bytes()).unwrap();
            zw.finish().unwrap();
        }
        let out = DocxExtractor.extract(&zip_bytes).unwrap();
        match out {
            Extraction::Pages(pages) => {
                assert_eq!(pages.len(), 1);
                assert!(pages[0].text.contains("Hello researcher"));
                assert!(pages[0].text.contains("\n\n"));
            }
            _ => panic!("expected pages"),
        }
    }
}
