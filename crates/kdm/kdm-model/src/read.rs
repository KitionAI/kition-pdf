//! Reading a `kit.pdf` replica back as a COS document: its base (opened once per base and kept,
//! [`Bases`]) with the current epoch's overlay entries applied, references to added objects
//! that are gone dropped from the page tree ([`crate::dangling`]), and the pages' `/Annots`
//! repaired after concurrent additions ([`crate::lists`]).

use std::collections::VecDeque;
use std::sync::Arc;

use kition_kit::kdm::{KdmError, Replica};
use kition_kit::session::{BlobRef, BlobSource};
use kition_pdf_cos::{Document, ObjRef};
use serde_json::Value;

use crate::codec::Decoder;
use crate::ids::{Ids, base_number};
use crate::write::SAVE_KEYS;
use crate::{KdmResult, TRAILER, kdm_err, root_id};

/// How many opened bases a view keeps (the current one, and those an undo or a peer's rebase
/// may bring back).
const KEPT_BASES: usize = 4;

/// Bases opened afresh, by blob hash, most recent last. A base is opened (its cross-reference
/// data parsed) once; the view is a clone of it with the overlay applied.
#[derive(Clone, Default)]
pub struct Bases {
    open: VecDeque<(String, Document)>,
}

impl std::fmt::Debug for Bases {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bases").field("open", &self.open.len()).finish()
    }
}

impl Bases {
    pub fn get(&self, hash: &str) -> Option<&Document> {
        self.open.iter().find(|(h, _)| h == hash).map(|(_, doc)| doc)
    }

    /// Keeps `pristine` (a base with no edits) as the base whose blob hash is `hash`.
    pub fn insert(&mut self, hash: String, pristine: Document) {
        self.open.retain(|(h, _)| *h != hash);
        self.open.push_back((hash, pristine));
        while self.open.len() > KEPT_BASES {
            self.open.pop_front();
        }
    }

    /// The base with blob hash `hash`, opened from `blobs` with `password` if not open yet.
    pub fn open(&mut self, hash: &str, blobs: &dyn BlobSource, password: Option<&str>) -> KdmResult<Document> {
        if let Some(doc) = self.get(hash) {
            return Ok(doc.clone());
        }
        let bytes = blobs.get(hash).ok_or_else(|| KdmError::Engine(format!("the base PDF {hash} is missing")))?;
        let doc =
            Document::open_with_password(Arc::new(bytes.to_vec()), password).map_err(|e| kdm_err(format!("the base PDF does not open: {e}")))?;
        self.insert(hash.to_owned(), doc.clone());
        Ok(doc)
    }
}

/// The replica's base: its blob, highest object number and epoch.
pub(crate) struct Base {
    pub blob: BlobRef,
    pub max: u32,
    pub epoch: String,
}

pub(crate) fn base_of(replica: &Replica) -> KdmResult<Base> {
    let root = root_id();
    let blob: BlobRef = serde_json::from_value(replica.get(&root, "base")?).map_err(|e| kdm_err(format!("base: {e}")))?;
    let max = replica.get(&root, "baseMax")?.as_u64().and_then(|n| u32::try_from(n).ok()).unwrap_or(0);
    let epoch = replica.get(&root, "epoch")?.as_str().unwrap_or_default().to_owned();
    Ok(Base { blob, max, epoch })
}

/// The overlay entries of another epoch than the current one: edits a peer made against a base
/// that a rebase has since replaced. They are not read, and the repair deletes them.
pub(crate) fn stale_entries(replica: &Replica) -> KdmResult<Vec<String>> {
    let epoch = base_of(replica)?.epoch;
    Ok(replica
        .map_entries(&root_id(), "overlay")?
        .into_iter()
        .filter(|(_, value)| value.get("epoch").and_then(Value::as_str) != Some(epoch.as_str()))
        .map(|(key, _)| key)
        .collect())
}

/// The replica as a COS document: its base with the current epoch's edits, numbered through
/// `ids` (which keeps the numbers it gave before). Also answers the opened base.
pub fn read_document(
    replica: &Replica,
    blobs: &dyn BlobSource,
    bases: &mut Bases,
    ids: &mut Ids,
    password: Option<&str>,
) -> KdmResult<(Document, Document)> {
    let base = base_of(replica)?;
    let pristine = bases.open(&base.blob.hash, blobs, password)?;
    ids.enter(&base.epoch, base.max);
    let mut doc = pristine.clone();
    let floor = pristine.next_num();
    let entries: Vec<(String, Value)> = replica
        .map_entries(&root_id(), "overlay")?
        .into_iter()
        .filter(|(_, value)| value.get("epoch").and_then(Value::as_str) == Some(base.epoch.as_str()))
        .collect();
    // Number every added object first, in key order, so references between them resolve.
    let mut added = Vec::new();
    let mut freed = std::collections::HashSet::new();
    for (key, value) in &entries {
        if key == TRAILER {
            continue;
        }
        let num = ids.num_of(key, floor);
        if let Some(num) = num
            && base_number(key).is_none()
            && value.get("object").is_some()
        {
            added.push((num, key.clone(), crate::lists::marks_of(value)));
        }
    }
    for (key, value) in &entries {
        if key == TRAILER {
            let Some(object) = value.get("object") else { continue };
            let mut num = |k: &str| ids.num_of(k, floor);
            let mut trailer = Decoder { num: &mut num, blobs }.dict(object).map_err(|e| kdm_err(format!("trailer: {e}")))?;
            for save_key in SAVE_KEYS {
                trailer.remove(save_key);
                if let Some(v) = pristine.trailer().get(save_key) {
                    trailer.set(save_key.to_vec(), v.clone());
                }
            }
            *doc.trailer_mut() = trailer;
            continue;
        }
        let Some(target) = ids.num_of(key, floor) else { continue };
        let generation = value.get("gen").and_then(Value::as_u64).and_then(|g| u16::try_from(g).ok()).unwrap_or(0);
        if value.get("freed").and_then(Value::as_bool) == Some(true) {
            doc.set_freed(target, generation);
            freed.insert(target);
            continue;
        }
        let Some(object) = value.get("object") else { continue };
        let mut num = |k: &str| ids.num_of(k, floor);
        let object = Decoder { num: &mut num, blobs }.object(object).map_err(|e| kdm_err(format!("object {key}: {e}")))?;
        doc.set(ObjRef::new(target, generation), object);
    }
    // Objects that are gone while a peer's write may still list them: added objects this
    // numbering knows with no live entry (an undo took back the addition), and freed ones (a
    // deleted page). The page tree and the pages' `/Annots` stop listing them.
    let live: std::collections::HashSet<u32> = added.iter().map(|(num, _, _)| *num).collect();
    let mut missing: std::collections::HashSet<u32> = ids.added().map(|(_, num)| num).filter(|num| !live.contains(num)).collect();
    missing.extend(freed);
    crate::dangling::drop_missing(&mut doc, &missing);
    crate::lists::repair(&mut doc, &added);
    doc.raise_next_num(ids.next_free(floor));
    Ok((doc, pristine))
}
