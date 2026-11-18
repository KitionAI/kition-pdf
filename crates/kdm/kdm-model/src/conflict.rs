//! Conflict objects (Kition platform, rule M22), as `kition_kdm::conflict` defines them:
//! the reserved `Conflict` type (declared in `model/schema.json`), ids derived from what a
//! conflict is about so every replica's repair records the same object, and resolution by
//! deleting it. Kept here until this crate's kition-sdk pin carries `kition_kdm::conflict`; the
//! ids and fields are the same.

use kition_kit::kdm::{KdmError, ObjectId, Replica, Tx};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::KdmResult;

pub const CONFLICT_TYPE: &str = "Conflict";

/// The kind of the conflict a full rewrite's repair records for each peer edit it drops.
pub const DROPPED_EDIT: &str = "pdf.droppedEdit";

/// The id of the conflict of `kind` about `key`: `kition_kdm::conflict::conflict_id`.
pub fn conflict_id(kind: &str, key: &str) -> ObjectId {
    let mut hash = Sha256::new();
    hash.update(kind.as_bytes());
    hash.update([0]);
    hash.update(key.as_bytes());
    let hex: String = hash.finalize().iter().take(16).map(|byte| format!("{byte:02x}")).collect();
    ObjectId::from_string(format!("conflict-{hex}"))
}

/// Records a conflict in a repair transaction unless the document already has it.
pub(crate) fn record(tx: &mut Tx<'_>, kind: &str, key: &str, summary: &str, objects: &[String], kept: Option<Value>) -> KdmResult<bool> {
    let id = conflict_id(kind, key);
    if tx.exists(&id) {
        return Ok(false);
    }
    let mut fields = Map::new();
    fields.insert("kind".into(), Value::String(kind.to_owned()));
    fields.insert("summary".into(), Value::String(summary.to_owned()));
    fields.insert("objects".into(), Value::Array(objects.iter().cloned().map(Value::String).collect()));
    if let Some(kept) = kept {
        fields.insert("kept".into(), kept);
    }
    match tx.create_with_id(CONFLICT_TYPE, &id, fields) {
        Ok(_) => Ok(true),
        Err(KdmError::InvalidValue { field, .. }) if field == "$id" => Ok(false),
        Err(error) => Err(error),
    }
}

/// One unresolved conflict, as the editor reports it to its host (Kit UI protocol section 7c).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ConflictSummary {
    pub id: String,
    pub kind: String,
    pub summary: String,
}

/// The document's unresolved conflicts, by id.
pub fn conflicts(replica: &Replica) -> Vec<ConflictSummary> {
    let text = |id: &ObjectId, field: &str| replica.get(id, field).ok().and_then(|value| value.as_str().map(str::to_owned)).unwrap_or_default();
    let mut out: Vec<ConflictSummary> = replica
        .objects_of_type(CONFLICT_TYPE)
        .into_iter()
        .map(|id| ConflictSummary { kind: text(&id, "kind"), summary: text(&id, "summary"), id: id.to_string() })
        .collect();
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Resolves a conflict: deletes its object.
pub fn resolve(tx: &mut Tx<'_>, id: &ObjectId) -> KdmResult<()> {
    tx.delete(id)
}
