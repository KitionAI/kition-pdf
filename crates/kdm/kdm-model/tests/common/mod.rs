//! Shared test helpers: a small PDF to edit (pages, a form field), edits the way the engine
//! makes them (comments, form fills, page moves and deletes, a full rewrite), and replicas of
//! one document exchanging changes and blobs. Each `tests/*.rs` binary uses only some.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use kdm_model::{PdfSession, canonical};
use kition_kit::kdm::Origin;
use kition_kit::session::MemoryBlobs;
use kition_pdf_annot::{Meta, NewAnnotation, NoteIcon, Shape, Style};
use kition_pdf_cos::{Document, SaveOptions, write_full};
use kition_pdf_forms::{FieldValue, NewField};

pub fn person(name: &str) -> Origin {
    Origin::person(format!("user:{name}"))
}

/// `doc` written in full and opened again: a document with a base file, as the engine has.
pub fn reopened(doc: &Document) -> Document {
    let bytes = write_full(doc, &SaveOptions::default()).unwrap_or_else(|e| panic!("write: {e}"));
    Document::open(Arc::new(bytes)).unwrap_or_else(|e| panic!("open: {e}"))
}

/// A PDF of `pages` Letter pages with a text field `Name` on the first.
pub fn fixture(pages: usize) -> Document {
    let mut doc = kition_pdf_create::blank(612.0, 792.0, pages).unwrap_or_else(|e| panic!("blank: {e}"));
    kition_pdf_forms::add_field(&mut doc, 0, [72.0, 600.0, 300.0, 624.0], &NewField::Text { multiline: false }, Some("Name"))
        .unwrap_or_else(|e| panic!("field: {e}"));
    reopened(&doc)
}

/// The PDFs committed to this repository.
pub fn repo_pdfs() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    ["web/public/sample/Sample.pdf", "crates/engine/sign/tests/data/openssl-signed.pdf", "crates/engine/sign/tests/data/x509-rsa-sha1.pdf"]
        .iter()
        .map(|p| root.join(p))
        .filter(|p| p.exists())
        .collect()
}

/// The PDFs under `$KITION_PDF_CORPUS` (recursively), if set.
pub fn corpus_pdfs() -> Vec<PathBuf> {
    let Ok(dir) = std::env::var("KITION_PDF_CORPUS") else { return Vec::new() };
    let mut out = Vec::new();
    let mut stack = vec![PathBuf::from(dir)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

pub fn meta(id: &str) -> Meta {
    Meta { date: Some("D:20261017120000Z".into()), id: id.into() }
}

/// Adds a sticky note with `text` on `page`.
pub fn add_note(doc: &mut Document, page: usize, text: &str) {
    let note = NewAnnotation {
        page,
        shape: Shape::Note { at: [100.0, 100.0], icon: NoteIcon::Comment },
        style: Style::default(),
        contents: text.into(),
        author: "Test".into(),
    };
    kition_pdf_annot::add_annotation(doc, &note, &meta(text)).unwrap_or_else(|e| panic!("note: {e}"));
}

pub fn fill(doc: &mut Document, value: &str) {
    kition_pdf_forms::set_value(doc, "Name", &FieldValue::Text(value.into())).unwrap_or_else(|e| panic!("fill: {e}"));
}

/// The comments of the view: (page, contents), in page order.
pub fn comments(doc: &Document) -> Vec<(usize, String)> {
    kition_pdf_annot::summaries(doc).into_iter().filter(|s| s.subtype != "Widget").map(|s| (s.page, s.contents.unwrap_or_default())).collect()
}

pub fn field_value(doc: &Document) -> Vec<String> {
    kition_pdf_forms::fields(doc).into_iter().find(|f| f.name == "Name").map(|f| f.value).unwrap_or_default()
}

pub fn page_count(doc: &Document) -> usize {
    kition_pdf_organize::page_count(doc).unwrap_or(0)
}

/// What a full save makes of `doc` (applying redactions, changing security): the document
/// rewritten in full and opened again, a new base.
pub fn rewritten(doc: &Document) -> Document {
    reopened(doc)
}

/// A replica of `doc` (its creating transaction).
pub fn first(doc: &Document) -> PdfSession {
    PdfSession::create(doc, None).unwrap_or_else(|e| panic!("create: {e}"))
}

/// A peer of `of`, opened from its snapshot and blobs.
pub fn peer(of: &PdfSession) -> PdfSession {
    let snapshot = of.snapshot().unwrap_or_else(|e| panic!("snapshot: {e}"));
    PdfSession::open_snapshot(&snapshot, of.blobs().clone(), None).unwrap_or_else(|e| panic!("open_snapshot: {e}"))
}

/// Edits `session`'s view with `f` and records what changed, as the engine bridge does.
pub fn edit(session: &mut PdfSession, f: impl FnOnce(&mut Document)) -> u32 {
    let mut after = session.document().clone();
    f(&mut after);
    session.record(&person("test"), &after).unwrap_or_else(|e| panic!("record: {e}"))
}

/// Replaces `session`'s view with `f`'s document (a full rewrite) and records it: a rebase.
pub fn replace(session: &mut PdfSession, f: impl FnOnce(&Document) -> Document) -> u32 {
    let after = f(session.document());
    session.record(&person("test"), &after).unwrap_or_else(|e| panic!("record: {e}"))
}

/// Sends `from`'s changes that `to` lacks, with the blobs they need.
pub fn send(from: &PdfSession, to: &mut PdfSession) {
    let since = to.version();
    let bytes = from.export_changes(&since).unwrap_or_else(|e| panic!("export: {e}"));
    to.add_blobs(&MemoryBlobs(from.blobs_since(&since).into_iter().collect()));
    to.import_changes(&bytes).unwrap_or_else(|e| panic!("import: {e}"));
}

pub fn sync(a: &mut PdfSession, b: &mut PdfSession) {
    send(a, b);
    send(b, a);
    send(a, b);
}

/// Exchanges until every session holds the same version (repairs are changes too).
pub fn sync_all(sessions: &mut [PdfSession]) {
    for _ in 0..8 {
        for i in 0..sessions.len() {
            for j in 0..sessions.len() {
                if i != j {
                    let (from, to) = if i < j {
                        let (left, right) = sessions.split_at_mut(j);
                        (&left[i], &mut right[0])
                    } else {
                        let (left, right) = sessions.split_at_mut(i);
                        (&right[0], &mut left[j])
                    };
                    send(from, to);
                }
            }
        }
        if sessions.windows(2).all(|w| w[0].version() == w[1].version()) {
            return;
        }
    }
    panic!("the replicas did not converge");
}

/// The view as replicas compare it.
pub fn view(session: &PdfSession) -> serde_json::Value {
    canonical(session.document(), session.model().pristine(), session.model().ids())
}

/// Same replica state, same view (whatever numbers each gave added objects), and a view that
/// saves and reopens.
pub fn assert_converged(a: &PdfSession, b: &PdfSession) {
    assert_eq!(a.state_hash().ok(), b.state_hash().ok(), "same replica state");
    assert_eq!(view(a), view(b), "same view");
    assert_eq!(comments(a.document()), comments(b.document()), "same comments");
    assert_eq!(page_count(a.document()), page_count(b.document()), "same pages");
    for session in [a, b] {
        let saved = kition_pdf_cos::write_incremental(session.document(), &SaveOptions::default()).unwrap_or_else(|e| panic!("save: {e}"));
        Document::open(Arc::new(saved)).unwrap_or_else(|e| panic!("reopen: {e}"));
    }
}
