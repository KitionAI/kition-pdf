//! `document.identity` and `document.fork` (Kit Process Protocol section 5.6, KitModel M7a)
//! through the real `kition-pdf` process. A `.pdf` embeds no replica: the kit saves the PDF's own
//! objects (an incremental update, or a rewrite), and makes a replica under a new document id each
//! time it opens a file (`kition_pdf_engine`'s `kdm_bridge`). So a PDF file carries no identity,
//! a byte copy carries none either, and a fork is the same bytes with no document id; a fork still
//! never replaces a file.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use kition_kit::KitDocument;
use kition_kit_host::{HeadlessKit, LaunchOptions};
use kition_pdf_kit::PdfDocument;
use serde_json::json;

const LIMIT: Duration = Duration::from_secs(30);

fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kition-pdf-identity-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(dir).unwrap()
}

async fn launch(dir: &Path) -> HeadlessKit {
    let mut options = LaunchOptions::new(env!("CARGO_BIN_EXE_kition-pdf"));
    options.workspace = Some(dir.to_path_buf());
    HeadlessKit::launch(options).await.unwrap()
}

/// A PDF the kit made and edited: saved, opened again, a comment added and saved incrementally,
/// as the kit saves a person's or an agent's edits.
fn report() -> Vec<u8> {
    let mut doc = PdfDocument::create("Report.pdf", 612, 792).unwrap();
    doc.execute("page_insert_blank", json!({ "at": 2 })).unwrap();
    let first = doc.save(None).unwrap();
    let mut doc = PdfDocument::open("Report.pdf", &first).unwrap();
    doc.execute("comment_add", json!({ "page": 1, "type": "note", "at": [100, 100], "contents": "Kept" })).unwrap();
    doc.save(None).unwrap()
}

#[tokio::test]
async fn a_pdf_carries_no_identity_and_forks_as_the_same_bytes() {
    let dir = workspace("no-replica");
    let bytes = report();
    std::fs::write(dir.join("Report.pdf"), &bytes).unwrap();
    let kit = launch(&dir).await;
    assert!(kit.serves("document.identity"), "{}", kit.capabilities());

    // The file carries no identity, and neither does a byte copy of it.
    assert_eq!(kit.identity("Report.pdf", LIMIT).await.unwrap().doc_id, None, "a .pdf embeds no replica");
    std::fs::copy(dir.join("Report.pdf"), dir.join("Copy.pdf")).unwrap();
    assert_eq!(kit.identity("Copy.pdf", LIMIT).await.unwrap().doc_id, None);

    // A fork writes the same bytes to a new file, with no document id.
    let fork = kit.fork("Report.pdf", "Fork.pdf", LIMIT).await.unwrap();
    assert_eq!(fork.doc_id, None);
    assert_eq!(fork.name, "Fork.pdf");
    assert!(!fork.version.is_empty());
    assert_eq!(std::fs::read(dir.join("Fork.pdf")).unwrap(), bytes, "a file without a replica forks unchanged");
    assert_eq!(std::fs::read(dir.join("Report.pdf")).unwrap(), bytes, "the source is unchanged");
    assert_eq!(kit.identity("Fork.pdf", LIMIT).await.unwrap().doc_id, None);

    // The fork opens as any PDF, with the source's pages and comment.
    let mut forked = PdfDocument::open("Fork.pdf", &std::fs::read(dir.join("Fork.pdf")).unwrap()).unwrap();
    let listed = forked.execute("comment_list", json!({})).unwrap();
    let comments: Vec<&str> = listed["comments"].as_array().unwrap().iter().filter_map(|comment| comment["contents"].as_str()).collect();
    assert_eq!(comments, ["Kept"]);

    // A fork never replaces a file.
    std::fs::write(dir.join("Taken.pdf"), b"a file already there").unwrap();
    let taken = kit.fork("Report.pdf", "Taken.pdf", LIMIT).await.unwrap_err();
    assert_eq!(taken.reason(), Some("file_conflict"), "{taken}");
    assert_eq!(std::fs::read(dir.join("Taken.pdf")).unwrap(), b"a file already there", "the file there is kept");
    kit.shutdown(Duration::from_secs(5)).await;
    let _ = std::fs::remove_dir_all(dir);
}
