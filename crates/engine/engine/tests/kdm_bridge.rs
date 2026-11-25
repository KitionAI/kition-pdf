//! The engine's command bridge to each document's KDM replica: one transaction per edit, undo
//! on the replica keeping another session's edits, saves that keep the replica's base, a full
//! rewrite as a rebase another session follows, and reopening with a stored replica. Two
//! sessions exchange their changes in one order.

use std::sync::Arc;

use kition_pdf_engine::{
    DocId, Edit, EditError, FieldValue, MemoryBlobs, NewAnnotation, NewField, NoteIcon, Origin, Session, Shape, Style, UndoScope, Version,
};

fn session() -> Session {
    Session::new().with_clock(|| 1_700_000_000)
}

/// A PDF of `pages` Letter pages with a text field `Name` on the first, made by the engine.
fn fixture(pages: usize) -> Arc<Vec<u8>> {
    let mut s = session();
    let blank = s.create_blank(612.0, 792.0, pages).unwrap_or_else(|e| panic!("blank: {e}"));
    let id = s.open("fixture.pdf", None, blank, None).unwrap_or_else(|e| panic!("open: {e}"));
    let field = Edit::AddField { page: 0, rect: [72.0, 600.0, 300.0, 624.0], kind: NewField::Text { multiline: false }, name: Some("Name".into()) };
    s.apply(id, field).unwrap_or_else(|e| panic!("field: {e}"));
    s.save_full_bytes(id).unwrap_or_else(|e| panic!("save: {e}"))
}

fn open(bytes: Arc<Vec<u8>>) -> (Session, DocId) {
    let mut s = session();
    let id = s.open("doc.pdf", None, bytes, None).unwrap_or_else(|e| panic!("open: {e}"));
    (s, id)
}

fn note(page: usize, text: &str) -> Edit {
    Edit::AddAnnotation(NewAnnotation {
        page,
        shape: Shape::Note { at: [100.0, 100.0], icon: NoteIcon::Comment },
        style: Style::default(),
        contents: text.into(),
        author: "Test".into(),
    })
}

fn fill(value: &str) -> Edit {
    Edit::SetFieldValue { name: "Name".into(), value: FieldValue::Text(value.into()) }
}

fn comments(s: &Session, id: DocId) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = s
        .get(id)
        .map(|d| {
            d.info
                .annotations
                .iter()
                .filter(|a| a.subtype != "Widget" && a.subtype != "Popup")
                .map(|a| (a.page, a.contents.clone().unwrap_or_default()))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

fn field(s: &Session, id: DocId) -> Option<String> {
    s.get(id).and_then(|d| d.form.iter().find(|f| f.name == "Name").map(|f| f.value.join("\n")))
}

fn pages(s: &Session, id: DocId) -> usize {
    s.get(id).map_or(0, |d| d.info.pages.len())
}

/// A second session opened from `a`'s replica (snapshot and blobs).
fn peer(a: &mut Session, id: DocId) -> (Session, DocId) {
    let snapshot = a.snapshot(id).unwrap_or_else(|e| panic!("snapshot: {e}"));
    let blobs = a.blobs(id).unwrap_or_else(|e| panic!("blobs: {e}"));
    let mut b = session();
    let bid = b.open_snapshot("peer.pdf", &snapshot, blobs, None).unwrap_or_else(|e| panic!("open_snapshot: {e}"));
    (b, bid)
}

fn send(from: &mut Session, fid: DocId, to: &mut Session, tid: DocId) {
    let since = to.version(tid).unwrap_or_else(|e| panic!("version: {e}"));
    let changes = from.export_changes(fid, &since).unwrap_or_else(|e| panic!("export: {e}"));
    let blobs = from.blobs_since(fid, &since).unwrap_or_else(|e| panic!("blobs: {e}"));
    to.add_blobs(tid, &blobs).unwrap_or_else(|e| panic!("add_blobs: {e}"));
    to.import_changes(tid, &changes).unwrap_or_else(|e| panic!("import: {e}"));
}

fn sync(a: &mut Session, aid: DocId, b: &mut Session, bid: DocId) {
    for _ in 0..4 {
        send(a, aid, b, bid);
        send(b, bid, a, aid);
        if a.version(aid).ok() == b.version(bid).ok() {
            return;
        }
    }
    panic!("the replicas did not converge");
}

fn assert_converged(a: &mut Session, aid: DocId, b: &mut Session, bid: DocId) {
    assert_eq!(a.state_hash(aid).ok(), b.state_hash(bid).ok(), "same replica state");
    assert_eq!(comments(a, aid), comments(b, bid), "same comments");
    assert_eq!(field(a, aid), field(b, bid), "same form values");
    assert_eq!(pages(a, aid), pages(b, bid), "same pages");
    for (s, id) in [(&*a, aid), (&*b, bid)] {
        let saved = s.save_bytes(id).unwrap_or_else(|e| panic!("save: {e}"));
        kition_pdf_cos::Document::open(saved).unwrap_or_else(|e| panic!("reopen: {e}"));
    }
}

#[test]
fn every_edit_is_one_transaction_and_one_undo_step() {
    let (mut s, id) = open(fixture(2));
    assert!(s.kdm(id).is_none(), "the replica is made lazily");
    s.apply(id, note(0, "one")).unwrap_or_else(|e| panic!("{e}"));
    let after_one = s.version(id).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(s.kdm(id).map(|k| k.steps()), Some((1, 0)));
    s.apply(id, fill("Ada")).unwrap_or_else(|e| panic!("{e}"));
    assert_ne!(s.version(id).ok(), Some(after_one.clone()));
    assert_eq!(s.kdm(id).map(|k| k.steps()), Some((2, 0)));
    s.apply(id, Edit::RotatePages { pages: vec![1], degrees: 90 }).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(s.kdm(id).map(|k| k.steps()), Some((3, 0)));
    s.undo(id).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(s.kdm(id).map(|k| k.steps()), Some((2, 1)));
    assert_eq!(field(&s, id).as_deref(), Some("Ada"));
    s.redo(id).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(s.kdm(id).map(|k| k.steps()), Some((3, 0)));
    assert_eq!(s.kdm_resets(id), 0);
}

#[test]
fn an_edit_as_an_agent_is_recorded_as_theirs_and_undoes_here() {
    let (mut s, id) = open(fixture(1));
    let agent = Origin::agent("agent:test");
    s.apply_as(id, note(0, "from the agent"), &agent).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(comments(&s, id), [(0, "from the agent".to_string())]);
    // KitModel M12a: the person's undo never takes back the agent's step; the agent's does.
    assert!(matches!(s.undo(id), Err(EditError::NothingToUndo)), "the person has no step of their own");
    assert_eq!(comments(&s, id), [(0, "from the agent".to_string())]);
    s.undo_as(id, &agent).unwrap_or_else(|e| panic!("{e}"));
    assert!(comments(&s, id).is_empty());
}

#[test]
fn the_persons_undo_keeps_the_agents_step_and_the_agents_undo_takes_it_back() {
    // KitModel 1.1 M12a and engine rule E14a: the person's and the agents' undo histories are
    // separate; undo and redo work in their scope, else in that of who runs them.
    let (mut s, id) = open(fixture(1));
    let agent = Origin::agent("agent:test");
    s.apply(id, note(0, "mine")).unwrap_or_else(|e| panic!("{e}"));
    s.apply_as(id, note(0, "theirs"), &agent).unwrap_or_else(|e| panic!("{e}"));
    let texts = |s: &Session| comments(s, id).into_iter().map(|(_, t)| t).collect::<Vec<_>>();
    s.undo(id).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(texts(&s), ["theirs"], "the person's undo takes back the person's comment and keeps the agent's later one");
    s.redo(id).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(texts(&s), ["mine", "theirs"]);
    s.undo_as(id, &agent).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(texts(&s), ["mine"], "the agent's undo takes back the agent's comment");
    s.redo_in(id, UndoScope::Agent).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(texts(&s), ["mine", "theirs"], "the person can redo the agent's step by naming its scope");
}

#[test]
fn an_agent_undo_under_the_persons_later_step_keeps_the_persons_edit() {
    let (mut s, id) = open(fixture(1));
    let agent = Origin::agent("agent:test");
    let texts = |s: &Session| comments(s, id).into_iter().map(|(_, t)| t).collect::<Vec<_>>();
    s.apply_as(id, note(0, "theirs"), &agent).unwrap_or_else(|e| panic!("{e}"));
    s.apply(id, note(0, "mine")).unwrap_or_else(|e| panic!("{e}"));
    s.undo_as(id, &agent).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(texts(&s), ["mine"], "only the agent's comment goes; the person's later one stays");
    s.undo(id).unwrap_or_else(|e| panic!("{e}"));
    assert!(texts(&s).is_empty(), "then the person's undo takes back the person's comment");
    s.redo_as(id, &agent).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(texts(&s), ["theirs"], "and the agent's redo brings the agent's comment back");
}

#[test]
fn a_comment_and_a_form_fill_reach_the_other_session() {
    let (mut a, aid) = open(fixture(2));
    let (mut b, bid) = peer(&mut a, aid);
    a.apply(aid, note(1, "from a")).unwrap_or_else(|e| panic!("{e}"));
    sync(&mut a, aid, &mut b, bid);
    b.apply(bid, fill("Bob")).unwrap_or_else(|e| panic!("{e}"));
    sync(&mut a, aid, &mut b, bid);
    assert_converged(&mut a, aid, &mut b, bid);
    assert_eq!(comments(&b, bid), [(1, "from a".to_string())]);
    assert_eq!(field(&a, aid).as_deref(), Some("Bob"));
    assert!(b.get(bid).is_some_and(|d| d.dirty), "a collaborator's change is unsaved here");
}

#[test]
fn undo_after_a_remote_edit_keeps_the_remote_edit() {
    let (mut a, aid) = open(fixture(2));
    let (mut b, bid) = peer(&mut a, aid);
    a.apply(aid, note(0, "mine")).unwrap_or_else(|e| panic!("{e}"));
    sync(&mut a, aid, &mut b, bid);
    b.apply(bid, note(1, "theirs")).unwrap_or_else(|e| panic!("{e}"));
    sync(&mut a, aid, &mut b, bid);
    a.undo(aid).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(comments(&a, aid), [(1, "theirs".to_string())], "the undo takes back only mine");
    sync(&mut a, aid, &mut b, bid);
    assert_converged(&mut a, aid, &mut b, bid);
}

#[test]
fn a_full_rewrite_is_a_rebase_the_other_session_follows_and_its_undo_brings_the_old_base_back() {
    let (mut a, aid) = open(fixture(2));
    let (mut b, bid) = peer(&mut a, aid);
    a.apply(aid, Edit::Sanitize).unwrap_or_else(|e| panic!("{e}"));
    assert!(a.kdm(aid).is_some_and(|k| k.last_written().rebased), "sanitizing rebases the replica");
    sync(&mut a, aid, &mut b, bid);
    assert_converged(&mut a, aid, &mut b, bid);
    assert_eq!(a.kdm_dropped(aid) + b.kdm_dropped(bid), 0, "nothing was written against the old base");
    // The undo of the rewrite brings the old base back, and both follow.
    a.undo(aid).unwrap_or_else(|e| panic!("{e}"));
    sync(&mut a, aid, &mut b, bid);
    assert_converged(&mut a, aid, &mut b, bid);
    assert_eq!(field(&a, aid).as_deref(), Some(""));
}

/// The one way edits of another base still reach the replica: the person undoes a full rewrite
/// (sanitizing) under an agent's later comment. The undo brings the old base back and keeps the
/// agent's entries, written against the rewritten one, so the repair deletes them: the agent's
/// comment goes with the rewrite, and the document counts every entry it wrote
/// (`kdm_dropped`, which the editor shows).
#[test]
fn the_persons_undo_of_a_full_rewrite_drops_the_agents_later_comment_and_counts_it() {
    let (mut s, id) = open(fixture(2));
    let agent = Origin::agent("agent:test");
    s.apply(id, note(0, "before")).unwrap_or_else(|e| panic!("{e}"));
    s.apply(id, Edit::Sanitize).unwrap_or_else(|e| panic!("{e}"));
    assert!(comments(&s, id).is_empty(), "sanitizing removes the comments");
    s.apply_as(id, note(1, "the agent's"), &agent).unwrap_or_else(|e| panic!("{e}"));
    let written = s.kdm(id).map_or(0, |k| k.last_written().keys.len());
    assert!(written > 0, "the agent's comment wrote overlay entries");
    assert_eq!(s.kdm_dropped(id), 0);
    s.undo(id).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(comments(&s, id), [(0, "before".to_string())], "the old base and the person's comment are back; the agent's comment is gone");
    assert_eq!(s.kdm_dropped(id), written as u64, "every entry the agent's comment wrote was dropped and counted");
    assert_eq!(s.kdm_resets(id), 0);
}

#[test]
fn saving_keeps_the_replica_and_the_next_save_appends() {
    let (mut s, id) = open(fixture(2));
    s.apply(id, note(0, "saved")).unwrap_or_else(|e| panic!("{e}"));
    let version = s.version(id).unwrap_or_else(|e| panic!("{e}"));
    let saved = s.save_bytes(id).unwrap_or_else(|e| panic!("{e}"));
    s.mark_saved(id, saved.clone(), None).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(s.version(id).ok(), Some(version), "an incremental save writes nothing to the replica");
    s.apply(id, fill("after")).unwrap_or_else(|e| panic!("{e}"));
    let again = s.save_bytes(id).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(&again[..saved.len()], saved.as_slice(), "the next save appends");
    // Undo past the save still works, on the replica.
    s.undo(id).unwrap_or_else(|e| panic!("{e}"));
    s.undo(id).unwrap_or_else(|e| panic!("{e}"));
    assert!(comments(&s, id).is_empty());
    assert!(s.get(id).is_some_and(|d| d.dirty));
}

#[test]
fn a_saved_file_reopens_with_its_replica_and_a_changed_file_wins() {
    let (mut a, aid) = open(fixture(2));
    a.apply(aid, note(0, "kept")).unwrap_or_else(|e| panic!("{e}"));
    let saved = a.save_bytes(aid).unwrap_or_else(|e| panic!("{e}"));
    let snapshot = a.snapshot(aid).unwrap_or_else(|e| panic!("{e}"));
    let blobs = a.blobs(aid).unwrap_or_else(|e| panic!("{e}"));
    a.mark_saved(aid, saved.clone(), None).unwrap_or_else(|e| panic!("{e}"));
    let version = a.version(aid).unwrap_or_else(|e| panic!("{e}"));

    let mut b = session();
    let bid = b.open_with_replica("doc.pdf", &saved, &snapshot, blobs.clone(), None).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(b.version(bid).ok(), Some(version.clone()), "the same replica");
    assert_eq!(b.doc_id(bid), a.doc_id(aid));
    assert!(!b.get(bid).is_some_and(|d| d.dirty), "it opens clean");
    assert_eq!(comments(&b, bid), [(0, "kept".to_string())]);
    b.apply(bid, fill("next")).unwrap_or_else(|e| panic!("{e}"));
    let next = b.save_bytes(bid).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(&next[..saved.len()], saved.as_slice(), "saving after reopening appends to the file");

    // A file changed elsewhere wins: recorded onto the replica, which moves on.
    let (mut other, oid) = open(saved.clone());
    other.apply(oid, note(1, "elsewhere")).unwrap_or_else(|e| panic!("{e}"));
    let changed = other.save_bytes(oid).unwrap_or_else(|e| panic!("{e}"));
    let mut c = session();
    let cid = c.open_with_replica("doc.pdf", &changed, &snapshot, blobs.clone(), None).unwrap_or_else(|e| panic!("{e}"));
    assert_ne!(c.version(cid).ok(), Some(version));
    assert_eq!(comments(&c, cid).len(), 2);
    assert!(!c.get(cid).is_some_and(|d| d.dirty));

    // No file yet: the replica alone.
    let mut d = session();
    let did = d.open_with_replica("shared.pdf", &[], &snapshot, blobs, None).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(comments(&d, did), [(0, "kept".to_string())]);
}

#[test]
fn an_original_object_at_an_appended_offset_reopens_without_a_replica_transaction() {
    use kition_pdf_cos::{Document, SaveOptions, XrefEntry, write_full, write_incremental};

    // Contributor-original base fixture. Full writing only creates its initial stable bytes;
    // the relocation under test is an incremental update, not a full-save repair.
    let options = SaveOptions { mod_date: Some("D:20261024000000Z".into()), object_streams: false, ..SaveOptions::default() };
    let blank = kition_pdf_create::blank(240.0, 200.0, 1).unwrap();
    let original_bytes = Arc::new(write_full(&blank, &options).unwrap());
    let original = Document::open(original_bytes.clone()).unwrap();
    assert!(original.trailer().contains(b"Info"));
    assert!(original.trailer().contains(b"ID"));
    let root = original.root().unwrap();
    let catalog = original.try_get(root.num).unwrap();
    assert!(matches!(original.xref_entry(root.num), Some(XrefEntry::InFile { .. })));

    let (mut a, aid) = open(original_bytes.clone());
    // version() creates the replica before identity/state are captured.
    let version = a.version(aid).unwrap();
    assert_ne!(version, Version::empty());
    let identity = a.doc_id(aid).unwrap();
    let state = a.state_hash(aid).unwrap();
    let snapshot = a.snapshot(aid).unwrap();
    let blobs = a.blobs(aid).unwrap();
    assert_eq!(a.kdm(aid).unwrap().document().bytes(), &original_bytes);

    // Re-serialize the exact original catalog at a new uncompressed offset. Do not
    // record this physical write as a logical engine edit before testing reopen.
    let mut relocated = original.clone();
    relocated.set(root, (*catalog).clone());
    let restored = Arc::new(write_incremental(&relocated, &options).unwrap());
    let restored_cos = Document::open(restored.clone()).unwrap();
    assert!(restored.starts_with(original_bytes.as_slice()));
    assert!(restored_cos.revisions().len() > original.revisions().len());
    assert_eq!(restored_cos.try_get(root.num).unwrap(), catalog);
    assert_eq!(restored_cos.generation(root.num), original.generation(root.num));
    assert_ne!(restored_cos.xref_entry(root.num), original.xref_entry(root.num));
    assert!(matches!(
        restored_cos.xref_entry(root.num),
        Some(XrefEntry::InFile { offset, generation })
            if offset >= original_bytes.len() as u64 && generation == root.generation
    ));

    let mut b = session();
    let bid = b.open_with_replica("restored.pdf", &restored, &snapshot, blobs, None).unwrap();
    // A system import can leave the undo stack empty while still adding a transaction.
    // Unchanged version and replicated-state hash are therefore essential assertions.
    assert_eq!(b.version(bid).unwrap(), version, "physical restoration adds no import transaction");
    assert_eq!(b.doc_id(bid).as_deref(), Some(identity.as_str()));
    assert_eq!(b.state_hash(bid).unwrap(), state);
    assert_eq!(b.kdm(bid).unwrap().document().bytes(), &restored, "the exact physical file is adopted");
    let doc = b.get(bid).unwrap();
    assert_eq!(doc.bytes, restored);
    assert!(!doc.dirty);
    assert!(doc.can_undo().is_none(), "a new actor starts with empty local history");
    assert_eq!(b.kdm(bid).unwrap().steps(), (0, 0));
    assert_eq!(b.kdm_resets(bid), 0);
    assert_eq!(b.kdm_dropped(bid), 0);
}

#[test]
fn reverting_writes_the_difference_and_clears_the_history() {
    let (mut a, aid) = open(fixture(1));
    let (mut b, bid) = peer(&mut a, aid);
    a.apply(aid, note(0, "to revert")).unwrap_or_else(|e| panic!("{e}"));
    a.revert(aid).unwrap_or_else(|e| panic!("{e}"));
    assert!(a.get(aid).is_some_and(|d| d.can_undo().is_none()));
    assert_eq!(a.kdm(aid).map(|k| k.steps()), Some((0, 0)));
    sync(&mut a, aid, &mut b, bid);
    assert!(comments(&b, bid).is_empty());
    assert_converged(&mut a, aid, &mut b, bid);
}

#[test]
fn view_toggles_are_this_persons_own() {
    let (mut a, aid) = open(fixture(1));
    let (mut b, bid) = peer(&mut a, aid);
    let version = a.version(aid).unwrap_or_else(|e| panic!("{e}"));
    a.set_hide_comments(aid, true);
    assert_eq!(a.version(aid).ok(), Some(version));
    a.apply(aid, note(0, "visible to b")).unwrap_or_else(|e| panic!("{e}"));
    sync(&mut a, aid, &mut b, bid);
    assert!(a.get(aid).is_some_and(|d| d.comments_hidden()), "the toggle survives an exchange");
    assert!(!b.get(bid).is_some_and(|d| d.comments_hidden()));
}

#[test]
fn a_replica_is_made_on_request_and_a_missing_document_has_none() {
    let mut s = session();
    let blank = s.create_blank(612.0, 792.0, 1).unwrap_or_else(|e| panic!("{e}"));
    let id = s.open("x.pdf", None, blank, None).unwrap_or_else(|e| panic!("{e}"));
    assert!(s.version(id).is_ok());
    assert!(matches!(s.version(DocId(999)), Err(EditError::NoDocument)));
}

#[test]
fn blobs_since_ships_only_what_a_live_change_needs() {
    let (mut a, aid) = open(fixture(1));
    let (mut b, bid) = peer(&mut a, aid);
    let since = b.version(bid).unwrap_or_else(|e| panic!("{e}"));
    a.apply(aid, note(0, "small")).unwrap_or_else(|e| panic!("{e}"));
    let blobs: MemoryBlobs = a.blobs_since(aid, &since).unwrap_or_else(|e| panic!("{e}"));
    let base = a.get(aid).map_or(0, |d| d.bytes.len());
    assert!(blobs.0.values().all(|bytes| bytes.len() < base), "the base is not shipped again");
    let all = a.blobs_since(aid, &Version::empty()).unwrap_or_else(|e| panic!("{e}"));
    assert!(!all.0.is_empty());
    send(&mut a, aid, &mut b, bid);
    assert_eq!(comments(&b, bid), [(0, "small".to_string())]);
}
