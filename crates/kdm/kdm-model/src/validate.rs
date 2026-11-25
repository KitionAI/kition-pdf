//! The repair after an import, an undo or redo, a merge or a restore (`KitModel::validate`):
//! overlay entries of another epoch than the current one are deleted. Their object numbers belong
//! to another base than the replica's (a full save: redaction, security, sanitizing, a signature,
//! replaced it), so they mean nothing in it and cannot be kept; the session counts them
//! ([`crate::PdfSession::dropped`]). Only an undo or redo brings such entries: one scope's full
//! rewrite taken back under the other scope's later edits (the old base returns, the later
//! entries stay). An import does not (a peer's changes apply in one order and carry its own
//! repair), and a draft that would is refused before it merges
//! ([`crate::PdfSession::merged_state`]).
//!
//! Deterministic: the same state gives the same deletions.

use std::collections::BTreeSet;

use kition_kit::kdm::{ObjectId, Tx};
use serde_json::Value;

use crate::{KdmResult, root_id};

pub(crate) fn validate(tx: &mut Tx<'_>, changed: &BTreeSet<ObjectId>) -> KdmResult<bool> {
    let root = root_id();
    if !changed.contains(&root) || !tx.exists(&root) {
        return Ok(false);
    }
    let epoch = tx.get(&root, "epoch")?.as_str().unwrap_or_default().to_owned();
    let mut wrote = false;
    for (key, value) in tx.map_entries(&root, "overlay")? {
        let written_in = value.get("epoch").and_then(Value::as_str).unwrap_or_default().to_owned();
        if written_in != epoch {
            tx.map_delete(&root, "overlay", &key)?;
            wrote = true;
        }
    }
    Ok(wrote)
}
