//! `kition-pdf` opens a sync room's state with no file, as a server's headless kit pool does
//! (`model.open`, `model.import`, `model.export`, `model.save`; kition-sdk `docs/platform/reference/object-model.md`).

use kition_headless::{Headless, KitInfo};
use kition_kit::KitDocument;
use kition_kit_server::{ErrorReason, KitBackend};
use kition_pdf_kit::PdfDocument;
use serde_json::{Value, json};

fn kit() -> Headless<PdfDocument> {
    Headless::with_replica_port(KitInfo::from_manifest(include_str!("../../../kit.json"), &[], kition_pdf_kit::EDITOR_STATE).unwrap())
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for k in 0..4 {
            out.push(if k <= chunk.len() { char::from(BASE64[((n >> (18 - 6 * k)) & 63) as usize]) } else { '=' });
        }
    }
    out
}

fn decode(text: &str) -> Vec<u8> {
    let (mut out, mut buffer, mut bits) = (Vec::new(), 0u32, 0u32);
    for byte in text.trim_end_matches('=').bytes() {
        let value = BASE64.iter().position(|known| *known == byte).expect("base64");
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    out
}

fn notes(doc: &mut PdfDocument) -> Vec<String> {
    let listed = doc.execute("comment_list", json!({})).unwrap();
    listed["comments"].as_array().unwrap().iter().filter_map(|comment| comment["contents"].as_str().map(str::to_owned)).collect()
}

fn note(page: u32, text: &str) -> Value {
    json!({ "page": page, "type": "note", "at": [100, 100], "contents": text })
}

/// The room's document, as a collaborator holds it.
fn peer() -> PdfDocument {
    let mut doc = PdfDocument::create("Report.pdf", 612, 792).unwrap();
    doc.execute("page_insert_blank", json!({ "at": 2 })).unwrap();
    let bytes = doc.save(None).unwrap();
    let mut doc = PdfDocument::open("Report.pdf", &bytes).unwrap();
    doc.version().unwrap();
    doc
}

#[test]
fn opens_a_room_without_a_file_and_imports_exports_and_saves_it() {
    let mut kit = kit();
    let features = kit.capabilities()["features"].clone();
    for feature in ["model.open", "model.import", "model.export", "model.save"] {
        assert!(features.as_array().unwrap().iter().any(|listed| listed == feature), "{feature} in {features}");
    }

    let mut peer = peer();
    let (snapshot, blobs) = (peer.snapshot().unwrap(), peer.blobs().unwrap());
    let opened = kit.model("model.open", json!({ "name": "Report.pdf", "snapshot": encode(&snapshot), "blobs": encode(&blobs) })).unwrap();
    let id = opened["documentId"].as_str().unwrap().to_owned();
    assert_eq!(opened["format"], "pdf");

    // A collaborator's change comes in.
    let since = peer.version().unwrap();
    peer.execute("comment_add", note(1, "from a person")).unwrap();
    let change = peer.export_changes(&since).unwrap();
    kit.model("model.import", json!({ "documentId": id, "changes": encode(&change) })).unwrap();

    // The kit edits on top of it, and the collaborator takes what it exports.
    let batch = json!({ "documentId": id, "ops": [{ "op": "command", "id": "comment_add", "params": note(2, "from the server") }] });
    kit.model("model.batch", batch).unwrap();
    let exported = kit.model("model.export", json!({ "documentId": id, "since": encode(&peer.version().unwrap()) })).unwrap();
    peer.import_changes(&decode(exported["changes"].as_str().unwrap())).unwrap();
    let mut seen = notes(&mut peer);
    seen.sort();
    assert_eq!(seen, ["from a person", "from the server"]);
    let everything = kit.model("model.export", json!({ "documentId": id })).unwrap();
    let mut joined =
        PdfDocument::open_with_replica("Report.pdf", &[], &decode(everything["changes"].as_str().unwrap()), &peer.blobs().unwrap()).unwrap();
    assert_eq!(joined.version().unwrap(), peer.version().unwrap(), "every change the document has");

    let saved = kit.model("model.save", json!({ "documentId": id })).unwrap();
    assert_eq!((saved["name"].as_str(), saved["format"].as_str()), (Some("Report.pdf"), Some("pdf")));
    let mut file = PdfDocument::open("Report.pdf", &decode(saved["data"].as_str().unwrap())).unwrap();
    let mut saved_notes = notes(&mut file);
    saved_notes.sort();
    assert_eq!(saved_notes, ["from a person", "from the server"]);
}

#[test]
fn never_opens_an_empty_document_for_a_replica_it_cannot_read() {
    let mut kit = kit();
    for snapshot in [&b"not a replica"[..], &b""[..]] {
        let error = kit.model("model.open", json!({ "name": "Report.pdf", "snapshot": encode(snapshot) })).unwrap_err();
        assert_eq!(error.code, ErrorReason::UnsupportedFormat.code(), "{error:?}");
    }
    // The replica without its base PDF (which travels as a blob) does not open either.
    let snapshot = peer().snapshot().unwrap();
    let error = kit.model("model.open", json!({ "name": "Report.pdf", "snapshot": encode(&snapshot) })).unwrap_err();
    assert_eq!(error.code, ErrorReason::UnsupportedFormat.code(), "no base: {error:?}");
}

/// What `model.export` answers for a PDF opened from its file, its changes and the blobs they
/// reference (the base PDF), opens again with `model.open` as the same document, an annotation
/// included, with no file. An agent's headless draft is such a replica (KHP A12); the export had
/// no blobs, so the replica opened without its base and the draft fell back to a copy of the file.
#[test]
fn a_pdf_opens_from_its_exported_replica_with_its_base() {
    use kition_headless::HostFolder;
    use kition_kit_server::OpenRequest;
    use kition_kit_server::protocol::Origin;
    use serde_json::Map;

    let params = |value: Value| -> Map<String, Value> { value.as_object().cloned().unwrap_or_default() };
    let root = std::env::temp_dir().join(format!("kition-pdf-replica-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = std::fs::canonicalize(root).unwrap();
    let mut file = PdfDocument::create("Report.pdf", 612, 792).unwrap();
    file.execute("page_insert_blank", json!({ "at": 2 })).unwrap();
    file.execute("form_add_field", json!({ "page": 1, "type": "text", "rect": [72, 600, 300, 624], "name": "Name" })).unwrap();
    std::fs::write(root.join("Report.pdf"), file.save(None).unwrap()).unwrap();

    let mut kit = kit();
    let folder = HostFolder::new(&root).unwrap();
    kit.set_host(folder.host());
    let opened = kit.open(OpenRequest { file: &folder.file("Report.pdf").unwrap(), read_only: false, create: None }).unwrap().id;
    kit.execute(&opened, "comment_add", params(note(1, "from the agent")), Origin::Agent).unwrap();

    let exported = kit.model("model.export", json!({ "documentId": opened })).unwrap();
    assert!(exported["blobs"].is_string(), "the base PDF travels with the changes: {exported}");
    let replica = kit
        .model("model.open", json!({ "name": "Report.pdf", "snapshot": exported["changes"], "blobs": exported["blobs"] }))
        .unwrap_or_else(|error| panic!("model.open: {}", error.message));
    let replica = replica["documentId"].as_str().unwrap().to_owned();
    assert_ne!(replica, opened);

    let saved = |kit: &mut Headless<PdfDocument>, id: &str| -> PdfDocument {
        let saved = kit.model("model.save", json!({ "documentId": id })).unwrap();
        PdfDocument::open("Report.pdf", &decode(saved["data"].as_str().unwrap())).unwrap()
    };
    let (mut original, mut copy) = (saved(&mut kit, &opened), saved(&mut kit, &replica));
    assert_eq!(notes(&mut copy), ["from the agent"], "the annotation came with the replica");
    assert_eq!(copy.inspect().unwrap()["pages"], original.inspect().unwrap()["pages"], "the same pages");
    let fields = |doc: &mut PdfDocument| doc.execute("form_fields", json!({})).unwrap()["fields"].clone();
    assert_eq!(fields(&mut copy), fields(&mut original), "the same form fields");
    assert_eq!(
        copy.render(0.0, 0.0, 1.0, copy.width(), 300).unwrap(),
        original.render(0.0, 0.0, 1.0, original.width(), 300).unwrap(),
        "the same pixels"
    );

    // The replica takes edits of its own and leaves the file's document alone.
    kit.execute(&replica, "comment_add", params(note(2, "in the draft")), Origin::Agent).unwrap();
    let mut draft = notes(&mut saved(&mut kit, &replica));
    draft.sort();
    assert_eq!(draft, ["from the agent", "in the draft"]);
    assert_eq!(notes(&mut saved(&mut kit, &opened)), ["from the agent"]);
    let _ = std::fs::remove_dir_all(root);
}

/// Changes the document cannot take (here, from a newer build whose schema this kit does not
/// read) are refused, and the document is closed.
#[test]
fn refuses_changes_it_cannot_take_and_closes_the_document() {
    use kition_kit::kdm::{ActorId, Origin, Replica, Schema};
    use std::sync::Arc;

    let mut kit = kit();
    let mut peer = peer();
    let (snapshot, blobs) = (peer.snapshot().unwrap(), peer.blobs().unwrap());
    let opened = kit.model("model.open", json!({ "name": "Report.pdf", "snapshot": encode(&snapshot), "blobs": encode(&blobs) })).unwrap();
    let id = opened["documentId"].as_str().unwrap().to_owned();

    let mut raw: Value = serde_json::from_str(include_str!("../../../crates/kdm/kdm-model/model/schema.json")).unwrap();
    let newer = raw["schemaVersion"].as_u64().unwrap() + 1;
    raw["schemaVersion"] = json!(newer);
    let mut newer_peer = Replica::load(Arc::new(Schema::parse(&raw.to_string()).unwrap()), &snapshot, ActorId::from_u64(77).unwrap()).unwrap();
    let since = newer_peer.version();
    newer_peer.transact(&Origin::system("migrate"), |tx| tx.raise_schema_version(u32::try_from(newer).unwrap()).map(|_| ())).unwrap();
    let refused = newer_peer.export_changes(&since).unwrap();

    let error = kit.model("model.import", json!({ "documentId": id, "changes": encode(&refused) })).unwrap_err();
    assert_eq!(error.code, ErrorReason::InvalidParams.code(), "{error:?}");
    assert_eq!(error.data.as_ref().unwrap()["refused"], true);
    let closed = kit.model("model.export", json!({ "documentId": id })).unwrap_err();
    assert_eq!(closed.code, ErrorReason::DocumentNotFound.code());
}
