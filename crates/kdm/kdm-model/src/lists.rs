//! Concurrent additions to one list: a page's `/Annots`, a page tree node's or a field's
//! `/Kids`, the form's `/Fields`.
//!
//! Each list is one overlay entry (the dictionary holding it, or the array object it references),
//! so two peers adding to one list at once each write the whole list, and the merge keeps one
//! (last writer wins): the other's objects arrive, but nothing lists them. Two comments on one
//! page, two pages inserted, two form fields added would lose one. The view repairs that as it
//! is read ([`repair`]), the same on every replica: an object added since the base that its list
//! does not hold is appended to it, in a fixed order (by the adding replica, then the order it
//! added them), and a page tree's `/Count`s follow.
//!
//! What belongs where ([`holders`]):
//!
//! - an annotation in its page's `/Annots` (its `/P`, or a pop-up's parent's);
//! - a page in its `/Parent` page tree node's `/Kids`;
//! - a field or widget with a `/Parent` field in that field's `/Kids`;
//! - a field without a parent in the form's `/Fields` (the catalog's `/AcroForm`; when a
//!   concurrent catalog write dropped the form, the view takes back the form an added field
//!   was written with, or makes one).
//!
//! A deletion also leaves an object unlisted, so each write marks the added objects its document
//! leaves out of a list (`"detached": ["annots" | "kids" | "fields", …]` in their overlay
//! entries, [`detached`]; an older `"detached": true` means `annots`): the repair appends only
//! unmarked ones, and takes a marked one out of a list that holds it again (written by a peer
//! that had not seen the deletion). A list therefore holds what its last writer listed, plus
//! every object added since that nobody removed. Undoing a removal restores the unmarked entry,
//! which brings the object back.
//!
//! Objects of the base are never repaired: every writer knew them, so a list that leaves one out
//! removed it.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use kition_pdf_cos::{Dict, Document, ObjRef, Object, PdfString};

/// The deepest page tree a `/Count` fix walks up.
const MAX_DEPTH: usize = 64;

/// A list an added object belongs in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Holder {
    /// The `/Annots` of this page.
    Page(u32),
    /// The `/Kids` of this page tree node or field.
    Parent(u32),
    /// The form's `/Fields`.
    Form,
}

impl Holder {
    fn mark(self) -> &'static str {
        match self {
            Holder::Page(_) => "annots",
            Holder::Parent(_) => "kids",
            Holder::Form => "fields",
        }
    }

    fn key(self) -> &'static [u8] {
        match self {
            Holder::Page(_) => b"Annots",
            Holder::Parent(_) => b"Kids",
            Holder::Form => b"Fields",
        }
    }
}

/// An object's list marks as an overlay entry stores them: `"detached"` is a list of
/// [`Holder::mark`]s, or `true` (written before there were others) for `annots`.
pub(crate) fn marks_of(value: &serde_json::Value) -> Vec<String> {
    match value.get("detached") {
        Some(serde_json::Value::Bool(true)) => vec!["annots".into()],
        Some(serde_json::Value::Array(items)) => items.iter().filter_map(|m| m.as_str().map(str::to_owned)).collect(),
        _ => Vec::new(),
    }
}

fn is_pages(d: &Dict) -> bool {
    d.name(b"Type") == Some(b"Pages")
}

/// A field dictionary: it has a field type, or is a named node of a field tree.
fn is_field(d: &Dict) -> bool {
    !is_pages(d) && (d.contains(b"FT") || (d.contains(b"T") && d.contains(b"Kids") && !d.contains(b"Subtype")))
}

/// The page an annotation belongs to: its `/P`, or a pop-up's parent's (when that is a page).
fn annotation_page(doc: &Document, annot: &Dict) -> Option<u32> {
    let page = match annot.reference(b"P") {
        Some(page) => page,
        None if annot.name(b"Subtype") == Some(b"Popup") => doc.get(annot.reference(b"Parent")?).as_dict()?.reference(b"P")?,
        None => return None,
    };
    let page_object = doc.get(page);
    (page_object.as_dict()?.name(b"Type") == Some(b"Page")).then_some(page.num)
}

/// The lists object `num` belongs in.
fn holders(doc: &Document, num: u32) -> Vec<Holder> {
    let Ok(object) = doc.try_get(num) else { return Vec::new() };
    let Some(d) = object.as_dict() else { return Vec::new() };
    let parent = d.reference(b"Parent");
    let parent_dict = parent.map(|p| doc.get(p));
    let parent_dict = parent_dict.as_deref().and_then(Object::as_dict);
    match d.name(b"Type") {
        Some(b"Page") => return parent.filter(|_| parent_dict.is_some_and(is_pages)).map(|p| Holder::Parent(p.num)).into_iter().collect(),
        Some(b"Pages") => return Vec::new(),
        _ => {}
    }
    let mut out = Vec::new();
    if d.contains(b"Subtype")
        && let Some(page) = annotation_page(doc, d)
    {
        out.push(Holder::Page(page));
    }
    if is_field(d) || d.name(b"Subtype") == Some(b"Widget") {
        match (parent, parent_dict) {
            (Some(p), Some(pd)) if is_field(pd) => out.push(Holder::Parent(p.num)),
            (None, _) if d.contains(b"FT") => out.push(Holder::Form),
            _ => {}
        }
    }
    out
}

/// Where a holder's dictionary lives: an object, or (the form) inline in the catalog.
enum Place {
    Object(ObjRef),
    InCatalog(ObjRef),
}

fn holder_dict(doc: &Document, holder: Holder) -> Option<(Place, Dict)> {
    let at = |num: u32| ObjRef::new(num, doc.generation(num));
    match holder {
        Holder::Page(num) | Holder::Parent(num) => Some((Place::Object(at(num)), doc.get(at(num)).as_dict()?.clone())),
        Holder::Form => {
            let catalog = doc.root()?;
            let form = doc.get(catalog).as_dict()?.get(b"AcroForm")?.clone();
            match form {
                Object::Ref(r) => Some((Place::Object(r), doc.get(r).as_dict()?.clone())),
                Object::Dict(d) => Some((Place::InCatalog(catalog), d)),
                _ => None,
            }
        }
    }
}

/// The list's entries (empty when the holder or its list is missing).
fn list_of(doc: &Document, holder: Holder) -> Vec<Object> {
    let Some((_, d)) = holder_dict(doc, holder) else { return Vec::new() };
    d.get(holder.key()).map(|list| doc.resolve(list).as_array().cloned().unwrap_or_default()).unwrap_or_default()
}

/// Writes the list: into the array object it references when indirect, else into its holder.
fn set_list(doc: &mut Document, holder: Holder, list: Vec<Object>) {
    let Some((place, d)) = holder_dict(doc, holder) else { return };
    let key = holder.key();
    if let Some(array) = d.reference(key).filter(|r| doc.get(*r).as_array().is_some()) {
        doc.set(array, Object::Array(list));
        return;
    }
    let put = |d: &mut Dict| {
        if list.is_empty() && holder != Holder::Form {
            d.remove(key);
        } else {
            d.set(key.to_vec(), Object::Array(list));
        }
    };
    match place {
        Place::Object(r) => {
            let _ = doc.update_dict(r, put);
        }
        Place::InCatalog(catalog) => {
            let mut form = d;
            put(&mut form);
            let _ = doc.update_dict(catalog, |c| c.set(b"AcroForm".to_vec(), Object::Dict(form)));
        }
    }
}

/// For each of `nums` (objects added since the base): every list it belongs in, and whether the
/// list holds it.
fn placement(doc: &Document, nums: impl IntoIterator<Item = u32>) -> Vec<(u32, Holder, bool)> {
    let mut lists: HashMap<Holder, HashSet<u32>> = HashMap::new();
    let mut out = Vec::new();
    for num in nums {
        for holder in holders(doc, num) {
            let list = lists.entry(holder).or_insert_with(|| list_of(doc, holder).iter().filter_map(Object::as_ref).map(|r| r.num).collect());
            out.push((num, holder, list.contains(&num)));
        }
    }
    out
}

/// Which of `nums` (objects added since the base) a list they belong in leaves out, with the
/// marks a write gives them (sorted).
pub(crate) fn detached(doc: &Document, nums: impl IntoIterator<Item = u32>) -> HashMap<u32, Vec<&'static str>> {
    let mut out: HashMap<u32, BTreeSet<&'static str>> = HashMap::new();
    for (num, holder, listed) in placement(doc, nums) {
        if !listed {
            out.entry(num).or_default().insert(holder.mark());
        }
    }
    out.into_iter().map(|(num, marks)| (num, marks.into_iter().collect())).collect()
}

/// The order repaired objects are appended in: by the adding replica, then its counter.
fn order(key: &str) -> (&str, u64, &str) {
    match key.split_once('.') {
        Some((actor, n)) => (actor, n.parse().unwrap_or(u64::MAX), key),
        None => (key, u64::MAX, key),
    }
}

/// A form for the catalog when a concurrent catalog write dropped it: the first added form
/// dictionary (one with `/Fields`), else a new one.
fn restore_form(doc: &mut Document, added: &[&(u32, String, Vec<String>)]) {
    let Some(catalog) = doc.root() else { return };
    if doc.get(catalog).as_dict().is_none_or(|c| c.contains(b"AcroForm")) {
        return;
    }
    let found = added.iter().map(|(num, _, _)| *num).find(|num| {
        doc.try_get(*num).ok().is_some_and(|o| o.as_dict().is_some_and(|d| d.contains(b"Fields") && !d.contains(b"Type") && !is_field(d)))
    });
    let form = match found {
        Some(num) => Object::Ref(ObjRef::new(num, doc.generation(num))),
        None => {
            let mut form = Dict::new();
            form.set(b"Fields".to_vec(), Object::Array(Vec::new()));
            form.set(b"DA".to_vec(), PdfString::literal(b"/Helv 0 Tf 0 g".to_vec()));
            Object::Dict(form)
        }
    };
    let _ = doc.update_dict(catalog, |c| c.set(b"AcroForm".to_vec(), form));
}

/// Adds `delta` to the `/Count` of page tree node `node` and of every node above it.
fn recount(doc: &mut Document, node: u32, delta: i64) {
    let mut at = Some(ObjRef::new(node, doc.generation(node)));
    let mut seen = HashSet::new();
    while let Some(r) = at {
        if seen.len() > MAX_DEPTH || !seen.insert(r.num) {
            return;
        }
        let object = doc.get(r);
        let Some(d) = object.as_dict().filter(|d| is_pages(d)) else { return };
        let count = d.int(b"Count").unwrap_or(0).saturating_add(delta).max(0);
        at = d.reference(b"Parent");
        let _ = doc.update_dict(r, |d| d.set(b"Count".to_vec(), Object::Int(count)));
    }
}

/// Repairs the view `doc` after its overlay was applied: `added` are the objects added since the
/// base (number, overlay key, the list marks of its entry). See the module docs.
pub(crate) fn repair(doc: &mut Document, added: &[(u32, String, Vec<String>)]) {
    let mut added: Vec<&(u32, String, Vec<String>)> = added.iter().collect();
    added.sort_by(|a, b| order(&a.1).cmp(&order(&b.1)));
    let marks: HashMap<u32, &Vec<String>> = added.iter().map(|(num, _, marks)| (*num, marks)).collect();
    let mut append: BTreeMap<Holder, Vec<u32>> = BTreeMap::new();
    let mut remove: BTreeMap<Holder, HashSet<u32>> = BTreeMap::new();
    for (num, holder, listed) in placement(doc, added.iter().map(|(num, _, _)| *num)) {
        let marked = marks.get(&num).is_some_and(|m| m.iter().any(|m| m == holder.mark()));
        match (listed, marked) {
            (false, false) => append.entry(holder).or_default().push(num),
            (true, true) => {
                remove.entry(holder).or_default().insert(num);
            }
            _ => {}
        }
    }
    if append.contains_key(&Holder::Form) {
        restore_form(doc, &added);
    }
    let changed: BTreeSet<Holder> = append.keys().chain(remove.keys()).copied().collect();
    for holder in changed {
        let before = list_of(doc, holder);
        let gone = remove.remove(&holder).unwrap_or_default();
        let mut list: Vec<Object> = before.iter().filter(|o| !o.as_ref().is_some_and(|r| gone.contains(&r.num))).cloned().collect();
        let removed = before.len() - list.len();
        let appended = append.remove(&holder).unwrap_or_default();
        let count = appended.len();
        list.extend(appended.into_iter().map(|num| Object::Ref(ObjRef::new(num, doc.generation(num)))));
        set_list(doc, holder, list);
        if let Holder::Parent(node) = holder
            && doc.get(ObjRef::new(node, doc.generation(node))).as_dict().is_some_and(is_pages)
        {
            recount(doc, node, count as i64 - removed as i64);
        }
    }
}
