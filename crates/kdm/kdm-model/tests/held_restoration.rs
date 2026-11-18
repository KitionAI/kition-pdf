//! Exact original values may be serialized again without a logical replica edit.
//!
//! Construct the physical revisions directly with COS; no acknowledged-base projection
//! implementation or altered history behavior is needed to exercise the matcher.

use std::io::Write;
use std::sync::Arc;

use kdm_model::PdfSession;
use kition_pdf_cos::{Document, ObjRef, Object, PdfString, SaveOptions, XrefEntry, parse_indirect, serialize, write_full, write_incremental};

fn options(object_streams: bool) -> SaveOptions {
    SaveOptions { mod_date: Some("D:20261024000000Z".into()), object_streams, ..SaveOptions::default() }
}

fn save(doc: &Document) -> Arc<Vec<u8>> {
    Arc::new(write_incremental(doc, &options(false)).unwrap())
}

fn fixture(object_streams: bool) -> Document {
    // Contributor-original blank document, with stable Info and ID already in its base.
    let doc = kition_pdf_create::blank(240.0, 200.0, 1).unwrap();
    let mut doc = Document::open(Arc::new(write_full(&doc, &options(object_streams)).unwrap())).unwrap();
    let opaque = doc.add(Object::Int(42));
    let null = doc.add(Object::Null);
    doc.trailer_mut().set(b"SyntheticOpaque".to_vec(), Object::Ref(opaque));
    doc.trailer_mut().set(b"SyntheticNull".to_vec(), Object::Ref(null));
    let doc = Document::open(save(&doc)).unwrap();
    assert!(doc.trailer().contains(b"Info"));
    assert!(doc.trailer().contains(b"ID"));
    doc
}

fn restored(original: &Document) -> Arc<Vec<u8>> {
    let root = original.root().unwrap();
    let catalog = original.try_get(root.num).unwrap();
    let mut rewritten = original.clone();
    rewritten.set(root, (*catalog).clone());
    let bytes = save(&rewritten);
    let reopened = Document::open(bytes.clone()).unwrap();
    assert!(bytes.starts_with(original.bytes()));
    assert!(reopened.revisions().len() > original.revisions().len());
    assert_eq!(reopened.try_get(root.num).unwrap(), catalog);
    assert_eq!(reopened.generation(root.num), original.generation(root.num));
    assert_ne!(reopened.xref_entry(root.num), original.xref_entry(root.num));
    assert!(matches!(reopened.xref_entry(root.num), Some(XrefEntry::InFile { offset, .. }) if offset >= original.bytes().len() as u64));
    bytes
}

#[test]
fn unchanged_catalog_at_appended_offset_holds_and_adopts_without_replica_edit() {
    let original = fixture(false);
    let mut session = PdfSession::create(&original, None).unwrap();
    let version = session.version();
    let identity = session.doc_id();
    let state = session.state_hash().unwrap();
    let bytes = restored(&original);
    let numbering = session.holds(&bytes).expect("an exact original catalog at a new offset must still hold the clean replica");
    assert!(numbering.is_empty());
    session.adopt_file(Document::open(bytes.clone()).unwrap(), &numbering);
    assert_eq!(session.document().bytes(), &bytes);
    assert_eq!(session.version(), version);
    assert_eq!(session.doc_id(), identity);
    assert_eq!(session.state_hash().unwrap(), state);
}

#[test]
fn original_catalog_restored_after_an_intermediate_change_is_held() {
    let original = fixture(false);
    let session = PdfSession::create(&original, None).unwrap();
    let root = original.root().unwrap();
    let mut changed = original.clone();
    changed.update_dict(root, |dict| dict.set(b"SyntheticMarker".to_vec(), Object::Int(73))).unwrap();
    let changed = save(&changed);
    assert!(session.holds(&changed).is_none());
    let mut restored = Document::open(changed.clone()).unwrap();
    restored.set(root, (*original.try_get(root.num).unwrap()).clone());
    let restored = save(&restored);
    assert!(restored.starts_with(&changed));
    let reopened = Document::open(restored.clone()).unwrap();
    assert_eq!(reopened.try_get(root.num).unwrap(), original.try_get(root.num).unwrap());
    assert_eq!(reopened.generation(root.num), root.generation);
    assert_ne!(reopened.xref_entry(root.num), original.xref_entry(root.num));
    assert!(session.holds(&restored).is_some());
}

#[test]
fn restored_nonzero_generation_is_held() {
    let mut original = fixture(false);
    let root = original.root().unwrap();
    let catalog = original.try_get(root.num).unwrap();
    let root = ObjRef::new(root.num, 7);
    original.set(root, (*catalog).clone());
    original.trailer_mut().set(b"Root".to_vec(), Object::Ref(root));
    let original = Document::open(save(&original)).unwrap();
    assert_eq!(original.generation(root.num), 7);
    let session = PdfSession::create(&original, None).unwrap();
    assert!(session.holds(&restored(&original)).is_some());
}

fn refuses_after_restoration(change: impl FnOnce(&mut Document)) {
    let original = fixture(false);
    let session = PdfSession::create(&original, None).unwrap();
    let bytes = restored(&original);
    let mut changed = Document::open(bytes.clone()).unwrap();
    change(&mut changed);
    let changed = save(&changed);
    assert!(changed.starts_with(&bytes));
    assert!(session.holds(&changed).is_none(), "restoration must not hide unrelated changes");
}

#[test]
fn changed_catalog_value_is_refused() {
    refuses_after_restoration(|doc| {
        doc.update_dict(doc.root().unwrap(), |dict| dict.set(b"SyntheticMarker".to_vec(), Object::Int(73))).unwrap();
    });
}

#[test]
fn changed_catalog_generation_with_equal_value_is_refused() {
    refuses_after_restoration(|doc| {
        let root = doc.root().unwrap();
        let catalog = doc.try_get(root.num).unwrap();
        doc.set(ObjRef::new(root.num, root.generation + 1), (*catalog).clone());
    });
}

#[test]
fn unrelated_original_object_deletion_is_refused() {
    refuses_after_restoration(|doc| doc.free(doc.trailer().reference(b"SyntheticOpaque").unwrap()));
}

#[test]
fn live_null_replaced_by_free_entry_is_refused() {
    refuses_after_restoration(|doc| {
        let reference = doc.trailer().reference(b"SyntheticNull").unwrap();
        assert_eq!(*doc.try_get(reference.num).unwrap(), Object::Null);
        assert!(matches!(doc.xref_entry(reference.num), Some(XrefEntry::InFile { .. })));
        doc.free(reference);
    });
}

#[test]
fn unrelated_new_object_is_refused() {
    refuses_after_restoration(|doc| {
        doc.add(Object::Int(99));
    });
}

#[test]
fn changed_unknown_trailer_value_is_refused() {
    refuses_after_restoration(|doc| doc.trailer_mut().set(b"SyntheticOpaque".to_vec(), Object::Int(42)));
}

#[test]
fn changed_unknown_trailer_reference_target_is_refused() {
    refuses_after_restoration(|doc| doc.set(doc.trailer().reference(b"SyntheticOpaque").unwrap(), Object::Int(43)));
}

#[test]
fn changed_existing_identifier_is_refused() {
    refuses_after_restoration(|doc| {
        let id = Object::String(PdfString::literal(b"synthetic-new-id".to_vec()));
        doc.trailer_mut().set(b"ID".to_vec(), Object::Array(vec![id.clone(), id]));
    });
}

#[test]
fn changed_information_metadata_is_refused() {
    refuses_after_restoration(|doc| {
        let info = doc.trailer().reference(b"Info").unwrap();
        doc.update_dict(info, |dict| dict.set(b"SyntheticMetadata".to_vec(), Object::Bool(true))).unwrap();
    });
}

#[test]
fn compressed_catalog_relocation_remains_refused() {
    let original = fixture(true);
    assert!(matches!(original.xref_entry(original.root().unwrap().num), Some(XrefEntry::InStream { .. })));
    let session = PdfSession::create(&original, None).unwrap();
    assert!(session.holds(&restored(&original)).is_none());
}

#[test]
fn compressed_container_relocation_remains_refused() {
    let original = fixture(true);
    let root = original.root().unwrap();
    let Some(XrefEntry::InStream { stream, .. }) = original.xref_entry(root.num) else { panic!("the catalog must be compressed") };
    let session = PdfSession::create(&original, None).unwrap();
    let mut changed = original.clone();
    changed.set(ObjRef::new(stream, original.generation(stream)), (*original.try_get(stream).unwrap()).clone());
    let bytes = save(&changed);
    let reopened = Document::open(bytes.clone()).unwrap();
    assert_eq!(reopened.xref_entry(root.num), original.xref_entry(root.num));
    assert_ne!(reopened.xref_entry(stream), original.xref_entry(stream));
    assert_eq!(reopened.try_get(root.num).unwrap(), original.try_get(root.num).unwrap());
    assert!(session.holds(&bytes).is_none());
}

#[test]
fn malformed_appended_original_object_is_refused() {
    let original = fixture(false);
    let session = PdfSession::create(&original, None).unwrap();
    let reference = original.trailer().reference(b"SyntheticOpaque").unwrap();
    let mut appended = Document::open(restored(&original)).unwrap();
    appended.set(reference, (*original.try_get(reference.num).unwrap()).clone());
    let bytes = save(&appended);
    let reopened = Document::open(bytes.clone()).unwrap();
    let Some(XrefEntry::InFile { offset, generation }) = reopened.xref_entry(reference.num) else {
        panic!("the restored object must be uncompressed")
    };
    assert_eq!(generation, reference.generation);
    assert!(offset >= original.bytes().len() as u64);
    let offset = usize::try_from(offset).unwrap();
    let mut bytes = bytes.as_ref().clone();
    let header = format!("{} {generation} obj\n", reference.num);
    assert!(bytes[offset..].starts_with(header.as_bytes()));
    bytes[offset + header.len()] = b']';
    let bytes = Arc::new(bytes);
    let reopened = Document::open(bytes.clone()).unwrap();
    assert_eq!(reopened.revisions().len(), appended.revisions().len() + 1);
    assert!(parse_indirect(&bytes, offset, &|_| None).is_err(), "the appended object itself must be malformed");
    assert_eq!(reopened.try_get(reference.num).unwrap(), original.try_get(reference.num).unwrap(), "ordinary reading recovers the original object");
    assert!(session.holds(&bytes).is_none());
}

#[test]
fn redirect_to_older_offset_inside_original_prefix_is_refused() {
    let earlier = fixture(false);
    let root = earlier.root().unwrap();
    let Some(XrefEntry::InFile { offset, generation }) = earlier.xref_entry(root.num) else { panic!("the original catalog must be uncompressed") };
    let original = Document::open(restored(&earlier)).unwrap();
    let session = PdfSession::create(&original, None).unwrap();
    let mut bytes = original.bytes().as_ref().clone();
    let xref_offset = bytes.len();
    assert!(offset < xref_offset as u64);
    writeln!(&mut bytes, "xref\n{} 1\n{offset:010} {generation:05} n ", root.num).unwrap();
    let mut trailer = original.trailer().clone();
    trailer.set(b"Prev".to_vec(), Object::Int(i64::try_from(original.revisions().last().unwrap().xref_offset).unwrap()));
    writeln!(&mut bytes, "trailer").unwrap();
    serialize(&Object::Dict(trailer), &mut bytes);
    writeln!(&mut bytes, "\nstartxref\n{xref_offset}\n%%EOF").unwrap();
    let bytes = Arc::new(bytes);
    let reopened = Document::open(bytes.clone()).unwrap();
    assert_eq!(reopened.xref_entry(root.num), earlier.xref_entry(root.num));
    assert_ne!(reopened.xref_entry(root.num), original.xref_entry(root.num));
    assert_eq!(reopened.try_get(root.num).unwrap(), original.try_get(root.num).unwrap());
    assert!(session.holds(&bytes).is_none());
}

fn mismatched_appended_header(header: &str) {
    let original = fixture(false);
    let reference = original.trailer().reference(b"SyntheticOpaque").unwrap();
    let session = PdfSession::create(&original, None).unwrap();
    let mut bytes = original.bytes().as_ref().clone();
    let offset = bytes.len();
    let header = header.replace("NUMBER", &reference.num.to_string());
    writeln!(&mut bytes, "{header} obj\n42\nendobj").unwrap();
    let xref = bytes.len();
    writeln!(&mut bytes, "xref\n{} 1\n{offset:010} {:05} n ", reference.num, reference.generation).unwrap();
    let mut trailer = original.trailer().clone();
    trailer.set(b"Prev".to_vec(), Object::Int(i64::try_from(original.revisions().last().unwrap().xref_offset).unwrap()));
    writeln!(&mut bytes, "trailer").unwrap();
    serialize(&Object::Dict(trailer), &mut bytes);
    writeln!(&mut bytes, "\nstartxref\n{xref}\n%%EOF").unwrap();
    let bytes = Arc::new(bytes);
    let reopened = Document::open(bytes.clone()).unwrap();
    assert_eq!(reopened.revisions().len(), original.revisions().len() + 1);
    assert!(
        matches!(reopened.xref_entry(reference.num), Some(XrefEntry::InFile { offset: actual, generation }) if actual == offset as u64 && generation == reference.generation)
    );
    assert_eq!(*reopened.try_get(reference.num).unwrap(), Object::Int(42), "lenient lookup alone cannot establish exact header identity");
    assert!(reopened.try_get_original_at_xref(reference).is_err());
    assert!(session.holds(&bytes).is_none());
}

#[test]
fn appended_original_with_wrong_header_number_is_refused() {
    mismatched_appended_header("0 0");
}

#[test]
fn appended_original_with_wrong_header_generation_is_refused() {
    mismatched_appended_header("NUMBER 1");
}
