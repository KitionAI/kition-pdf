//! Regression (cases/regressions/issue-pdf-state-clock): a document's `~state` and saved bytes are a
//! function of its content and its edits, never of the moment they are read or saved. `/ModDate`
//! is the time of the edit that made the content (recorded when the edit is applied), not the
//! time of saving; a document never edited saves as its file, with the file's own `/ModDate`; an
//! undo or redo brings back the time of the state it returns to. Before, every save
//! stamped the clock's "now", so on the native kit the `~state` of an unchanged document changed
//! every second and a premise on it could be refused although nothing changed.
//!
//! Each test drives its own fixed clock (the session's clock seam) and moves it by hand.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use kition_kit::KitDocument;
use kition_pdf_cos::pdf_date;
use kition_pdf_kit::PdfDocument;
use serde_json::{Value, json};

/// The file's own dates, which a save after no edit must keep.
const FILE_MOD_DATE: &str = "D:20200102030405Z";
const FILE_CREATION_DATE: &str = "D:20190101000000Z";

/// A one-page PDF with a document information dictionary that has dates.
fn source() -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>".to_owned(),
        "<< /Length 21 >>\nstream\n72 72 200 100 re f\n\nendstream".to_owned(),
        format!("<< /Title (Dated) /CreationDate ({FILE_CREATION_DATE}) /ModDate ({FILE_MOD_DATE}) >>"),
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

/// The document information dictionary's entry `key` in `bytes` (the last revision's).
fn info(bytes: &[u8], key: &[u8]) -> Option<String> {
    let doc = kition_pdf_cos::Document::open(Arc::new(bytes.to_vec())).unwrap();
    let info = doc.resolve(doc.trailer().get(b"Info")?);
    Some(info.as_dict()?.get(key)?.as_string()?.to_text())
}

fn state(doc: &PdfDocument) -> Value {
    let derived = doc.derived(&[], &[]).unwrap();
    serde_json::to_value(&derived).unwrap()
}

fn save(doc: &PdfDocument) -> Vec<u8> {
    doc.save(None).unwrap()
}

static CLOCK_A: AtomicI64 = AtomicI64::new(1_800_000_000);

/// (a) no edit, the clock moves: the same `~state`; (b) two saves across a tick: the same bytes;
/// (c) an edit changes `~state` and its time is the saved `/ModDate`, however late the save.
#[test]
fn the_state_follows_edits_not_the_clock() {
    let mut doc = PdfDocument::open("dated.pdf", &source()).unwrap().with_clock(|| CLOCK_A.load(Ordering::SeqCst));
    let opened = state(&doc);
    CLOCK_A.store(1_800_000_000, Ordering::SeqCst);
    doc.execute("comment_add", json!({ "page": 1, "type": "note", "at": [100, 100], "contents": "first" })).unwrap();
    let first = state(&doc);
    assert_ne!(first, opened, "an edit changes the state");

    // (a) The clock moves, nothing is edited: the state is the same.
    CLOCK_A.store(1_800_000_001, Ordering::SeqCst);
    assert_eq!(state(&doc), first, "one second later, no edit");
    CLOCK_A.store(1_800_003_600, Ordering::SeqCst);
    assert_eq!(state(&doc), first, "an hour later, no edit");

    // (b) Two saves across a clock tick write the same bytes, dated at the edit.
    let saved = save(&doc);
    CLOCK_A.store(1_800_003_601, Ordering::SeqCst);
    assert!(save(&doc) == saved, "a second save a second later wrote other bytes");
    assert_eq!(info(&saved, b"ModDate").as_deref(), Some(pdf_date(1_800_000_000).as_str()), "ModDate is the edit's time");
    assert_eq!(info(&saved, b"CreationDate").as_deref(), Some(FILE_CREATION_DATE), "CreationDate is the file's");

    // (c) A later edit changes the state; its time is the new ModDate, not the save's.
    CLOCK_A.store(1_800_010_000, Ordering::SeqCst);
    doc.execute("page_rotate", json!({ "pages": [1], "degrees": 90 })).unwrap();
    let second = state(&doc);
    assert_ne!(second, first, "a second edit changes the state");
    CLOCK_A.store(1_800_020_000, Ordering::SeqCst);
    assert_eq!(state(&doc), second);
    let saved = save(&doc);
    assert_eq!(info(&saved, b"ModDate").as_deref(), Some(pdf_date(1_800_010_000).as_str()), "ModDate is the last edit's time");

    // The saved file opens as the same state, whenever it is read.
    let reopened = PdfDocument::open("dated.pdf", &saved).unwrap().with_clock(|| CLOCK_A.load(Ordering::SeqCst));
    assert_eq!(state(&reopened), second, "the saved file is the state in memory");
}

static CLOCK_D: AtomicI64 = AtomicI64::new(1_800_000_000);

/// (d) A document opened and never edited saves as its file: its own `/ModDate`, and the
/// `~state` of the file itself, whenever it is read or saved.
#[test]
fn a_document_never_edited_saves_as_its_file() {
    let file = source();
    let doc = PdfDocument::open("dated.pdf", &file).unwrap().with_clock(|| CLOCK_D.load(Ordering::SeqCst));
    let opened = state(&doc);
    assert_eq!(opened[0]["values"]["state"], json!(kition_kit::session::sha256_hex(&file)), "the state of the file itself");
    CLOCK_D.store(1_800_000_005, Ordering::SeqCst);
    assert_eq!(state(&doc), opened);
    let saved = save(&doc);
    assert!(saved == file, "saving a document never edited writes the file as it is");
    assert_eq!(info(&saved, b"ModDate").as_deref(), Some(FILE_MOD_DATE));
}

static CLOCK_E: AtomicI64 = AtomicI64::new(1_800_000_000);

/// (e) Undo and redo bring back the time of the state they return to, never the time of the
/// undo: undoing every edit gives the file as opened, byte for byte (its own `/ModDate`, the
/// `~state` read at opening), however late; undoing one of two edits is dated at the first edit,
/// and the state it gives holds still while the clock moves. (That its bytes are the bytes saved
/// right after the first edit is undo_numbers.rs's, regressions/issue-pdf-undo-size.)
#[test]
fn undo_and_redo_bring_back_the_time_of_the_state() {
    let file = source();
    let mut doc = PdfDocument::open("dated.pdf", &file).unwrap().with_clock(|| CLOCK_E.load(Ordering::SeqCst));
    let opened = state(&doc);
    CLOCK_E.store(1_800_000_100, Ordering::SeqCst);
    doc.execute("page_rotate", json!({ "pages": [1], "degrees": 90 })).unwrap();
    let first = state(&doc);
    CLOCK_E.store(1_800_000_200, Ordering::SeqCst);
    doc.execute("comment_add", json!({ "page": 1, "type": "note", "at": [100, 100], "contents": "second" })).unwrap();
    let second = state(&doc);
    assert_ne!(second, first);

    CLOCK_E.store(1_800_000_300, Ordering::SeqCst);
    assert!(doc.undo().unwrap().0);
    let undone = state(&doc);
    assert_ne!(undone, second);
    let saved = save(&doc);
    assert_eq!(info(&saved, b"ModDate").as_deref(), Some(pdf_date(1_800_000_100).as_str()), "dated at the first edit, not the undo");
    CLOCK_E.store(1_800_000_301, Ordering::SeqCst);
    assert_eq!(state(&doc), undone, "the state after the undo holds still while the clock moves");
    assert!(save(&doc) == saved);

    CLOCK_E.store(1_800_000_400, Ordering::SeqCst);
    assert!(doc.undo().unwrap().0);
    assert_eq!(state(&doc), opened, "undoing every edit gives the file as opened");
    let saved = save(&doc);
    assert!(saved == file, "the file as opened, byte for byte");
    assert_eq!(info(&saved, b"ModDate").as_deref(), Some(FILE_MOD_DATE));

    CLOCK_E.store(1_800_000_500, Ordering::SeqCst);
    assert!(doc.redo().unwrap().0);
    assert_eq!(state(&doc), undone, "redo gives the state the first undo gave");
    assert_eq!(info(&save(&doc), b"ModDate").as_deref(), Some(pdf_date(1_800_000_100).as_str()), "dated at the first edit, not the redo");
    assert!(doc.redo().unwrap().0);
    assert_eq!(state(&doc), second, "and redoing the second edit gives its state");
    assert_eq!(info(&save(&doc), b"ModDate").as_deref(), Some(pdf_date(1_800_000_200).as_str()));
}
