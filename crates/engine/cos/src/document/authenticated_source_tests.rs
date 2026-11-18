//! Same-source authentication reuse must preserve identity, parser limits and fallback.

use std::cell::Cell;
use std::sync::OnceLock;

use super::*;
use crate::{Algorithm, Auth, NewEncryption, PdfString, SaveOptions, Stream, serialize, write_full, write_incremental};

thread_local! {
    static PASSWORD_AUTHENTICATIONS: Cell<usize> = const { Cell::new(0) };
}

pub(super) fn note_password_authentication() {
    PASSWORD_AUTHENTICATIONS.with(|count| count.set(count.get() + 1));
}

fn authentications() -> usize {
    PASSWORD_AUTHENTICATIONS.with(Cell::get)
}

const LIMIT: usize = 1 << 20;
const LARGE_STRING_BYTES: usize = 8192;

fn encrypted(algorithm: Algorithm, user: &str, owner: &str, metadata: bool, packed: bool, seed: u8) -> Arc<Vec<u8>> {
    let mut doc = Document::new_empty();
    let sample = doc.add(PdfString::literal(vec![b'x'; LARGE_STRING_BYTES]));
    let payload = doc.add(Object::Stream(Stream { dict: Dict::new(), raw: b"synthetic private stream".to_vec().into() }));
    let mut metadata_dict = Dict::new();
    metadata_dict.set(b"Type".to_vec(), Object::name("Metadata"));
    metadata_dict.set(b"Subtype".to_vec(), Object::name("XML"));
    let metadata_ref = doc.add(Object::Stream(Stream { dict: metadata_dict, raw: b"<synthetic>metadata</synthetic>".to_vec().into() }));
    doc.update_dict(doc.root().unwrap(), |root| {
        root.set(b"Sample".to_vec(), Object::Ref(sample));
        root.set(b"Payload".to_vec(), Object::Ref(payload));
        root.set(b"Metadata".to_vec(), Object::Ref(metadata_ref));
    })
    .unwrap();
    doc.set_encryption(&NewEncryption {
        algorithm,
        user_password: user,
        owner_password: owner,
        permissions: 0b0100,
        encrypt_metadata: metadata,
        seed: [seed; 32],
    })
    .unwrap();
    Arc::new(write_full(&doc, &SaveOptions { object_streams: packed, ..SaveOptions::default() }).unwrap())
}

fn fixture(packed: bool) -> Arc<Vec<u8>> {
    static CLASSIC: OnceLock<Arc<Vec<u8>>> = OnceLock::new();
    static PACKED: OnceLock<Arc<Vec<u8>>> = OnceLock::new();
    let cell = if packed { &PACKED } else { &CLASSIC };
    cell.get_or_init(|| encrypted(Algorithm::Aes256, "synthetic-user", "synthetic-owner", true, packed, 19)).clone()
}

fn sample(doc: &Document) -> ObjRef {
    doc.get(doc.root().unwrap()).as_dict().unwrap().reference(b"Sample").unwrap()
}

fn assert_same_objects(left: &Document, right: &Document) {
    assert_eq!(left.object_numbers(), right.object_numbers());
    assert_eq!(left.trailer(), right.trailer());
    assert_eq!(left.revisions(), right.revisions());
    for num in left.object_numbers() {
        assert_eq!(left.generation(num), right.generation(num));
        assert_eq!(left.try_get(num).unwrap(), right.try_get(num).unwrap(), "decoded object {num}");
    }
}

#[test]
fn r6_source_reparse_preserves_user_and_owner_identity_without_authentication() {
    for packed in [false, true] {
        for (password, role) in [("synthetic-user", Auth::User), ("synthetic-owner", Auth::Owner)] {
            let source = Document::open_with_password(fixture(packed), Some(password)).unwrap();
            let identity = source.source_identity();
            let cache_size = source.cache.lock().unwrap().len();
            let object_streams = source.objstms.lock().unwrap().len();
            let before = authentications();
            let reused = source.try_reopen_authenticated_source_with_stream_limit(LIMIT).unwrap();
            assert_eq!(authentications(), before, "same-source parsing must not reauthenticate");
            assert_eq!(source.source_identity(), identity);
            assert_eq!(source.cache.lock().unwrap().len(), cache_size);
            assert_eq!(source.objstms.lock().unwrap().len(), object_streams);
            assert!(source.overlay.is_empty() && !source.encryption_changed && !source.full_save);
            assert_ne!(reused.source_identity(), identity);
            assert!(Arc::ptr_eq(source.bytes(), reused.bytes()));
            assert!(Arc::ptr_eq(source.security.as_ref().unwrap(), reused.security.as_ref().unwrap()));
            assert!(!Arc::ptr_eq(&source.cache, &reused.cache));
            assert!(!Arc::ptr_eq(&source.objstms, &reused.objstms));
            assert!(!Arc::ptr_eq(&source.entries, &reused.entries));
            assert_eq!(reused.security().unwrap().auth(), role);
            assert_eq!(reused.security().unwrap().recovered_user_password(), source.security().unwrap().recovered_user_password());
            assert_eq!(reused.permissions(), source.permissions());
            assert_eq!(reused.permissions().unwrap().owner, role == Auth::Owner);
            let before = authentications();
            let ordinary = Document::open_with_stream_limit(source.bytes().clone(), Some(password), LIMIT).unwrap();
            assert_eq!(authentications() - before, 1, "ordinary opening still authenticates");
            assert_same_objects(&reused, &ordinary);
            assert!(matches!(write_full(&reused, &SaveOptions::default()), Err(CosError::ReadOnlyLimit)));
            assert!(matches!(write_incremental(&reused, &SaveOptions::default()), Err(CosError::ReadOnlyLimit)));
        }
    }
}

#[test]
fn source_reuse_does_not_change_missing_or_wrong_password_errors() {
    let bytes = fixture(false);
    let authenticated = Document::open_with_password(bytes.clone(), Some("synthetic-user")).unwrap();
    assert!(authenticated.try_reopen_authenticated_source_with_stream_limit(LIMIT).is_some());
    assert!(matches!(Document::open(bytes.clone()), Err(CosError::NeedsPassword)));
    assert!(matches!(Document::open_with_password(bytes.clone(), Some("synthetic-wrong")), Err(CosError::WrongPassword)));
    assert!(matches!(Document::open_with_stream_limit(bytes, Some("synthetic-wrong"), LIMIT), Err(CosError::WrongPassword)));
}

#[test]
fn source_reparse_preserves_empty_and_normalized_r6_credentials() {
    let latin_small_e_with_acute = char::from_u32(0x00E9).unwrap();
    let non_ascii = format!("synthetic-{latin_small_e_with_acute}");
    let long = format!("{}-ignored-tail", "x".repeat(127));
    for password in [String::new(), non_ascii, long] {
        let bytes = encrypted(Algorithm::Aes256, &password, "synthetic-distinct-owner", false, true, 23);
        let supplied = (!password.is_empty()).then_some(password.as_str());
        let source = Document::open_with_password(bytes.clone(), supplied).unwrap();
        let reused = source.try_reopen_authenticated_source_with_stream_limit(LIMIT).unwrap();
        let ordinary = Document::open_with_stream_limit(bytes, supplied, LIMIT).unwrap();
        assert_eq!(reused.security().unwrap().auth(), Auth::User);
        assert_eq!(reused.permissions(), ordinary.permissions());
        assert!(!reused.security().unwrap().encrypts_metadata());
        assert_same_objects(&reused, &ordinary);
    }
}

#[test]
fn source_reparse_does_not_reuse_warmed_oversized_object_streams() {
    let bytes = fixture(true);
    let source = Document::open_with_password(bytes.clone(), Some("synthetic-user")).unwrap();
    let item = sample(&source);
    assert!(matches!(source.xref_entry(item.num), Some(XrefEntry::InStream { .. })));
    assert_eq!(source.get(item).as_string().unwrap().bytes.len(), LARGE_STRING_BYTES);
    assert!(!source.objstms.lock().unwrap().is_empty(), "the source cache must be warm");
    let cached_objects = source.cache.lock().unwrap().len();
    let cached_streams = source.objstms.lock().unwrap().len();
    let reused = source.try_reopen_authenticated_source_with_stream_limit(1024).unwrap();
    let ordinary = Document::open_with_stream_limit(bytes, Some("synthetic-user"), 1024).unwrap();
    assert!(reused.try_get(item.num).is_err(), "the tighter reader cannot use a source-cache hit");
    assert_eq!(reused.try_get(item.num).unwrap_err(), ordinary.try_get(item.num).unwrap_err());
    assert!(reused.objstms.lock().unwrap().is_empty());
    assert_eq!(source.cache.lock().unwrap().len(), cached_objects);
    assert_eq!(source.objstms.lock().unwrap().len(), cached_streams);
    assert_eq!(source.get(item).as_string().unwrap().bytes.len(), LARGE_STRING_BYTES);
    // A caller asking for a larger bound must use its normal password-open path;
    // silently keeping the smaller bound changes subsequent read/fallback behavior.
    assert!(reused.try_reopen_authenticated_source_with_stream_limit(LIMIT).is_none());
    let again = reused.try_reopen_authenticated_source_with_stream_limit(1024).unwrap();
    assert_eq!(again.stream_limit, 1024);
    assert!(again.try_get(item.num).is_err());
}

#[test]
fn source_reparse_does_not_bypass_xref_stream_limits_or_repairs() {
    let bytes = fixture(true);
    let source = Document::open_with_password(bytes.clone(), Some("synthetic-user")).unwrap();
    assert!(source.revisions().iter().any(|revision| revision.is_stream));
    let before = authentications();
    assert!(source.try_reopen_authenticated_source_with_stream_limit(1).is_none());
    assert_eq!(authentications(), before);
    let fallback = Document::open_with_stream_limit(bytes, Some("synthetic-user"), 1);
    // The original reader can tolerate this via reconstruction; any such result must
    // retain its repair record and read-only limit instead of becoming an eligible hint.
    if let Ok(repaired) = fallback {
        assert!(!repaired.repair_log().is_empty());
        assert!(repaired.try_reopen_authenticated_source_with_stream_limit(LIMIT).is_none());
        assert!(matches!(write_full(&repaired, &SaveOptions::default()), Err(CosError::ReadOnlyLimit)));
    }
}

#[test]
fn pending_changes_and_legacy_handlers_are_ineligible() {
    let source = Document::open_with_password(fixture(false), Some("synthetic-owner")).unwrap();
    let mut overlay = source.clone();
    overlay.update_dict(overlay.root().unwrap(), |root| root.set(b"Pending".to_vec(), Object::Bool(true))).unwrap();
    assert!(overlay.try_reopen_authenticated_source_with_stream_limit(LIMIT).is_none());
    let mut rewrite = source.clone();
    rewrite.require_full_save();
    assert!(rewrite.try_reopen_authenticated_source_with_stream_limit(LIMIT).is_none());
    let mut removed = source.clone();
    removed.remove_encryption();
    assert!(removed.try_reopen_authenticated_source_with_stream_limit(LIMIT).is_none());
    let mut rekeyed = source.clone();
    rekeyed
        .set_encryption(&NewEncryption {
            algorithm: Algorithm::Aes256,
            user_password: "synthetic-new-user",
            owner_password: "synthetic-new-owner",
            permissions: -4,
            encrypt_metadata: true,
            seed: [37; 32],
        })
        .unwrap();
    assert!(rekeyed.try_reopen_authenticated_source_with_stream_limit(LIMIT).is_none());
    let mut repaired = source.clone();
    repaired.note_repair("synthetic repair".into());
    assert!(repaired.try_reopen_authenticated_source_with_stream_limit(LIMIT).is_none());
    for algorithm in [Algorithm::Rc4_40, Algorithm::Rc4_128, Algorithm::Aes128] {
        let bytes = encrypted(algorithm, "synthetic-user", "synthetic-owner", true, false, 29);
        let legacy = Document::open_with_password(bytes.clone(), Some("synthetic-owner")).unwrap();
        assert!(legacy.try_reopen_authenticated_source_with_stream_limit(LIMIT).is_none());
        let ordinary = Document::open_with_stream_limit(bytes, Some("synthetic-owner"), LIMIT).unwrap();
        assert_eq!(ordinary.security().unwrap().auth(), Auth::Owner);
        assert_same_objects(&legacy, &ordinary);
    }
    let plain = Document::new_empty();
    assert!(plain.try_reopen_authenticated_source_with_stream_limit(LIMIT).is_none());
}

#[test]
fn mutable_trailer_is_not_mistaken_for_original_source_state() {
    let bytes = fixture(false);
    let mut source = Document::open_with_password(bytes.clone(), Some("synthetic-user")).unwrap();
    let original_trailer = source.trailer().clone();
    source.trailer_mut().set(b"Root".to_vec(), Object::Ref(ObjRef::new(999_999, 0)));
    source.trailer_mut().set(b"Encrypt".to_vec(), Object::Null);
    source.trailer_mut().set(b"SyntheticPending".to_vec(), Object::Bool(true));
    assert!(!source.is_modified(), "trailer_mut does not create an overlay");
    let changed_trailer = source.trailer().clone();
    let reused = source.try_reopen_authenticated_source_with_stream_limit(LIMIT).unwrap();
    assert_eq!(source.trailer(), &changed_trailer, "rereading must not mutate the editor");
    assert_eq!(reused.trailer(), &original_trailer);
    assert_eq!(reused.encrypt_num, original_trailer.reference(b"Encrypt").map(|reference| reference.num));
    let ordinary = Document::open_with_stream_limit(bytes, Some("synthetic-user"), LIMIT).unwrap();
    assert_same_objects(&reused, &ordinary);
}

fn replace_classic_trailer(source: &Document, trailer: Dict) -> Vec<u8> {
    let start = rfind(source.bytes(), b"trailer", 0).unwrap();
    let xref = source.revisions().last().unwrap().xref_offset;
    let mut bytes = source.bytes()[..start].to_vec();
    bytes.extend_from_slice(b"trailer\n");
    serialize(&Object::Dict(trailer), &mut bytes);
    bytes.extend_from_slice(format!("\nstartxref\n{xref}\n%%EOF\n").as_bytes());
    bytes
}

#[test]
fn same_source_authentication_compares_every_effective_dictionary_field() {
    let source = Document::open_with_password(fixture(false), Some("synthetic-user")).unwrap();
    let enc = source.get(source.trailer().reference(b"Encrypt").unwrap()).as_dict().unwrap().clone();
    let mut changes = Vec::new();
    for key in [b"O".as_slice(), b"U", b"OE", b"UE", b"Perms"] {
        let mut changed = enc.clone();
        changed.set(key.to_vec(), PdfString::literal(b"synthetic-mismatch".to_vec()));
        changes.push(changed);
    }
    for key in [b"StmF".as_slice(), b"StrF", b"EFF"] {
        let mut changed = enc.clone();
        changed.set(key.to_vec(), Object::name("Identity"));
        changes.push(changed);
    }
    for (key, value) in [
        (b"P".as_slice(), Object::Int(-4)),
        (b"EncryptMetadata", Object::Bool(false)),
        (b"CF", Object::Dict(Dict::new())),
        (b"R", Object::Int(5)),
        (b"V", Object::Int(4)),
        (b"Filter", Object::name("SyntheticUnsupported")),
    ] {
        let mut changed = enc.clone();
        changed.set(key.to_vec(), value);
        changes.push(changed);
    }
    for changed in changes {
        let mut trailer = source.trailer().clone();
        trailer.set(b"Encrypt".to_vec(), Object::Dict(changed));
        // Only this private test calls the internal mode with deliberately mismatched
        // bytes. The public capability has no bytes/handler arguments.
        let bytes = Arc::new(replace_classic_trailer(&source, trailer));
        let before = authentications();
        let result = Document::open_with_authentication(bytes, Authentication::AuthenticatedSource(source.security.as_ref().unwrap()), LIMIT);
        assert!(matches!(result, Err(CosError::Security(_))), "a changed effective dictionary must not use the retained key");
        assert_eq!(authentications(), before);
    }
}

#[test]
fn unavailable_encryption_dictionary_declines_reuse_without_changing_normal_opening() {
    let source = Document::open_with_password(fixture(false), Some("synthetic-user")).unwrap();
    let mut trailer = source.trailer().clone();
    trailer.set(b"Encrypt".to_vec(), Object::Ref(ObjRef::new(999_999, 0)));
    let bytes = Arc::new(replace_classic_trailer(&source, trailer));
    let reuse = Document::open_with_authentication(bytes.clone(), Authentication::AuthenticatedSource(source.security.as_ref().unwrap()), LIMIT);
    assert!(matches!(reuse, Err(CosError::Security(_))));
    // The established tolerant reader treats a dangling Encrypt as unencrypted.
    let ordinary = Document::open_with_stream_limit(bytes, None, LIMIT).unwrap();
    assert!(ordinary.security().is_none());
    assert!(!ordinary.trailer().contains(b"Encrypt"));
    assert!(ordinary.try_reopen_authenticated_source_with_stream_limit(LIMIT).is_none());
}

#[test]
fn reconstruction_cannot_reuse_a_handler_for_its_different_trailer() {
    let source = Document::open_with_password(fixture(false), Some("synthetic-user")).unwrap();
    let other_bytes = encrypted(Algorithm::Aes256, "synthetic-user", "synthetic-owner", true, false, 31);
    let other = Document::open_with_password(other_bytes, Some("synthetic-user")).unwrap();
    let other_encrypt = other.get(other.trailer().reference(b"Encrypt").unwrap()).as_dict().unwrap().clone();
    let pages = source.get(source.root().unwrap()).as_dict().unwrap().reference(b"Pages").unwrap();
    let mut bad_root = source.trailer().clone();
    bad_root.set(b"Root".to_vec(), Object::Ref(pages));
    let mut bytes = replace_classic_trailer(&source, bad_root);
    let mut repaired_trailer = source.trailer().clone();
    repaired_trailer.set(b"Encrypt".to_vec(), Object::Dict(other_encrypt));
    bytes.extend_from_slice(b"trailer\n");
    serialize(&Object::Dict(repaired_trailer), &mut bytes);
    bytes.push(b'\n');
    let bytes = Arc::new(bytes);
    let before = authentications();
    let reuse = Document::open_with_authentication(bytes.clone(), Authentication::AuthenticatedSource(source.security.as_ref().unwrap()), LIMIT);
    assert!(matches!(reuse, Err(CosError::Security(_))), "the second authentication site must compare the reconstructed trailer");
    assert_eq!(authentications(), before);
    let normal = Document::open_with_stream_limit(bytes.clone(), Some("synthetic-user"), LIMIT).unwrap();
    assert_eq!(authentications() - before, 2, "normal repair authenticates each trailer with the supplied password");
    assert!(!normal.repair_log().is_empty());
    assert_eq!(normal.security().unwrap().dict(), other.security().unwrap().dict());
    assert!(normal.try_reopen_authenticated_source_with_stream_limit(LIMIT).is_none());
    assert!(matches!(Document::open_with_stream_limit(bytes, Some("synthetic-wrong"), LIMIT), Err(CosError::WrongPassword)));
}
