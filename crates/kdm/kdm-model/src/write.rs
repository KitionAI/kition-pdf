//! Writing a COS document to a `kit.pdf` replica: the whole of it ([`write_document`]), what one
//! edit changed ([`write_changes`]), or a new base ([`rebase`]).

use std::sync::Arc;

use kition_kit::kdm::{DocId, KdmError, ObjectId, Tx};
use kition_kit::session::BlobSink;
use kition_pdf_cos::{Dict, Document, Object};
use serde_json::{Map, Value, json};

use crate::codec::Encoder;
use crate::ids::Ids;
use crate::lists;
use crate::{KdmResult, PDF_MIME, TRAILER, kdm_err, root_id, same_bytes};

/// Trailer entries every save writes anew: never replicated, always the base's own.
pub(crate) const SAVE_KEYS: [&[u8]; 3] = [b"Size", b"Prev", b"XRefStm"];

/// What a write wrote: the overlay keys set or deleted, and whether it was a rebase.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Written {
    pub keys: Vec<String>,
    pub rebased: bool,
}

/// A new epoch's name: random, so concurrent rebases never share one.
pub(crate) fn new_epoch() -> String {
    DocId::new().to_string()
}

/// The highest object number of a base, as a freshly opened `pristine` document of it sees it.
pub(crate) fn base_max(pristine: &Document) -> u32 {
    pristine.next_num().saturating_sub(1)
}

pub(crate) fn without_save_keys(trailer: &Dict) -> Dict {
    trailer.iter().filter(|(key, _)| !SAVE_KEYS.contains(&key.as_slice())).map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// Whether two trailers say the same apart from what saving writes anew.
pub(crate) fn same_trailer(a: &Dict, b: &Dict) -> bool {
    without_save_keys(a) == without_save_keys(b)
}

fn encode(object: &Object, ids: &mut Ids, blobs: &mut dyn BlobSink) -> KdmResult<Value> {
    let mut key = |num: u32| ids.key_of(num);
    Encoder { key: &mut key, blobs }.object(object).map_err(kdm_err)
}

/// One overlay entry of `epoch`: the object (generation `generation`), or freed. `detached`
/// names the lists an added object belongs in that leave it out ([`crate::lists`]).
pub(crate) fn entry(
    epoch: &str,
    generation: u16,
    object: Option<&Object>,
    detached: &[&str],
    ids: &mut Ids,
    blobs: &mut dyn BlobSink,
) -> KdmResult<Value> {
    Ok(match object {
        Some(object) if !detached.is_empty() => {
            json!({ "epoch": epoch, "gen": generation, "object": encode(object, ids, blobs)?, "detached": detached })
        }
        Some(object) => json!({ "epoch": epoch, "gen": generation, "object": encode(object, ids, blobs)? }),
        None => json!({ "epoch": epoch, "gen": generation, "freed": true }),
    })
}

fn trailer_entry(epoch: &str, trailer: &Dict, ids: &mut Ids, blobs: &mut dyn BlobSink) -> KdmResult<Value> {
    let trailer = without_save_keys(trailer);
    let mut key = |num: u32| ids.key_of(num);
    let value = Encoder { key: &mut key, blobs }.dict(&trailer).map_err(kdm_err)?;
    Ok(json!({ "epoch": epoch, "object": value }))
}

/// The list marks `detached` ([`lists::detached`]) gives `num`.
fn marks<'a>(detached: &'a std::collections::HashMap<u32, Vec<&'static str>>, num: u32) -> &'a [&'static str] {
    detached.get(&num).map_or(&[], Vec::as_slice)
}

fn blob_value(bytes: &Arc<Vec<u8>>, blobs: &mut dyn BlobSink) -> KdmResult<Value> {
    serde_json::to_value(blobs.put(bytes, PDF_MIME)).map_err(kdm_err)
}

/// Writes `doc`'s overlay and trailer (as edits of `pristine`, the base opened afresh) under
/// `ids`' current epoch.
fn write_overlay(tx: &mut Tx<'_>, doc: &Document, pristine: &Document, ids: &mut Ids, blobs: &mut dyn BlobSink, out: &mut Written) -> KdmResult<()> {
    let root = root_id();
    let epoch = ids.epoch().to_owned();
    let base_max = ids.base_max();
    let detached = lists::detached(doc, doc.overlay_entries().map(|(num, _, _)| num).filter(|num| *num > base_max));
    for (num, generation, object) in doc.overlay_entries() {
        let key = ids.key_of(num);
        let value = entry(&epoch, generation, object.as_deref(), marks(&detached, num), ids, blobs)?;
        tx.map_set(&root, "overlay", &key, value)?;
        out.keys.push(key);
    }
    if !same_trailer(doc.trailer(), pristine.trailer()) {
        let value = trailer_entry(&epoch, doc.trailer(), ids, blobs)?;
        tx.map_set(&root, "overlay", TRAILER, value)?;
        out.keys.push(TRAILER.to_owned());
    }
    Ok(())
}

/// Writes `doc` as a new replica's whole content, in its creating transaction: `pristine` (its
/// base opened afresh, sharing its bytes) as the base blob, its edits as the overlay of a new
/// epoch.
pub fn write_document(tx: &mut Tx<'_>, doc: &Document, pristine: &Document, ids: &mut Ids, blobs: &mut dyn BlobSink) -> KdmResult<Written> {
    let epoch = new_epoch();
    let max = base_max(pristine);
    ids.enter(&epoch, max);
    let mut fields = Map::new();
    fields.insert("base".into(), blob_value(pristine.bytes(), blobs)?);
    fields.insert("baseMax".into(), json!(max));
    fields.insert("epoch".into(), json!(epoch));
    tx.create_with_id("Pdf", &root_id(), fields)?;
    let mut out = Written { rebased: true, ..Written::default() };
    write_overlay(tx, doc, pristine, ids, blobs, &mut out)?;
    crate::projection::sync(tx, doc, ids)?;
    Ok(out)
}

/// A rebase transaction: `pristine`'s bytes become the base of a new epoch, the overlay is
/// cleared, and `doc`'s own edits of `pristine` (usually none) are its new overlay. Edits a
/// peer made against the old base meanwhile are dropped when they merge
/// ([`crate::validate`]).
///
/// The three registers are written first, always in the same order, so that of two concurrent
/// rebases every replica keeps all three of the same one.
pub fn rebase(tx: &mut Tx<'_>, doc: &Document, pristine: &Document, ids: &mut Ids, blobs: &mut dyn BlobSink) -> KdmResult<Written> {
    let root = root_id();
    let epoch = new_epoch();
    let max = base_max(pristine);
    tx.set(&root, "base", blob_value(pristine.bytes(), blobs)?)?;
    tx.set(&root, "baseMax", json!(max))?;
    tx.set(&root, "epoch", json!(epoch))?;
    let mut out = Written { rebased: true, ..Written::default() };
    for (key, _) in tx.map_entries(&root, "overlay")? {
        tx.map_delete(&root, "overlay", &key)?;
        out.keys.push(key);
    }
    ids.enter(&epoch, max);
    write_overlay(tx, doc, pristine, ids, blobs, &mut out)?;
    crate::projection::sync(tx, doc, ids)?;
    Ok(out)
}

/// What `doc` holds at `num` (an edit, or what its own base holds), as an overlay slot.
fn slot(doc: &Document, num: u32) -> Option<(u16, Option<Arc<Object>>)> {
    let generation = doc.generation(num);
    match doc.try_get(num) {
        Ok(object) if matches!(*object, Object::Null) && !doc.object_numbers().contains(&num) => None,
        Ok(object) => Some((generation, Some(object))),
        Err(_) => None,
    }
}

/// Writes what `after` changed from `before` (the replica's view as it was) as overlay entries.
///
/// `root` is the replica's base opened afresh: an object `after` no longer edits but its own
/// base holds differently from `root` (a saved file the view was rebased on locally, see
/// [`crate::PdfSession::adopt_local`]) is written as it is there; otherwise its entry is
/// deleted. When `after` sits on other bytes than `before` (a full save: redaction, security,
/// sanitizing, a signature), the write is a [`rebase`] onto `after`'s bytes, opened afresh as
/// `pristine_after`.
#[allow(clippy::too_many_arguments)]
pub fn write_changes(
    tx: &mut Tx<'_>,
    before: &Document,
    after: &Document,
    root: &Document,
    pristine_after: Option<&Document>,
    ids: &mut Ids,
    blobs: &mut dyn BlobSink,
) -> KdmResult<Written> {
    if !same_bytes(before.bytes(), after.bytes()) {
        let pristine = pristine_after.ok_or_else(|| KdmError::Engine("a rebase needs its new base opened".into()))?;
        return rebase(tx, after, pristine, ids, blobs);
    }
    let object_id: ObjectId = root_id();
    let epoch = ids.epoch().to_owned();
    let on_root = same_bytes(after.bytes(), root.bytes());
    ids.take_minted();
    let mut out = Written::default();
    let mut nums: Vec<u32> = before.overlay_entries().map(|(n, _, _)| n).chain(after.overlay_entries().map(|(n, _, _)| n)).collect();
    nums.sort_unstable();
    nums.dedup();
    let edits_of = |doc: &Document| -> std::collections::HashMap<u32, (u16, Option<Arc<Object>>)> {
        doc.overlay_entries().map(|(n, g, o)| (n, (g, o))).collect()
    };
    let (was, now) = (edits_of(before), edits_of(after));
    // Added objects a list leaves out are marked (`crate::lists`): every added object
    // this numbering knows (on a saved file of the view's, some sit in the file, not the
    // overlay), whose marks the view, read and repaired, matches.
    let base_max = ids.base_max();
    let mut added: Vec<u32> = ids.added().map(|(_, num)| num).chain(nums.iter().copied().filter(|num| *num > base_max)).collect();
    added.sort_unstable();
    added.dedup();
    let detached = lists::detached(after, added.iter().copied());
    let was_detached = lists::detached(before, added.iter().copied());
    let mut wrote = std::collections::HashSet::new();
    for num in nums {
        let old = was.get(&num);
        let new = now.get(&num);
        if let (Some((g0, o0)), Some((g1, o1))) = (old, new)
            && g0 == g1
            && match (o0, o1) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b) || a == b,
                (None, None) => true,
                _ => false,
            }
        {
            continue;
        }
        let key = ids.key_of(num);
        wrote.insert(num);
        match new {
            Some((generation, object)) => {
                let value = entry(&epoch, *generation, object.as_deref(), marks(&detached, num), ids, blobs)?;
                tx.map_set(&object_id, "overlay", &key, value)?;
            }
            None if on_root => tx.map_delete(&object_id, "overlay", &key)?,
            None => match (slot(after, num), slot(root, num)) {
                (Some(mine), Some(base)) if mine.0 == base.0 && mine.1 == base.1 => tx.map_delete(&object_id, "overlay", &key)?,
                (None, None) => tx.map_delete(&object_id, "overlay", &key)?,
                (Some((generation, object)), _) => {
                    let value = entry(&epoch, generation, object.as_deref(), marks(&detached, num), ids, blobs)?;
                    tx.map_set(&object_id, "overlay", &key, value)?;
                }
                (None, Some((generation, _))) => {
                    let value = entry(&epoch, generation.saturating_add(1), None, &[], ids, blobs)?;
                    tx.map_set(&object_id, "overlay", &key, value)?;
                }
            },
        }
        out.keys.push(key);
    }
    // An unchanged object a list stopped (or started) holding: only its marks change.
    for num in added {
        if wrote.contains(&num) || marks(&detached, num) == marks(&was_detached, num) {
            continue;
        }
        let Some((generation, Some(object))) = slot(after, num) else { continue };
        let key = ids.key_of(num);
        let value = entry(&epoch, generation, Some(&object), marks(&detached, num), ids, blobs)?;
        tx.map_set(&object_id, "overlay", &key, value)?;
        out.keys.push(key);
    }
    if !same_trailer(before.trailer(), after.trailer()) {
        if same_trailer(after.trailer(), root.trailer()) {
            tx.map_delete(&object_id, "overlay", TRAILER)?;
        } else {
            let value = trailer_entry(&epoch, after.trailer(), ids, blobs)?;
            tx.map_set(&object_id, "overlay", TRAILER, value)?;
        }
        out.keys.push(TRAILER.to_owned());
    }
    // An incremental save can create Info only in the adopted local bytes.
    // Editing that object does not change the local trailer reference, but the
    // replica's older base still needs the reference to make the edit reachable.
    let edited_local_info = !on_root && after.trailer().reference(b"Info").is_some_and(|info| info.num > base_max && wrote.contains(&info.num));
    if edited_local_info {
        let value = trailer_entry(&epoch, after.trailer(), ids, blobs)?;
        if tx.map_get(&object_id, "overlay", TRAILER)? != value {
            tx.map_set(&object_id, "overlay", TRAILER, value)?;
            out.keys.push(TRAILER.to_owned());
        }
    }
    if !on_root {
        write_local_objects(tx, after, &now, &epoch, ids, blobs, &mut out)?;
    }
    crate::projection::sync(tx, after, ids)?;
    Ok(out)
}

/// On a base of its own (a saved file the view was rebased on locally), a view can reference an
/// object the save added that the replica has never seen (the information dictionary a save
/// stamped `/ModDate` into): every such object the write referenced is written too, and what it
/// references in turn.
fn write_local_objects(
    tx: &mut Tx<'_>,
    after: &Document,
    edits: &std::collections::HashMap<u32, (u16, Option<Arc<Object>>)>,
    epoch: &str,
    ids: &mut Ids,
    blobs: &mut dyn BlobSink,
    out: &mut Written,
) -> KdmResult<()> {
    let object_id = root_id();
    let mut seen = std::collections::HashSet::new();
    let mut pending = ids.take_minted();
    let mut rounds = 0usize;
    while !pending.is_empty() && rounds < 10_000 {
        rounds += 1;
        for num in std::mem::take(&mut pending) {
            if edits.contains_key(&num) || !seen.insert(num) {
                continue;
            }
            let Some((generation, Some(object))) = slot(after, num) else { continue };
            let key = ids.key_of(num);
            let detached = lists::detached(after, [num]);
            let value = entry(epoch, generation, Some(&object), marks(&detached, num), ids, blobs)?;
            tx.map_set(&object_id, "overlay", &key, value)?;
            out.keys.push(key);
        }
        pending = ids.take_minted();
    }
    Ok(())
}
