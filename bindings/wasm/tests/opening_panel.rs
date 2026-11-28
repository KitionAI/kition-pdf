//! `inspect()` names the panel a document asks to open with (`/PageMode`), so the editor opens the
//! bookmarks of a file that says `/UseOutlines` (plan "PDF usable by hand", stage 7, A2): `page`
//! for none, and the document's own setting as it changes and is undone.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use kition_kit::KitDocument;
use kition_pdf_kit::PdfDocument;
use serde_json::json;

fn navigation(doc: &PdfDocument) -> String {
    doc.inspect().unwrap()["navigation"].as_str().unwrap_or_default().to_owned()
}

#[test]
fn inspect_names_the_panel_the_document_opens_with() {
    let mut doc = PdfDocument::create("Outline.pdf", 240, 200).unwrap();
    assert_eq!(navigation(&doc), "page");
    doc.execute("file.setInitialView", json!({ "navigation": "bookmarks" })).unwrap();
    assert_eq!(navigation(&doc), "bookmarks");
    doc.execute("file.setInitialView", json!({ "navigation": "pages" })).unwrap();
    assert_eq!(navigation(&doc), "pages");
    assert!(doc.undo().unwrap().0, "there was an edit to undo");
    assert_eq!(navigation(&doc), "bookmarks");
    // Kept in the file: the saved bytes open asking for the bookmarks.
    let bytes = doc.save(None).unwrap();
    let reopened = PdfDocument::open("Outline.pdf", &bytes).unwrap();
    assert_eq!(navigation(&reopened), "bookmarks");
}
