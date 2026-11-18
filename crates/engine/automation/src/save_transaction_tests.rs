//! Failure injection through the actual compound save and atomic writer.
use super::*;
use std::io::Write;
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
    let comments = ok(a, "comment_list", json!({ "doc": id.0 }));
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
    ok(&mut a, "fill_sign_add", json!({ "doc": id.0, "page": 1, "type": "check", "at": [20, 40] }));
    ok(&mut a, "comment_add", json!({ "doc": id.0, "page": 1, "type": "note", "at": [80, 40], "contents": "Retained prior annotation" }));
    ok(&mut a, "comment_add", json!({ "doc": id.0, "page": 1, "type": "note", "at": [100, 40], "contents": "Redo branch annotation" }));
    ok(&mut a, "edit_undo", json!({ "doc": id.0 }));
    let current = a.session().get(id).unwrap();
    assert!(current.dirty && current.can_undo().is_some() && current.can_redo().is_some());
    let comments = ok(&mut a, "comment_list", json!({ "doc": id.0 }));
    assert_eq!(comments["count"], 2);
    assert!(comments["comments"].as_array().unwrap().iter().any(|note| note["contents"] == "Retained prior annotation"));
    (a, id)
}

fn assert_redo_contents(a: &mut Automation, id: DocId) {
    ok(a, "edit_redo", json!({ "doc": id.0 }));
    let comments = ok(a, "comment_list", json!({ "doc": id.0 }));
    assert_eq!(comments["count"], 3);
    assert!(comments["comments"].as_array().unwrap().iter().any(|note| note["contents"] == "Redo branch annotation"));
    ok(a, "edit_undo", json!({ "doc": id.0 }));
    assert_eq!(ok(a, "comment_list", json!({ "doc": id.0 }))["count"], 2);
}

#[derive(Clone, Copy, Debug)]
enum Failure {
    Directory,
    StagingOpen,
    PartialWrite,
    Rename,
}

#[test]
fn staged_filesystem_failures_preserve_working_replica_and_real_redo_branch() {
    for failure in [Failure::Directory, Failure::StagingOpen, Failure::PartialWrite, Failure::Rename] {
        for (already_saved, full) in [(false, false), (false, true), (true, false), (true, true)] {
            let dir = Workdir::new("staged-failure");
            let (mut a, id) = prepared(&dir, "");
            if already_saved {
                ok(&mut a, "doc_save", json!({ "doc": id.0, "full": false }));
            }
            let source = std::fs::read(dir.0.join("input.pdf")).unwrap();
            let target = dir.0.join("output.pdf");
            if matches!(failure, Failure::Directory) {
                std::fs::create_dir(&target).unwrap();
            } else {
                std::fs::write(&target, b"destination sentinel").unwrap();
            }
            let staging = dir.0.join(".output.pdf.0000000000000007.kition-pdf-tmp");
            if matches!(failure, Failure::StagingOpen) {
                std::fs::write(&staging, b"occupied staging sentinel").unwrap();
            }
            let before = snapshot(&mut a, id);
            let args = json!({ "doc": id.0, "path": "output.pdf", "flatten_fill_sign": true, "full": full });
            let error = a
                .doc_save_with(&Args(&args), |path, bytes| {
                    write_atomic_using(
                        path,
                        bytes,
                        [7],
                        |file, bytes| {
                            if matches!(failure, Failure::PartialWrite) {
                                file.write_all(bytes.get(..32).unwrap())?;
                                return Err(std::io::Error::other("injected partial-write failure"));
                            }
                            file.write_all(bytes)
                        },
                        |from, to| {
                            if matches!(failure, Failure::Rename) {
                                return Err(std::io::Error::other("injected rename failure"));
                            }
                            std::fs::rename(from, to)
                        },
                    )
                })
                .unwrap_err();
            assert!(!error.to_string().is_empty());
            assert_eq!(std::fs::read(dir.0.join("input.pdf")).unwrap(), source);
            if matches!(failure, Failure::Directory) {
                assert!(target.is_dir());
            } else {
                assert_eq!(std::fs::read(&target).unwrap(), b"destination sentinel");
            }
            if matches!(failure, Failure::StagingOpen) {
                assert_eq!(std::fs::read(&staging).unwrap(), b"occupied staging sentinel");
            } else {
                assert!(!staging.exists(), "staging must be cleaned after {failure:?}");
            }
            assert_retained(&mut a, id, &before);
            assert_redo_contents(&mut a, id);
        }
    }
}

#[test]
fn staged_default_save_as_full_and_incremental_commit_exact_bytes() {
    for full in [None, Some(true), Some(false)] {
        let dir = Workdir::new("staged-success");
        let (mut a, id) = prepared(&dir, "");
        let original = std::fs::read(dir.0.join("input.pdf")).unwrap();
        let before = snapshot(&mut a, id);
        let mut args = json!({ "doc": id.0, "path": "output.pdf", "flatten_fill_sign": true });
        if let Some(full) = full {
            args["full"] = json!(full);
        }
        let mut written = Vec::new();
        a.doc_save_with(&Args(&args), |path, bytes| {
            written = bytes.to_vec();
            write_atomic(path, bytes)
        })
        .unwrap();
        assert_eq!(std::fs::read(dir.0.join("output.pdf")).unwrap(), written);
        assert_eq!(std::fs::read(dir.0.join("input.pdf")).unwrap(), original);
        let doc = a.session().get(id).unwrap();
        assert_eq!(*doc.bytes, written);
        assert_eq!(doc.edit_generation(), before.generation + 2);
        assert!(!doc.dirty && doc.can_redo().is_none());
        assert_eq!(a.session().doc_id(id), before.replica_id);
        assert_eq!(a.session().kdm_resets(id), before.resets);
        assert_eq!(a.session().kdm_dropped(id), before.dropped);
        assert_eq!(ok(&mut a, "comment_list", json!({ "doc": id.0 }))["count"], 1);
        let reopened = ok(&mut a, "doc_open", json!({ "path": "output.pdf" }));
        assert_eq!(ok(&mut a, "comment_list", json!({ "doc": reopened["doc"] }))["count"], 1);
    }
}

#[test]
fn staged_incremental_save_preserves_actual_undo_and_redo() {
    let dir = Workdir::new("staged-incremental-history");
    let (mut a, id) = prepared(&dir, "");
    ok(&mut a, "doc_save", json!({ "doc": id.0, "path": "output.pdf", "full": false, "flatten_fill_sign": true }));
    assert_eq!(ok(&mut a, "comment_list", json!({ "doc": id.0 }))["count"], 1);
    ok(&mut a, "edit_undo", json!({ "doc": id.0 }));
    assert_eq!(ok(&mut a, "comment_list", json!({ "doc": id.0 }))["count"], 2);
    ok(&mut a, "edit_redo", json!({ "doc": id.0 }));
    assert_eq!(ok(&mut a, "comment_list", json!({ "doc": id.0 }))["count"], 1);
}
