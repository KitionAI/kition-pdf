//! Known save refusals must be checked before a compound operation mutates marks.
use super::*;

fn source(extension: &str) -> Arc<Vec<u8>> {
    let bytes = crate::tests::fixture(1);
    let text = String::from_utf8(bytes).unwrap();
    Arc::new(text.replace("/Root 1 0 R >>", &format!("/Root 1 0 R {extension} >>")).into_bytes())
}

fn prepared(extension: &str) -> (Session, DocId) {
    let mut session = Session::new().with_clock(|| 1_700_000_000);
    let id = session.open("preflight.pdf", None, source(extension), None).unwrap();
    let shape = Shape::Mark { rect: [20.0, 40.0, 40.0, 60.0], mark: FillMark::Check };
    let style = Style::default_for(&shape);
    session.apply(id, Edit::AddAnnotation(NewAnnotation { page: 0, shape, style, contents: String::new(), author: "Test".into() })).unwrap();
    session.apply(id, Edit::SetInfo { key: "Title".into(), value: "Redo branch".into() }).unwrap();
    session.undo(id).unwrap();
    assert!(session.get(id).unwrap().can_redo().is_some());
    (session, id)
}

#[derive(Debug, PartialEq)]
struct State {
    bytes: Arc<Vec<u8>>,
    display: Arc<Vec<u8>>,
    dirty: bool,
    generation: u64,
    undo: Option<String>,
    redo: Option<String>,
    undo_len: usize,
    redo_len: usize,
    version: String,
    state_hash: String,
    identity: Option<String>,
    resets: u64,
    dropped: u64,
}

fn state(session: &mut Session, id: DocId) -> State {
    let version = format!("{:?}", session.version(id).unwrap());
    let state_hash = session.state_hash(id).unwrap();
    let doc = session.get(id).unwrap();
    let editor = doc.editor.as_ref().unwrap();
    State {
        bytes: doc.bytes.clone(),
        display: doc.display.clone(),
        dirty: doc.dirty,
        generation: doc.generation,
        undo: doc.can_undo().map(str::to_owned),
        redo: doc.can_redo().map(str::to_owned),
        undo_len: editor.undo.len(),
        redo_len: editor.redo.len(),
        version,
        state_hash,
        identity: session.doc_id(id),
        resets: session.kdm_resets(id),
        dropped: session.kdm_dropped(id),
    }
}

fn assert_unchanged(session: &mut Session, id: DocId, before: State) {
    let after = state(session, id);
    assert_eq!(after, before);
    assert!(Arc::ptr_eq(&after.bytes, &before.bytes));
    assert!(Arc::ptr_eq(&after.display, &before.display));
    assert!(kition_pdf_annot::has_visible_fill_sign(&session.get(id).unwrap().editor.as_ref().unwrap().cos));
}

#[test]
fn forced_incremental_routes_refuse_before_flatten_and_keep_state() {
    #[derive(Clone, Copy, Debug)]
    enum Route {
        Required,
        Encryption,
        Repair,
    }
    for (extension, message) in [("/DocChecksum /synthetic", "DocChecksum"), ("/SyntheticTrailer 17", "unsupported trailer extension")] {
        for route in [Route::Required, Route::Encryption, Route::Repair] {
            let (mut session, id) = prepared(extension);
            let cos = &mut session.doc_mut(id).unwrap().editor.as_mut().unwrap().cos;
            match route {
                Route::Required => cos.require_full_save(),
                Route::Encryption => {
                    cos.set_encryption(&kition_pdf_cos::NewEncryption {
                        algorithm: kition_pdf_cos::Algorithm::Aes256,
                        user_password: "obviously-fake-user",
                        owner_password: "obviously-fake-owner",
                        permissions: -1,
                        encrypt_metadata: true,
                        seed: [11; 32],
                    })
                    .unwrap();
                    assert!(cos.encryption_changed());
                }
                Route::Repair => {
                    let mut bytes = write_incremental(cos, &SaveOptions::default()).unwrap();
                    let at = bytes.windows(b"startxref\n".len()).rposition(|part| part == b"startxref\n").unwrap();
                    bytes.truncate(at);
                    bytes.extend_from_slice(b"startxref\n0\n%%EOF\n");
                    *cos = kition_pdf_cos::Document::open(Arc::new(bytes)).unwrap();
                    assert!(cos.revisions().is_empty() && !cos.repair_log().is_empty());
                }
            }
            let before = state(&mut session, id);
            let error = session.check_save_with_flatten(id, false).unwrap_err();
            assert!(error.to_string().contains(message), "{route:?}: {error}");
            assert_unchanged(&mut session, id, before);
        }
    }
}

#[test]
fn unchanged_marked_file_is_checked_for_the_prospective_write() {
    let (mut session, id) = prepared("/SyntheticTrailer 17");
    let saved = session.save_bytes(id).unwrap();
    session.mark_saved(id, saved, None).unwrap();
    assert!(!session.get(id).unwrap().editor.as_ref().unwrap().cos.is_modified());
    let before = state(&mut session, id);
    assert!(session.check_save_with_flatten(id, true).unwrap_err().to_string().contains("unsupported trailer extension"));
    assert_unchanged(&mut session, id, before);
}

#[test]
fn no_marks_preserve_unchanged_incremental_fast_path() {
    let mut session = Session::new();
    let id = session.open("no-marks.pdf", None, source("/SyntheticTrailer 17"), None).unwrap();
    session.doc_mut(id).unwrap().editor.as_mut().unwrap().cos.require_full_save();
    let original = session.get(id).unwrap().bytes.clone();
    let generation = session.get(id).unwrap().generation;
    assert_eq!(session.check_save_with_flatten(id, false), Ok(()));
    session.apply(id, Edit::FlattenFillSign).unwrap();
    assert!(Arc::ptr_eq(&session.save_bytes(id).unwrap(), &original));
    assert_eq!(session.get(id).unwrap().generation, generation);
    assert!(session.get(id).unwrap().can_undo().is_none());
    assert_eq!(session.check_save_with_flatten(id, true), Ok(()));
    assert!(session.save_full_bytes(id).unwrap_err().to_string().contains("unsupported trailer extension"));
}

#[test]
fn stream_limit_and_checksum_keep_their_error_precedence() {
    let (mut session, id) = prepared("/SyntheticTrailer 17 /DocChecksum /synthetic");
    let before = state(&mut session, id);
    assert!(session.check_save_with_flatten(id, true).unwrap_err().to_string().contains("DocChecksum"));
    assert_unchanged(&mut session, id, before);
    let bytes = session.save_bytes(id).unwrap();
    session.doc_mut(id).unwrap().editor.as_mut().unwrap().cos = kition_pdf_cos::Document::open_with_stream_limit(bytes, None, 1 << 20).unwrap();
    let before = state(&mut session, id);
    for full in [false, true] {
        assert_eq!(session.check_save_with_flatten(id, full), Err(EditError::Write(kition_pdf_cos::CosError::ReadOnlyLimit.to_string())));
    }
    assert_unchanged(&mut session, id, before);
}

#[test]
fn permissions_and_read_only_checks_precede_trailer_capability() {
    let (mut session, id) = prepared("");
    session
        .apply(
            id,
            Edit::Protect(Protection {
                changes: Changes::CommentFillSign,
                permissions_password: Some("obviously-fake-owner".into()),
                ..Protection::default()
            }),
        )
        .unwrap();
    let saved = session.save_bytes(id).unwrap();
    let mut limited = Session::new();
    let id = limited.open("restricted.pdf", None, saved, None).unwrap();
    limited.doc_mut(id).unwrap().editor.as_mut().unwrap().cos.trailer_mut().set(b"SyntheticTrailer".to_vec(), kition_pdf_cos::Object::Int(17));
    let before = state(&mut limited, id);
    assert_eq!(limited.check_save_with_flatten(id, true), Err(EditError::NotPermitted("changes to the document")));
    assert_unchanged(&mut limited, id, before);
    let doc = limited.doc_mut(id).unwrap();
    doc.editor = None;
    doc.read_only_reason = Some("synthetic read-only reason".into());
    assert_eq!(limited.check_save_with_flatten(id, true), Err(EditError::ReadOnly("synthetic read-only reason".into())));
}

#[test]
fn signed_full_save_refuses_before_flatten_but_incremental_remains_allowed() {
    let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sign/tests/data/ec-p256.p12")).unwrap();
    let digital_id = sign::pkcs12::open(&p12, "test").unwrap();
    let (mut session, id) = prepared("");
    let signed = session.sign(id, &digital_id, SignOptions::default()).unwrap();
    session.mark_signed(id, signed, None).unwrap();
    let cos = &mut session.doc_mut(id).unwrap().editor.as_mut().unwrap().cos;
    cos.trailer_mut().set(b"SyntheticTrailer".to_vec(), kition_pdf_cos::Object::Int(17));
    cos.trailer_mut().set(b"DocChecksum".to_vec(), kition_pdf_cos::Object::Name(b"synthetic".to_vec()));
    let before = state(&mut session, id);
    assert_eq!(session.check_save_with_flatten(id, true), Err(EditError::Signed));
    assert_eq!(session.check_save_with_flatten(id, false), Ok(()));
    assert_unchanged(&mut session, id, before);
}
