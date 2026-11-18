//! Repeated acknowledged saves keep one replica; unrelated appended data never matches it.

mod common;

use std::io::Write;
use std::sync::Arc;

use common::{add_note, comments, edit, first, fixture, peer};
use kdm_model::PdfSession;
use kition_pdf_cos::{Dict, Document, Object, PdfString, SaveOptions, Stream, generated_file_id, write_incremental};

fn original() -> Document {
    let mut doc = fixture(1);
    let opaque = doc.add(Object::Int(42));
    let root = doc.root().unwrap();
    doc.update_dict(root, |dict| dict.set(b"SyntheticOpaque".to_vec(), Object::Ref(opaque))).unwrap();
    doc.trailer_mut().set(b"SyntheticTrailer".to_vec(), Object::Int(17));
    // The fixture already has a standard base. Append a real revision so the
    // private trailer field is present without asking a full writer to guess it.
    let bytes = write_incremental(&doc, &SaveOptions::default()).unwrap();
    assert!(bytes.starts_with(doc.bytes()));
    assert!(bytes.len() > doc.bytes().len());
    let reopened = Document::open(Arc::new(bytes)).unwrap();
    assert_eq!(reopened.trailer().get(b"SyntheticTrailer"), Some(&Object::Int(17)));
    reopened
}

fn save(doc: &Document) -> Arc<Vec<u8>> {
    Arc::new(write_incremental(doc, &SaveOptions { mod_date: Some("D:20261022000000Z".into()), ..SaveOptions::default() }).unwrap())
}

fn saved_three_times() -> (PdfSession, Arc<Vec<u8>>) {
    let mut a = first(&original());
    let mut prior = a.document().bytes().clone();
    for count in 1..=3 {
        edit(&mut a, |doc| add_note(doc, 0, &format!("Original edit {count}")));
        let version = a.version();
        let saved = save(a.document());
        assert!(saved.starts_with(&prior));
        let saved_cos = Document::open(saved.clone()).unwrap();
        assert_eq!(saved_cos.trailer().get(b"SyntheticTrailer"), Some(&Object::Int(17)));
        let mut b = peer(&a);
        let numbering = b.holds(&saved).unwrap_or_else(|| panic!("save {count} must hold its replica"));
        b.adopt_file(Document::open(saved.clone()).unwrap(), &numbering);
        assert_eq!(b.document().trailer().get(b"SyntheticTrailer"), Some(&Object::Int(17)));
        assert_eq!(b.version(), version, "adopting a saved file adds no replica edit");
        assert_eq!(comments(b.document()).len(), count);
        assert_eq!(b.document().bytes().as_slice(), saved.as_slice());
        a.adopt_local(Document::open(saved.clone()).unwrap());
        assert_eq!(a.document().trailer().get(b"SyntheticTrailer"), Some(&Object::Int(17)));
        assert_eq!(a.version(), version);
        prior = saved;
    }
    (a, prior)
}

#[test]
fn consecutive_saved_revisions_reopen_without_rebase_and_keep_undo() {
    let (mut a, saved) = saved_three_times();
    assert!(Document::open(saved).unwrap().revisions().len() >= 4);
    assert!(a.undo().unwrap().0);
    assert_eq!(comments(a.document()).len(), 2);
    assert!(a.redo().unwrap().0);
    assert_eq!(comments(a.document()).len(), 3);
}

fn rejects_foreign_edit(change: impl FnOnce(&mut Document)) {
    let (a, saved) = saved_three_times();
    let b = peer(&a);
    assert!(b.holds(&saved).is_some());
    let mut foreign = Document::open(saved.clone()).unwrap();
    change(&mut foreign);
    let changed = save(&foreign);
    assert!(changed.starts_with(&saved));
    assert!(b.holds(&changed).is_none(), "unrelated appended data must be imported, never silently adopted");
}

#[test]
fn unrelated_appended_annotation_is_not_held() {
    rejects_foreign_edit(|doc| add_note(doc, 0, "Foreign annotation"));
}

#[test]
fn unrelated_appended_object_is_not_held() {
    rejects_foreign_edit(|doc| {
        doc.add(Object::Int(99));
    });
}

#[test]
fn unrelated_base_object_deletion_is_not_held() {
    rejects_foreign_edit(|doc| {
        let root = doc.get(doc.root().unwrap());
        let opaque = root.as_dict().unwrap().reference(b"SyntheticOpaque").unwrap();
        doc.free(opaque);
    });
}

#[test]
fn unrelated_base_object_replacement_is_not_held() {
    rejects_foreign_edit(|doc| {
        let root = doc.get(doc.root().unwrap());
        let opaque = root.as_dict().unwrap().reference(b"SyntheticOpaque").unwrap();
        doc.set(opaque, Object::Int(43));
    });
}

#[test]
fn unrelated_trailer_change_is_not_held() {
    rejects_foreign_edit(|doc| doc.trailer_mut().set(b"SyntheticTrailer".to_vec(), Object::Int(18)));
}

#[test]
fn unrelated_information_metadata_is_not_held() {
    rejects_foreign_edit(|doc| {
        let info = doc.trailer().reference(b"Info").unwrap();
        doc.update_dict(info, |dict| dict.set(b"SyntheticMetadata".to_vec(), Object::Bool(true))).unwrap();
    });
}

#[test]
fn unreferenced_fake_xref_stream_is_not_held() {
    rejects_foreign_edit(|doc| {
        let mut dict = Dict::new();
        dict.set(b"Type".to_vec(), Object::name("XRef"));
        doc.add(Object::Stream(Stream::from_raw(dict, b"Original unrelated stream".to_vec())));
    });
}

/// Original minimal PDF, intentionally without metadata that the writer normally creates.
fn without_metadata() -> Document {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 200] /Resources <<>> /Contents 4 0 R >>",
        "<< /Length 0 >>\nstream\n\nendstream",
    ];
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        writeln!(&mut bytes, "{} 0 obj\n{object}\nendobj", index + 1).unwrap();
    }
    let start = bytes.len();
    writeln!(&mut bytes, "xref\n0 5\n0000000000 65535 f ").unwrap();
    for offset in offsets {
        writeln!(&mut bytes, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(&mut bytes, "trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{start}\n%%EOF").unwrap();
    let doc = Document::open(Arc::new(bytes)).unwrap();
    assert_eq!(doc.revisions().len(), 1);
    assert!(!doc.trailer().contains(b"ID"));
    assert!(!doc.trailer().contains(b"Info"));
    doc
}

#[test]
fn generated_id_and_info_preserve_the_replica_on_every_bookmark_save() {
    let original = without_metadata();
    let expected_id = generated_file_id(original.bytes(), SaveOptions::default().id_seed);
    let mut a = first(&original);
    let mut prior = original.bytes().clone();
    for count in 1..=3 {
        edit(&mut a, |doc| {
            kition_pdf_organize::add_bookmark(doc, &[], count - 1, &format!("Original bookmark {count}"), 0).unwrap();
        });
        let version = a.version();
        let saved = save(a.document());
        assert!(saved.starts_with(&prior));
        let opened = Document::open(saved.clone()).unwrap();
        let ids = opened.trailer().get(b"ID").unwrap().as_array().unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.iter().all(|id| id.as_string().unwrap().bytes == expected_id));
        assert!(opened.trailer().contains(b"Info"));
        let mut b = peer(&a);
        let numbering = b.holds(&saved).unwrap_or_else(|| panic!("metadata-free bookmark save {count} must hold its replica"));
        b.adopt_file(opened.clone(), &numbering);
        assert_eq!(b.version(), version, "even the first reopen must not create a new epoch");
        assert_eq!(kition_pdf_organize::bookmarks(b.document()).len(), count);
        a.adopt_local(opened);
        prior = saved;
    }
}

fn foreign_id() -> Object {
    let id = Object::String(PdfString::literal(vec![17; 16]));
    Object::Array(vec![id.clone(), id])
}

#[test]
fn arbitrarily_added_id_is_not_a_generated_id() {
    let mut a = first(&without_metadata());
    edit(&mut a, |doc| add_note(doc, 0, "Original note"));
    let mut foreign = Document::open(save(a.document())).unwrap();
    foreign.trailer_mut().set(b"ID".to_vec(), foreign_id());
    assert!(peer(&a).holds(&save(&foreign)).is_none());
}

#[test]
fn modifying_an_existing_id_is_not_held() {
    rejects_foreign_edit(|doc| doc.trailer_mut().set(b"ID".to_vec(), foreign_id()));
}

#[test]
fn newly_generated_info_cannot_hide_unrelated_metadata() {
    let mut a = first(&without_metadata());
    edit(&mut a, |doc| add_note(doc, 0, "Original note"));
    let mut foreign = Document::open(save(a.document())).unwrap();
    let info = foreign.trailer().reference(b"Info").unwrap();
    foreign.update_dict(info, |dict| dict.set(b"SyntheticMetadata".to_vec(), Object::Bool(true))).unwrap();
    assert!(peer(&a).holds(&save(&foreign)).is_none());
}

fn metadata_control(existing_info: bool) -> Document {
    let raw = without_metadata().bytes().clone();
    let text = String::from_utf8(raw.as_ref().clone()).unwrap();
    let mut doc = Document::open(Arc::new(text.replace("/Root 1 0 R >>", "/Root 1 0 R /SyntheticTrailer 17 >>").into_bytes())).unwrap();
    if existing_info {
        kition_pdf_organize::set_info(&mut doc, "Title", "Original title").unwrap();
        doc = Document::open(Arc::new(write_incremental(&doc, &SaveOptions::default()).unwrap())).unwrap();
    }
    assert_eq!(doc.trailer().contains(b"Info"), existing_info);
    assert_eq!(doc.trailer().get(b"SyntheticTrailer"), Some(&Object::Int(17)));
    doc
}

fn title(doc: &Document) -> Option<String> {
    kition_pdf_organize::info(doc, "Title")
}

fn assert_saved_view(session: &PdfSession, expected: Option<&str>, remote_note: bool) {
    assert_eq!(title(session.document()).as_deref(), expected);
    let reopened = Document::open(Arc::new(write_incremental(session.document(), &SaveOptions::default()).unwrap())).unwrap();
    assert_eq!(title(&reopened).as_deref(), expected);
    assert_eq!(reopened.trailer().get(b"SyntheticTrailer"), Some(&Object::Int(17)));
    if remote_note {
        assert!(comments(&reopened).iter().any(|(_, text)| text == "Remote note after save"));
    }
}

#[test]
fn saved_generated_info_metadata_undo_redo_keeps_reachable_values() {
    for existing_info in [false, true] {
        for saves in [1, 3] {
            let mut a = first(&metadata_control(existing_info));
            let identity = a.doc_id();
            let epoch = a.model().ids().epoch().to_owned();
            for round in 0..saves {
                edit(&mut a, |doc| add_note(doc, 0, &format!("Before save {round}")));
                let version = a.version();
                a.adopt_local(Document::open(save(a.document())).unwrap());
                assert_eq!(a.version(), version);
            }
            let previous = title(a.document());
            assert_eq!(edit(&mut a, |doc| kition_pdf_organize::set_info(doc, "Title", "Title after save").unwrap()), 1);
            assert_eq!(a.last_written().keys.iter().any(|key| key == "trailer"), !existing_info);
            assert!(!a.last_written().rebased);
            assert_saved_view(&a, Some("Title after save"), false);
            assert!(a.undo().unwrap().0);
            assert_saved_view(&a, previous.as_deref(), false);
            assert!(a.redo().unwrap().0);
            assert_saved_view(&a, Some("Title after save"), false);
            assert_eq!(a.doc_id(), identity);
            assert_eq!(a.model().ids().epoch(), epoch);
        }
    }
}

#[test]
fn saved_generated_info_repeated_edits_do_not_rewrite_identical_trailer_overlay() {
    let mut a = first(&metadata_control(false));
    edit(&mut a, |doc| add_note(doc, 0, "Before save"));
    a.adopt_local(Document::open(save(a.document())).unwrap());
    edit(&mut a, |doc| kition_pdf_organize::set_info(doc, "Title", "First title").unwrap());
    assert!(a.last_written().keys.iter().any(|key| key == "trailer"));
    edit(&mut a, |doc| kition_pdf_organize::set_info(doc, "Title", "Second title").unwrap());
    assert!(!a.last_written().keys.iter().any(|key| key == "trailer"));
    assert!(a.undo().unwrap().0);
    assert_saved_view(&a, Some("First title"), false);
    assert!(a.redo().unwrap().0);
    assert_saved_view(&a, Some("Second title"), false);
}

#[test]
fn saved_untouched_info_does_not_add_trailer_write_for_annotation_edits() {
    for existing_info in [false, true] {
        let mut a = first(&metadata_control(existing_info));
        edit(&mut a, |doc| add_note(doc, 0, "Before save"));
        a.adopt_local(Document::open(save(a.document())).unwrap());
        edit(&mut a, |doc| add_note(doc, 0, "After save"));
        assert!(!a.last_written().keys.iter().any(|key| key == "trailer"));
    }
}

#[test]
fn saved_generated_info_exchanges_metadata_and_keeps_imported_annotation_on_undo() {
    for existing_info in [false, true] {
        let mut a = first(&metadata_control(existing_info));
        edit(&mut a, |doc| add_note(doc, 0, "Before save"));
        a.adopt_local(Document::open(save(a.document())).unwrap());
        let mut b = peer(&a);
        let previous = title(a.document());
        edit(&mut a, |doc| kition_pdf_organize::set_info(doc, "Title", "Shared title").unwrap());
        common::send(&a, &mut b);
        assert_saved_view(&b, Some("Shared title"), false);
        edit(&mut b, |doc| add_note(doc, 0, "Remote note after save"));
        common::send(&b, &mut a);
        assert_saved_view(&a, Some("Shared title"), true);
        assert!(a.undo().unwrap().0);
        assert_saved_view(&a, previous.as_deref(), true);
        common::send(&a, &mut b);
        assert_saved_view(&b, previous.as_deref(), true);
        assert!(a.redo().unwrap().0);
        assert_saved_view(&a, Some("Shared title"), true);
        common::send(&a, &mut b);
        assert_saved_view(&b, Some("Shared title"), true);
        common::assert_converged(&a, &b);
        assert_eq!(a.dropped(), 0);
        assert_eq!(b.dropped(), 0);
    }
}
