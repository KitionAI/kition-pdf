//! The editor renames and deletes bookmarks in place (plan "PDF usable by hand", stage 7, F3), so
//! the web engine offers the bookmark tools: their `path` is a bookmark's place in the outline,
//! not a file, and was taken for one (the tools were left out). It still refuses a file argument
//! where one is a file.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_kit::KitDocument;
use kition_pdf_kit::PdfDocument;
use serde_json::{Value, json};

fn titles(doc: &mut PdfDocument) -> Vec<String> {
    let list = doc.execute("bookmark.list", json!({})).unwrap();
    let mut out = Vec::new();
    fn walk(list: &Value, depth: usize, out: &mut Vec<String>) {
        for item in list.as_array().into_iter().flatten() {
            out.push(format!("{}{}@{}", "-".repeat(depth), item["title"].as_str().unwrap_or_default(), item["page"]));
            walk(&item["children"], depth + 1, out);
        }
    }
    walk(&list["bookmarks"], 0, &mut out);
    out
}

#[test]
fn the_web_engine_renames_deletes_moves_and_repoints_bookmarks() {
    let mut doc = PdfDocument::create("Outline.pdf", 240, 200).unwrap();
    doc.execute("page.insertBlank", json!({ "at": 2 })).unwrap();
    for id in ["bookmark_rename", "bookmark_delete", "bookmark_move", "bookmark_set_page"] {
        let command = doc.commands().into_iter().find(|command| command.id == id).unwrap_or_else(|| panic!("{id} is not offered"));
        let schema = command.schema.unwrap();
        assert!(schema.get("path").is_some(), "{id}: its path is a parameter: {schema}");
        assert!(command.mutates, "{id}");
    }
    doc.execute("bookmark.add", json!({ "title": "One", "page": 1 })).unwrap();
    doc.execute("bookmark.add", json!({ "title": "Two", "page": 2 })).unwrap();
    doc.execute("bookmark.add", json!({ "title": "Inner", "page": 1, "parent": [1] })).unwrap();
    doc.execute("bookmark_rename", json!({ "path": [1, 1], "title": "Methods" })).unwrap();
    assert_eq!(titles(&mut doc), ["One@1", "-Methods@1", "Two@2"]);
    doc.execute("bookmark_set_page", json!({ "path": [1, 1], "page": 2 })).unwrap();
    doc.execute("bookmark_move", json!({ "path": [2], "position": 1 })).unwrap();
    assert_eq!(titles(&mut doc), ["Two@2", "One@1", "-Methods@2"]);
    doc.execute("bookmark_delete", json!({ "path": [2] })).unwrap();
    assert_eq!(titles(&mut doc), ["Two@2"]);
    assert!(doc.undo().unwrap().0);
    assert_eq!(titles(&mut doc), ["Two@2", "One@1", "-Methods@2"]);
    // Another tool's `path` is still a file, and refused.
    assert!(doc.commands().iter().all(|command| command.id != "page_insert_file"));
}
