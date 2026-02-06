//! Tests for markdown <-> XmlFragment conversion

use yrs::{Doc, Transact, WriteTxn, XmlFragment};

use super::{markdown_to_xml_fragment, xml_fragment_to_markdown};

/// Test basic paragraph conversion
#[test]
fn test_paragraph() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    markdown_to_xml_fragment(&mut txn, fragment.clone(), "Hello world").unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);
    assert!(output.contains("Hello world"));
}

/// Test heading conversion
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

/// Test code block conversion
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
}

/// Test bullet list conversion
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

/// Test ordered list conversion
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

/// Test blockquote conversion
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

/// Test task list conversion
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

/// Test horizontal rule
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

/// Test roundtrip - markdown -> XmlFragment -> markdown preserves content
#[test]
fn test_roundtrip_simple() {
    let doc = Doc::new();
    let mut txn = doc.transact_mut();
    let fragment = txn.get_or_insert_xml_fragment("content");

    let original = "# Title\n\nSome paragraph text.\n\n- Item one\n- Item two";

    markdown_to_xml_fragment(&mut txn, fragment.clone(), original).unwrap();

    let output = xml_fragment_to_markdown(&txn, fragment);

    // Check that key content is preserved
    assert!(output.contains("# Title"));
    assert!(output.contains("Some paragraph text"));
    assert!(output.contains("Item one"));
    assert!(output.contains("Item two"));
}
