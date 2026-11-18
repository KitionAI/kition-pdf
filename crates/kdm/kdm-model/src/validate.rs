//! The repair after a merge (`KitModel::validate`): overlay entries of another epoch than the
//! current one are deleted. They are edits a peer made against a base that a concurrent rebase
//! (a full save: redaction, security, sanitizing, a signature) replaced; their object numbers
//! mean nothing in the new base, so they cannot be kept in it. Each is recorded as a conflict
//! object instead (standard 2.0 M22, [`crate::conflict`]) that keeps the dropped entry, so the
//! editor can tell the person and nothing is lost silently; the session also counts them
//! ([`crate::PdfSession::dropped`]). A lease on the whole document while rewriting it in full
//! (sync protocol section 8) keeps them from happening while everyone is online.
//!
//! Deterministic: every replica deletes the same entries of the same state, and records the same
//! conflict objects (their ids derive from the entry and the epoch it was written in).

use std::collections::BTreeSet;

use kition_kit::kdm::{ObjectId, Tx};
use serde_json::Value;

use crate::conflict::{DROPPED_EDIT, record};
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
            let summary = format!("An edit of object {key}, made while someone rewrote the file in full, could not be kept");
            record(tx, DROPPED_EDIT, &format!("{key} {written_in}"), &summary, std::slice::from_ref(&key), Some(value))?;
            tx.map_delete(&root, "overlay", &key)?;
            wrote = true;
        }
    }
    Ok(wrote)
}
