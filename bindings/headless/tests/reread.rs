//! The session survives another program's save (`regressions/issue-c8-pdf-reread`), through the
//! headless host: when the file changes on disk while the agent's session is clean, the kit reads
//! it again into the same session (`KitDocument::reconcile_reread`) instead of dropping the
//! session's undo history. The other program's edit is no undo step: the agent's `edit.undo`
//! takes back only its own write and keeps the other program's edit.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use common::*;
use serde_json::json;

/// The contents of the comments on `page` (from 1) in the file on disk.
fn comment_on(f: &Fixture, page: u64) -> Vec<String> {
    let listed = read_on_disk(&f.file(), "comment_list", json!({ "page": page }));
    listed["comments"].as_array().unwrap().iter().map(|comment| comment["contents"].as_str().unwrap_or_default().to_owned()).collect()
}

/// The agent writes, another program saves an unrelated edit, the agent observes again and
/// undoes its write: the reread keeps the agent's history, and the undo keeps the other edit.
#[tokio::test(flavor = "multi_thread")]
async fn reread_keeps_the_agents_undo_and_the_outside_edit() {
    let f = Fixture::new(3);
    f.ok("run", json!({ "steps": [step("comment_edit", json!({ "page": 1, "index": 1, "contents": "mine" }))] })).await;
    assert_eq!(comment_on(&f, 1), ["mine"]);
    f.colleague(&[("comment_edit", json!({ "page": 3, "index": 1, "contents": "theirs" }))]);

    let (_, basis) = f.inspect().await;
    let undone = f.ok("run", json!({ "basis": basis, "steps": [{ "command": "edit.undo", "params": {} }] })).await;
    assert!(undone.get("error").is_none(), "{undone}");
    assert_eq!(comment_on(&f, 1), ["note 1"], "the agent's own write is taken back");
    assert_eq!(comment_on(&f, 3), ["theirs"], "the other program's edit stays");

    // The redo brings the agent's write back over the other program's edit, which still stays.
    let (_, basis) = f.inspect().await;
    f.ok("run", json!({ "basis": basis, "steps": [{ "command": "edit.redo", "params": {} }] })).await;
    assert_eq!(comment_on(&f, 1), ["mine"]);
    assert_eq!(comment_on(&f, 3), ["theirs"]);
    f.shutdown().await;
}

/// Two outside saves in a row, each read again into the same session: the agent's two writes
/// undo one by one, newest first, and both outside edits stay.
#[tokio::test(flavor = "multi_thread")]
async fn rereads_in_a_row_keep_every_step() {
    let f = Fixture::new(3);
    f.ok("run", json!({ "steps": [step("comment_edit", json!({ "page": 1, "index": 1, "contents": "mine 1" }))] })).await;
    f.colleague(&[("comment_edit", json!({ "page": 3, "index": 1, "contents": "theirs 1" }))]);
    let (_, basis) = f.inspect().await;
    f.ok("run", json!({ "basis": basis, "steps": [step("comment_edit", json!({ "page": 2, "index": 1, "contents": "mine 2" }))] })).await;
    f.colleague(&[("form_fill", json!({ "values": { "F3": "theirs 2" } }))]);

    let (_, basis) = f.inspect().await;
    f.ok("run", json!({ "basis": basis, "steps": [{ "command": "edit.undo", "params": {} }] })).await;
    assert_eq!(comment_on(&f, 2), ["note 2"]);
    assert_eq!(comment_on(&f, 1), ["mine 1"]);
    let (_, basis) = f.inspect().await;
    f.ok("run", json!({ "basis": basis, "steps": [{ "command": "edit.undo", "params": {} }] })).await;
    assert_eq!(comment_on(&f, 1), ["note 1"]);
    assert_eq!(comment_on(&f, 3), ["theirs 1"]);
    let fields = read_on_disk(&f.file(), "form_fields", json!({}));
    let f3 = fields["fields"].as_array().unwrap().iter().find(|field| field["name"] == "F3").unwrap().clone();
    assert_eq!(f3["value"], "theirs 2", "{f3}");
    f.shutdown().await;
}

/// Another program that writes the file anew (not appending to it) leaves nothing the session's
/// history can be stepped against: with history, the reread is refused (`file-conflict`) and
/// the file stays as the other program wrote it; nothing is dropped silently.
#[tokio::test(flavor = "multi_thread")]
async fn a_rewrite_by_another_program_with_history_is_refused() {
    let f = Fixture::new(2);
    f.ok("run", json!({ "steps": [step("comment_edit", json!({ "page": 1, "index": 1, "contents": "mine" }))] })).await;
    let at = std::fs::metadata(f.file()).unwrap().modified().unwrap();
    let mut doc = lopdf::Document::load_mem(&std::fs::read(f.file()).unwrap()).unwrap();
    doc.renumber_objects_with(100);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    std::fs::write(f.file(), &bytes).unwrap();
    std::fs::File::options().write(true).open(f.file()).unwrap().set_modified(at - std::time::Duration::from_secs(3600)).unwrap();

    let refused = f.refused("inspect", json!({})).await;
    assert!(refused.to_string().contains("cannot reconcile"), "{refused}");
    assert_eq!(std::fs::read(f.file()).unwrap(), bytes, "nothing was written");
    f.shutdown().await;
}
