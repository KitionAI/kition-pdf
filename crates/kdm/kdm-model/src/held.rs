//! Whether a saved file holds the replica's view ([`crate::PdfSession::holds`]).
//!
//! A host keeps a PDF's replica beside the file it saved. Saving appended the view's edits to the
//! base as incremental updates, numbering the added objects as that session numbered them,
//! and stamped `/ModDate`; the replica kept its base. So the file holds the view when it is the
//! base itself (and the view has no edits), or when it is the base with appended updates
//! whose objects are exactly the view's edits: the base's objects by number, the added ones by
//! where the edited objects reference them (a numbering found while comparing), the document
//! information dictionary compared without `/ModDate`, and streams without `/Length`.
//!
//! A false negative is safe (the file then wins, recorded onto the replica); only a match is
//! taken as "nothing changed".

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use kition_pdf_cos::{Dict, Document, Object, SaveOptions, XrefEntry, generated_file_id};

/// Most objects compared before giving up (the file then counts as changed).
const MAX_COMPARED: usize = 200_000;

/// A numbering of the view's added objects in the file, found while comparing.
#[derive(Default)]
struct Matching {
    to_file: HashMap<u32, u32>,
    from_file: HashMap<u32, u32>,
    queue: VecDeque<(u32, u32)>,
}

impl Matching {
    /// Whether view object `v` may be file object `f` (recording it, and queueing the pair).
    fn pair(&mut self, v: u32, f: u32) -> bool {
        match (self.to_file.get(&v), self.from_file.get(&f)) {
            (Some(known), _) => *known == f,
            (None, Some(_)) => false,
            (None, None) => {
                self.to_file.insert(v, f);
                self.from_file.insert(f, v);
                self.queue.push_back((v, f));
                true
            }
        }
    }
}

fn without(dict: &Dict, keys: &[&[u8]]) -> Dict {
    dict.iter().filter(|(k, _)| !keys.contains(&k.as_slice())).map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// Whether view object `a` equals file object `b`, references of added objects (numbers above
/// `base_max`) matched through `m`.
fn same(a: &Object, b: &Object, base_max: u32, m: &mut Matching, depth: usize) -> bool {
    if depth > 256 {
        return false;
    }
    match (a, b) {
        (Object::Ref(x), Object::Ref(y)) => {
            if x.generation != y.generation {
                return false;
            }
            if x.num <= base_max || y.num <= base_max { x.num == y.num } else { m.pair(x.num, y.num) }
        }
        // The writer writes a whole real as an integer.
        (Object::Real(r), Object::Int(i)) | (Object::Int(i), Object::Real(r)) => *r == *i as f64,
        (Object::String(x), Object::String(y)) => x.bytes == y.bytes,
        (Object::Array(x), Object::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q, base_max, m, depth + 1)),
        (Object::Dict(x), Object::Dict(y)) => same_dict(x, y, base_max, m, depth),
        (Object::Stream(x), Object::Stream(y)) => {
            x.raw.as_ref() == y.raw.as_ref() && same_dict(&without(&x.dict, &[b"Length"]), &without(&y.dict, &[b"Length"]), base_max, m, depth)
        }
        (x, y) => x == y,
    }
}

fn same_dict(x: &Dict, y: &Dict, base_max: u32, m: &mut Matching, depth: usize) -> bool {
    x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w, base_max, m, depth + 1)))
}

/// The live objects `file`'s appended updates wrote (at or after byte `from`).
fn appended(file: &Document, from: usize) -> HashSet<u32> {
    let from = from as u64;
    let in_update = |num: u32| match file.xref_entry(num) {
        Some(XrefEntry::InFile { offset, .. }) => offset >= from,
        _ => false,
    };
    file.object_numbers()
        .into_iter()
        .filter(|num| match file.xref_entry(*num) {
            Some(XrefEntry::InStream { stream, .. }) => in_update(stream),
            _ => in_update(*num),
        })
        .collect()
}

/// When `file` holds `view` (whose base is `pristine`), the numbering of the view's added
/// objects in it (view number → file number). See the module docs.
pub(crate) fn match_saved(
    view: &Document,
    pristine: &Document,
    base_max: u32,
    file: &Arc<Vec<u8>>,
    password: Option<&str>,
) -> Option<HashMap<u32, u32>> {
    let base = pristine.bytes();
    if crate::same_bytes(base, file) {
        let clean = !view.is_modified() && crate::write::same_trailer(view.trailer(), pristine.trailer());
        return clean.then(HashMap::new);
    }
    if file.len() <= base.len() || !file.starts_with(base) {
        return None;
    }
    let saved = Document::open_with_password(file.clone(), password).ok()?;
    if saved.revisions().len() <= pristine.revisions().len() || !saved.revisions().starts_with(pristine.revisions()) {
        return None;
    }
    let info = saved.trailer().reference(b"Info");
    let mut m = Matching::default();
    let mut expected: HashSet<u32> = HashSet::new();
    // All semantic trailer entries matter, including unknown extension keys. Only the
    // serialization keys and the separately checked information dictionary are omitted.
    let keys: &[&[u8]] = &[b"Size", b"Prev", b"XRefStm", b"Info"];
    let view_trailer = without(view.trailer(), keys);
    let mut saved_trailer = without(saved.trailer(), keys);
    // Saving a file without /ID adds the writer's deterministic default identifier. It may
    // remain absent from the replica after local saves, which intentionally add no KDM edits.
    // Recognize only that exact value; an existing or arbitrarily added ID still must match.
    if !view_trailer.contains(b"ID") && !pristine.trailer().contains(b"ID") {
        let expected_id = generated_file_id(base, SaveOptions::default().id_seed);
        if saved_trailer
            .get(b"ID")
            .and_then(Object::as_array)
            .is_some_and(|ids| ids.len() == 2 && ids.iter().all(|id| id.as_string().is_some_and(|id| id.bytes == expected_id)))
        {
            saved_trailer.remove(b"ID");
        }
    }
    if !same_dict(&view_trailer, &saved_trailer, base_max, &mut m, 0) {
        return None;
    }
    match (view.trailer().get(b"Info"), saved.trailer().get(b"Info")) {
        (Some(a), Some(b)) if same(a, b, base_max, &mut m, 0) => {
            // This also checks an unchanged base Info object, which has no overlay entry.
            let (a, b) = (view.resolve(a), saved.resolve(b));
            let (a, b) = (a.as_dict()?, b.as_dict()?);
            if !same_dict(&without(a, &[b"ModDate"]), &without(b, &[b"ModDate"]), base_max, &mut m, 0) {
                return None;
            }
        }
        (None, None) => {}
        // A generated Info object is permitted to contain only the save timestamp.
        (None, Some(Object::Ref(r))) => {
            let object = saved.try_get(r.num).ok()?;
            let dict = object.as_dict()?;
            if r.num <= base_max || dict.len() != 1 || dict.get(b"ModDate").and_then(Object::as_string).is_none() {
                return None;
            }
        }
        _ => return None,
    }
    let mut compared = 0usize;
    let compare = |v: u32, f: u32, m: &mut Matching| -> bool {
        let (Ok(a), Ok(b)) = (view.try_get(v), saved.try_get(f)) else { return false };
        if Some(f) == info.map(|r| r.num) {
            match (a.as_dict(), b.as_dict()) {
                (Some(x), Some(y)) => same_dict(&without(x, &[b"ModDate"]), &without(y, &[b"ModDate"]), base_max, m, 0),
                _ => same(&a, &b, base_max, m, 0),
            }
        } else {
            same(&a, &b, base_max, m, 0)
        }
    };
    // The current live xref set alone cannot expose an unrelated free entry. Unedited base
    // objects normally keep their xref entries (and their container's entry when compressed).
    // Load only relocated uncompressed objects: an appended revision may restore the exact
    // original value. Keep compressed relocation conservative until separately verified.
    for num in pristine.object_numbers().into_iter().filter(|num| !view.is_edited(*num)) {
        compared += 1;
        if compared > MAX_COMPARED {
            return None;
        }
        let before = pristine.xref_entry(num);
        let same_entry = before == saved.xref_entry(num)
            && match before {
                Some(XrefEntry::InStream { stream, .. }) => pristine.xref_entry(stream) == saved.xref_entry(stream),
                _ => true,
            };
        // The information dictionary's permitted timestamp difference was checked above.
        if !same_entry && Some(num) != info.map(|r| r.num) {
            let restored = matches!(
                (before, saved.xref_entry(num)),
                (
                    Some(XrefEntry::InFile { generation: original_generation, .. }),
                    Some(XrefEntry::InFile { generation, offset }),
                ) if generation == original_generation && offset >= base.len() as u64
            );
            if !restored {
                return None;
            }
            // Ordinary reads may recover an older object after the appended definition
            // fails. Compare the value read at this exact live xref location instead.
            let reference = kition_pdf_cos::ObjRef::new(num, saved.generation(num));
            let exact = saved.try_get_original_at_xref(reference).ok()?;
            let original = view.try_get(num).ok()?;
            if !same(&original, &exact, base_max, &mut m, 0) {
                return None;
            }
            expected.insert(num);
        }
    }
    for (num, generation, object) in view.overlay_entries() {
        compared += 1;
        if compared > MAX_COMPARED {
            return None;
        }
        if num > base_max {
            continue; // matched through the references to it
        }
        if saved.generation(num) != generation {
            return None;
        }
        match object {
            Some(_) => {
                if !compare(num, num, &mut m) {
                    return None;
                }
                expected.insert(num);
            }
            None => {
                if !matches!(saved.xref_entry(num), Some(XrefEntry::Free { .. }) | None) {
                    return None;
                }
            }
        }
    }
    while let Some((v, f)) = m.queue.pop_front() {
        compared += 1;
        if compared > MAX_COMPARED {
            return None;
        }
        // An added object not edited in the view is one of the base's: its number is its own.
        if !view.is_edited(v) {
            return None;
        }
        if !compare(v, f, &mut m) {
            return None;
        }
        expected.insert(f);
    }
    // Every added object of the view was found, and the updates wrote nothing else (but the
    // stamped information dictionary).
    let added_in_view = view.overlay_entries().filter(|(n, _, o)| *n > base_max && o.is_some()).count();
    if m.to_file.len() != added_in_view {
        return None;
    }
    let mut written = appended(&saved, base.len());
    if let Some(r) = info {
        written.remove(&r.num);
        expected.remove(&r.num);
    }
    // Ignore only actual xref/object-stream containers, not arbitrary extra objects claiming
    // those Type names. Their member objects are still compared above.
    let object_streams: HashSet<u32> = saved
        .object_numbers()
        .into_iter()
        .filter_map(|num| match saved.xref_entry(num) {
            Some(XrefEntry::InStream { stream, .. }) => Some(stream),
            _ => None,
        })
        .collect();
    written.retain(|num| {
        let xref = matches!(saved.xref_entry(*num), Some(XrefEntry::InFile { offset, .. }) if saved.revisions().iter().any(|r| r.is_stream && r.xref_offset == offset));
        !xref && !object_streams.contains(num)
    });
    (written == expected).then_some(m.to_file)
}
