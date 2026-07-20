//! PDF text extraction via pdfium (BSD-licensed; MuPDF rejected — AGPL).
//!
//! pdfium-render binds libpdfium dynamically at runtime — no compile-time
//! link. The library is looked up from `VIRTUES_PDFIUM_PATH`, then next to the
//! executable, then system defaults. A missing library fails extraction with a
//! clear message (files stay `failed`, retryable after install) rather than
//! failing the build or the daemon.

use pdfium_render::prelude::*;

use super::{Extraction, ExtractedPage, TextExtractor};
use crate::error::{Error, Result};

pub struct PdfExtractor {
    pdfium: Pdfium,
}

impl PdfExtractor {
    /// Bind pdfium. Not cached globally: `Pdfium` is not Sync; extraction runs
    /// inside `spawn_blocking`, and binding is cheap relative to parsing.
    pub fn shared() -> Result<Self> {
        let bindings = Self::bind()?;
        Ok(Self {
            pdfium: Pdfium::new(bindings),
        })
    }

    fn bind() -> Result<Bindings> {
        // 1. Explicit override.
        if let Ok(path) = std::env::var("VIRTUES_PDFIUM_PATH") {
            return Pdfium::bind_to_library(&path)
                .map_err(|e| Error::Other(format!("pdfium at VIRTUES_PDFIUM_PATH: {e}")));
        }
        // 2. Next to the executable (appliance layout), then system paths.
        Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path("./"))
            .or_else(|_| Pdfium::bind_to_system_library())
            .map_err(|e| {
                Error::Other(format!(
                    "libpdfium not found (set VIRTUES_PDFIUM_PATH or install pdfium): {e}"
                ))
            })
    }
}

type Bindings = Box<dyn PdfiumLibraryBindings>;

impl TextExtractor for PdfExtractor {
    fn extract(&self, bytes: &[u8]) -> Result<Extraction> {
        let doc = self
            .pdfium
            .load_pdf_from_byte_slice(bytes, None)
            .map_err(|e| Error::Other(format!("pdf parse: {e}")))?;

        let mut pages_out = Vec::new();
        let mut total_len = 0usize;
        for (i, page) in doc.pages().iter().enumerate() {
            let text = page
                .text()
                .map(|t| t.all())
                .unwrap_or_default();
            total_len += text.trim().len();
            pages_out.push(ExtractedPage {
                page_num: Some(i as i32 + 1),
                text,
            });
        }

        // Scanned/no-text detection: a document whose pages carry (almost) no
        // text is image-only — the D5 OCR queue, not an error.
        if total_len < 32 {
            return Ok(Extraction::NoText);
        }
        Ok(Extraction::Pages(pages_out))
    }
}
