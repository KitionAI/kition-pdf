//! Regression (cases/regressions/issue-pdf-undo-size): an undo restores exactly the state before
//! the edit, so the bytes a document saves as (and the `~state` hashed from them) after "A, B,
//! undo B" are the bytes of "A". Before, the object numbers the undone edit took stayed counted:
//! the trailer's `/Size` (one greater than the highest object number the file defines, ISO
//! 32000-1 §7.5.5) still counted them although no object of the file has them, and the next edit
//! numbered its objects after them. So the `~state` an agent read after "A" no longer matched
//! after "A, B, undo".
//!
//! Each sequence runs on a fresh document with the same fixed clock (the session's clock seam),
//! so `/ModDate` and the ids new comments get follow from the edits alone. The edit after an undo
//! is `page_add_text`, which adds objects and mints no ids (a comment's `/NM` also follows from
//! how many changes the session made, so a new comment after an undo has another name than in a
//! session without the undone edit, as it should).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use kition_kit::KitDocument;
use kition_pdf_kit::PdfDocument;
use serde_json::{Value, json};

/// A one-page PDF with a document information dictionary.
fn source() -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] >>",
        "<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>",
        "<< /Length 21 >>\nstream\n72 72 200 100 re f\n\nendstream",
        "<< /Title (Numbers) /ModDate (D:20200102030405Z) >>",
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R /Info 5 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    out
}

fn open() -> PdfDocument {
    PdfDocument::open("numbers.pdf", &source()).unwrap().with_clock(|| 1_800_000_000)
}

/// A: a note (an annotation and the objects drawing it).
fn a(doc: &mut PdfDocument) {
    doc.execute("comment_add", json!({ "page": 1, "type": "note", "at": [100, 100], "contents": "first" })).unwrap();
}

/// B: a rectangle comment (more new objects, after A's).
fn b(doc: &mut PdfDocument) {
    doc.execute("comment_add", json!({ "page": 1, "type": "rectangle", "rect": [72, 300, 300, 360], "contents": "second" })).unwrap();
}

/// C: text on the page (a new content stream and its font), no new ids.
fn c(doc: &mut PdfDocument) {
    doc.execute("page_add_text", json!({ "page": 1, "at": [72, 500], "width": 400, "text": "Third" })).unwrap();
}

fn undo(doc: &mut PdfDocument) {
    assert!(doc.undo().unwrap().0, "there was an edit to undo");
}

fn redo(doc: &mut PdfDocument) {
    assert!(doc.redo().unwrap().0, "there was an edit to redo");
}

fn state(doc: &PdfDocument) -> Value {
    serde_json::to_value(doc.derived(&[], &[]).unwrap()).unwrap()
}

/// The last revision's trailer `/Size`.
fn size(bytes: &[u8]) -> Option<i64> {
    kition_pdf_cos::Document::open(Arc::new(bytes.to_vec())).unwrap().trailer().int(b"Size")
}

/// `got` (the sequence `what`) saves as `want` and has its `~state`.
fn assert_same(got: &PdfDocument, want: &PdfDocument, what: &str) {
    let (saved, expected) = (got.save(None).unwrap(), want.save(None).unwrap());
    assert_eq!(size(&saved), size(&expected), "{what}: the trailer /Size");
    assert!(saved == expected, "{what}: other bytes than the same state reached without the undone edit");
    assert_eq!(state(got), state(want), "{what}: the ~state");
}

/// "A, B, undo B" saves as "A": the same bytes and the same `~state`.
#[test]
fn an_undone_edit_leaves_no_numbers_behind() {
    let mut got = open();
    a(&mut got);
    b(&mut got);
    undo(&mut got);
    let mut want = open();
    a(&mut want);
    assert_same(&got, &want, "A, B, undo");
}

/// "A, B, undo B, redo B" saves as "A, B": the redone objects come back under their numbers.
#[test]
fn a_redone_edit_saves_as_the_edit() {
    let mut got = open();
    a(&mut got);
    b(&mut got);
    undo(&mut got);
    redo(&mut got);
    let mut want = open();
    a(&mut want);
    b(&mut want);
    assert_same(&got, &want, "A, B, undo, redo");
}

/// "A, B, undo B, C" saves as "A, C": C numbers its objects as it would have without B, taking
/// the numbers B had (no object of any revision of the file has them).
#[test]
fn an_edit_after_an_undo_numbers_as_without_the_undone_edit() {
    let mut got = open();
    a(&mut got);
    b(&mut got);
    undo(&mut got);
    c(&mut got);
    let mut want = open();
    a(&mut want);
    c(&mut want);
    assert_same(&got, &want, "A, B, undo, C");
}

/// "A, B, save, undo B" saves as "A": the save written after the undo is the file as opened with
/// A appended (it replaces the file holding B, so B's numbers are in no revision of it).
#[test]
fn an_undo_past_a_save_saves_as_the_state_before() {
    let mut got = open();
    a(&mut got);
    b(&mut got);
    got.save(None).unwrap();
    got.mark_saved();
    assert!(!got.dirty(), "the save was acknowledged");
    undo(&mut got);
    let mut want = open();
    a(&mut want);
    assert_same(&got, &want, "A, B, save, undo");
}
