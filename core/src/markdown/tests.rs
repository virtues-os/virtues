//! Tests for markdown <-> XmlFragment conversion

use yrs::{Doc, Transact, WriteTxn, Xml, XmlFragment, XmlNode};

use super::{markdown_to_xml_fragment, xml_fragment_to_markdown};

// =============================================================================
// Basic roundtrip tests
// =============================================================================

#[test]
fn test_paragraph() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    markdown_to_xml_fragment(&mut txn, fragment.clone(), "Hello world").unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);
    assert!(output.contains("Hello world"));
}

#[test]
fn test_heading() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    markdown_to_xml_fragment(&mut txn, fragment.clone(), "# Hello\n\n## World").unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);
    assert!(output.contains("# Hello"));
    assert!(output.contains("## World"));
}

#[test]
fn test_code_block() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    let markdown = r#"```rust
fn main() {
    println!("Hello");
}
```"#;

    markdown_to_xml_fragment(&mut txn, fragment.clone(), markdown).unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);
    assert!(output.contains("```rust"));
    assert!(output.contains("println!"));

    // Roundtrip stability: a second pass should produce identical output
    let doc2 = Doc::new();
    let mut txn2 = doc2.transact_mut();
    let fragment2 = txn2.get_or_insert_xml_fragment("content");
    markdown_to_xml_fragment(&mut txn2, fragment2.clone(), &output).unwrap();
    let output2 = xml_fragment_to_markdown(&txn2, fragment2);
    assert_eq!(output, output2, "Code block roundtrip should be stable");
}

#[test]
fn test_bullet_list() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    let markdown = "- Item 1\n- Item 2\n- Item 3";

    markdown_to_xml_fragment(&mut txn, fragment.clone(), markdown).unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);
    assert!(output.contains("Item 1"));
    assert!(output.contains("Item 2"));
    assert!(output.contains("Item 3"));
}

#[test]
fn test_ordered_list() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    let markdown = "1. First\n2. Second\n3. Third";

    markdown_to_xml_fragment(&mut txn, fragment.clone(), markdown).unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);
    assert!(output.contains("First"));
    assert!(output.contains("Second"));
    assert!(output.contains("Third"));
}

#[test]
fn test_blockquote() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    let markdown = "> This is a quote\n> With multiple lines";

    markdown_to_xml_fragment(&mut txn, fragment.clone(), markdown).unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);
    assert!(output.contains("> "));
    assert!(output.contains("This is a quote"));
}

#[test]
fn test_task_list() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    let markdown = "- [ ] Todo item\n- [x] Done item";

    markdown_to_xml_fragment(&mut txn, fragment.clone(), markdown).unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);
    assert!(output.contains("Todo item"));
    assert!(output.contains("Done item"));
}

#[test]
fn test_horizontal_rule() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    let markdown = "Before\n\n---\n\nAfter";

    markdown_to_xml_fragment(&mut txn, fragment.clone(), markdown).unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);
    assert!(output.contains("---"));
}

#[test]
fn test_roundtrip_simple() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    let original = "# Title\n\nSome paragraph text.\n\n- Item one\n- Item two";

    markdown_to_xml_fragment(&mut txn, fragment.clone(), original).unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);

    assert!(output.contains("# Title"));
    assert!(output.contains("Some paragraph text"));
    assert!(output.contains("Item one"));
    assert!(output.contains("Item two"));
}

// =============================================================================
// Entity link roundtrip tests
// =============================================================================

#[test]
fn test_entity_link_person() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    markdown_to_xml_fragment(&mut txn, fragment.clone(), "Hello [John](/person/123) world").unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);
    assert!(output.contains("[John](/person/123)"), "Entity link should roundtrip. Got: {}", output);
}

#[test]
fn test_entity_link_page() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    markdown_to_xml_fragment(&mut txn, fragment.clone(), "See [My Notes](/page/abc-def)").unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);
    assert!(output.contains("[My Notes](/page/abc-def)"), "Page link should roundtrip. Got: {}", output);
}

#[test]
fn test_entity_link_all_prefixes() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    let markdown = "\
[Person](/person/1)\n\n\
[Place](/place/2)\n\n\
[Org](/org/3)\n\n\
[Page](/page/4)\n\n\
[Day](/day/2024-01-01)\n\n\
[Year](/year/2024)\n\n\
[Source](/source/5)\n\n\
[Chat](/chat/6)";

    markdown_to_xml_fragment(&mut txn, fragment.clone(), markdown).unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);
    assert!(output.contains("[Person](/person/1)"), "Got: {}", output);
    assert!(output.contains("[Place](/place/2)"), "Got: {}", output);
    assert!(output.contains("[Org](/org/3)"), "Got: {}", output);
    assert!(output.contains("[Page](/page/4)"), "Got: {}", output);
    assert!(output.contains("[Day](/day/2024-01-01)"), "Got: {}", output);
    assert!(output.contains("[Year](/year/2024)"), "Got: {}", output);
    assert!(output.contains("[Source](/source/5)"), "Got: {}", output);
    assert!(output.contains("[Chat](/chat/6)"), "Got: {}", output);
}

#[test]
fn test_entity_link_creates_element_not_mark() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    markdown_to_xml_fragment(&mut txn, fragment.clone(), "Hello [John](/person/123) world").unwrap();

    // Walk the fragment to find the entity_link element
    let mut found_entity_link = false;
    if let Some(XmlNode::Element(para)) = fragment.get(&txn, 0) {
        for i in 0..para.len(&txn) {
            if let Some(XmlNode::Element(el)) = para.get(&txn, i) {
                if el.tag().as_ref() == "entity_link" {
                    found_entity_link = true;
                    assert_eq!(el.get_attribute(&txn, "href").unwrap(), "/person/123");
                    assert_eq!(el.get_attribute(&txn, "label").unwrap(), "John");
                }
            }
        }
    }
    assert!(found_entity_link, "Should create entity_link XmlElement, not a link mark");
}

// =============================================================================
// File card roundtrip tests
// =============================================================================

#[test]
fn test_file_card_roundtrip() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    markdown_to_xml_fragment(&mut txn, fragment.clone(), "See [report.pdf](/drive/abc-123)").unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);
    assert!(output.contains("[report.pdf](/drive/abc-123)"), "File card should roundtrip. Got: {}", output);
}

#[test]
fn test_file_card_creates_element() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    markdown_to_xml_fragment(&mut txn, fragment.clone(), "[doc.pdf](/drive/xyz)").unwrap();

    let mut found_file_card = false;
    if let Some(XmlNode::Element(para)) = fragment.get(&txn, 0) {
        for i in 0..para.len(&txn) {
            if let Some(XmlNode::Element(el)) = para.get(&txn, i) {
                if el.tag().as_ref() == "file_card" {
                    found_file_card = true;
                    assert_eq!(el.get_attribute(&txn, "href").unwrap(), "/drive/xyz");
                    assert_eq!(el.get_attribute(&txn, "name").unwrap(), "doc.pdf");
                }
            }
        }
    }
    assert!(found_file_card, "Should create file_card XmlElement");
}

// =============================================================================
// Media node roundtrip tests
// =============================================================================

#[test]
fn test_media_image_roundtrip() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    markdown_to_xml_fragment(&mut txn, fragment.clone(), "![photo](https://example.com/photo.jpg)").unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);
    assert!(output.contains("![photo](https://example.com/photo.jpg)"), "Image should roundtrip. Got: {}", output);
}

#[test]
fn test_media_audio_roundtrip() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    markdown_to_xml_fragment(&mut txn, fragment.clone(), "![track.mp3](/drive/audio-123)").unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);
    assert!(output.contains("![track.mp3](/drive/audio-123)"), "Audio should roundtrip. Got: {}", output);
}

#[test]
fn test_media_video_roundtrip() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    markdown_to_xml_fragment(&mut txn, fragment.clone(), "![clip.mp4](/drive/video-456)").unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);
    assert!(output.contains("![clip.mp4](/drive/video-456)"), "Video should roundtrip. Got: {}", output);
}

#[test]
fn test_media_creates_unified_element() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    markdown_to_xml_fragment(&mut txn, fragment.clone(), "![song.mp3](/drive/abc)").unwrap();

    // Should create a <media> element with type="audio"
    if let Some(XmlNode::Element(media)) = fragment.get(&txn, 0) {
        assert_eq!(media.tag().as_ref(), "media", "Should be a unified 'media' element");
        assert_eq!(media.get_attribute(&txn, "type").unwrap(), "audio");
        assert_eq!(media.get_attribute(&txn, "src").unwrap(), "/drive/abc");
        assert_eq!(media.get_attribute(&txn, "alt").unwrap(), "song.mp3");
    } else {
        panic!("Expected a media element");
    }
}

#[test]
fn test_media_type_detection() {
    let test_cases = vec![
        ("![x](file.jpg)", "image"),
        ("![x](file.jpeg)", "image"),
        ("![x](file.png)", "image"),
        ("![x](file.gif)", "image"),
        ("![x](file.webp)", "image"),
        ("![x](file.svg)", "image"),
        ("![x](file.mp3)", "audio"),
        ("![x](file.wav)", "audio"),
        ("![x](file.m4a)", "audio"),
        ("![x](file.ogg)", "audio"),
        ("![x](file.flac)", "audio"),
        ("![x](file.mp4)", "video"),
        ("![x](file.mov)", "video"),
        ("![x](file.webm)", "video"),
        ("![x](file.avi)", "video"),
        ("![x](file.mkv)", "video"),
        ("![x](file.unknown)", "image"), // Unknown defaults to image
    ];

    for (markdown, expected_type) in test_cases {
        let doc = Doc::new();
        let mut txn = doc.transact_mut();
        let fragment = txn.get_or_insert_xml_fragment("content");

        markdown_to_xml_fragment(&mut txn, fragment.clone(), markdown).unwrap();

        if let Some(XmlNode::Element(media)) = fragment.get(&txn, 0) {
            let actual_type = media.get_attribute(&txn, "type").unwrap_or_default();
            assert_eq!(actual_type, expected_type, "For {}, expected type={}, got={}", markdown, expected_type, actual_type);
        } else {
            panic!("Expected a media element for: {}", markdown);
        }
    }
}

// =============================================================================
// Checkbox roundtrip tests (list_item checked attribute)
// =============================================================================

#[test]
fn test_checkbox_roundtrip() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    markdown_to_xml_fragment(&mut txn, fragment.clone(), "- [ ] Todo\n- [x] Done").unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);
    assert!(output.contains("[ ] Todo"), "Unchecked should roundtrip. Got: {}", output);
    assert!(output.contains("[x] Done"), "Checked should roundtrip. Got: {}", output);
}

#[test]
fn test_checkbox_as_list_item_attribute() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    markdown_to_xml_fragment(&mut txn, fragment.clone(), "- [ ] Unchecked\n- [x] Checked\n- Normal").unwrap();

    // Find the bullet_list
    if let Some(XmlNode::Element(list)) = fragment.get(&txn, 0) {
        assert_eq!(list.tag().as_ref(), "bullet_list");

        // First item: unchecked
        if let Some(XmlNode::Element(item0)) = list.get(&txn, 0) {
            assert_eq!(item0.get_attribute(&txn, "checked").unwrap(), "false",
                       "First item should have checked=false");
        }

        // Second item: checked
        if let Some(XmlNode::Element(item1)) = list.get(&txn, 1) {
            assert_eq!(item1.get_attribute(&txn, "checked").unwrap(), "true",
                       "Second item should have checked=true");
        }

        // Third item: no checked attribute
        if let Some(XmlNode::Element(item2)) = list.get(&txn, 2) {
            assert!(item2.get_attribute(&txn, "checked").is_none(),
                    "Normal item should have no checked attribute");
        }
    } else {
        panic!("Expected a bullet_list element");
    }
}

// =============================================================================
// Underline roundtrip tests
// =============================================================================

#[test]
fn test_underline_roundtrip() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    markdown_to_xml_fragment(&mut txn, fragment.clone(), "Hello <u>underlined</u> world").unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);
    assert!(output.contains("<u>underlined</u>"), "Underline should roundtrip via <u> tag. Got: {}", output);
}

// =============================================================================
// Regular link tests (should NOT become entity_link or file_card)
// =============================================================================

#[test]
fn test_regular_link_stays_as_mark() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    markdown_to_xml_fragment(&mut txn, fragment.clone(), "[Google](https://google.com)").unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);
    assert!(output.contains("[Google](https://google.com)"), "Regular link should roundtrip. Got: {}", output);
}

// =============================================================================
// Mixed content tests
// =============================================================================

#[test]
fn test_paragraph_with_entity_link_and_text() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    markdown_to_xml_fragment(&mut txn, fragment.clone(),
        "I met [John](/person/123) at [Home](/place/456) yesterday").unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);
    assert!(output.contains("I met"), "Got: {}", output);
    assert!(output.contains("[John](/person/123)"), "Got: {}", output);
    assert!(output.contains(" at "), "Got: {}", output);
    assert!(output.contains("[Home](/place/456)"), "Got: {}", output);
    assert!(output.contains(" yesterday"), "Got: {}", output);
}

#[test]
fn test_entity_link_with_bold_text_nearby() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    markdown_to_xml_fragment(&mut txn, fragment.clone(),
        "**Hello** [John](/person/123) **world**").unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);
    assert!(output.contains("**Hello**"), "Bold should survive. Got: {}", output);
    assert!(output.contains("[John](/person/123)"), "Entity link should survive. Got: {}", output);
    assert!(output.contains("**world**"), "Bold should survive. Got: {}", output);
}
