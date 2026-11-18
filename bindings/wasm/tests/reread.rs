//! A clean session reads its file again, changed by another program, without losing its undo
//! histories (kition-sdk `KitDocument::reconcile_reread`): the update the other program appended
//! is written onto the replica as one import, and a later undo takes back only the session's own
//! edit. A file written anew is not reconciled and leaves the session as it was.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::Arc;

use kition_kit::KitDocument;
use kition_kit::kdm::Origin;
use kition_pdf_kit::PdfDocument;
use serde_json::json;

const NAME: &str = "Reread.pdf";

fn fixture() -> Vec<u8> {
    let mut doc = PdfDocument::create(NAME, 612, 792).unwrap();
    doc.execute("page_insert_blank", json!({ "at": 2 })).unwrap();
    doc.execute("form_add_field", json!({ "page": 1, "type": "text", "rect": [72, 600, 300, 624], "name": "Name" })).unwrap();
    doc.save(None).unwrap()
}

fn note(doc: &mut PdfDocument, page: u32, text: &str) {
    doc.execute("comment_add", json!({ "page": page, "type": "note", "at": [100, 100], "contents": text })).unwrap();
}

fn notes(doc: &mut PdfDocument, page: u32) -> Vec<String> {
    let listed = doc.execute("comment_list", json!({ "page": page })).unwrap();
    listed["comments"].as_array().unwrap().iter().filter_map(|c| c["contents"].as_str().map(str::to_owned)).collect()
}

fn field(doc: &mut PdfDocument) -> serde_json::Value {
    doc.execute("form_fields", json!({})).unwrap()["fields"][0]["value"].clone()
}

/// The live session's saved file, with `edit` run on it by another instance of the engine and
/// saved back (an incremental update), opened afresh as the host's reread.
fn reread(saved: &[u8], edit: impl FnOnce(&mut PdfDocument)) -> (Vec<u8>, PdfDocument) {
    let mut other = PdfDocument::open(NAME, saved).unwrap();
    edit(&mut other);
    let bytes = other.save(None).unwrap();
    assert!(bytes.starts_with(saved), "the other program appended an update");
    let fresh = PdfDocument::open(NAME, &bytes).unwrap();
    (bytes, fresh)
}

#[test]
fn a_reread_keeps_the_sessions_undo_and_the_files_edit() {
    let mut live = PdfDocument::open(NAME, &fixture()).unwrap();
    note(&mut live, 1, "mine");
    let saved = live.save(None).unwrap();
    live.mark_saved();
    let version = live.version().unwrap();
    let (file, fresh) = reread(&saved, |other| note(other, 2, "theirs"));

    assert_eq!(live.reconcile_reread(&fresh), Ok(true));
    assert!(!live.dirty(), "the reread session holds what the file holds");
    assert_ne!(live.version().unwrap(), version, "the update was written onto the replica");
    assert_eq!(notes(&mut live, 1), ["mine"]);
    assert_eq!(notes(&mut live, 2), ["theirs"]);
    assert!(live.can_undo(), "the session's own step is still undoable");
    let next = live.save(None).unwrap();
    assert_eq!(&next[..file.len()], file.as_slice(), "the next save appends to the file as reread");

    live.execute("edit.undo", json!({})).unwrap();
    assert_eq!(notes(&mut live, 1), Vec::<String>::new(), "undo takes back the session's edit");
    assert_eq!(notes(&mut live, 2), ["theirs"], "and keeps the other program's");
    live.execute("edit.redo", json!({})).unwrap();
    assert_eq!(notes(&mut live, 1), ["mine"]);
    assert_eq!(notes(&mut live, 2), ["theirs"]);
}

#[test]
fn an_agents_undo_after_a_reread_keeps_the_other_programs_fill() {
    let mut live = PdfDocument::open(NAME, &fixture()).unwrap();
    let agent = Origin::agent("agent:test");
    live.execute_as("comment_add", json!({ "page": 2, "type": "note", "at": [10, 10], "contents": "agent" }), &agent).unwrap();
    let saved = live.save(None).unwrap();
    live.mark_saved();
    let (_, fresh) = reread(&saved, |other| {
        other.execute("form_fill", json!({ "values": { "Name": "Bob" } })).unwrap();
    });

    assert_eq!(live.reconcile_reread(&fresh), Ok(true));
    assert_eq!(field(&mut live), json!("Bob"));
    live.execute_as("edit.undo", json!({}), &agent).unwrap();
    assert_eq!(notes(&mut live, 2), Vec::<String>::new(), "the agent's comment is undone");
    assert_eq!(field(&mut live), json!("Bob"), "the other program's fill stays");
}

#[test]
fn an_unchanged_reread_keeps_everything() {
    let mut live = PdfDocument::open(NAME, &fixture()).unwrap();
    note(&mut live, 1, "mine");
    let saved = live.save(None).unwrap();
    live.mark_saved();
    let version = live.version().unwrap();
    let fresh = PdfDocument::open(NAME, &saved).unwrap();

    assert_eq!(live.reconcile_reread(&fresh), Ok(true));
    assert_eq!(live.version().unwrap(), version, "nothing was written");
    live.execute("edit.undo", json!({})).unwrap();
    assert_eq!(notes(&mut live, 1), Vec::<String>::new());
}

/// A program that writes the file anew numbers its objects again: no update to apply onto the
/// session, so the hook declines and the session is left exactly as it was (the host then
/// refuses the reread rather than drop the history).
#[test]
fn a_file_written_anew_is_not_reconciled() {
    let mut live = PdfDocument::open(NAME, &fixture()).unwrap();
    note(&mut live, 1, "mine");
    let saved = live.save(None).unwrap();
    live.mark_saved();
    let version = live.version().unwrap();
    let doc = kition_pdf_cos::Document::open(Arc::new(saved.clone())).unwrap();
    let rewritten = kition_pdf_cos::write_full(&doc, &kition_pdf_cos::SaveOptions::default()).unwrap();
    let fresh = PdfDocument::open(NAME, &rewritten).unwrap();

    assert_eq!(live.reconcile_reread(&fresh), Ok(false));
    assert_eq!(live.version().unwrap(), version);
    assert_eq!(live.save(None).unwrap(), saved, "the session still holds its own file");
    assert!(!live.dirty());
    live.execute("edit.undo", json!({})).unwrap();
    assert_eq!(notes(&mut live, 1), Vec::<String>::new());
}
