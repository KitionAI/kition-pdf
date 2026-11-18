//! Contributor-original encrypted fixtures for inspection equivalence and password gates.
use super::*;
use kition_pdf_cos::{Algorithm, Auth, NewEncryption, SaveOptions, write_full};

fn encrypted_fixture(encrypt_metadata: bool) -> Arc<Vec<u8>> {
    let mut source = Document::with_version("1.7");
    let pages = source.new_object_id();
    let note = source.add_object(lopdf::dictionary! {
        "Type" => "Annot", "Subtype" => "Text", "Rect" => vec![10.into(), 10.into(), 30.into(), 30.into()],
        "Contents" => Object::string_literal("Pending inspection note"), "T" => Object::string_literal("Synthetic author")
    });
    let field = source.add_object(lopdf::dictionary! {
        "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx", "T" => Object::string_literal("Synthetic field"),
        "V" => Object::string_literal("Field value"), "Rect" => vec![10.into(), 40.into(), 90.into(), 60.into()]
    });
    let page = source.add_object(lopdf::dictionary! {
        "Type" => "Page", "Parent" => pages, "MediaBox" => vec![0.into(), 0.into(), 120.into(), 160.into()],
        "Annots" => vec![Object::Reference(note), Object::Reference(field)]
    });
    source.objects.insert(pages, lopdf::dictionary! { "Type" => "Pages", "Kids" => vec![Object::Reference(page)], "Count" => 1 }.into());
    let embedded = source.add_object(lopdf::Stream::new(lopdf::dictionary! { "Type" => "EmbeddedFile" }, b"original attachment".to_vec()));
    let attachment = source.add_object(lopdf::dictionary! {
        "Type" => "Filespec", "F" => Object::string_literal("synthetic.txt"), "EF" => lopdf::dictionary! { "F" => embedded }
    });
    let metadata = source
        .add_object(lopdf::Stream::new(lopdf::dictionary! { "Type" => "Metadata", "Subtype" => "XML" }, b"<metadata>synthetic</metadata>".to_vec()));
    let root = source.add_object(lopdf::dictionary! {
        "Type" => "Catalog", "Pages" => pages, "Metadata" => metadata,
        "AcroForm" => lopdf::dictionary! { "Fields" => vec![Object::Reference(field)] },
        "Names" => lopdf::dictionary! { "EmbeddedFiles" => lopdf::dictionary! {
            "Names" => vec![Object::string_literal("synthetic.txt"), Object::Reference(attachment)]
        } }
    });
    let info = source.add_object(lopdf::dictionary! { "Title" => Object::string_literal("Authenticated inspection") });
    source.trailer.set("Root", root);
    source.trailer.set("Info", info);
    let mut plain = Vec::new();
    source.save_to(&mut plain).unwrap();
    let mut cos = kition_pdf_cos::Document::open(Arc::new(plain)).unwrap();
    cos.set_encryption(&NewEncryption {
        algorithm: Algorithm::Aes256,
        user_password: "fake-user",
        owner_password: "fake-owner",
        permissions: -3904,
        encrypt_metadata,
        seed: [37; 32],
    })
    .unwrap();
    Arc::new(write_full(&cos, &SaveOptions { object_streams: false, ..SaveOptions::default() }).unwrap())
}

fn assert_same_info(bytes: Arc<Vec<u8>>, password: &str, source: &kition_pdf_cos::Document) -> DocInfo {
    let normal = inspect(bytes.clone(), Some(password)).unwrap();
    let reused = inspect_with_authenticated_source(bytes, Some(password), source).unwrap();
    // Debug includes every field, so future additions are compared as well.
    assert_eq!(format!("{reused:?}"), format!("{normal:?}"));
    reused
}

#[test]
fn authenticated_inspection_preserves_metadata_forms_annotations_and_attachments() {
    for encrypt_metadata in [false, true] {
        let bytes = encrypted_fixture(encrypt_metadata);
        for (password, role) in [("fake-user", Auth::User), ("fake-owner", Auth::Owner)] {
            let cos = kition_pdf_cos::Document::open_with_password(bytes.clone(), Some(password)).unwrap();
            assert!(cos.repair_log().is_empty());
            assert_eq!(cos.security().unwrap().auth(), role);
            let info = assert_same_info(bytes.clone(), password, &cos);
            assert_eq!(info.title.as_deref(), Some("Authenticated inspection"));
            assert_eq!(info.pages.len(), 1);
            assert!(info.encrypted);
            assert_eq!(info.fields.len(), 1);
            assert_eq!(info.attachments.len(), 1);
            assert!(info.annotations.iter().any(|note| note.contents.as_deref() == Some("Pending inspection note")));
            assert_eq!(cos.security().unwrap().auth(), role, "inspection leaves the caller's permissions unchanged");
        }
    }
}

#[test]
fn authenticated_inspection_does_not_bypass_wrong_or_missing_render_password() {
    let bytes = encrypted_fixture(true);
    let cos = kition_pdf_cos::Document::open_with_password(bytes.clone(), Some("fake-owner")).unwrap();
    for password in [None, Some("wrong-fake-password")] {
        let normal = inspect(bytes.clone(), password).unwrap_err();
        let reused = inspect_with_authenticated_source(bytes.clone(), password, &cos).unwrap_err();
        assert_eq!(reused.to_string(), normal.to_string());
    }
}

#[test]
fn authenticated_inspection_ignores_other_bytes_and_pending_edits() {
    let bytes = encrypted_fixture(true);
    let mut cos = kition_pdf_cos::Document::open_with_password(bytes.clone(), Some("fake-user")).unwrap();
    // Identical contents in another allocation are still not a same-source capability.
    assert_same_info(Arc::new(bytes.as_ref().clone()), "fake-user", &cos);
    let info = cos.trailer().reference(b"Info").unwrap();
    let mut changed = cos.get(info).as_dict().unwrap().clone();
    changed.set(b"Title", kition_pdf_cos::Object::String(kition_pdf_cos::PdfString::text("Unsaved title")));
    cos.set(info, kition_pdf_cos::Object::Dict(changed));
    let inspected = assert_same_info(bytes.clone(), "fake-user", &cos);
    assert_eq!(inspected.title.as_deref(), Some("Authenticated inspection"));
    cos.remove_encryption();
    assert_same_info(bytes, "fake-user", &cos);
}

#[test]
fn authenticated_inspection_preserves_repair_fallback() {
    let original = encrypted_fixture(true);
    let mut repaired = original.as_ref().clone();
    repaired.splice(0..5, *b"%BAD-");
    let bytes = Arc::new(repaired);
    let cos = kition_pdf_cos::Document::open_with_password(bytes.clone(), Some("fake-user")).unwrap();
    assert!(!cos.repair_log().is_empty());
    let normal = inspect(bytes.clone(), Some("fake-user"));
    let reused = inspect_with_authenticated_source(bytes, Some("fake-user"), &cos);
    // A successful renderer open plus the recorded COS repair requires compatibility
    // structural fallback; an early renderer error would not exercise this boundary.
    assert_eq!(normal.as_ref().unwrap().pages.len(), 1);
    assert_eq!(format!("{reused:?}"), format!("{normal:?}"));
    assert!(cos.try_reopen_authenticated_source_with_stream_limit(256 << 20).is_none());
}
