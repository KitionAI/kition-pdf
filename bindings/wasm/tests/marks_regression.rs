//! C8-pdf-4: page-scoped mark removal must preserve marks on other pages.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kition_kit::KitDocument;
use kition_pdf_kit::PdfDocument;
use serde_json::json;

#[test]
fn removing_marks_honors_the_page_list_after_reopen() {
    for (kind, tool, settings) in [
        ("watermark", "file.setWatermark", json!({"text": "Watermark"})),
        ("header_footer", "file.setHeaderFooter", json!({"headerLeft": "Header"})),
        ("background", "file.setBackground", json!({"color": "yellow"})),
    ] {
        let mut doc = PdfDocument::create("marks.pdf", 612, 792).unwrap();
        doc.execute("page.insertBlank", json!({"at": 2})).unwrap();
        doc.execute(tool, settings).unwrap();
        let bytes = doc.save(None).unwrap();
        doc.execute("file.removeMarks", json!({"kind": kind, "pages": [1]})).unwrap();
        let saved = doc.save(None).unwrap();
        let before = kition_pdf_cos::Document::open(std::sync::Arc::new(bytes)).unwrap();
        let after = kition_pdf_cos::Document::open(std::sync::Arc::new(saved)).unwrap();
        let page_content = |cos: &kition_pdf_cos::Document, page: usize| {
            let r = kition_pdf_organize::pages(cos).unwrap()[page].obj;
            let obj = cos.get(r);
            let contents = cos.resolve(obj.as_dict().unwrap().get(b"Contents").unwrap());
            contents.as_array().unwrap().iter().map(|o| format!("{:?}", cos.resolve(o))).collect::<Vec<_>>()
        };
        assert!(page_content(&after, 0).len() < page_content(&before, 0).len(), "{kind}: removed on page 1");
        assert_eq!(page_content(&after, 1), page_content(&before, 1), "{kind}: page 2 preserved");
    }
}
