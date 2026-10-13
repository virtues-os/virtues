//! The page converter as a WebAssembly module: markdown in, the blocks the
//! editor inserts out, as `virtues_document::convert_for_editor_json`
//! writes them. The editor calls it inside a paste, a drop and the inline
//! writer's end (`apps/web/src/lib/document/converter.ts`), so what was
//! pasted is in the page when the paste event returns.
//!
//! A raw ABI, no wasm-bindgen: the module imports nothing, the page builds a
//! fresh instance whenever it likes (after a trap, or to give back memory a
//! large conversion grew), and no glue has to match a CLI's version. Three
//! exports, every address and length a 32-bit number:
//!
//! - `buffer_alloc(len) -> ptr`: room for `len` bytes, for the markdown;
//! - `convert(ptr, len, mode) -> u64`: the UTF-8 markdown at `ptr`, read as
//!   a writer's when `mode` is 0, a paste when 1, and a writer's into an
//!   article when 2 (`virtues_document::EditorMode`); the answer's address
//!   in the high 32 bits and its length in the low 32, JSON in UTF-8;
//! - `buffer_free(ptr, len)`: give back the markdown's room and the answer.
//!
//! `editor_max_bytes()` says the most markdown one conversion takes, so the
//! page need not copy more in to be told it is too much.

use virtues_document::EditorMode;

/// The answer to `markdown` (UTF-8 bytes) read in `mode`
/// ([`EditorMode`]'s number), as JSON bytes. Bytes that are not UTF-8 never
/// come from the page's `TextEncoder`, and a mode the page never passes is
/// no mode; both are refused all the same.
pub fn answer(markdown: &[u8], mode: u32) -> Vec<u8> {
    let Some(mode) = EditorMode::from_number(mode) else {
        return br#"{"refused":"problems","problems":[{"at":"mode","message":"no such mode"}]}"#.to_vec();
    };
    match std::str::from_utf8(markdown) {
        Ok(md) => virtues_document::convert_for_editor_json(md, mode).into_bytes(),
        Err(_) => br#"{"refused":"problems","problems":[{"at":"markdown","message":"the markdown is not UTF-8"}]}"#
            .to_vec(),
    }
}

#[cfg(target_arch = "wasm32")]
mod abi {
    /// Room for `len` bytes, freed with [`buffer_free`].
    #[no_mangle]
    pub extern "C" fn buffer_alloc(len: usize) -> *mut u8 {
        let mut room = std::mem::ManuallyDrop::new(Vec::<u8>::with_capacity(len));
        room.as_mut_ptr()
    }

    /// Give back `len` bytes at `ptr`.
    ///
    /// # Safety
    ///
    /// `ptr` and `len` are a buffer [`buffer_alloc`] or [`convert`] handed
    /// out, freed once.
    #[no_mangle]
    pub unsafe extern "C" fn buffer_free(ptr: *mut u8, len: usize) {
        drop(Vec::from_raw_parts(ptr, 0, len));
    }

    /// Convert the markdown at `ptr`; see the crate's documentation.
    ///
    /// # Safety
    ///
    /// `ptr` and `len` are a buffer [`buffer_alloc`] handed out, holding
    /// `len` bytes the page wrote.
    #[no_mangle]
    pub unsafe extern "C" fn convert(ptr: *const u8, len: usize, mode: u32) -> u64 {
        let markdown = std::slice::from_raw_parts(ptr, len);
        // A boxed slice's room is its length, which `buffer_free` is given.
        let out = std::mem::ManuallyDrop::new(super::answer(markdown, mode).into_boxed_slice());
        ((out.as_ptr() as u64) << 32) | out.len() as u64
    }

    /// The most markdown one conversion takes.
    #[no_mangle]
    pub extern "C" fn editor_max_bytes() -> usize {
        virtues_document::EDITOR_MAX_BYTES
    }
}

#[cfg(test)]
mod tests {
    use super::answer;
    use virtues_document::EditorMode;

    #[test]
    fn answers_as_the_crate_does() {
        let md = "## Plan\n\n- [ ] Book seats\n";
        assert_eq!(
            String::from_utf8(answer(md.as_bytes(), 1)).unwrap(),
            virtues_document::convert_for_editor_json(md, EditorMode::Paste)
        );
        let refused = String::from_utf8(answer(&[0xff, 0xfe], 1)).unwrap();
        assert!(refused.starts_with(r#"{"refused":"problems""#), "{refused}");
        let refused = String::from_utf8(answer(md.as_bytes(), 3)).unwrap();
        assert!(refused.starts_with(r#"{"refused":"problems""#), "{refused}");
    }

    /// Each mode is the crate's: 0 a writer's, 1 a paste, 2 a writer's into
    /// an article, whose footnotes are the article's own.
    #[test]
    fn a_mode_is_a_number() {
        let md = "Lunch at noon.[^ev-1]\n\n[^ev-1]: Message · 12:10 PM\n";
        for (n, mode) in [(0, EditorMode::Written), (1, EditorMode::Paste), (2, EditorMode::Article)] {
            assert_eq!(
                String::from_utf8(answer(md.as_bytes(), n)).unwrap(),
                virtues_document::convert_for_editor_json(md, mode)
            );
        }
        assert!(String::from_utf8(answer(md.as_bytes(), 2)).unwrap().contains("virtues-footnote"));
        assert!(!String::from_utf8(answer(md.as_bytes(), 0)).unwrap().contains("virtues-footnote"));
    }
}
