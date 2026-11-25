//! Synthetic compound-save capability regressions; filesystem transaction failures remain separate.
use kition_pdf_automation::{Automation, Content};
use kition_pdf_engine::DocId;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc, time::SystemTime};

struct Workdir(PathBuf);
impl Workdir {
    fn new(label: &str) -> Self {
        let nonce = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("kition-pdf-save-transaction-{label}-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        Self(dir)
    }
}
impl Drop for Workdir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixture(extension: &str) -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Resources << >> >>",
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(format!("trailer\n<< /Size 4 /Root 1 0 R {extension} >>\nstartxref\n{xref}\n%%EOF\n").as_bytes());
    bytes
}

fn ok(a: &mut Automation, tool: &str, args: Value) -> Value {
    let result = a.call(tool, &args).unwrap();
    match result.into_iter().next().unwrap() {
        Content::Json(value) => value,
        other => panic!("expected JSON from {tool}, got {other:?}"),
    }
}

#[derive(Debug)]
struct Snapshot {
    bytes: Arc<Vec<u8>>,
    display: Arc<Vec<u8>>,
    comments: Value,
    dirty: bool,
    generation: u64,
    undo: Option<String>,
    redo: Option<String>,
    name: String,
    path: Option<String>,
    version: String,
    state_hash: String,
    replica_id: Option<String>,
    resets: u64,
    dropped: u64,
}

fn snapshot(a: &mut Automation, id: DocId) -> Snapshot {
    let comments = ok(a, "comment.list", json!({ "doc": id.0 }));
    // Ensure the replica exists before recording identity and counters.
    let version = format!("{:?}", a.session_mut().version(id).unwrap());
    let state_hash = a.session_mut().state_hash(id).unwrap();
    let doc = a.session().get(id).unwrap();
    Snapshot {
        bytes: doc.bytes.clone(),
        display: doc.display.clone(),
        comments,
        dirty: doc.dirty,
        generation: doc.edit_generation(),
        undo: doc.can_undo().map(str::to_owned),
        redo: doc.can_redo().map(str::to_owned),
        name: doc.name.clone(),
        path: doc.path.clone(),
        version,
        state_hash,
        replica_id: a.session().doc_id(id),
        resets: a.session().kdm_resets(id),
        dropped: a.session().kdm_dropped(id),
    }
}

fn assert_retained(a: &mut Automation, id: DocId, before: &Snapshot) {
    let after = snapshot(a, id);
    assert_eq!(after.comments, before.comments, "save failure must retain editable Fill & Sign marks and prior notes");
    assert_eq!(after.bytes, before.bytes, "save failure must not replace working bytes");
    assert!(Arc::ptr_eq(&after.bytes, &before.bytes));
    assert_eq!(after.display, before.display);
    assert!(Arc::ptr_eq(&after.display, &before.display));
    assert_eq!(after.dirty, before.dirty);
    assert_eq!(after.generation, before.generation);
    assert_eq!(after.undo, before.undo);
    assert_eq!(after.redo, before.redo);
    assert_eq!(after.name, before.name);
    assert_eq!(after.path, before.path);
    assert_eq!(after.version, before.version);
    assert_eq!(after.state_hash, before.state_hash);
    assert_eq!(after.replica_id, before.replica_id);
    assert_eq!(after.resets, before.resets);
    assert_eq!(after.dropped, before.dropped);
}

fn prepared(dir: &Workdir, extension: &str) -> (Automation, DocId) {
    std::fs::write(dir.0.join("input.pdf"), fixture(extension)).unwrap();
    let mut a = Automation::new().with_root(&dir.0).unwrap().with_clock(|| 1_700_000_000);
    let id = DocId(ok(&mut a, "doc_open", json!({ "path": "input.pdf" }))["doc"].as_u64().unwrap());
    ok(&mut a, "fillSign.add", json!({ "doc": id.0, "page": 1, "type": "check", "at": [20, 40] }));
    ok(&mut a, "comment.add", json!({ "doc": id.0, "page": 1, "type": "note", "at": [80, 40], "contents": "Retained prior annotation" }));
    ok(&mut a, "comment.add", json!({ "doc": id.0, "page": 1, "type": "note", "at": [100, 40], "contents": "Redo branch annotation" }));
    ok(&mut a, "edit_undo", json!({ "doc": id.0 }));
    let current = a.session().get(id).unwrap();
    assert!(current.dirty && current.can_undo().is_some() && current.can_redo().is_some());
    let comments = ok(&mut a, "comment.list", json!({ "doc": id.0 }));
    assert_eq!(comments["count"], 2);
    assert!(comments["comments"].as_array().unwrap().iter().any(|note| note["contents"] == "Retained prior annotation"));
    (a, id)
}

#[test]
fn unsupported_full_and_default_save_as_keep_working_history_and_replica_state() {
    for (extension, expected) in [
        ("/DocChecksum /synthetic-checksum", "DocChecksum"),
        ("/SyntheticTrailer 17", "unsupported trailer extension"),
        ("/SyntheticTrailer 17 /DocChecksum /synthetic-checksum", "DocChecksum"),
    ] {
        for explicit_full in [false, true] {
            for already_saved in [false, true] {
                let dir = Workdir::new("capability");
                let (mut a, id) = prepared(&dir, extension);
                if already_saved {
                    ok(&mut a, "doc_save", json!({ "doc": id.0, "full": false }));
                    assert!(!a.session().get(id).unwrap().dirty);
                }
                let disk_before = std::fs::read(dir.0.join("input.pdf")).unwrap();
                let sentinel = b"existing destination sentinel";
                std::fs::write(dir.0.join("output.pdf"), sentinel).unwrap();
                let before = snapshot(&mut a, id);
                let mut args = json!({ "doc": id.0, "path": "output.pdf", "flatten_fill_sign": true });
                if explicit_full {
                    args["full"] = json!(true);
                }
                let error = a.call("doc_save", &args).unwrap_err();
                assert!(error.to_string().contains(expected), "wrong failure: {error}");
                assert!(!error.to_string().contains("SyntheticTrailer"));
                assert_eq!(std::fs::read(dir.0.join("output.pdf")).unwrap(), sentinel);
                assert_eq!(std::fs::read(dir.0.join("input.pdf")).unwrap(), disk_before);
                assert_retained(&mut a, id, &before);
            }
        }
    }
}

#[test]
fn ordinary_incremental_flatten_still_saves_extensions_and_keeps_original_prefix() {
    for extension in ["/DocChecksum /synthetic-checksum", "/SyntheticTrailer 17", ""] {
        let dir = Workdir::new("incremental");
        let (mut a, id) = prepared(&dir, extension);
        let original = std::fs::read(dir.0.join("input.pdf")).unwrap();
        ok(&mut a, "doc_save", json!({ "doc": id.0, "path": "output.pdf", "full": false, "flatten_fill_sign": true }));
        let saved = std::fs::read(dir.0.join("output.pdf")).unwrap();
        assert!(saved.starts_with(&original) && saved.len() > original.len());
        assert_eq!(std::fs::read(dir.0.join("input.pdf")).unwrap(), original);
        assert!(!a.session().get(id).unwrap().dirty);
        let comments = ok(&mut a, "comment.list", json!({ "doc": id.0 }));
        assert_eq!(comments["count"], 1);
        assert_eq!(comments["comments"][0]["contents"], "Retained prior annotation");
        let cos = kition_pdf_cos::Document::open(Arc::new(saved)).unwrap();
        if extension.contains("DocChecksum") {
            assert!(cos.trailer().contains(b"DocChecksum"));
        }
        if extension.contains("SyntheticTrailer") {
            assert_eq!(cos.trailer().get(b"SyntheticTrailer"), Some(&kition_pdf_cos::Object::Int(17)));
        }
    }
}

#[test]
fn supported_full_flatten_still_saves_and_reopens() {
    let dir = Workdir::new("full");
    let (mut a, id) = prepared(&dir, "");
    ok(&mut a, "doc_save", json!({ "doc": id.0, "path": "output.pdf", "full": true, "flatten_fill_sign": true }));
    assert!(!a.session().get(id).unwrap().dirty);
    let reopened = ok(&mut a, "doc_open", json!({ "path": "output.pdf" }));
    let comments = ok(&mut a, "comment.list", json!({ "doc": reopened["doc"] }));
    assert_eq!(comments["count"], 1);
    assert_eq!(comments["comments"][0]["contents"], "Retained prior annotation");
}
