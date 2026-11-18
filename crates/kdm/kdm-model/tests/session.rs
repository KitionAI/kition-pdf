//! `PdfSession` itself: undo and redo, blobs shipped with live changes, history trimming, and a
//! saved file recognised as the replica's view.

mod common;

use std::sync::Arc;

use common::{add_note, comments, edit, first, fixture, peer, sync};
use kdm_model::PdfSession;
use kition_kit::kdm::Version;
use kition_kit::session::MemoryBlobs;
use kition_pdf_cos::{Document, SaveOptions, write_incremental};

#[test]
fn undo_and_redo_step_the_replica() {
    let mut a = first(&fixture(2));
    edit(&mut a, |doc| add_note(doc, 0, "one"));
    edit(&mut a, |doc| add_note(doc, 1, "two"));
    assert_eq!(a.steps(), (2, 0));
    assert!(a.undo().unwrap_or_else(|e| panic!("undo: {e}")).0);
    assert_eq!(comments(a.document()), [(0, "one".to_string())]);
    assert!(a.redo().unwrap_or_else(|e| panic!("redo: {e}")).0);
    assert_eq!(comments(a.document()).len(), 2);
    a.retain_steps(1, 0);
    assert_eq!(a.steps(), (1, 0));
}

#[test]
fn a_peer_gets_the_base_and_the_stream_data_it_needs() {
    let mut a = first(&fixture(1));
    // From nothing: everything the current state references, the base among it.
    let all = a.blobs_since(&Version::empty());
    assert!(all.iter().any(|(_, bytes)| bytes.len() == a.document().bytes().len()), "the base is shipped");
    let snapshot = a.snapshot().unwrap_or_else(|e| panic!("snapshot: {e}"));
    let mut b = PdfSession::open_snapshot(&snapshot, MemoryBlobs(all.into_iter().collect()), None).unwrap_or_else(|e| panic!("open: {e}"));
    let since = b.version();
    edit(&mut a, |doc| add_note(doc, 0, "live"));
    let needed = a.blobs_since(&since);
    assert!(needed.iter().all(|(_, bytes)| bytes.len() != a.document().bytes().len()), "a live change does not ship the base again");
    b.add_blobs(&MemoryBlobs(needed.into_iter().collect()));
    b.import_changes(&a.export_changes(&since).unwrap_or_else(|e| panic!("export: {e}"))).unwrap_or_else(|e| panic!("import: {e}"));
    assert_eq!(comments(b.document()), [(0, "live".to_string())]);
}

#[test]
fn a_file_saved_from_the_view_is_held_by_the_replica() {
    let mut a = first(&fixture(2));
    edit(&mut a, |doc| add_note(doc, 0, "saved"));
    edit(&mut a, |doc| common::fill(doc, "Ada"));
    let saved = Arc::new(
        write_incremental(a.document(), &SaveOptions { mod_date: Some("D:20261017000000Z".into()), ..SaveOptions::default() })
            .unwrap_or_else(|e| panic!("save: {e}")),
    );
    // A peer that never saw the file (another session's numbering) recognises it.
    let b = peer(&a);
    let numbering = b.holds(&saved).unwrap_or_else(|| panic!("the saved file holds the view"));
    let mut b = b;
    let file = Document::open(saved.clone()).unwrap_or_else(|e| panic!("open: {e}"));
    b.adopt_file(file, &numbering);
    assert_eq!(comments(b.document()), [(0, "saved".to_string())]);
    // The next edit diffs against the file and the next save appends to it.
    edit(&mut b, |doc| add_note(doc, 1, "after"));
    let again = write_incremental(b.document(), &SaveOptions::default()).unwrap_or_else(|e| panic!("save: {e}"));
    assert_eq!(&again[..saved.len()], saved.as_slice());
    sync(&mut a, &mut b);
    assert_eq!(comments(a.document()).len(), 2);
    assert_eq!(comments(b.document()).len(), 2);
}

#[test]
fn a_file_changed_elsewhere_is_not_held() {
    let mut a = first(&fixture(1));
    edit(&mut a, |doc| add_note(doc, 0, "mine"));
    let mut other = a.document().clone();
    add_note(&mut other, 0, "made elsewhere");
    let changed = Arc::new(write_incremental(&other, &SaveOptions::default()).unwrap_or_else(|e| panic!("save: {e}")));
    assert!(a.holds(&changed).is_none());
    assert!(a.holds(&Arc::new(b"not a pdf".to_vec())).is_none());
    // The base itself is held only by a view with no edits.
    let base = a.model().pristine().bytes().clone();
    assert!(a.holds(&base).is_none());
    let fresh = fixture(1);
    assert!(first(&fresh).holds(fresh.bytes()).is_some(), "an unedited replica holds its own file");
}

#[test]
fn adopting_a_saved_file_locally_keeps_undo() {
    let mut a = first(&fixture(2));
    edit(&mut a, |doc| add_note(doc, 0, "before saving"));
    let saved = Arc::new(write_incremental(a.document(), &SaveOptions::default()).unwrap_or_else(|e| panic!("save: {e}")));
    a.adopt_local(Document::open(saved).unwrap_or_else(|e| panic!("open: {e}")));
    let version = a.version();
    assert_eq!(a.version(), version, "saving writes nothing to the replica");
    assert!(a.undo().unwrap_or_else(|e| panic!("undo: {e}")).0, "an undo past the save");
    assert!(comments(a.document()).is_empty());
}
