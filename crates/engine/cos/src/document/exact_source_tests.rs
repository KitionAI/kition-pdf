//! Exact-source provenance remains separate from ordinary recovery and editing caches.

use std::io::Write;

use super::*;
use crate::{Algorithm, NewEncryption, PdfString, SaveOptions, serialize, write_full};

fn base(extra: &[&str]) -> Document {
    let mut bodies = vec!["<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [] /Count 0 >>"];
    bodies.extend_from_slice(extra);
    Document::open(Arc::new(super::tests::build(&bodies, "/Root 1 0 R"))).unwrap()
}

fn append(doc: &Document, reference: ObjRef, raw: &str) -> Document {
    let mut bytes = doc.bytes().as_ref().clone();
    let offset = bytes.len();
    bytes.extend_from_slice(raw.as_bytes());
    let xref = bytes.len();
    writeln!(&mut bytes, "xref\n{} 1\n{offset:010} {:05} n ", reference.num, reference.generation).unwrap();
    let mut trailer = doc.trailer().clone();
    trailer.set(b"Prev".to_vec(), Object::Int(doc.revisions().last().unwrap().xref_offset as i64));
    writeln!(&mut bytes, "trailer").unwrap();
    serialize(&Object::Dict(trailer), &mut bytes);
    writeln!(&mut bytes, "\nstartxref\n{xref}\n%%EOF").unwrap();
    Document::open(Arc::new(bytes)).unwrap()
}

#[test]
fn exact_source_ignores_editing_overlay_and_borrows_stream_payload() {
    let mut doc = base(&["<< /Length 7 >>\nstream\npayload\nendstream"]);
    let reference = ObjRef::new(3, 0);
    doc.set(reference, Object::Int(99));
    assert_eq!(*doc.try_get(3).unwrap(), Object::Int(99));
    let Object::Stream(stream) = doc.try_get_original_at_xref(reference).unwrap() else { panic!("expected source stream") };
    assert_eq!(&*stream.raw, b"payload");
    assert!(stream.raw.shares(doc.bytes()));
}

#[test]
fn exact_source_refuses_malformed_definition_after_recovery_warmed_cache() {
    let doc = append(&base(&["42"]), ObjRef::new(3, 0), "3 0 obj\n]2\nendobj\n");
    assert_eq!(*doc.try_get(3).unwrap(), Object::Int(42), "ordinary lookup recovers the older definition");
    assert!(doc.cache.lock().unwrap().contains_key(&3));
    assert!(doc.try_get_original_at_xref(ObjRef::new(3, 0)).is_err());
}

#[test]
fn exact_source_checks_header_number_generation_and_untruncated_tokens() {
    for (reference, header) in [(ObjRef::new(3, 0), "4 0"), (ObjRef::new(3, 0), "3 1"), (ObjRef::new(3, u16::MAX), "3 65536")] {
        let doc = append(&base(&["42"]), reference, &format!("{header} obj\n42\nendobj\n"));
        assert!(doc.try_get_original_at_xref(reference).is_err(), "{header}");
    }
    // A synthetic entry at the u32 boundary exposes parser clamping without requiring
    // an enormous xref table. The exact reader must reject the original oversized token.
    let mut doc = append(&base(&["42"]), ObjRef::new(3, 0), "4294967296 0 obj\n42\nendobj\n");
    let entry = doc.xref_entry(3).unwrap();
    Arc::make_mut(&mut doc.entries).insert(u32::MAX, entry);
    assert!(doc.try_get_original_at_xref(ObjRef::new(u32::MAX, 0)).is_err());
    assert!(base(&["42"]).try_get_original_at_xref(ObjRef::new(3, 1)).is_err(), "requested identity must match xref");
}

#[test]
fn exact_source_never_shifts_scans_or_reads_missing_free_compressed_entries() {
    let mut doc = base(&["42"]);
    let Some(XrefEntry::InFile { offset, generation }) = doc.xref_entry(3) else { panic!("in-file fixture") };
    Arc::make_mut(&mut doc.entries).insert(3, XrefEntry::InFile { offset: offset - 1, generation });
    assert_eq!(*doc.try_get(3).unwrap(), Object::Int(42));
    assert!(doc.try_get_original_at_xref(ObjRef::new(3, 0)).is_err(), "preceding newline is not the exact header");
    assert!(doc.try_get_original_at_xref(ObjRef::new(0, 0)).is_err());
    assert!(doc.try_get_original_at_xref(ObjRef::new(99, 0)).is_err());
    Arc::make_mut(&mut doc.entries).insert(3, XrefEntry::InStream { stream: 2, index: 0 });
    assert!(doc.try_get_original_at_xref(ObjRef::new(3, 0)).is_err());
}

#[test]
fn exact_source_indirect_lengths_do_not_use_recovered_cached_values() {
    let original = base(&["<< /Length 4 0 R >>\nstream\npayload\nendstream", "7"]);
    let Object::Stream(stream) = original.try_get_original_at_xref(ObjRef::new(3, 0)).unwrap() else { panic!("source stream") };
    assert_eq!(&*stream.raw, b"payload");
    let broken = append(&original, ObjRef::new(4, 0), "4 0 obj\n]7\nendobj\n");
    assert_eq!(*broken.try_get(4).unwrap(), Object::Int(7));
    assert!(broken.try_get(3).is_ok(), "ordinary stream loading can recover the length");
    assert!(broken.try_get_original_at_xref(ObjRef::new(3, 0)).is_err());
    let wrong_generation = base(&["<< /Length 4 1 R >>\nstream\npayload\nendstream", "7"]);
    assert!(wrong_generation.try_get_original_at_xref(ObjRef::new(3, 0)).is_err());
}

#[test]
fn exact_source_cyclic_and_noninteger_lengths_fail_without_repair() {
    let cyclic = base(&["<< /Length 4 0 R >>\nstream\nx\nendstream", "<< /Length 3 0 R >>\nstream\ny\nendstream"]);
    assert!(cyclic.try_get_original_at_xref(ObjRef::new(3, 0)).is_err());
    for value in ["null", "-1", "(7)"] {
        let doc = base(&["<< /Length 4 0 R >>\nstream\npayload\nendstream", value]);
        assert!(doc.try_get_original_at_xref(ObjRef::new(3, 0)).is_err());
    }
}

#[test]
fn exact_source_preserves_documented_local_parser_tolerance() {
    let doc = base(&["<< /Length 999 >>\nstream\npayload\nendstream"]);
    let Object::Stream(stream) = doc.try_get_original_at_xref(ObjRef::new(3, 0)).unwrap() else { panic!("source stream") };
    assert_eq!(&*stream.raw, b"payload", "direct length recovery remains parser policy");
}

#[test]
fn exact_source_uses_authenticated_decryption_for_strings_and_streams() {
    for algorithm in [Algorithm::Rc4_40, Algorithm::Aes256] {
        let mut doc = Document::new_empty();
        let string = doc.add(PdfString::literal(b"synthetic private text".to_vec()));
        let stream = doc.add(Object::Stream(crate::Stream { dict: Dict::new(), raw: b"synthetic payload".to_vec().into() }));
        doc.update_dict(doc.root().unwrap(), |catalog| {
            catalog.set(b"SyntheticString".to_vec(), Object::Ref(string));
            catalog.set(b"SyntheticStream".to_vec(), Object::Ref(stream));
        })
        .unwrap();
        doc.set_encryption(&NewEncryption {
            algorithm,
            user_password: "synthetic-user",
            owner_password: "synthetic-owner",
            permissions: -1,
            encrypt_metadata: true,
            seed: [37; 32],
        })
        .unwrap();
        let bytes = Arc::new(write_full(&doc, &SaveOptions { object_streams: false, ..SaveOptions::default() }).unwrap());
        let opened = Document::open_with_password(bytes, Some("synthetic-user")).unwrap();
        // Full writing renumbers objects; recover their references from the saved catalog.
        let catalog = opened.try_get(opened.root().unwrap().num).unwrap();
        let string = catalog.as_dict().unwrap().reference(b"SyntheticString").unwrap();
        let stream = catalog.as_dict().unwrap().reference(b"SyntheticStream").unwrap();
        assert_eq!(opened.try_get_original_at_xref(string).unwrap(), *opened.try_get(string.num).unwrap());
        assert_eq!(opened.try_get_original_at_xref(string).unwrap().as_string().unwrap().bytes, b"synthetic private text");
        let exact = opened.try_get_original_at_xref(stream).unwrap();
        assert_eq!(exact, *opened.try_get(stream.num).unwrap());
        let Object::Stream(exact) = exact else { panic!("decrypted stream") };
        assert_eq!(exact.decoded().unwrap(), b"synthetic payload");
    }
}
