//! The repair after an import or an undo (`KitModel::validate`): overlay entries of another epoch
//! than the current one are deleted. Their object numbers belong to a base that a rebase (a full
//! save: redaction, security, sanitizing, a signature) replaced, so they mean nothing in the new
//! base and cannot be kept in it; the session counts them ([`crate::PdfSession::dropped`]). The
//! replica's changes apply in one order, so none is expected; the repair keeps the view readable
//! if one appears.
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
