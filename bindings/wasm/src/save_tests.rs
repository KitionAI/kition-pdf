//! SDK save preparation, host acknowledgement, and KDM reopening on original fixtures.

use super::*;
use kition_kit::session::sha256_hex;
use kition_pdf_cos::{Dict, Object, SaveOptions, Stream, write_full};
use std::path::{Path, PathBuf};

#[path = "../../../crates/engine/automation/tests/support/search_geometry_fixture.rs"]
mod search_fixture;

fn original() -> Vec<u8> {
    PdfDocument::create("Save.pdf", 240, 200).unwrap().save(None).unwrap()
}

fn note(doc: &mut PdfDocument, text: &str) {
    doc.execute("comment_add", json!({"page":1,"type":"note","at":[24,24],"contents":text})).unwrap();
}

fn comment_count(doc: &mut PdfDocument) -> usize {
    doc.execute("comment_list", json!({})).unwrap()["comments"].as_array().unwrap().len()
}

#[test]
fn saved_acknowledgement_does_not_discard_a_newer_local_edit() {
    let mut doc = PdfDocument::open("Save.pdf", &original()).unwrap();
    note(&mut doc, "Prepared edit");
    let prepared = doc.save(None).unwrap();
    note(&mut doc, "Newer edit");
    let version = doc.version().unwrap();
    assert_eq!(comment_count(&mut doc), 2);
    doc.mark_saved();
    assert_eq!(comment_count(&mut doc), 2, "a delayed host acknowledgement must retain newer edits");
    assert!(doc.dirty(), "only the older prepared revision reached storage");
    assert_eq!(doc.version().unwrap(), version);
    let mut saved = PdfDocument::open("Stored.pdf", &prepared).unwrap();
    assert_eq!(comment_count(&mut saved), 1);
    let mut retry = PdfDocument::open("Retry.pdf", &doc.save(None).unwrap()).unwrap();
    assert_eq!(comment_count(&mut retry), 2);
}

#[test]
fn unchanged_save_is_byte_exact_and_unprepared_acknowledgement_is_a_noop() {
    let bytes = original();
    let mut doc = PdfDocument::open("Save.pdf", &bytes).unwrap();
    for _ in 0..3 {
        assert_eq!(doc.save(None).unwrap(), bytes);
        doc.mark_saved();
        assert!(!doc.dirty());
    }
    note(&mut doc, "Not written");
    let revision = doc.revision();
    doc.mark_saved();
    assert!(doc.dirty(), "without save preparation there is no acknowledged file");
    assert_eq!(doc.revision(), revision);
    assert_eq!(comment_count(&mut doc), 1);
}

#[test]
fn repeated_incremental_saves_rebase_bytes_and_keep_kdm_identity_version_and_undo() {
    let mut prior = original();
    let mut doc = PdfDocument::open("Save.pdf", &prior).unwrap();
    doc.version().unwrap();
    let identity = doc.doc_id();
    for count in 1..=3 {
        note(&mut doc, &format!("Edit {count}"));
        let version = doc.version().unwrap();
        let saved = doc.save(None).unwrap();
        assert!(saved.len() > prior.len() && saved.starts_with(&prior));
        let snapshot = doc.snapshot().unwrap();
        let blobs = doc.blobs().unwrap();
        doc.mark_saved();
        assert!(!doc.dirty());
        assert_eq!(doc.version().unwrap(), version, "save acknowledgement adds no KDM edit");
        assert_eq!(doc.doc_id(), identity);
        assert_eq!(doc.kdm_resets(), 0);
        assert_eq!(doc.kdm_dropped(), 0);
        assert_eq!(doc.save(None).unwrap(), saved, "a no-edit repeat does not append again");
        let mut reopened = PdfDocument::open_with_replica("Save.pdf", &saved, &snapshot, &blobs).unwrap();
        assert!(reopened.has_replica());
        assert_eq!(reopened.doc_id(), identity);
        assert_eq!(reopened.version().unwrap(), version);
        assert_eq!(comment_count(&mut reopened), count);
        assert!(!reopened.dirty());
        prior = saved;
    }
    assert!(doc.undo().unwrap().0);
    assert_eq!(comment_count(&mut doc), 2);
    assert!(doc.redo().unwrap().0);
    assert_eq!(comment_count(&mut doc), 3);
}

#[test]
fn original_type3_without_info_or_id_keeps_version_after_each_bookmark_save() {
    search_fixture::export();
    let mut prior = search_fixture::pdf();
    let parsed = kition_pdf_cos::Document::open(Arc::new(prior.clone())).unwrap();
    assert!(!parsed.trailer().contains(b"Info") && !parsed.trailer().contains(b"ID"));
    let mut doc = PdfDocument::open("OriginalType3.pdf", &prior).unwrap();
    doc.version().unwrap();
    let identity = doc.doc_id();
    for count in 1..=3 {
        doc.execute("bookmark_add", json!({"title":format!("Original bookmark {count}"),"page":count})).unwrap();
        let version = doc.version().unwrap();
        let saved = doc.save(None).unwrap();
        assert!(saved.starts_with(&prior));
        let snapshot = doc.snapshot().unwrap();
        let blobs = doc.blobs().unwrap();
        doc.mark_saved();
        assert_eq!(doc.version().unwrap(), version);
        let mut reopened = PdfDocument::open_with_replica("OriginalType3.pdf", &saved, &snapshot, &blobs).unwrap();
        assert_eq!(reopened.doc_id(), identity);
        assert_eq!(reopened.version().unwrap(), version, "bookmark save {count} must not become an import");
        assert!(!reopened.dirty());
        prior = saved;
    }
}

#[test]
fn failed_host_write_keeps_dirty_state_and_allows_retry() {
    let mut doc = PdfDocument::open("Save.pdf", &original()).unwrap();
    note(&mut doc, "Retained after write failure");
    let prepared = doc.save(None).unwrap();
    let revision = doc.revision();
    let version = doc.version().unwrap();
    // A directory cannot be overwritten as a file. The host must not acknowledge failure.
    assert!(std::fs::write(std::env::temp_dir(), &prepared).is_err());
    assert!(doc.dirty());
    assert_eq!(doc.revision(), revision);
    assert_eq!(doc.version().unwrap(), version);
    assert_eq!(doc.saved.borrow().as_deref().map(Vec::as_slice), Some(prepared.as_slice()));
    note(&mut doc, "Retry edit");
    let retry = doc.save(None).unwrap();
    doc.mark_saved();
    assert!(!doc.dirty());
    assert_eq!(comment_count(&mut PdfDocument::open("Retry.pdf", &retry).unwrap()), 2);
}

#[test]
fn stale_acknowledgement_keeps_undo_and_remote_import_changes() {
    let mut doc = PdfDocument::open("Save.pdf", &original()).unwrap();
    note(&mut doc, "Undo this preparation");
    doc.save(None).unwrap();
    doc.undo().unwrap();
    doc.mark_saved();
    assert_eq!(comment_count(&mut doc), 0);
    assert!(doc.dirty());
    assert!(doc.redo().unwrap().0);
    let bytes = doc.save(None).unwrap();
    let snapshot = doc.snapshot().unwrap();
    let blobs = doc.blobs().unwrap();
    let mut peer = PdfDocument::open_with_replica("Peer.pdf", &bytes, &snapshot, &blobs).unwrap();
    let since = doc.version().unwrap();
    note(&mut peer, "Remote edit after preparation");
    doc.add_blobs(&peer.blobs_since(&since).unwrap()).unwrap();
    doc.import_changes(&peer.export_changes(&since).unwrap()).unwrap();
    let version = doc.version().unwrap();
    doc.mark_saved();
    assert_eq!(comment_count(&mut doc), 2);
    assert_eq!(doc.version().unwrap(), version);
    assert!(doc.dirty());
    assert_eq!(doc.kdm_resets() + doc.kdm_dropped(), 0);
}

fn unknown_fixture(checksum: bool) -> Vec<u8> {
    let mut cos = kition_pdf_cos::Document::open(Arc::new(original())).unwrap();
    let payload = cos.add(Object::Stream(Stream::from_raw(Dict::new(), b"Original opaque payload\0\xff".to_vec())));
    let mut unknown = Dict::new();
    unknown.set(b"Payload".to_vec(), Object::Ref(payload));
    unknown.set(b"Values".to_vec(), Object::Array(vec![Object::Int(42), Object::Bool(true), Object::name("Opaque")]));
    let reference = cos.add(Object::Dict(unknown));
    let root = cos.root().unwrap();
    let mut catalog = cos.get(root).as_dict().unwrap().clone();
    catalog.set(b"SyntheticUnknown".to_vec(), Object::Ref(reference));
    cos.set(root, Object::Dict(catalog));
    let full = write_full(&cos, &SaveOptions { object_streams: false, ..Default::default() }).unwrap();
    let reopened = kition_pdf_cos::Document::open(Arc::new(full.clone())).unwrap();
    let catalog = reopened.get(reopened.root().unwrap());
    let Object::Ref(reference) = catalog.as_dict().unwrap().get(b"SyntheticUnknown").unwrap() else { panic!("expected reference") };
    let marker = b"trailer\n<<";
    let at = full.windows(marker.len()).position(|part| part == marker).unwrap() + marker.len();
    let checksum = if checksum { "/DocChecksum /synthetic-checksum " } else { "" };
    let inserted = format!("/SyntheticTrailer {reference} {checksum}");
    [full[..at].to_vec(), inserted.into_bytes(), full[at..].to_vec()].concat()
}

fn unknown_state(bytes: &[u8]) -> Vec<Object> {
    let cos = kition_pdf_cos::Document::open(Arc::new(bytes.to_vec())).unwrap();
    let catalog = cos.get(cos.root().unwrap());
    let reference = catalog.as_dict().unwrap().get(b"SyntheticUnknown").unwrap();
    let object = cos.resolve(reference);
    let payload = cos.resolve(object.as_dict().unwrap().get(b"Payload").unwrap());
    vec![
        reference.clone(),
        (*object).clone(),
        (*payload).clone(),
        cos.trailer().get(b"SyntheticTrailer").unwrap().clone(),
        cos.trailer().get(b"DocChecksum").cloned().unwrap_or(Object::Null),
    ]
}

#[test]
fn incremental_sdk_and_kdm_roundtrip_preserve_unknown_objects_and_checksum_token() {
    for checksum in [false, true] {
        let original = unknown_fixture(checksum);
        let expected = unknown_state(&original);
        let mut doc = PdfDocument::open("Unknown.pdf", &original).unwrap();
        assert_eq!(doc.save(None).unwrap(), original);
        note(&mut doc, "Incremental annotation");
        let saved = doc.save(None).unwrap();
        assert!(saved.starts_with(&original));
        assert_eq!(unknown_state(&saved), expected);
        let snapshot = doc.snapshot().unwrap();
        let blobs = doc.blobs().unwrap();
        doc.mark_saved();
        let mut reopened = PdfDocument::open_with_replica("Unknown.pdf", &saved, &snapshot, &blobs).unwrap();
        assert_eq!(reopened.doc_id(), doc.doc_id());
        assert_eq!(reopened.version().unwrap(), doc.version().unwrap());
        note(&mut reopened, "Second annotation");
        let second = reopened.save(None).unwrap();
        assert!(second.starts_with(&saved));
        assert_eq!(unknown_state(&second), expected);
        assert_eq!(comment_count(&mut PdfDocument::open("Unknown.pdf", &second).unwrap()), 2);
    }
}

#[test]
fn checksum_rewrite_refusal_keeps_working_state_preparation_and_kdm() {
    let original = unknown_fixture(true);
    let parsed = kition_pdf_cos::Document::open(Arc::new(original.clone())).unwrap();
    assert!(parsed.trailer().contains(b"DocChecksum"), "synthetic source must contain checksum");
    assert!(write_full(&parsed, &SaveOptions::default()).is_err(), "direct COS rewrite must refuse checksum");
    let mut doc = PdfDocument::open("Checksum.pdf", &original).unwrap();
    assert!(doc.auto.session().save_full_bytes(doc.id).is_err(), "fresh SDK document must refuse checksum rewrite");
    note(&mut doc, "Pending annotation");
    assert!(doc.auto.session().save_full_bytes(doc.id).is_err(), "annotated SDK document must refuse checksum rewrite");
    let prepared = doc.save(None).unwrap();
    let revision = doc.revision();
    let version = doc.version().unwrap();
    let working = doc.doc().unwrap().bytes.clone();
    let error = doc.auto.session().save_full_bytes(doc.id).unwrap_err().to_string();
    assert!(error.contains("DocChecksum") && error.contains("incremental save"));
    assert!(doc.dirty());
    assert_eq!(doc.revision(), revision);
    assert_eq!(doc.version().unwrap(), version);
    assert!(Arc::ptr_eq(&doc.doc().unwrap().bytes, &working));
    assert_eq!(doc.saved.borrow().as_deref().map(Vec::as_slice), Some(prepared.as_slice()));
    assert_eq!(comment_count(&mut doc), 1);
    doc.mark_saved();
    assert!(!doc.dirty());
    assert_eq!(unknown_state(&doc.save(None).unwrap()), unknown_state(&original));
}

#[test]
fn failed_rebase_retains_prepared_bytes_and_working_state() {
    let mut doc = PdfDocument::open("Save.pdf", &original()).unwrap();
    note(&mut doc, "Retained on rebase failure");
    doc.save(None).unwrap();
    let revision = doc.revision();
    let version = doc.version().unwrap();
    let working = doc.doc().unwrap().bytes.clone();
    // Inject corrupt prepared bytes to exercise the real Session reopening error.
    let corrupt = Arc::new(b"not a PDF preparation".to_vec());
    *doc.saved.borrow_mut() = Some(corrupt.clone());
    doc.mark_saved();
    assert!(doc.dirty());
    assert_eq!(doc.revision(), revision);
    assert_eq!(doc.version().unwrap(), version);
    assert!(Arc::ptr_eq(&doc.doc().unwrap().bytes, &working));
    assert!(doc.saved.borrow().as_ref().is_some_and(|bytes| Arc::ptr_eq(bytes, &corrupt)));
    assert_eq!(doc.saved_revision.get(), Some(revision));
    assert_eq!(comment_count(&mut doc), 1);
    doc.save(None).unwrap();
    doc.mark_saved();
    assert!(!doc.dirty());
}

fn write_once(path: &Path, bytes: &[u8]) {
    if path.exists() {
        assert!(std::fs::read(path).unwrap() == bytes, "refuse to replace differing save evidence");
    } else {
        std::fs::write(path, bytes).unwrap();
    }
}

#[test]
#[ignore = "requires a pinned four-case corpus manifest; missing inputs must fail"]
fn pinned_corpus_sdk_saves_and_kdm_reopen_are_strict() {
    let input = PathBuf::from(std::env::var_os("KITION_PDF_SAVE_SDK_INPUT").expect("set the pinned input manifest"));
    let directory = input.parent().unwrap();
    let input: Value = serde_json::from_slice(&std::fs::read(&input).unwrap()).unwrap();
    let cases = input["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4, "three existing smoke cases and one checksum case");
    let mut results = Vec::new();
    for case in cases {
        let sha = case["sha256"].as_str().unwrap();
        assert!(sha.len() == 64 && sha.bytes().all(|byte| byte.is_ascii_hexdigit()));
        let bytes = std::fs::read(directory.join(case["originalPath"].as_str().unwrap())).expect("pinned source must exist");
        assert_eq!(sha256_hex(&bytes), sha, "pinned source must match");
        let original_cos = kition_pdf_cos::Document::open(Arc::new(bytes.clone())).unwrap();
        let checksum = original_cos.trailer().get(b"DocChecksum").cloned();
        let output_dir = directory.join(sha);
        std::fs::create_dir_all(&output_dir).unwrap();
        let mut outputs = Map::new();
        let mut checks = Map::new();
        for mode in ["noEdit", "bookmark", "annotation"] {
            let mut doc = PdfDocument::open("Corpus.pdf", &bytes).expect("pinned SDK source must open");
            doc.auto = std::mem::take(&mut doc.auto).with_clock(|| 1_700_000_000);
            assert_eq!(doc.doc().unwrap().info.pages.len(), case["pages"].as_u64().unwrap() as usize);
            let comments_before = comment_count(&mut doc);
            match mode {
                "bookmark" => {
                    doc.execute("bookmark_add", json!({"title":"SDK save oracle","page":1})).unwrap();
                }
                "annotation" => note(&mut doc, "SDK save oracle"),
                _ => {}
            }
            let version = doc.version().unwrap();
            let identity = doc.doc_id();
            let saved = doc.save(None).expect("pinned SDK save must succeed");
            assert!(saved.starts_with(&bytes), "incremental prefix must retain source");
            if mode == "noEdit" {
                assert!(saved == bytes, "no-edit save must retain all bytes");
            }
            let snapshot = doc.snapshot().unwrap();
            let blobs = doc.blobs().unwrap();
            doc.mark_saved();
            assert!(!doc.dirty());
            assert_eq!(doc.version().unwrap(), version);
            assert_eq!(doc.doc_id(), identity);
            assert!(doc.save(None).unwrap() == saved, "no-edit repeat after acknowledgement is byte exact");
            let mut reopened = PdfDocument::open_with_replica("Corpus.pdf", &saved, &snapshot, &blobs).unwrap();
            assert!(reopened.has_replica(), "replica must not fall back to a plain file");
            assert_eq!(reopened.version().unwrap(), version);
            assert_eq!(reopened.doc_id(), identity);
            assert_eq!(reopened.doc().unwrap().info.pages.len(), case["pages"].as_u64().unwrap() as usize);
            assert_eq!(comment_count(&mut reopened), comments_before + usize::from(mode == "annotation"));
            assert_eq!(reopened.kdm_resets() + reopened.kdm_dropped(), 0);
            assert!(!reopened.dirty());
            let saved_cos = kition_pdf_cos::Document::open(Arc::new(saved.clone())).unwrap();
            assert_eq!(saved_cos.trailer().get(b"DocChecksum"), checksum.as_ref());
            if checksum.is_some() {
                let Err(error) = doc.auto.session().save_full_bytes(doc.id) else { panic!("checksum rewrite must be refused") };
                assert!(error.to_string().contains("DocChecksum"));
            }
            let relative = format!("{sha}/{mode}.pdf");
            write_once(&directory.join(&relative), &saved);
            outputs.insert(mode.into(), json!(relative));
            checks.insert(mode.into(), json!({"passed":true,"bytes":saved.len(),"sha256":sha256_hex(&saved),"originalPrefix":true,
                "noEditByteExact":mode == "noEdit","repeatAfterAckByteExact":true,"kdmReopened":true,"kdmIdentityAndVersionStable":true,
                "checksumPresent":checksum.is_some(),"checksumTokenPreserved":true,"snapshotSha256":sha256_hex(&snapshot),"blobsSha256":sha256_hex(&blobs)}));
        }
        results.push(
            json!({"sha256":sha,"group":case["group"],"pages":case["pages"],"originalPath":case["originalPath"],"outputs":outputs,"checks":checks}),
        );
        println!("SDK corpus case={} pages={} legs=3 status=passed", &sha[..12], case["pages"]);
    }
    let manifest = json!({"schema":1,"cases":results,"attemptedCases":4,"passedCases":4,"saveLegs":12,"fullCorpusTested":false});
    write_once(&directory.join("manifest.json"), &serde_json::to_vec_pretty(&manifest).unwrap());
}

#[test]
fn unsupported_trailer_protect_and_sanitize_fail_before_working_state_or_kdm_commit() {
    for (command, args) in [("doc_protect", json!({"open_password":"obviously-fake-test-password"})), ("doc_remove_hidden", json!({}))] {
        let original = unknown_fixture(false);
        let mut doc = PdfDocument::open("Unknown.pdf", &original).unwrap();
        doc.auto = std::mem::take(&mut doc.auto).with_clock(|| 1_700_000_000);
        note(&mut doc, "Keep pending annotation");
        let prepared = doc.save(None).unwrap();
        let revision = doc.revision();
        let version = doc.version().unwrap();
        let identity = doc.doc_id();
        let working = doc.doc().unwrap().bytes.clone();
        let undo = doc.doc().unwrap().can_undo().map(str::to_owned);
        let redo = doc.doc().unwrap().can_redo().map(str::to_owned);
        let counters = (doc.kdm_resets(), doc.kdm_dropped());
        let error = doc.execute(command, args).unwrap_err();
        assert!(error.contains("unsupported trailer extension"), "{command}: {error}");
        assert!(!error.contains("SyntheticTrailer"));
        assert!(doc.dirty());
        assert_eq!(doc.revision(), revision);
        assert_eq!(doc.version().unwrap(), version);
        assert_eq!(doc.doc_id(), identity);
        assert_eq!((doc.kdm_resets(), doc.kdm_dropped()), counters);
        assert!(Arc::ptr_eq(&doc.doc().unwrap().bytes, &working));
        assert!(!doc.doc().unwrap().info.encrypted);
        assert_eq!(doc.doc().unwrap().can_undo().map(str::to_owned), undo);
        assert_eq!(doc.doc().unwrap().can_redo().map(str::to_owned), redo);
        assert_eq!(doc.saved.borrow().as_deref().map(Vec::as_slice), Some(prepared.as_slice()));
        assert_eq!(doc.saved_revision.get(), Some(revision));
        assert_eq!(comment_count(&mut doc), 1);
        assert_eq!(doc.save(None).unwrap(), prepared);
        doc.mark_saved();
        assert!(!doc.dirty());
        assert_eq!(unknown_state(&doc.save(None).unwrap()), unknown_state(&original));
    }
}
