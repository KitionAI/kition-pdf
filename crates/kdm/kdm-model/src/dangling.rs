//! References to added objects that are gone.
//!
//! Undoing an addition deletes the added objects' overlay entries; a peer that had seen them
//! may meanwhile have written an object that references one (the page tree after moving pages,
//! with a page the other just took back). The merge keeps both, and the reference resolves to
//! nothing: a page tree listing a page that does not exist is a file viewers refuse ("the
//! document has no pages"). The view therefore drops such references from the page tree (fixing
//! `/Count` on the way up) and from the pages' `/Annots`, the same on every replica. Other
//! dangling references read as `null`, as PDF defines.

use std::collections::HashSet;

use kition_pdf_cos::{Document, ObjRef, Object};

/// The deepest page tree walked (real files nest a few levels).
const MAX_DEPTH: usize = 64;

/// Drops references to `missing` (objects added since the base that have no live entry) from
/// the page tree and the pages' `/Annots`.
pub(crate) fn drop_missing(doc: &mut Document, missing: &HashSet<u32>) {
    if missing.is_empty() {
        return;
    }
    let Some(root) = doc.root() else { return };
    let Some(pages) = doc.get(root).as_dict().and_then(|d| d.reference(b"Pages")) else { return };
    let mut visited = HashSet::new();
    walk(doc, pages, missing, &mut visited, 0);
}

fn is_missing(object: &Object, missing: &HashSet<u32>) -> bool {
    object.as_ref().is_some_and(|r| missing.contains(&r.num))
}

/// Writes `list` as `key` of the dictionary `node`: into the array object it references when
/// indirect, else inline.
fn set_list(doc: &mut Document, node: ObjRef, key: &[u8], list: Vec<Object>) {
    let indirect = doc.get(node).as_dict().and_then(|d| d.reference(key)).filter(|r| doc.get(*r).as_array().is_some());
    match indirect {
        Some(array) => doc.set(array, Object::Array(list)),
        None => {
            let _ = doc.update_dict(node, |d| d.set(key.to_vec(), Object::Array(list)));
        }
    }
}

/// Fixes the subtree at `node`; answers how many pages it holds and whether anything changed.
fn walk(doc: &mut Document, node: ObjRef, missing: &HashSet<u32>, visited: &mut HashSet<u32>, depth: usize) -> (i64, bool) {
    if depth > MAX_DEPTH || !visited.insert(node.num) {
        return (0, false);
    }
    let object = doc.get(node);
    let Some(dict) = object.as_dict() else { return (0, false) };
    if dict.name(b"Type") != Some(b"Pages") {
        // A page: its `/Annots` stop listing what is gone.
        let Some(annots) = dict.get(b"Annots") else { return (1, false) };
        let list = doc.resolve(annots).as_array().cloned().unwrap_or_default();
        if list.iter().any(|a| is_missing(a, missing)) {
            let kept = list.into_iter().filter(|a| !is_missing(a, missing)).collect();
            set_list(doc, node, b"Annots", kept);
        }
        return (1, false);
    }
    let kids = dict.get(b"Kids").map(|k| doc.resolve(k).as_array().cloned().unwrap_or_default()).unwrap_or_default();
    let count = dict.int(b"Count");
    let kept: Vec<Object> = kids.iter().filter(|k| !is_missing(k, missing)).cloned().collect();
    let mut changed = kept.len() != kids.len();
    let mut total = 0;
    for kid in &kept {
        if let Some(r) = kid.as_ref() {
            let (pages, below) = walk(doc, r, missing, visited, depth + 1);
            total += pages;
            changed |= below;
        }
    }
    if kept.len() != kids.len() {
        set_list(doc, node, b"Kids", kept);
    }
    if changed && count != Some(total) {
        let _ = doc.update_dict(node, |d| d.set(b"Count".to_vec(), Object::Int(total)));
    }
    (total, changed)
}
