//! Kition's collaboration members (engine contract 2, Kition phase 3g): the PDF as a KDM
//! replica.
//!
//! The engine keeps its own `Session` (the command bridge to the replica,
//! `kition_pdf_engine::kdm_bridge`), not a `kition_kit::session::KitSession` directly, so
//! `kition_kit::export_kit_session!` does not apply here; these members are added by hand with
//! the exact JS names and shapes `kition-sdk`'s `session::web` gives
//! (`crates/kition-kit/src/session/web.rs`), so Kition's host code is the same for every kit, as
//! Kition Slide's and Photo's are. The wasm bindings live in [`crate::web`].
//!
//! The replica holds the base PDF and the stream data of edited objects as blobs, so
//! [`PdfDocument::blobs`], [`PdfDocument::add_blobs`] and [`PdfDocument::blobs_since`] move real
//! content, encoded the same way Kition Photo's do: `(64-char ASCII-hex sha256, u64 LE length,
//! bytes) * N`. A live change ships only the stream data it added, never the base again.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::Arc;

use kition_kit::KitDocument;
use kition_pdf_automation::Automation;
use kition_pdf_engine::{Damage, MemoryBlobs, Origin, Version};
use serde_json::{Value, json};

use crate::{PASTEBOARD, PdfDocument};

fn engine_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

impl PdfDocument {
    /// Opens a file with the replica a host stored for it (the Kit UI protocol's `replica`
    /// capability: a PDF cannot carry its own replica): the document is the replica's, so it
    /// keeps its KDM identity across saves. A file saved from the replica's view (its base with
    /// the view's edits appended) opens as it is, and the next save appends to it; a file changed
    /// since (an older save, edited in another program) is recorded onto the replica as one
    /// import. A replica that does not open (damaged, a newer schema, a protected base) is
    /// ignored and the file opens as [`PdfDocument::open`] opens it.
    ///
    /// Empty `bytes` mean there is no file yet, only the replica (a host opening a shared
    /// document for the first time from its sync room's snapshot, to save it as a new file).
    pub fn open_with_replica(name: &str, bytes: &[u8], snapshot: &[u8], blobs: &[u8]) -> Result<Self, String> {
        let restored = (|| -> Result<(Automation, kition_pdf_engine::DocId), String> {
            let blobs = decode_blobs(blobs)?;
            let mut auto = Automation::new();
            let id = auto.session_mut().open_with_replica(name.to_owned(), bytes, snapshot, blobs, None).map_err(engine_error)?;
            Ok((auto, id))
        })();
        match restored {
            Ok((auto, id)) => Ok(Self {
                auto,
                id,
                renderer: RefCell::new(None),
                saved: RefCell::new(None),
                saved_revision: Cell::new(None),
                pasteboard: PASTEBOARD,
                exchange: Default::default(),
            }),
            Err(_) if bytes.is_empty() => Err("the stored replica does not open and there is no file".into()),
            Err(_) => Self::open(name, bytes),
        }
    }

    /// Whether the document has made its replica yet: it makes one lazily, on the first edit or
    /// when asked for it ([`PdfDocument::version`] and the rest).
    pub fn has_replica(&self) -> bool {
        self.auto.session().kdm(self.id).is_some()
    }

    /// The replica's own document id (a `kition-sdk` `DocId`, stable across saves and across
    /// opening another replica of the same document, never the file's name): Kit UI's generic
    /// runtime reads this for the Kit UI protocol's `replica/info` message. `None` (`undefined`
    /// in JS) before the document has made a replica ([`PdfDocument::has_replica`]).
    pub fn doc_id(&self) -> Option<String> {
        self.auto.session().doc_id(self.id)
    }

    /// The document's replica version (it makes the replica if there is none yet).
    pub fn version(&mut self) -> Result<Vec<u8>, String> {
        Ok(self.auto.session_mut().version(self.id).map_err(engine_error)?.encode())
    }

    /// The document's changes since `since` (empty bytes: every change).
    pub fn export_changes(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        let since = if since.is_empty() { Version::empty() } else { Version::decode(since).map_err(engine_error)? };
        self.auto.session_mut().export_changes(self.id, &since).map_err(engine_error)
    }

    /// Imports a peer's (or stored) changes; add their blobs first with
    /// [`PdfDocument::add_blobs`]. Answers what went stale on screen.
    pub fn import_changes(&mut self, bytes: &[u8]) -> Result<Damage, String> {
        self.auto.session_mut().import_changes(self.id, bytes).map_err(engine_error)
    }

    /// The document's replica as a snapshot, for a peer to open as a new replica of the same
    /// document ([`PdfDocument::open`]'ing the saved bytes does not: a PDF carries no replica).
    pub fn snapshot(&mut self) -> Result<Vec<u8>, String> {
        self.auto.session_mut().snapshot(self.id).map_err(engine_error)
    }

    /// Undoes the person's last step on the document's replica. `done` is `false`, not an error,
    /// when there is nothing to undo.
    pub fn undo(&mut self) -> Result<(bool, Damage), String> {
        let done = self.can_undo();
        if done {
            self.execute("edit.undo", json!({ "scope": "person" }))?;
        }
        Ok((done, if done { Damage::Full } else { Damage::None }))
    }

    pub fn redo(&mut self) -> Result<(bool, Damage), String> {
        let done = self.can_redo();
        if done {
            self.execute("edit.redo", json!({ "scope": "person" }))?;
        }
        Ok((done, if done { Damage::Full } else { Damage::None }))
    }

    pub fn can_undo(&self) -> bool {
        self.auto.session().get(self.id).is_some_and(|doc| doc.can_undo().is_some())
    }

    pub fn can_redo(&self) -> bool {
        self.auto.session().get(self.id).is_some_and(|doc| doc.can_redo().is_some())
    }

    /// Runs a command as `origin` (a person, an agent): the replica records it as theirs, and it
    /// undoes as this replica's own step. Contract 1's [`PdfDocument::execute`](kition_kit::KitDocument::execute)
    /// is this with the local person.
    pub fn execute_as(&mut self, command: &str, params: Value, origin: &Origin) -> Result<Value, String> {
        let previous = self.auto.session_mut().set_origin(Some(origin.clone()));
        let result = self.execute(command, params);
        self.auto.session_mut().set_origin(previous);
        result
    }

    /// Every blob the document's replica references or can still bring back (the base PDF and
    /// stream data), as one buffer a peer feeds straight into [`PdfDocument::add_blobs`].
    pub fn blobs(&mut self) -> Result<Vec<u8>, String> {
        let blobs = self.auto.session_mut().blobs(self.id).map_err(engine_error)?;
        Ok(encode_blobs(&blobs))
    }

    /// Adds a peer's blobs (from their [`PdfDocument::blobs`]): kept until the next
    /// [`PdfDocument::import_changes`], which a host calls right after.
    pub fn add_blobs(&mut self, bytes: &[u8]) -> Result<(), String> {
        let blobs = decode_blobs(bytes)?;
        self.auto.session_mut().add_blobs(self.id, &blobs).map_err(engine_error)
    }

    /// The blobs the replica has newly needed since `since` (empty bytes: every blob the current
    /// state references), encoded the same way as [`PdfDocument::blobs`]: for live sync, so one
    /// change ships only the blobs it needs rather than the whole store.
    pub fn blobs_since(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        let since = if since.is_empty() { Version::empty() } else { Version::decode(since).map_err(engine_error)? };
        let blobs = self.auto.session_mut().blobs_since(self.id, &since).map_err(engine_error)?;
        Ok(encode_blobs(&blobs))
    }

    /// How many times the document left its replica because it refused a write or an undo. The
    /// document keeps working, undoing from its own history; the next edit makes a fresh replica,
    /// which collaborators must resync (its full [`PdfDocument::snapshot`]). Hosts watch this.
    pub fn kdm_resets(&self) -> u32 {
        u32::try_from(self.auto.session().kdm_resets(self.id)).unwrap_or(u32::MAX)
    }

    /// How many edits (overlay entries) were dropped because an undo or redo of a full rewrite
    /// (applying redactions, changing security, sanitizing, signing) brought back another base
    /// than the one they were made against: the person undoes a sanitize after an agent's
    /// comment, which goes with it. The editor tells the person when it grows
    /// (`web/src/droppedEdits.ts`).
    pub fn kdm_dropped(&self) -> u32 {
        u32::try_from(self.auto.session().kdm_dropped(self.id)).unwrap_or(u32::MAX)
    }

    /// The replicated state, the replica made first if there is none yet.
    fn replicated_state(&mut self) -> Result<Value, String> {
        self.auto.session_mut().version(self.id).map_err(engine_error)?;
        let kdm = self.auto.session().kdm(self.id).ok_or("the document has no replica")?;
        kdm.replica().state_json().map_err(engine_error)
    }

    /// Takes a draft's changes (from a replica forked from this one) in as one undo step of the
    /// person who accepts it, whoever made the draft (`mergeChanges`, as `kition-sdk`'s
    /// `export_kit_session!` answers it; KitModel M12a: `origin` is who the change is recorded
    /// as, the host passes the agent's): the engine's document follows, and the person's Undo
    /// takes the whole draft back. Add the draft's blobs first. Answers `{"damage", "summary", "scope"}`,
    /// `summary` the `StateDiff` of the replicated state before and after. A refused merge
    /// changes nothing and fails with its reason: the KDM's, or that the document was rewritten
    /// since the draft was made ([`kdm_model::REWRITTEN_SINCE`]), which the editor shows.
    pub fn merge_changes(&mut self, bytes: &[u8], origin: &Origin) -> Result<Value, String> {
        let before = self.replicated_state()?;
        let (scope, damage) = self.auto.session_mut().merge_changes(self.id, bytes, origin).map_err(engine_error)?;
        let after = self.replicated_state()?;
        let summary = kition_kit::kdm::diff::diff_states(&before, &after).to_json();
        Ok(json!({ "damage": damage, "summary": summary, "scope": scope.map(kition_kit::kdm::UndoScope::as_str) }))
    }

    /// What merging a draft's changes would change in the document now, as `StateDiff` JSON;
    /// nothing changes. Refuses what [`PdfDocument::merge_changes`] refuses, with the same
    /// reason, so the editor knows before the person accepts.
    pub fn compare_changes(&mut self, bytes: &[u8]) -> Result<Value, String> {
        Ok(self.auto.session_mut().compare_changes(self.id, bytes).map_err(engine_error)?.to_json())
    }

    /// Writes the state a snapshot holds (a checkpoint of this document, the Kit UI protocol's
    /// `replica/restore`) over the document by object id, as one undo step of `origin`'s scope:
    /// the engine's document follows. Add the blobs it references first. Nothing changes when the
    /// states are equal; another document's snapshot is refused.
    pub fn restore_state(&mut self, snapshot: &[u8], origin: &Origin) -> Result<Damage, String> {
        self.auto.session_mut().restore_state(self.id, snapshot, origin).map(|(_, damage)| damage).map_err(engine_error)
    }

    /// An agent's reply (JSON text) with every KDM object id it holds as a value or a key named as
    /// the tool interface names the object (its tool id, KitObjectModel OM9: a comment's `/NM`, a
    /// page's `page:<id>`, a field's `field:<id>`); unchanged without a replica. Text that merely
    /// holds an id stays. The document keeps its own ids.
    ///
    /// The editor sends every `ai/inspect`, `document/check` and `ai/run` reply through this, so
    /// it reads the tool ids of the objects the reply names alone (`ToolIds::show_reply`, through
    /// `Replica::tool_ids_of`), never the whole document's; a reply naming no object is answered
    /// as it is.
    pub fn model_reply(&self, value: &str) -> Result<String, String> {
        let value: Value = serde_json::from_str(value).map_err(|error| format!("the reply is not JSON: {error}"))?;
        let Some(kdm) = self.auto.session().kdm(self.id) else { return Ok(value.to_string()) };
        Ok(kition_kit::kdm::tool_ids::ToolIds::show_reply(kdm.replica(), &value).to_string())
    }

    /// `model/describe` (`KitObjectModel 1.0`), through the document's change exchange
    /// (`kition_kit::session::model::Exchange`, as `kition-pdf` serves it headless).
    pub fn model_describe(&mut self) -> Result<Value, String> {
        let mut exchange = std::mem::take(&mut self.exchange);
        let answer = exchange.describe(self);
        self.exchange = exchange;
        answer.map_err(|error| error.to_json().to_string())
    }

    /// `model/batch` through the document's change exchange: the batch's model operations run on
    /// a scratch replica caught up with this one, and their changes come in as an import; its
    /// commands run in the engine. A failure's message is the error's JSON (`{code, index,
    /// message}`).
    pub fn model_batch(&mut self, request: &str) -> Result<String, String> {
        let request: Value = serde_json::from_str(request)
            .map_err(|error| json!({ "code": "invalid-op", "index": null, "message": format!("the batch is not JSON: {error}") }).to_string())?;
        let mut exchange = std::mem::take(&mut self.exchange);
        let answer = exchange.batch(self, &request, false);
        self.exchange = exchange;
        answer.map(|answer| answer.to_string()).map_err(|error| error.to_json().to_string())
    }

    /// What changed since the version `since` (a receipt, `Replica::changes_since`), as
    /// `kition-sdk`'s `session::web` answers it: `{"changes": StateDiff}`, or `{"changes": null,
    /// "unavailable": "<why>"}` when the replica's log does not hold `since` (another document's
    /// version, one from before the document was reopened) or there is no replica yet.
    pub fn changes_since(&self, since: &[u8]) -> String {
        let Some(kdm) = self.auto.session().kdm(self.id) else {
            return json!({ "changes": null, "unavailable": "the document has no replica yet" }).to_string();
        };
        let replica = kdm.replica();
        let since = if since.is_empty() { Ok(Version::empty()) } else { Version::decode(since) };
        match since.and_then(|since| replica.changes_since(&since)) {
            Ok(changes) => json!({ "changes": changes.to_recorded_json(replica.schema()) }),
            Err(error) => json!({ "changes": null, "unavailable": error.to_string() }),
        }
        .to_string()
    }
}

/// Encodes blobs for one wasm call's worth of blob transfer
/// ([`PdfDocument::blobs`]/[`PdfDocument::add_blobs`]): `(64-byte ASCII-hex hash, u64 LE length,
/// bytes) * N`. Every hash the KDM model produces is a 64-char hex SHA-256, so the hash field
/// needs no length prefix of its own.
pub(crate) fn encode_blobs(blobs: &MemoryBlobs) -> Vec<u8> {
    let mut out = Vec::new();
    for (hash, data) in &blobs.0 {
        out.extend_from_slice(hash.as_bytes());
        out.extend_from_slice(&(data.len() as u64).to_le_bytes());
        out.extend_from_slice(data);
    }
    out
}

/// Reverses [`encode_blobs`]; truncated or malformed input is an error, never a panic.
pub(crate) fn decode_blobs(bytes: &[u8]) -> Result<MemoryBlobs, String> {
    let mut map = HashMap::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let hash_end = i.checked_add(64).ok_or("blob bundle: overflow")?;
        let hash = bytes.get(i..hash_end).ok_or("truncated blob bundle: hash")?;
        let hash = std::str::from_utf8(hash).map_err(|_| "blob bundle: hash is not ASCII".to_owned())?.to_owned();
        let len_end = hash_end.checked_add(8).ok_or("blob bundle: overflow")?;
        let len_bytes: [u8; 8] =
            bytes.get(hash_end..len_end).ok_or("truncated blob bundle: length")?.try_into().map_err(|_| "blob bundle: length".to_owned())?;
        let len = usize::try_from(u64::from_le_bytes(len_bytes)).map_err(|_| "blob bundle: length too large".to_owned())?;
        let data_end = len_end.checked_add(len).ok_or("blob bundle: overflow")?;
        let data = bytes.get(len_end..data_end).ok_or("truncated blob bundle: data")?;
        map.insert(hash, Arc::from(data));
        i = data_end;
    }
    Ok(MemoryBlobs(map))
}

/// The object model (`KitObjectModel 1.0`) over the document's change exchange: `kition-pdf` serves
/// `model.describe` and `model.batch` headless through `kition_kit::session::model::Exchange`,
/// and `model.open`, `model.import` and `model.export` with the blobs the changes reference.
impl kition_kit::session::model::ReplicaPort for PdfDocument {
    fn model_schema(&self) -> std::sync::Arc<kition_kit::kdm::Schema> {
        <kdm_model::PdfModel as kition_kit::session::KitModel>::schema()
    }

    fn model_export(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        self.export_changes(since)
    }

    fn model_import(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.import_changes(bytes).map(|_| ())
    }

    fn model_command(&mut self, command: &str, params: Value, origin: &Origin) -> Result<Value, String> {
        self.execute_as(command, params, origin)
    }

    fn model_commands(&self) -> Vec<kition_kit::KitCommand> {
        self.commands()
    }

    /// The blobs a peer's changes reference (the base PDF, edited stream data), before the
    /// `model.import` that needs them.
    fn model_add_blobs(&mut self, blobs: &[u8]) -> Result<(), String> {
        self.add_blobs(blobs)
    }

    /// The blobs a replica at `since` needs for the changes since then: with empty `since` (a
    /// full `model.export`), every blob the replica references, the base PDF first among them, so
    /// the export opens elsewhere (`model.open`, an agent's headless draft) as the same document.
    fn model_blobs_since(&mut self, since: &[u8]) -> Result<Vec<u8>, String> {
        self.blobs_since(since)
    }

    /// A shared PDF with no file yet (a server's headless kit opening a sync room): the replica
    /// alone (its base travels in `blobs`), failing rather than falling back to an empty document.
    fn model_open_replica(name: &str, snapshot: &[u8], blobs: &[u8]) -> Result<Self, String> {
        let blobs = decode_blobs(blobs)?;
        let mut auto = Automation::new();
        let id = auto.session_mut().open_with_replica(name.to_owned(), &[], snapshot, blobs, None).map_err(engine_error)?;
        Ok(Self {
            auto,
            id,
            renderer: RefCell::new(None),
            saved: RefCell::new(None),
            saved_revision: Cell::new(None),
            pasteboard: PASTEBOARD,
            exchange: Default::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use kition_kit::KitDocument;
    use serde_json::json;

    use super::*;

    fn fixture() -> Vec<u8> {
        let mut doc = PdfDocument::create("Report.pdf", 612, 792).unwrap();
        doc.execute("page.insertBlank", json!({ "at": 2 })).unwrap();
        doc.execute("form.addField", json!({ "page": 1, "type": "text", "rect": [72, 600, 300, 624], "name": "Name" })).unwrap();
        doc.save(None).unwrap()
    }

    fn note(doc: &mut PdfDocument, page: u32, text: &str) {
        doc.execute("comment.add", json!({ "page": page, "type": "note", "at": [100, 100], "contents": text })).unwrap();
    }

    fn comment_count(doc: &mut PdfDocument) -> usize {
        doc.execute("comment.list", json!({})).unwrap()["comments"].as_array().map_or(0, Vec::len)
    }

    #[test]
    fn version_and_snapshot_make_a_replica_lazily() {
        let mut doc = PdfDocument::open("Report.pdf", &fixture()).unwrap();
        assert!(!doc.has_replica(), "opening a PDF makes no replica");
        assert_eq!(doc.doc_id(), None);
        let version = doc.version().unwrap();
        assert!(doc.has_replica());
        assert_eq!(doc.version().unwrap(), version, "asking twice makes no further replica");
        assert!(!doc.snapshot().unwrap().is_empty());
        assert!(doc.doc_id().is_some_and(|id| !id.is_empty()));
    }

    #[test]
    fn undo_redo_report_done_and_damage() {
        let mut doc = PdfDocument::open("Report.pdf", &fixture()).unwrap();
        assert!(!doc.can_undo());
        note(&mut doc, 1, "one");
        assert!(doc.can_undo());
        let (done, damage) = doc.undo().unwrap();
        assert!(done);
        assert_ne!(damage, Damage::None);
        assert!(doc.can_redo());
        assert!(doc.redo().unwrap().0);
        doc.undo().unwrap();
        let (done, damage) = doc.undo().unwrap();
        assert!(!done, "nothing to undo is a normal false");
        assert_eq!(damage, Damage::None);
    }

    #[test]
    fn execute_as_records_the_given_origin() {
        let mut doc = PdfDocument::open("Report.pdf", &fixture()).unwrap();
        let agent = Origin::agent("agent:test");
        doc.execute_as("comment.add", json!({ "page": 1, "type": "note", "at": [10, 10], "contents": "agent" }), &agent).unwrap();
        assert_eq!(comment_count(&mut doc), 1);
        // Engine rule E14a: canUndo is the person's, who has no step; the agent's undo takes it back.
        assert!(!doc.can_undo());
        doc.execute_as("edit.undo", json!({}), &agent).unwrap();
        assert_eq!(comment_count(&mut doc), 0);
    }

    #[test]
    fn blobs_round_trip_through_encode_and_decode() {
        let mut a = MemoryBlobs::default();
        a.0.insert("a".repeat(64), Arc::from(b"hello".as_slice()));
        a.0.insert("b".repeat(64), Arc::from(b"".as_slice()));
        let b = decode_blobs(&encode_blobs(&a)).unwrap();
        assert_eq!(b.0.len(), 2);
        assert_eq!(b.0.get(&"a".repeat(64)).map(|v| v.as_ref()), Some(b"hello".as_ref()));
        assert!(decode_blobs(&[1, 2, 3]).is_err());
        let mut malformed = "a".repeat(64).into_bytes();
        malformed.extend_from_slice(&u64::MAX.to_le_bytes());
        assert!(decode_blobs(&malformed).is_err(), "a length claiming more data than exists");
    }

    /// A PDF saved with its replica reopens as that replica: same version, same pixels, not
    /// dirty, and the next save appends to the file.
    #[test]
    fn a_pdf_reopens_with_its_stored_replica() {
        let mut a = PdfDocument::open("Report.pdf", &fixture()).unwrap();
        note(&mut a, 1, "kept");
        let bytes = a.save(None).unwrap();
        let (snapshot, blobs) = (a.snapshot().unwrap(), a.blobs().unwrap());
        a.mark_saved();
        let version = a.version().unwrap();
        let mut b = PdfDocument::open_with_replica("Report.pdf", &bytes, &snapshot, &blobs).unwrap();
        assert!(b.has_replica());
        assert_eq!(b.version().unwrap(), version);
        assert_eq!(b.doc_id(), a.doc_id());
        assert!(!b.dirty());
        assert_eq!(b.render(0.0, 0.0, 1.0, b.width(), 200).unwrap(), a.render(0.0, 0.0, 1.0, a.width(), 200).unwrap());
        note(&mut b, 2, "next");
        let next = b.save(None).unwrap();
        assert_eq!(&next[..bytes.len()], bytes.as_slice(), "the next save appends to the file");
    }

    /// A damaged stored replica never stops the file from opening: it opens plainly.
    #[test]
    fn a_damaged_stored_replica_opens_the_file_plainly() {
        let bytes = fixture();
        for (snapshot, blobs) in [(&b"not a snapshot"[..], &[][..]), (&[][..], &[1u8, 2, 3][..])] {
            let doc = PdfDocument::open_with_replica("Report.pdf", &bytes, snapshot, blobs).unwrap();
            assert!(!doc.has_replica(), "the file opened plainly");
            assert_eq!(doc.inspect().unwrap()["pages"].as_array().map(Vec::len), Some(2));
        }
        assert!(PdfDocument::open_with_replica("Report.pdf", b"not a pdf", b"not a snapshot", &[]).is_err());
        assert!(PdfDocument::open_with_replica("Report.pdf", &[], b"not a snapshot", &[]).is_err());
    }

    /// With no file yet (empty bytes), the replica alone opens as the same document.
    #[test]
    fn a_replica_opens_without_a_file() {
        let mut a = PdfDocument::open("Report.pdf", &fixture()).unwrap();
        note(&mut a, 1, "shared");
        let changes = a.export_changes(&[]).unwrap();
        let mut b = PdfDocument::open_with_replica("Shared.pdf", &[], &changes, &a.blobs().unwrap()).unwrap();
        assert_eq!(b.doc_id(), a.doc_id());
        assert_eq!(b.version().unwrap(), a.version().unwrap());
        assert!(!b.dirty());
        assert_eq!(comment_count(&mut b), 1);
        assert!(PdfDocument::open("Shared.pdf", &b.save(None).unwrap()).is_ok());
    }

    /// Two documents of one replica converge through the public SDK surface alone: a comment
    /// and a form fill each way, with only the blobs a change needs.
    #[test]
    fn two_documents_converge_through_export_and_import_changes() {
        let mut a = PdfDocument::open("Report.pdf", &fixture()).unwrap();
        note(&mut a, 1, "first");
        let bytes = a.save(None).unwrap();
        let (snapshot, blobs) = (a.snapshot().unwrap(), a.blobs().unwrap());
        let mut b = PdfDocument::open_with_replica("Report.pdf", &bytes, &snapshot, &blobs).unwrap();

        let since = b.version().unwrap();
        note(&mut a, 2, "from a");
        b.add_blobs(&a.blobs_since(&since).unwrap()).unwrap();
        b.import_changes(&a.export_changes(&since).unwrap()).unwrap();
        assert_eq!(comment_count(&mut b), 2);

        let since = a.version().unwrap();
        b.execute("form.fill", json!({ "values": { "Name": "Bob" } })).unwrap();
        a.add_blobs(&b.blobs_since(&since).unwrap()).unwrap();
        a.import_changes(&b.export_changes(&since).unwrap()).unwrap();
        assert_eq!(a.version().unwrap(), b.version().unwrap());
        let value = |doc: &mut PdfDocument| doc.execute("form.listFields", json!({})).unwrap()["fields"][0]["value"].clone();
        assert_eq!(value(&mut a), json!("Bob"));
        assert_eq!(a.render(0.0, 0.0, 1.0, a.width(), 300).unwrap(), b.render(0.0, 0.0, 1.0, b.width(), 300).unwrap(), "the same pixels");
        assert_eq!(a.kdm_resets() + b.kdm_resets(), 0);
        assert_eq!(a.kdm_dropped() + b.kdm_dropped(), 0);
    }

    /// The object ids of the document's replica, in order.
    fn object_ids(doc: &PdfDocument) -> Vec<String> {
        let kdm = doc.auto.session().kdm(doc.id).unwrap();
        kdm.replica().state_json().unwrap()["objects"].as_object().unwrap().keys().cloned().collect()
    }

    /// An unchanged document reopened with its stored replica saves the file it opened, byte for
    /// byte, and its replica keeps the document id and every object id; so does the next reopen,
    /// from the reopened document's own snapshot.
    #[test]
    fn an_unchanged_reopened_document_saves_its_bytes_and_keeps_its_object_ids() {
        let mut a = PdfDocument::open("Report.pdf", &fixture()).unwrap();
        note(&mut a, 1, "kept");
        let bytes = a.save(None).unwrap();
        let (mut snapshot, mut blobs) = (a.snapshot().unwrap(), a.blobs().unwrap());
        a.mark_saved();
        let ids = object_ids(&a);
        assert!(ids.len() > 1, "the root and the typed objects: {ids:?}");
        for _ in 0..2 {
            let mut b = PdfDocument::open_with_replica("Report.pdf", &bytes, &snapshot, &blobs).unwrap();
            assert!(!b.dirty());
            assert_eq!(b.save(None).unwrap(), bytes, "an unchanged document saves the file it opened");
            assert_eq!(b.doc_id(), a.doc_id());
            assert_eq!(object_ids(&b), ids, "the same object ids after reopening");
            (snapshot, blobs) = (b.snapshot().unwrap(), b.blobs().unwrap());
        }
    }

    fn person() -> Origin {
        Origin::person("person:local")
    }

    /// The contents of the document's comments, sorted.
    fn contents(doc: &mut PdfDocument) -> Vec<String> {
        let listed = doc.execute("comment.list", json!({})).unwrap();
        let mut out: Vec<String> =
            listed["comments"].as_array().unwrap().iter().filter_map(|comment| comment["contents"].as_str().map(str::to_owned)).collect();
        out.sort();
        out
    }

    /// A draft (a replica forked from the document's) merges in as one step of the person's Undo:
    /// the comparison beforehand names what the merge does and changes nothing, the engine's
    /// document follows the merge, and Undo and Redo take the whole draft back and again. A draft
    /// made while the document edited too merges key by key (data model 6.6, D3): a draft's
    /// comment on page 2 and the document's later one on page 1 write different `overlay` keys,
    /// and both stay. A draft and a later document edit that both rewrite one key (a comment each
    /// on page 1, which writes that page's object) are refused, the refusal naming the key, as are
    /// bytes that are no changes, and a refused merge changes nothing.
    #[test]
    fn a_draft_merges_in_as_one_undo_step_and_the_document_follows() {
        let mut doc = PdfDocument::open("Report.pdf", &fixture()).unwrap();
        note(&mut doc, 1, "before");
        let draft_of = |doc: &mut PdfDocument, page: u32, text: &str| -> (Vec<u8>, Vec<u8>) {
            let base = doc.version().unwrap();
            let mut draft = PdfDocument::open_with_replica("Report.pdf", &[], &doc.snapshot().unwrap(), &doc.blobs().unwrap()).unwrap();
            note(&mut draft, page, text);
            (draft.export_changes(&base).unwrap(), draft.blobs_since(&base).unwrap())
        };
        let (changes, blobs) = draft_of(&mut doc, 2, "from the draft");
        doc.add_blobs(&blobs).unwrap();
        let version = doc.version().unwrap();
        let preview = doc.compare_changes(&changes).unwrap();
        assert_eq!(doc.version().unwrap(), version, "comparing changes nothing");
        let answer = doc.merge_changes(&changes, &person()).unwrap();
        assert_eq!(answer["summary"], preview, "the comparison named what the merge did: {answer}");
        assert_ne!(answer["damage"], json!({ "kind": "none" }), "{answer}");
        assert_eq!(contents(&mut doc), ["before", "from the draft"], "the engine's document follows the merge");
        assert!(doc.dirty());
        assert!(PdfDocument::open("Merged.pdf", &doc.save(None).unwrap()).is_ok());
        assert!(doc.undo().unwrap().0);
        assert_eq!(contents(&mut doc), ["before"], "one undo takes the whole draft back");
        assert!(doc.redo().unwrap().0);
        assert_eq!(contents(&mut doc), ["before", "from the draft"]);
        assert!(doc.undo().unwrap().0);
        assert_eq!(contents(&mut doc), ["before"], "the person's earlier step is still below it");

        let (changes, blobs) = draft_of(&mut doc, 2, "late");
        doc.add_blobs(&blobs).unwrap();
        note(&mut doc, 1, "meanwhile");
        let answer = doc.merge_changes(&changes, &person()).unwrap();
        assert_ne!(answer["damage"], json!({ "kind": "none" }), "{answer}");
        assert_eq!(contents(&mut doc), ["before", "late", "meanwhile"], "a draft and a later edit of other overlay keys both stay");
        assert!(doc.undo().unwrap().0);
        assert_eq!(contents(&mut doc), ["before", "meanwhile"], "one undo takes the draft back and leaves the document's edit");

        let keys = overlay_keys(&mut doc);
        assert!(!keys.is_empty(), "the page 1 comments rewrote page 1's objects");
        let (changes, blobs) = draft_of(&mut doc, 1, "same page");
        doc.add_blobs(&blobs).unwrap();
        note(&mut doc, 1, "also meanwhile");
        let at = doc.version().unwrap();
        let refused = doc.merge_changes(&changes, &person()).unwrap_err();
        assert!(refused.contains("stale"), "{refused}");
        assert!(
            keys.iter().any(|key| refused.contains(&format!("overlay[\"{key}\"]"))),
            "the refusal names the overlay key both wrote ({keys:?}): {refused}"
        );
        assert!(doc.compare_changes(&changes).is_err());
        assert!(doc.merge_changes(b"not changes", &person()).is_err());
        assert_eq!(doc.version().unwrap(), at, "a refused merge changes nothing");
        assert_eq!(contents(&mut doc), ["also meanwhile", "before", "meanwhile"]);
    }

    /// A draft's preview (`compareChanges`, from the session's merged state) runs the kit's repair
    /// on its copy of the merge, as accepting the draft does: the previewed merged state is the
    /// replicated state after actually merging, for a draft alone and for one made while the
    /// document edited another page. (The kit's repair deletes overlay entries of another base;
    /// a draft whose merge would hold any is refused by the preview and the merge alike, in the
    /// two tests below, so an accepted draft's repair finds nothing to delete.)
    #[test]
    fn a_drafts_preview_is_the_state_merging_gives() {
        let mut doc = PdfDocument::open("Report.pdf", &fixture()).unwrap();
        note(&mut doc, 1, "before");
        let (changes, blobs) = draft_with(&mut doc, |draft| note(draft, 2, "from the draft"));
        doc.add_blobs(&blobs).unwrap();
        let previewed = doc.auto.session().kdm(doc.id).unwrap().merged_state(&changes).unwrap();
        let summary = doc.compare_changes(&changes).unwrap();
        let answer = doc.merge_changes(&changes, &person()).unwrap();
        assert_eq!(answer["summary"], summary, "{answer}");
        assert_eq!(doc.replicated_state().unwrap(), previewed, "the preview's merged state is the state after merging");

        let (changes, blobs) = draft_with(&mut doc, |draft| note(draft, 2, "late"));
        doc.add_blobs(&blobs).unwrap();
        note(&mut doc, 1, "meanwhile");
        let previewed = doc.auto.session().kdm(doc.id).unwrap().merged_state(&changes).unwrap();
        let summary = doc.compare_changes(&changes).unwrap();
        let answer = doc.merge_changes(&changes, &person()).unwrap();
        assert_eq!(answer["summary"], summary, "{answer}");
        assert_eq!(doc.replicated_state().unwrap(), previewed, "with the document's own edit meanwhile too");
        assert_eq!(contents(&mut doc), ["before", "from the draft", "late", "meanwhile"]);
        assert_eq!(doc.kdm_dropped(), 0);
    }

    /// The keys of the document's replica's `overlay` map (the objects its edits wrote).
    fn overlay_keys(doc: &mut PdfDocument) -> Vec<String> {
        let state = doc.replicated_state().unwrap();
        let pdf = state["objects"].as_object().unwrap().values().find(|object| object["$type"] == "Pdf").cloned();
        let overlay = pdf.and_then(|pdf| pdf["overlay"].as_object().cloned()).unwrap_or_else(|| panic!("no overlay map: {state}"));
        overlay.keys().cloned().collect()
    }

    /// An earlier state (a checkpoint's snapshot) is written over the document by object id as one
    /// undo step: the replica holds that state's objects again, under their ids, and Undo takes it
    /// back.
    #[test]
    fn a_restored_state_keeps_the_ids_and_undoes_as_one_step() {
        use kition_kit::kdm::{ActorId, Replica};
        let mut doc = PdfDocument::open("Report.pdf", &fixture()).unwrap();
        note(&mut doc, 1, "kept");
        let checkpoint = doc.snapshot().unwrap();
        note(&mut doc, 2, "later");
        assert_eq!(contents(&mut doc), ["kept", "later"]);
        let damage = doc.restore_state(&checkpoint, &person()).unwrap();
        assert_ne!(damage, Damage::None);
        assert_eq!(contents(&mut doc), ["kept"], "the engine's document follows the restore");
        let schema = <kdm_model::PdfModel as kition_kit::session::KitModel>::schema();
        let then = Replica::load(schema, &checkpoint, ActorId::random()).unwrap().state_json().unwrap();
        let ids: Vec<String> = then["objects"].as_object().unwrap().keys().cloned().collect();
        assert_eq!(object_ids(&doc), ids, "the checkpoint's objects, under their ids");
        let at = doc.version().unwrap();
        assert_eq!(doc.restore_state(&checkpoint, &person()).unwrap(), Damage::None, "the same state again changes nothing");
        assert_eq!(doc.version().unwrap(), at);
        assert!(doc.undo().unwrap().0);
        assert_eq!(contents(&mut doc), ["kept", "later"], "one undo takes the restore back");
        let other = PdfDocument::open("Report.pdf", &fixture()).unwrap().snapshot().unwrap();
        assert!(doc.restore_state(&other, &person()).is_err(), "another document's state is refused");
    }

    /// The KDM object id of the comment whose tool id is `tool`.
    fn comment_object(doc: &mut PdfDocument, tool: &str) -> String {
        let state = doc.replicated_state().unwrap();
        let found = state["objects"].as_object().unwrap().iter().find(|(_, object)| object["$type"] == "Comment" && object["id"] == tool);
        found.map(|(id, _)| id.clone()).unwrap_or_else(|| panic!("no Comment with tool id {tool}: {state}"))
    }

    /// The id the kit lists for the comment whose contents are `text`.
    fn listed_id(doc: &mut PdfDocument, text: &str) -> String {
        let listed = doc.execute("comment.list", json!({})).unwrap();
        let found = listed["comments"].as_array().unwrap().iter().find(|comment| comment["contents"] == text).cloned();
        found.and_then(|comment| comment["id"].as_str().map(str::to_owned)).unwrap_or_else(|| panic!("no comment {text}: {listed}"))
    }

    /// A reply names the comment's KDM object by its tool id (its `/NM`, what `comment.list` and
    /// receipts name it by); text that merely holds an id stays.
    #[test]
    fn a_reply_names_objects_by_their_tool_ids() {
        let mut doc = PdfDocument::open("Report.pdf", &fixture()).unwrap();
        note(&mut doc, 1, "kept");
        let tool = listed_id(&mut doc, "kept");
        let object = comment_object(&mut doc, &tool);
        assert_ne!(object, tool, "the KDM object id is the replica's own, not the comment's /NM");
        let shown: Value =
            serde_json::from_str(&doc.model_reply(&json!({ "target": object, "note": format!("about {object}") }).to_string()).unwrap()).unwrap();
        assert_eq!(shown["target"], json!(tool), "{shown}");
        assert_eq!(shown["note"], json!(format!("about {object}")), "text that holds an id stays");
        assert!(doc.model_reply("not json").is_err());
    }

    /// One id per comment at the tool interface: the id `comment.list` gives is the one the receipt
    /// names, the one `modelBatch` loads by, and after a save and a reopen with the stored replica
    /// it still names the same comment, behind which the replica keeps the same KDM object.
    #[test]
    fn a_comments_id_from_one_answer_works_in_the_next_call_and_after_a_reopen() {
        let mut doc = PdfDocument::open("Report.pdf", &fixture()).unwrap();
        let since = doc.version().unwrap();
        note(&mut doc, 1, "kept");
        let tool = listed_id(&mut doc, "kept");
        let receipt: Value = serde_json::from_str(&doc.changes_since(&since)).unwrap();
        let created: Vec<&str> = receipt["changes"]["created"].as_array().unwrap().iter().filter_map(|object| object["id"].as_str()).collect();
        assert!(created.contains(&tool.as_str()), "the receipt names the comment by the listed id: {receipt}");
        let described: Value = doc.model_describe().unwrap();
        assert_eq!(described["capability"], "KitObjectModel 1.0", "{described}");
        let load = |doc: &mut PdfDocument, target: &str| -> Value {
            let request = json!({ "ops": [], "load": [{ "target": target, "fields": ["contents"] }] });
            serde_json::from_str(&doc.model_batch(&request.to_string()).unwrap()).unwrap()
        };
        assert_eq!(load(&mut doc, &tool)["loaded"][0]["fields"]["contents"], "kept");
        let object = comment_object(&mut doc, &tool);
        let bytes = doc.save(None).unwrap();
        let (snapshot, blobs) = (doc.snapshot().unwrap(), doc.blobs().unwrap());
        doc.mark_saved();
        let mut reopened = PdfDocument::open_with_replica("Report.pdf", &bytes, &snapshot, &blobs).unwrap();
        assert_eq!(listed_id(&mut reopened, "kept"), tool, "the listed id survives the reopen");
        assert_eq!(load(&mut reopened, &tool)["loaded"][0]["fields"]["contents"], "kept");
        assert_eq!(comment_object(&mut reopened, &tool), object, "the same KDM object behind it");
        assert!(reopened.model_batch("not json").is_err());
    }

    /// A receipt names what changed since a version of this replica, and a version from another
    /// replica's line is unavailable, never an empty change set.
    #[test]
    fn changes_since_answers_what_changed_or_why_it_cannot() {
        let mut a = PdfDocument::open("Report.pdf", &fixture()).unwrap();
        let since = a.version().unwrap();
        note(&mut a, 1, "new");
        let answer: Value = serde_json::from_str(&a.changes_since(&since)).unwrap();
        assert!(answer["changes"].is_object(), "{answer}");
        assert!(answer["changes"]["created"].as_array().is_some_and(|created| !created.is_empty()), "the comment's objects: {answer}");
        let mut other = PdfDocument::open("Report.pdf", &fixture()).unwrap();
        let foreign = other.version().unwrap();
        let answer: Value = serde_json::from_str(&a.changes_since(&foreign)).unwrap();
        assert!(answer["changes"].is_null(), "{answer}");
        assert!(answer["unavailable"].as_str().is_some_and(|why| !why.is_empty()), "{answer}");
    }
    /// `kition-pdf` serves the object model through `ReplicaPort`: a command and a model edit in
    /// batches reach the engine, and a read-only batch refuses writes.
    #[test]
    fn serves_the_object_model_over_its_change_exchange() {
        use kition_kit::session::model::{Exchange, ModelErrorCode};
        use serde_json::{Value, json};

        let mut doc = PdfDocument::open("Report.pdf", &fixture()).unwrap();
        let mut exchange = Exchange::new();
        let described = exchange.describe(&mut doc).unwrap();
        assert_eq!(described["capability"], "KitObjectModel 1.0");
        assert!(described["commands"].as_array().unwrap().iter().any(|command| command["id"] == "comment.add"));

        let command = json!({ "ops": [{ "op": "command", "id": "comment.add", "params": { "page": 1, "type": "note", "at": [10, 10], "contents": "agent" } }] });
        assert_eq!(exchange.batch(&mut doc, &command, true).unwrap_err().code, ModelErrorCode::ReadOnly);
        let before = doc.version().unwrap();
        exchange.batch(&mut doc, &command, false).unwrap();
        assert_ne!(doc.version().unwrap(), before, "the command ran in the engine");

        let load = |exchange: &mut Exchange, doc: &mut PdfDocument, target: &str, field: &str| -> Value {
            let answer = exchange.batch(doc, &json!({ "ops": [], "load": [{ "target": target, "fields": [field] }] }), false).unwrap();
            answer["loaded"][0]["fields"][field].clone()
        };
        assert_eq!(comment_count(&mut doc), 1);
        let overlay = load(&mut exchange, &mut doc, "root", "overlay");
        let keys: Vec<String> = overlay.as_object().unwrap().keys().cloned().collect();
        let remove: Vec<Value> = keys.iter().map(|key| json!({ "op": "map.delete", "target": "root", "field": "overlay", "key": key })).collect();
        exchange.batch(&mut doc, &json!({ "ops": remove }), false).unwrap();
        assert_eq!(comment_count(&mut doc), 0);
    }

    /// What the person sees of the document and what the replica holds: the replica's state hash,
    /// every comment and form field as the kit lists them, and the pixels of the first page.
    fn view(doc: &mut PdfDocument) -> (String, Value, Value, Vec<u8>) {
        let hash = doc.auto.session_mut().state_hash(doc.id).unwrap();
        let comments = doc.execute("comment.list", json!({})).unwrap();
        let fields = doc.execute("form.listFields", json!({})).unwrap();
        let pixels = doc.render(0.0, 0.0, 1.0, doc.width(), 300).unwrap();
        (hash, comments, fields, pixels)
    }

    /// A draft of `doc` as it is now, `edit` run on it: its changes and the blobs they need.
    fn draft_with(doc: &mut PdfDocument, edit: impl FnOnce(&mut PdfDocument)) -> (Vec<u8>, Vec<u8>) {
        let base = doc.version().unwrap();
        let mut draft = PdfDocument::open_with_replica("Report.pdf", &[], &doc.snapshot().unwrap(), &doc.blobs().unwrap()).unwrap();
        edit(&mut draft);
        (draft.export_changes(&base).unwrap(), draft.blobs_since(&base).unwrap())
    }

    /// The `overlay` map of the replicated state `state`, and its `epoch`.
    fn overlay_of(state: &Value) -> (serde_json::Map<String, Value>, String) {
        let pdf = &state["objects"]["pdf"];
        (pdf["overlay"].as_object().cloned().unwrap_or_default(), pdf["epoch"].as_str().unwrap_or_default().to_owned())
    }

    /// KitModel M12a: a draft an agent made (the host passes the agent's origin to `mergeChanges`)
    /// is one undo step of the person who accepts it, in the engine's history as in the
    /// replica's: the person's Undo takes it back and the agent's has nothing to take. With the
    /// person's own comment made meanwhile (another page: another overlay key and another page's
    /// comment list), one Undo gives back the exact state before the merge (the replica's state
    /// hash, every comment and field the kit lists, the pixels) and Redo the merged one.
    #[test]
    fn an_agents_draft_merges_as_the_persons_undo_step_and_undo_and_redo_are_exact() {
        use kition_kit::kdm::UndoScope;
        let mut doc = PdfDocument::open("Report.pdf", &fixture()).unwrap();
        note(&mut doc, 1, "before");
        let agent = Origin::agent("agent:drafter");
        let (changes, blobs) = draft_with(&mut doc, |draft| note(draft, 2, "from the agent"));
        note(&mut doc, 1, "meanwhile");
        doc.add_blobs(&blobs).unwrap();
        let before = view(&mut doc);
        let answer = doc.merge_changes(&changes, &agent).unwrap();
        assert_ne!(answer["damage"], json!({ "kind": "none" }), "{answer}");
        assert_eq!(answer["scope"], json!("person"), "the merge answers the person's history, as the replica records it: {answer}");
        assert_eq!(contents(&mut doc), ["before", "from the agent", "meanwhile"], "both sides' comments are there");
        let merged = view(&mut doc);
        let kdm = doc.auto.session().kdm(doc.id).unwrap();
        assert!(!kdm.can_undo_in(UndoScope::Agent), "the replica holds the merge in the person's history, none in the agent's");
        assert!(kdm.can_undo_in(UndoScope::Person));
        assert!(doc.can_undo(), "and so does the engine's");
        assert!(doc.execute_as("edit.undo", json!({}), &agent).is_err(), "the agent has no step to undo");
        assert_eq!(view(&mut doc), merged, "the agent's Undo changed nothing");
        assert!(doc.undo().unwrap().0);
        assert_eq!(view(&mut doc), before, "the person's one Undo gives back the exact state before the merge");
        assert_eq!(contents(&mut doc), ["before", "meanwhile"]);
        assert!(doc.redo().unwrap().0);
        assert_eq!(view(&mut doc), merged, "Redo gives back the merged state exactly");
        assert_eq!(doc.kdm_resets(), 0, "the engine's and the replica's histories stayed in step");
    }

    /// A restored version undoes and redoes exactly: one Undo gives back the state before the
    /// restore and Redo the restored one (the replica's state hash, the kit's comments and fields,
    /// the pixels).
    #[test]
    fn a_restore_undoes_and_redoes_exactly() {
        let mut doc = PdfDocument::open("Report.pdf", &fixture()).unwrap();
        note(&mut doc, 1, "kept");
        let checkpoint = doc.snapshot().unwrap();
        note(&mut doc, 2, "later");
        doc.execute("form.fill", json!({ "values": { "Name": "Ada" } })).unwrap();
        let before = view(&mut doc);
        assert_ne!(doc.restore_state(&checkpoint, &person()).unwrap(), Damage::None);
        let restored = view(&mut doc);
        assert_eq!(contents(&mut doc), ["kept"]);
        assert!(doc.undo().unwrap().0);
        assert_eq!(view(&mut doc), before, "one Undo gives back the exact state before the restore");
        assert!(doc.redo().unwrap().0);
        assert_eq!(view(&mut doc), restored, "Redo gives back the restored state exactly");
        assert_eq!(doc.kdm_resets(), 0);
    }

    /// A draft made before the document was rewritten in full (here its comments removed, which
    /// writes the file anew: a new base, its overlay cleared) is refused, saying so, by the
    /// comparison and by the merge, and changes nothing. The KDM's rules alone would take it in:
    /// the draft wrote only overlay keys the rewrite did not touch, so its entries, of the old
    /// base, would land beside the new one, and the repair would delete them without a word.
    /// A draft made after the rewrite merges.
    #[test]
    fn a_draft_made_before_a_full_rewrite_is_refused_saying_so() {
        let mut doc = PdfDocument::open("Report.pdf", &fixture()).unwrap();
        note(&mut doc, 1, "before");
        let forked = overlay_of(&doc.replicated_state().unwrap()).0;
        let base = doc.version().unwrap();
        let mut draft = PdfDocument::open_with_replica("Report.pdf", &[], &doc.snapshot().unwrap(), &doc.blobs().unwrap()).unwrap();
        note(&mut draft, 2, "from the draft");
        let drafted = overlay_of(&draft.replicated_state().unwrap()).0;
        let mut wrote: Vec<String> = drafted.iter().filter(|(key, value)| forked.get(*key) != Some(value)).map(|(key, _)| key.clone()).collect();
        wrote.sort();
        assert!(!wrote.is_empty(), "the draft's comment wrote overlay entries");
        let (changes, blobs) = (draft.export_changes(&base).unwrap(), draft.blobs_since(&base).unwrap());
        doc.execute("file.removeHidden", json!({ "categories": ["comments"] })).unwrap();
        assert!(contents(&mut doc).is_empty(), "the rewrite removed the comments");
        doc.add_blobs(&blobs).unwrap();

        // The KDM's rules alone, the kit's check aside.
        let kdm = doc.auto.session().kdm(doc.id).unwrap();
        let alone = kdm.replica().merged_state(&changes).unwrap_or_else(|error| panic!("the KDM refuses it by itself: {error}"));
        let (overlay, epoch) = overlay_of(&alone);
        let mut stale: Vec<String> = overlay.iter().filter(|(_, entry)| entry["epoch"] != json!(epoch)).map(|(key, _)| key.clone()).collect();
        stale.sort();
        assert_eq!(stale, wrote, "every entry the draft wrote would be of the old base");

        let at = (doc.version().unwrap(), view(&mut doc));
        let refused = doc.compare_changes(&changes).unwrap_err();
        assert!(refused.contains(kdm_model::REWRITTEN_SINCE), "the comparison says why: {refused}");
        let refused = doc.merge_changes(&changes, &Origin::agent("agent:drafter")).unwrap_err();
        assert!(refused.contains(kdm_model::REWRITTEN_SINCE), "the merge says why: {refused}");
        assert_eq!((doc.version().unwrap(), view(&mut doc)), at, "a refused merge changes nothing");
        assert_eq!(doc.kdm_dropped(), 0);

        let (changes, blobs) = draft_with(&mut doc, |draft| note(draft, 2, "after the rewrite"));
        doc.add_blobs(&blobs).unwrap();
        doc.merge_changes(&changes, &person()).unwrap();
        assert_eq!(contents(&mut doc), ["after the rewrite"], "a draft of the rewritten document merges");
        assert_eq!(doc.kdm_dropped(), 0);
    }

    /// A draft that rewrote the file in full while the document was edited is refused, saying so:
    /// the KDM's rules alone would take it in (the document's comment wrote overlay keys the
    /// draft's rewrite did not touch), and the document's comment, of the old base, would go.
    #[test]
    fn a_draft_that_rewrote_the_file_while_the_document_was_edited_is_refused_saying_so() {
        let mut doc = PdfDocument::open("Report.pdf", &fixture()).unwrap();
        note(&mut doc, 1, "before");
        let (changes, blobs) = draft_with(&mut doc, |draft| {
            draft.execute("file.removeHidden", json!({ "categories": ["comments"] })).unwrap();
        });
        note(&mut doc, 2, "meanwhile");
        doc.add_blobs(&blobs).unwrap();
        let kdm = doc.auto.session().kdm(doc.id).unwrap();
        let alone = kdm.replica().merged_state(&changes).unwrap_or_else(|error| panic!("the KDM refuses it by itself: {error}"));
        let (overlay, epoch) = overlay_of(&alone);
        assert!(overlay.values().any(|entry| entry["epoch"] != json!(epoch)), "the document's entries would be of the old base: {overlay:?}");
        let at = (doc.version().unwrap(), view(&mut doc));
        let refused = doc.compare_changes(&changes).unwrap_err();
        assert!(refused.contains(kdm_model::REWRITE_LOSES_EDITS), "{refused}");
        let refused = doc.merge_changes(&changes, &person()).unwrap_err();
        assert!(refused.contains(kdm_model::REWRITE_LOSES_EDITS), "{refused}");
        assert_eq!((doc.version().unwrap(), view(&mut doc)), at, "a refused merge changes nothing");
        assert_eq!(contents(&mut doc), ["before", "meanwhile"]);
    }

    /// `modelReply` reads the tool ids of the objects a reply names alone: in a document of many
    /// comments, a reply naming one (or every object) gives the ids the whole document's tool ids
    /// give, and a reply naming none comes back unchanged.
    #[test]
    fn a_reply_reads_the_tool_ids_of_the_objects_it_names() {
        use kition_kit::kdm::tool_ids::ToolIds;
        let mut doc = PdfDocument::open("Report.pdf", &fixture()).unwrap();
        for n in 0..40 {
            note(&mut doc, 1 + n % 2, &format!("comment {n}"));
        }
        let tool = listed_id(&mut doc, "comment 17");
        let object = comment_object(&mut doc, &tool);
        let state = doc.replicated_state().unwrap();
        let every: Vec<String> = state["objects"].as_object().unwrap().keys().cloned().collect();
        assert!(every.len() > 40, "{}", every.len());
        let full = |doc: &PdfDocument, reply: &Value| ToolIds::of_replica(doc.auto.session().kdm(doc.id).unwrap().replica()).show_reply_json(reply);
        let keyed: serde_json::Map<String, Value> = [(object.clone(), json!(1))].into_iter().collect();
        for reply in [json!({ "target": object, "note": format!("about {object}") }), json!({ "objects": every, "by": keyed })] {
            let shown: Value = serde_json::from_str(&doc.model_reply(&reply.to_string()).unwrap()).unwrap();
            assert_eq!(shown, full(&doc, &reply), "the same ids as the whole document's");
        }
        let shown: Value = serde_json::from_str(&doc.model_reply(&json!({ "target": object }).to_string()).unwrap()).unwrap();
        assert_eq!(shown["target"], json!(tool));
        let plain = json!({ "summary": "two pages", "pages": [1, 2], "note": "page:1" });
        assert_eq!(doc.model_reply(&plain.to_string()).unwrap(), plain.to_string(), "a reply naming no object comes back unchanged");
    }
}
