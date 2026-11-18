//! Compound save preparation and compatibility regressions.
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
    name: String,
    path: Option<String>,
    js_output: js::JsOutput,
    xfa_warnings: Vec<String>,
    xfa: Option<XfaLayout>,
    keys: Keys,
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
        name: doc.name.clone(),
        path: doc.path.clone(),
        js_output: doc.js_output.clone(),
        xfa_warnings: doc.xfa_warnings.clone(),
        xfa: doc.xfa.clone(),
        keys: editor.keys.clone(),
    }
}

fn assert_unchanged(session: &mut Session, id: DocId, before: State) {
    let after = state(session, id);
    assert_eq!(after, before);
    assert!(Arc::ptr_eq(&after.bytes, &before.bytes));
    assert!(Arc::ptr_eq(&after.display, &before.display));
    assert!(kition_pdf_annot::has_visible_fill_sign(&session.get(id).unwrap().editor.as_ref().unwrap().cos));
}

fn redo_content(session: &mut Session, id: DocId) {
    session.redo(id).unwrap();
    assert_eq!(session.get(id).unwrap().info.title.as_deref(), Some("Redo branch"));
    session.undo(id).unwrap();
    assert_ne!(session.get(id).unwrap().info.title.as_deref(), Some("Redo branch"));
}

#[test]
fn preparation_failures_never_call_writer_or_change_history_and_replica() {
    for stage in [PreparationStage::Write, PreparationStage::Reopen, PreparationStage::View] {
        let (mut session, id) = prepared("");
        let before = state(&mut session, id);
        let mut called = false;
        PREPARATION_FAILURE.set(Some(stage));
        let result = session.save_with_flatten(id, true, Some("renamed.pdf".into()), |_| {
            called = true;
            Ok(())
        });
        PREPARATION_FAILURE.set(None);
        assert!(result.unwrap_err().to_string().contains("injected preparation failure"));
        assert!(!called);
        assert_unchanged(&mut session, id, before);
        redo_content(&mut session, id);
    }
}

#[test]
fn unsupported_save_and_writer_errors_preserve_the_real_redo_branch() {
    for extension in ["/DocChecksum /synthetic", "/SyntheticTrailer 17", ""] {
        let (mut session, id) = prepared(extension);
        let before = state(&mut session, id);
        let mut called = false;
        let result = session.save_with_flatten(id, true, None, |_| {
            called = true;
            Err(EditError::Write("injected writer failure".into()))
        });
        assert!(result.is_err());
        assert_eq!(called, extension.is_empty());
        assert_unchanged(&mut session, id, before);
        redo_content(&mut session, id);
    }
}

#[test]
fn no_mark_saves_commit_exact_bytes_and_preserve_history_entries() {
    for full in [false, true] {
        let mut session = Session::new();
        let id = session.open("no-marks.pdf", None, source(""), None).unwrap();
        session.apply(id, Edit::SetInfo { key: "Title".into(), value: "Redo branch".into() }).unwrap();
        session.undo(id).unwrap();
        let before = state(&mut session, id);
        let bytes = session.save_with_flatten(id, full, None, |_| Ok(())).unwrap();
        let after = state(&mut session, id);
        assert_eq!(after.generation, before.generation + 1);
        assert_eq!(after.undo_len, before.undo_len);
        assert_eq!(after.redo_len, before.redo_len);
        assert_eq!(after.identity, before.identity);
        assert!(!after.dirty);
        assert_eq!(after.bytes, bytes);
        let mut reopened = Session::new();
        let reopened_id = reopened.open("saved.pdf", None, bytes, None).unwrap();
        assert_eq!(reopened.get(reopened_id).unwrap().info.title, session.get(id).unwrap().info.title);
    }
}

#[test]
fn no_mark_incremental_save_preserves_actual_redo() {
    let mut session = Session::new();
    let id = session.open("no-marks.pdf", None, source(""), None).unwrap();
    session.apply(id, Edit::SetInfo { key: "Title".into(), value: "Redo branch".into() }).unwrap();
    session.undo(id).unwrap();
    session.save_with_flatten(id, false, None, |_| Ok(())).unwrap();
    redo_content(&mut session, id);
}

#[test]
fn incremental_compound_save_retains_peer_note_through_mixed_local_history() {
    for saves in [1, 3] {
        let mut session = Session::new();
        let id = session.open("mixed-history.pdf", None, source(""), None).unwrap();
        let shape = Shape::Mark { rect: [20.0, 40.0, 40.0, 60.0], mark: FillMark::Check };
        let style = Style::default_for(&shape);
        session
            .apply(id, Edit::AddAnnotation(NewAnnotation { page: 0, shape, style, contents: "Local mark".into(), author: "Local".into() }))
            .unwrap();
        let identity = session.doc_id(id);
        for _ in 0..saves {
            session.save_with_flatten(id, false, None, |_| Ok(())).unwrap();
        }
        let snapshot = session.snapshot(id).unwrap();
        let blobs = session.blobs(id).unwrap();
        let mut peer = Session::new();
        let peer_id = peer.open_snapshot("peer.pdf", &snapshot, blobs, None).unwrap();
        peer.apply(
            peer_id,
            Edit::AddAnnotation(NewAnnotation {
                page: 0,
                shape: Shape::Note { at: [100.0, 100.0], icon: NoteIcon::Comment },
                style: Style::default(),
                contents: "Remote note".into(),
                author: "Peer".into(),
            }),
        )
        .unwrap();
        let since = session.version(id).unwrap();
        let changes = peer.export_changes(peer_id, &since).unwrap();
        session.add_blobs(id, &peer.blobs_since(peer_id, &since).unwrap()).unwrap();
        session.import_changes(id, &changes).unwrap();
        session.apply(id, Edit::SetInfo { key: "Title".into(), value: "Later local title".into() }).unwrap();
        for (undo, title, mark) in [(true, false, false), (true, false, true), (false, false, false), (false, true, false)] {
            if undo {
                session.undo(id).unwrap();
            } else {
                session.redo(id).unwrap();
            }
            let doc = session.get(id).unwrap();
            let mut reopened = Session::new();
            let reopened_id = reopened.open("reopened.pdf", None, doc.bytes.clone(), None).unwrap();
            for view in [doc, reopened.get(reopened_id).unwrap()] {
                assert_eq!(view.info.title.as_deref() == Some("Later local title"), title);
                assert_eq!(view.info.annotations.iter().any(|note| note.contents.as_deref() == Some("Local mark")), mark);
                assert!(view.info.annotations.iter().any(|note| note.contents.as_deref() == Some("Remote note")));
            }
            assert_eq!(session.doc_id(id), identity);
            assert_eq!(session.kdm_resets(id), 0);
            assert_eq!(session.kdm_dropped(id), 0);
        }
    }
}

#[test]
fn signed_full_refusal_and_incremental_success_preserve_signature_prefix() {
    let p12 = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../sign/tests/data/ec-p256.p12")).unwrap();
    let digital_id = sign::pkcs12::open(&p12, "test").unwrap();
    let (mut session, id) = prepared("");
    let signed = session.sign(id, &digital_id, SignOptions::default()).unwrap();
    session.mark_signed(id, signed.clone(), None).unwrap();
    let before = state(&mut session, id);
    assert_eq!(session.save_with_flatten(id, true, None, |_| panic!("signed rewrite must not write")), Err(EditError::Signed));
    assert_unchanged(&mut session, id, before);
    let saved = session.save_with_flatten(id, false, None, |_| Ok(())).unwrap();
    assert!(saved.starts_with(&signed));
    assert!(session.get(id).unwrap().is_signed());
    assert!(!kition_pdf_annot::has_visible_fill_sign(&session.get(id).unwrap().editor.as_ref().unwrap().cos));
}

#[test]
fn restricted_and_read_only_documents_never_reach_writer() {
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
    let before = state(&mut limited, id);
    assert_eq!(limited.save_with_flatten(id, false, None, |_| panic!("restricted write")), Err(EditError::NotPermitted("changes to the document")));
    assert_unchanged(&mut limited, id, before);
    let doc = limited.doc_mut(id).unwrap();
    doc.editor = None;
    doc.read_only_reason = Some("synthetic read-only reason".into());
    assert_eq!(
        limited.save_with_flatten(id, false, None, |_| panic!("read-only write")),
        Err(EditError::ReadOnly("synthetic read-only reason".into()))
    );
}

#[test]
fn encrypted_saves_keep_owner_rights_and_reopen_exact_bytes() {
    for full in [false, true] {
        let (mut session, id) = prepared("");
        session
            .apply(
                id,
                Edit::Protect(Protection {
                    open_password: Some("obviously-fake-user".into()),
                    permissions_password: Some("obviously-fake-owner".into()),
                    ..Protection::default()
                }),
            )
            .unwrap();
        let bytes = session.save_with_flatten(id, full, None, |_| Ok(())).unwrap();
        let doc = session.get(id).unwrap();
        assert_eq!(doc.bytes, bytes);
        let editor = doc.editor.as_ref().unwrap();
        assert_eq!(editor.keys.reopen.as_deref(), Some("obviously-fake-owner"));
        assert!(!editor.keys.protection_pending);
        assert!(!kition_pdf_annot::has_visible_fill_sign(&editor.cos));
        assert!(kition_pdf_cos::Document::open_with_password(bytes, Some("obviously-fake-owner")).is_ok());
    }
}

#[test]
fn repaired_input_forced_rewrite_is_prepared_before_writer() {
    let (mut session, id) = prepared("");
    let cos = &mut session.doc_mut(id).unwrap().editor.as_mut().unwrap().cos;
    let mut bytes = write_incremental(cos, &SaveOptions::default()).unwrap();
    let at = bytes.windows(b"startxref\n".len()).rposition(|part| part == b"startxref\n").unwrap();
    bytes.truncate(at);
    bytes.extend_from_slice(b"startxref\n0\n%%EOF\n");
    *cos = kition_pdf_cos::Document::open(Arc::new(bytes)).unwrap();
    assert!(cos.revisions().is_empty());
    let bytes = session.save_with_flatten(id, false, None, |_| Ok(())).unwrap();
    let reopened = kition_pdf_cos::Document::open(bytes).unwrap();
    assert!(reopened.repair_log().is_empty());
    assert!(!kition_pdf_annot::has_visible_fill_sign(&reopened));
}

#[test]
fn xfa_values_and_layout_survive_staged_flatten_and_failed_write() {
    for js_off in [false, true] {
        let bytes = Arc::new(kition_pdf_xfa::fixtures::shell(&kition_pdf_xfa::fixtures::template(3)));
        let mut session = Session::new().with_clock(|| 1_700_000_000);
        let id = session.open("form.pdf", None, bytes, None).unwrap();
        session.js_off = js_off;
        session.apply(id, Edit::SetFieldValue { name: "familyName".into(), value: FieldValue::Text("Singh".into()) }).unwrap();
        let shape = Shape::Mark { rect: [20.0, 40.0, 40.0, 60.0], mark: FillMark::Check };
        let style = Style::default_for(&shape);
        session.apply(id, Edit::AddAnnotation(NewAnnotation { page: 0, shape, style, contents: String::new(), author: "Test".into() })).unwrap();
        let before = state(&mut session, id);
        assert!(session.save_with_flatten(id, false, None, |_| Err(EditError::Write("injected writer failure".into()))).is_err());
        assert_unchanged(&mut session, id, before);
        let saved = session.save_with_flatten(id, false, None, |_| Ok(())).unwrap();
        let cos = kition_pdf_cos::Document::open(saved.clone()).unwrap();
        let data = kition_pdf_xfa::parse_datasets(&kition_pdf_xfa::read_packets(&cos).unwrap().unwrap().xdp).unwrap();
        assert_eq!(data.text_at(&kition_pdf_xfa::som_to_path("form[0].head[0].familyName[0]")), Some("Singh"));
        let reopened = session.open("saved.pdf", None, saved, None).unwrap();
        let doc = session.get(reopened).unwrap();
        assert_eq!(doc.xfa.as_ref().map(|layout| (layout.pages, layout.fields)), Some((2, 14)));
        assert_eq!(doc.form.iter().find(|field| field.name == "familyName").unwrap().value, vec!["Singh"]);
    }
}
