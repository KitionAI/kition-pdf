//! C8-pdf-4: copied comments have independent identities in the saved PDF.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kition_kit::KitDocument;
use kition_pdf_kit::PdfDocument;
use serde_json::json;

#[test]
fn duplicated_pages_store_fresh_annotation_names() {
    let mut pdf = PdfDocument::create("copy.pdf", 612, 792).unwrap();
    pdf.execute("comment.add", json!({"page": 1, "type": "note", "at": [100, 100], "contents": "original"})).unwrap();
    let parent = pdf.execute("comment.list", json!({})).unwrap()["comments"][0]["id"].clone();
    pdf.execute("comment.reply", json!({"id": parent, "text": "reply"})).unwrap();
    pdf.execute("page.duplicate", json!({"pages": [1]})).unwrap();
    let saved = pdf.save(None).unwrap();
    let cos = kition_pdf_cos::Document::open(std::sync::Arc::new(saved.clone())).unwrap();
    let mut names = std::collections::HashSet::new();
    for page in kition_pdf_organize::pages(&cos).unwrap() {
        let dict = cos.get(page.obj);
        let annots = cos.resolve(dict.as_dict().unwrap().get(b"Annots").unwrap());
        for annot in annots.as_array().unwrap() {
            let dict = cos.resolve(annot);
            let Some(name) = dict.as_dict().unwrap().get(b"NM") else { continue };
            let name = name.as_string().unwrap().to_text();
            assert!(names.insert(name.clone()), "two saved annotations share /NM {name}");
        }
    }
    assert_eq!(names.len(), 4);
    let mut reopened = PdfDocument::open("copy.pdf", &saved).unwrap();
    let comments = reopened.execute("comment.list", json!({})).unwrap();
    let threads = comments["comments"].as_array().unwrap();
    assert_eq!(threads.len(), 2);
    for thread in threads {
        assert_eq!(thread["replies"].as_array().unwrap().len(), 1);
        assert_eq!(thread["replies"][0]["page"], thread["page"]);
        assert_eq!(thread["replies"][0]["contents"], "reply");
    }
    reopened.execute("comment.edit", json!({"id": parent, "contents": "changed original"})).unwrap();
    let comments = reopened.execute("comment.list", json!({"page": 2})).unwrap();
    assert!(comments["comments"].as_array().unwrap().iter().any(|c| c["contents"] == "original"));
}
