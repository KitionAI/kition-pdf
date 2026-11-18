//! A clean adopted physical view must not mistake its old logical base for the current file.

mod common;

use std::sync::Arc;

use kdm_model::PdfSession;
use kition_pdf_cos::{Dict, Document, Object, SaveOptions, write_full, write_incremental};

fn options() -> SaveOptions {
    SaveOptions { mod_date: Some("D:20261024000000Z".into()), ..SaveOptions::default() }
}

fn fixture() -> Document {
    // Contributor-original blank PDF. Existing Info and ID ensure the trailer does
    // not accidentally reject stale bytes before the physical-overlay check runs.
    let mut doc = kition_pdf_create::blank(240.0, 200.0, 1).unwrap();
    kition_pdf_organize::set_info(&mut doc, "Title", "Held pristine control").unwrap();
    let bytes = write_full(&doc, &options()).unwrap();
    let doc = Document::open(Arc::new(bytes)).unwrap();
    assert!(doc.trailer().contains(b"Info"));
    assert!(doc.trailer().contains(b"ID"));
    doc
}

fn semantic_trailer(doc: &Document) -> Dict {
    let mut trailer = doc.trailer().clone();
    for key in [b"Size".as_slice(), b"Prev", b"XRefStm"] {
        trailer.remove(key);
    }
    trailer
}

fn marker(doc: &Document) -> Option<Object> {
    let root = doc.trailer().reference(b"Root").unwrap();
    doc.try_get(root.num).unwrap().as_dict().unwrap().get(b"SyntheticBaseMarker").cloned()
}

fn edited() -> (PdfSession, Arc<Vec<u8>>) {
    let original = fixture();
    let source = original.bytes().clone();
    assert_eq!(marker(&original), None);
    let root = original.trailer().reference(b"Root").unwrap();
    let mut session = common::first(&original);
    assert_eq!(
        common::edit(&mut session, |doc| {
            let mut catalog = doc.try_get(root.num).unwrap().as_dict().unwrap().clone();
            catalog.set(b"SyntheticBaseMarker".to_vec(), Object::Int(73));
            doc.set(root, Object::Dict(catalog));
        }),
        1
    );
    assert_eq!(session.document().trailer().reference(b"Root"), Some(root));
    assert_eq!(semantic_trailer(session.document()), semantic_trailer(&original));
    assert_eq!(marker(session.document()), Some(Object::Int(73)));
    assert!(session.document().is_modified());
    (session, source)
}

fn adopted() -> (PdfSession, Arc<Vec<u8>>, Arc<Vec<u8>>) {
    let (mut session, source) = edited();
    let saved = Arc::new(write_incremental(session.document(), &options()).unwrap());
    assert!(saved.starts_with(&source));
    assert!(saved.len() > source.len());
    let original = Document::open(source.clone()).unwrap();
    let saved_doc = Document::open(saved.clone()).unwrap();
    assert_eq!(marker(&saved_doc), Some(Object::Int(73)));
    assert_eq!(marker(&original), None);
    assert_eq!(semantic_trailer(&saved_doc), semantic_trailer(&original));
    let version = session.version();
    let identity = session.doc_id();
    session.adopt_local(saved_doc);
    assert_eq!(session.version(), version);
    assert_eq!(session.doc_id(), identity);
    assert!(!session.document().is_modified());
    assert_eq!(session.document().bytes(), &saved);
    assert_eq!(session.model().pristine().bytes(), &source);
    assert_eq!(marker(session.document()), Some(Object::Int(73)));
    assert_eq!(semantic_trailer(session.document()), semantic_trailer(&original));
    (session, source, saved)
}

#[test]
fn clean_pristine_view_holds_its_original_file() {
    let original = fixture();
    let session = common::first(&original);
    assert!(session.holds(original.bytes()).is_some());
}

#[test]
fn edited_view_refuses_pristine_before_acknowledgement() {
    let (session, source) = edited();
    assert!(session.holds(&source).is_none());
}

#[test]
fn clean_adopted_physical_view_refuses_stale_pristine_file() {
    let (session, source, _) = adopted();
    assert!(session.holds(&source).is_none(), "an acknowledged clean physical view must reject stale original bytes with different base objects");
}

#[test]
fn clean_adopted_physical_view_holds_current_file() {
    let (session, _, saved) = adopted();
    assert!(session.holds(&saved).is_some(), "the exact acknowledged file must hold its physical view");
}
