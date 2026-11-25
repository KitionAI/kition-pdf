//! Regression: a document built by the same commands saves the same bytes when
//! `SOURCE_DATE_EPOCH` is set and ids are counted (as a gating test builds it): comments' dates
//! are the epoch, and their names follow from `KITION_REPRODUCIBLE_IDS` instead of randomness
//! (counted ids need the dev-only Cargo feature `reproducible-ids`, on here through the crate's
//! dev-dependencies; the epoch alone leaves ids random, kition-pdf-engine tests/random_ids.rs).
//! Each build runs in a process of its own: this test starts itself several times.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

use kition_kit::KitDocument;
use kition_pdf_kit::PdfDocument;
use serde_json::json;

const OUT: &str = "KITION_PDF_DETERMINISM_OUT";

fn build() -> Vec<u8> {
    let mut doc = PdfDocument::create("review.pdf", 612, 792).unwrap();
    doc.execute("page.addText", json!({ "page": 1, "at": [72, 72], "width": 400, "text": "Quarterly report", "size": 24 })).unwrap();
    doc.execute("comment.add", json!({ "page": 1, "type": "note", "at": [400, 80], "contents": "Check the totals" })).unwrap();
    doc.execute("comment.add", json!({ "page": 1, "type": "rectangle", "rect": [72, 300, 300, 360], "color": "#dd3333" })).unwrap();
    doc.execute("comment.add", json!({ "page": 1, "type": "highlight", "find": "Quarterly" })).unwrap();
    doc.execute("comment.add", json!({ "page": 1, "type": "stamp", "stamp": "approved", "dynamic": true, "at": [300, 600] })).unwrap();
    doc.save(Some("review.pdf")).unwrap()
}

/// The child's half: builds the document into the file `OUT` names (nothing without it).
#[test]
fn child_builds_a_document() {
    if let Ok(path) = std::env::var(OUT) {
        std::fs::write(path, build()).unwrap();
    }
}

fn build_in_a_process(path: &PathBuf) -> Vec<u8> {
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_builds_a_document", "--test-threads=1", "--quiet"])
        .env("SOURCE_DATE_EPOCH", "1794182400")
        .env("KITION_REPRODUCIBLE_IDS", "1794182400")
        .env(OUT, path)
        .status()
        .unwrap();
    assert!(status.success(), "the child build failed");
    std::fs::read(path).unwrap()
}

#[test]
fn the_same_commands_save_the_same_bytes() {
    let dir = std::env::temp_dir().join(format!("kition-pdf-determinism-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let first = build_in_a_process(&dir.join("0.pdf"));
    for build in 1..4 {
        let again = build_in_a_process(&dir.join(format!("{build}.pdf")));
        assert!(first == again, "build {build} saved other bytes than build 0");
    }
    std::fs::remove_dir_all(&dir).ok();
    // The comments carry the epoch as their date (2026-11-09, UTC).
    let text = String::from_utf8_lossy(&first);
    assert_eq!(text.matches("/M (D:20261109000000Z)").count(), 4, "every comment is dated at the epoch");
}
