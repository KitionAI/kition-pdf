//! A viewer (a read-only session: the host opened the file `readOnly`) runs the commands that only
//! read, and is refused the ones that change the document.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::path::PathBuf;

use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit_server::protocol::Origin;
use kition_kit_server::{ErrorReason, KitBackend, OpenRequest};
use kition_pdf_kit::PdfDocument;
use serde_json::{Map, Value, json};

/// Three pages headed Overview, Budget for 2026 and Appendix (written by
/// bindings/wasm/examples/ai_inputs.rs for the case pdf.agent.observe/budget-page).
const REPORT: &[u8] = include_bytes!("../../../cases/pdf.agent.observe/budget-page/input/report.pdf");

fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kition-pdf-viewer-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(dir).unwrap()
}

fn params(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

/// `file.listRevisions` only reads the file's saved revisions: it is declared `mutates: false`,
/// stays enabled for a viewer, and a viewer runs it (as the person and as the agent) without
/// changing the document. A write (`comment.add`) is refused the viewer.
#[test]
fn a_viewer_lists_the_revisions() {
    let root = workspace("revisions");
    std::fs::write(root.join("report.pdf"), REPORT).unwrap();
    let mut kit = Headless::<PdfDocument>::with_replica_port(
        KitInfo::from_manifest(include_str!("../../../kit.json"), &[], kition_pdf_kit::EDITOR_STATE).unwrap(),
    );
    let folder = HostFolder::new(&root).unwrap();
    kit.set_host(folder.host());
    let id = kit.open(OpenRequest { file: &folder.read_only("report.pdf").unwrap(), read_only: true, create: None }).unwrap().id;

    let commands = kit.commands(Some(&id)).unwrap();
    let listed = commands.iter().find(|command| command.id == "file.listRevisions").expect("file.listRevisions is a command");
    assert!(!listed.mutates, "file.listRevisions only reads");
    assert!(listed.enabled, "and a viewer is offered it");

    let before = kit.describe(&id).unwrap();
    for origin in [Origin::User, Origin::Agent] {
        let done =
            kit.execute(&id, "file.listRevisions", params(json!({})), origin).unwrap_or_else(|error| panic!("a viewer lists the revisions: {error}"));
        let revisions = done.result.unwrap_or(Value::Null)["revisions"].as_array().cloned().unwrap_or_else(|| panic!("no revisions"));
        assert!(!revisions.is_empty(), "the file has at least its first revision");
    }
    let after = kit.describe(&id).unwrap();
    assert_eq!((after.summary, Value::Object(after.state)), (before.summary, Value::Object(before.state)), "listing changes nothing");
    assert!(!kit.document(&id).unwrap().dirty);

    let refused =
        kit.execute(&id, "comment.add", params(json!({ "page": 1, "type": "note", "at": [10, 10], "contents": "no" })), Origin::User).unwrap_err();
    assert_eq!(refused.code, ErrorReason::CommandDisabled.code(), "{refused}");
    let _ = std::fs::remove_dir_all(root);
}
