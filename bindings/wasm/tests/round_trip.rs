//! Command-level round trips: a new PDF is edited only with engine commands, each edit is checked,
//! then the file is saved (incrementally, as Kition PDF saves; a redaction rewrites it in full),
//! reopened from the saved bytes alone, and compared whole: the inspected document (`doc_inspect`:
//! pages, annotations, fields, links, outline, metadata), the comments, bookmarks and form fields
//! as their own commands list them, every page's text, and every page's pixels. A second save of
//! the reopened file must reopen the same.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_kit::KitDocument;
use kition_pdf_kit::PdfDocument;
use serde_json::{Value, json};

const W: u32 = 240;
const H: u32 = 200;

fn run(doc: &mut PdfDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

fn new_doc() -> PdfDocument {
    PdfDocument::create("Form.pdf", W, H).unwrap()
}

/// What the file keeps. The session's document number, unsaved flag, undo and redo, the file it
/// was opened from, and the file's size in bytes (an incremental save appends) are not content.
fn lasting(doc: &mut PdfDocument) -> Value {
    let mut inspected = run(doc, "doc_inspect", json!({}));
    let object = inspected.as_object_mut().unwrap();
    object.remove("file_size");
    for key in ["dirty", "doc", "path", "undo", "redo"] {
        object["document"].as_object_mut().unwrap().remove(key);
    }
    let mut summary = doc.inspect().unwrap();
    for key in ["canUndo", "canRedo"] {
        summary.as_object_mut().unwrap().remove(key);
    }
    json!({
        "inspect": inspected,
        "summary": summary,
        "comments": run(doc, "comment_list", json!({})),
        "bookmarks": run(doc, "bookmark_list", json!({})),
        "fields": run(doc, "form_fields", json!({})),
        "links": run(doc, "link_list", json!({})),
        "text": run(doc, "text_extract", json!({})),
    })
}

fn pixels(doc: &PdfDocument) -> Vec<Vec<u8>> {
    let pages = doc.inspect().unwrap()["pages"].as_array().unwrap().clone();
    pages
        .iter()
        .map(|page| {
            let area = &page["area"];
            let (x, y) = (area["x"].as_f64().unwrap(), area["y"].as_f64().unwrap());
            let (w, h) = (area["width"].as_f64().unwrap() as u32, area["height"].as_f64().unwrap() as u32);
            doc.render(x, y, 1.0, w, h).unwrap()
        })
        .collect()
}

fn reopen(doc: &PdfDocument) -> PdfDocument {
    PdfDocument::open("Form.pdf", &doc.save(None).unwrap()).unwrap()
}

fn differences(path: &str, a: &Value, b: &Value, out: &mut Vec<String>) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for key in x.keys().chain(y.keys().filter(|key| !x.contains_key(*key))) {
                differences(&format!("{path}/{key}"), x.get(key).unwrap_or(&Value::Null), y.get(key).unwrap_or(&Value::Null), out);
            }
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            for (at, (x, y)) in x.iter().zip(y).enumerate() {
                differences(&format!("{path}/{at}"), x, y, out);
            }
        }
        _ if a != b => out.push(format!("{path}: {a} != {b}")),
        _ => {}
    }
}

fn assert_round_trip(doc: &mut PdfDocument) {
    let mut reopened = reopen(doc);
    let (before, after) = (lasting(doc), lasting(&mut reopened));
    let mut changed = Vec::new();
    differences("", &before, &after, &mut changed);
    assert!(changed.is_empty(), "the document after saving and reopening:\n{}", changed.join("\n"));
    assert!(pixels(&reopened) == pixels(doc), "the pages' pixels after saving and reopening");
    let mut again = reopen(&reopened);
    assert_eq!(lasting(&mut again), before, "a second save of the reopened file");
}

#[test]
fn pages_survive_save_and_reopen() {
    let mut doc = new_doc();
    run(&mut doc, "page_add_text", json!({ "page": 1, "text": "First", "at": [20, 40], "size": 18 }));
    run(&mut doc, "page_insert_blank", json!({ "at": 2, "width": 300, "height": 200 }));
    run(&mut doc, "page_add_text", json!({ "page": 2, "text": "Second", "at": [20, 40], "size": 18 }));
    run(&mut doc, "page_duplicate", json!({ "pages": [1] }));
    run(&mut doc, "page_rotate", json!({ "pages": [2], "degrees": 90 }));
    run(&mut doc, "page_move", json!({ "pages": [3], "to": 1 }));
    run(&mut doc, "page_set_box", json!({ "pages": [1], "box": "crop", "margins": [10, 10, 10, 10] }));
    run(&mut doc, "page_number", json!({ "from": 1, "to": 3, "style": "lower-roman", "prefix": "p-" }));
    let pages = run(&mut doc, "doc_inspect", json!({}))["pages"].clone();
    assert_eq!(pages.as_array().unwrap().len(), 3, "{pages}");
    assert_eq!(pages[0]["label"], "p-i", "{pages}");
    assert_round_trip(&mut doc);
}

#[test]
fn comments_and_replies_survive_save_and_reopen() {
    let mut doc = new_doc();
    run(&mut doc, "page_add_text", json!({ "page": 1, "text": "Review this sentence", "at": [20, 40], "size": 14 }));
    run(
        &mut doc,
        "comment_add",
        json!({ "page": 1, "type": "note", "at": [30, 30], "contents": "Check 中文 ✓", "author": "Ana", "color": "yellow" }),
    );
    run(&mut doc, "comment_add", json!({ "page": 1, "type": "highlight", "find": "this sentence", "author": "Ana" }));
    run(&mut doc, "comment_reply", json!({ "page": 1, "index": 1, "text": "Done", "author": "Ben" }));
    run(&mut doc, "comment_set_status", json!({ "page": 1, "index": 1, "status": "completed", "author": "Ben" }));
    let comments = run(&mut doc, "comment_list", json!({}));
    assert!(comments.to_string().contains("Check 中文 ✓"), "{comments}");
    assert_round_trip(&mut doc);
}

#[test]
fn bookmarks_survive_save_and_reopen() {
    let mut doc = new_doc();
    run(&mut doc, "page_insert_blank", json!({ "at": 2 }));
    run(&mut doc, "bookmark_add", json!({ "title": "Intro 中文", "page": 1 }));
    run(&mut doc, "bookmark_add", json!({ "title": "Details", "page": 2, "parent": [1] }));
    run(&mut doc, "bookmark_add", json!({ "title": "Appendix ✓", "page": 2 }));
    let bookmarks = run(&mut doc, "bookmark_list", json!({}));
    assert!(bookmarks.to_string().contains("Details"), "{bookmarks}");
    let mut reopened = reopen(&doc);
    assert_eq!(run(&mut reopened, "bookmark_list", json!({})), bookmarks, "the bookmarks themselves survive");
    assert_round_trip(&mut doc);
}

#[test]
fn form_fields_and_values_survive_save_and_reopen() {
    let mut doc = new_doc();
    run(&mut doc, "form_add_field", json!({ "page": 1, "type": "text", "name": "name", "rect": [20, 20, 160, 40] }));
    run(&mut doc, "form_add_field", json!({ "page": 1, "type": "checkbox", "name": "agree", "rect": [20, 60, 36, 76] }));
    run(&mut doc, "form_add_field", json!({ "page": 1, "type": "combo", "name": "size", "rect": [20, 90, 120, 110], "options": ["S", "M", "L"] }));
    run(&mut doc, "form_fill", json!({ "values": { "name": "Ana 中文", "agree": true, "size": "M" } }));
    let fields = run(&mut doc, "form_fields", json!({}));
    assert!(fields.to_string().contains("Ana 中文"), "{fields}");
    assert_round_trip(&mut doc);
}

#[test]
fn links_and_text_edits_survive_save_and_reopen() {
    let mut doc = new_doc();
    run(&mut doc, "page_insert_blank", json!({ "at": 2 }));
    run(&mut doc, "page_add_text", json!({ "page": 1, "text": "Kition PDF text", "at": [20, 40], "size": 16 }));
    run(&mut doc, "text_edit", json!({ "page": 1, "line": 1, "text": "Kition PDF edited", "bold": true, "color": "#aa2200" }));
    run(&mut doc, "link_add", json!({ "page": 1, "rect": [20, 60, 120, 80], "url": "https://kition.ai" }));
    run(&mut doc, "link_add", json!({ "page": 1, "rect": [20, 90, 120, 110], "to_page": 2 }));
    let text = run(&mut doc, "text_extract", json!({ "pages": [1] }));
    assert!(text.to_string().contains("Kition PDF edited"), "{text}");
    assert_eq!(run(&mut doc, "link_list", json!({}))["links"].as_array().map_or(0, Vec::len), 2);
    assert_round_trip(&mut doc);
}

#[test]
fn applied_redactions_survive_a_full_rewrite_and_reopen() {
    let mut doc = new_doc();
    run(&mut doc, "page_add_text", json!({ "page": 1, "text": "Public line", "at": [20, 40], "size": 14 }));
    run(&mut doc, "page_add_text", json!({ "page": 1, "text": "Secret 4111", "at": [20, 80], "size": 14 }));
    run(&mut doc, "redact_mark", json!({ "find": "Secret 4111", "fill": "#000000" }));
    run(&mut doc, "redact_apply", json!({}));
    let text = run(&mut doc, "text_extract", json!({}));
    assert!(!text.to_string().contains("Secret"), "the redacted text is gone: {text}");
    assert!(text.to_string().contains("Public line"), "{text}");
    assert_round_trip(&mut doc);
    let saved = doc.save(None).unwrap();
    assert!(!saved.windows(6).any(|bytes| bytes == b"Secret"), "the saved file keeps no copy of the redacted text");
}
