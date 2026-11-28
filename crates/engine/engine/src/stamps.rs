//! Page stamps: one hash per page of everything its picture is drawn from, so a viewer keeps the
//! pixels of the pages an edit did not touch (and re-renders only the ones it did).
//!
//! A page's stamp covers its dictionary (with the inherited `Resources`, `MediaBox`, `CropBox`
//! and `Rotate` filled in, `Parent` left out) and every object it reaches by reference: content
//! streams, resources, fonts, images, annotations and their appearances, the form fields its
//! widgets belong to. A reference to a page or a page-tree node (a link's destination, an
//! annotation's `/P`) counts as that object's number only, so editing one page never changes
//! another's stamp through it. The document parts every page is drawn with (the catalog's
//! `/OCProperties`, and `/AcroForm` without its `/Fields`) go into every stamp.
//!
//! Objects the file holds unchanged hash by their number, generation and (for a stream) data
//! length instead of their bytes: within one [`SourceIdentity`] they cannot change. Stamps are
//! only comparable between states of the same source, which is why [`PageStamps::source`] comes
//! with them: after a save rebases the document onto new bytes, every page counts as new.
//!
//! Equal stamps of the same source mean equal pictures; the converse is not promised (a change
//! that draws the same, such as rewriting an object with the same value, still changes a stamp).

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

use kition_pdf_cos::{Dict, Document as Cos, ObjRef, Object, SourceIdentity};

/// Nesting beyond this gives no stamps (the caller then treats every page as changed).
const MAX_DEPTH: usize = 256;
/// Object visits per computation, across all pages; beyond it, no stamps.
const MAX_VISITS: usize = 2_000_000;
/// No back-reference reached an object still being hashed.
const NONE_OPEN: usize = usize::MAX;

/// The stamps of every page, in page order, and the source they are comparable within.
#[derive(Clone, Debug)]
pub struct PageStamps {
    pub source: SourceIdentity,
    pub pages: Vec<u64>,
}

/// The stamps of `cos`'s pages; `None` when the object graph is too deep or too large to walk.
pub fn page_stamps(cos: &Cos) -> Option<PageStamps> {
    let pages = kition_pdf_model::pages(cos);
    let mut walk = Walk { cos, pages: pages.iter().map(|page| page.obj.num).collect(), memo: HashMap::new(), open: Vec::new(), visits: 0 };
    let salt = walk.salt()?;
    let mut out = Vec::with_capacity(pages.len());
    for page in pages {
        let mut dict = page.dict;
        dict.remove(b"Parent");
        let mut hasher = DefaultHasher::new();
        salt.hash(&mut hasher);
        walk.value(&Object::Dict(dict), None, &mut hasher, 0)?;
        out.push(hasher.finish());
    }
    Some(PageStamps { source: cos.source_identity(), pages: out })
}

struct Walk<'a> {
    cos: &'a Cos,
    /// Object numbers of the leaf pages: references to them are never followed.
    pages: HashSet<u32>,
    /// Finished objects whose hash does not depend on where the walk entered them.
    memo: HashMap<u32, u64>,
    /// Objects being hashed, outermost first.
    open: Vec<u32>,
    visits: usize,
}

impl Walk<'_> {
    /// The document parts every page is drawn with.
    fn salt(&mut self) -> Option<u64> {
        let mut hasher = DefaultHasher::new();
        let catalog = self.cos.root().map(|root| self.cos.get(root));
        let catalog = catalog.as_deref().and_then(Object::as_dict);
        for key in [&b"OCProperties"[..], b"AcroForm"] {
            key.hash(&mut hasher);
            let Some(value) = catalog.and_then(|catalog| catalog.get(key)) else {
                0u8.hash(&mut hasher);
                continue;
            };
            let value = self.cos.resolve(value);
            match (key, value.as_dict()) {
                (b"AcroForm", Some(form)) => {
                    let mut form: Dict = form.clone();
                    form.remove(b"Fields");
                    self.value(&Object::Dict(form), None, &mut hasher, 0)?;
                }
                _ => {
                    self.value(&value, None, &mut hasher, 0)?;
                }
            }
        }
        Some(hasher.finish())
    }

    /// Hashes the object `reference` names into a value of its own. Answers it with the lowest
    /// position in `open` a reference inside it reached (`NONE_OPEN` for none): such a hash
    /// depends on where the walk entered the cycle, so it is not remembered.
    fn object(&mut self, reference: ObjRef, depth: usize) -> Option<(u64, usize)> {
        if self.pages.contains(&reference.num) {
            return Some((token(b"page", reference.num), NONE_OPEN));
        }
        if let Some(hash) = self.memo.get(&reference.num) {
            return Some((*hash, NONE_OPEN));
        }
        if let Some(at) = self.open.iter().position(|num| *num == reference.num) {
            return Some((token(b"open", reference.num), at));
        }
        self.visits += 1;
        if self.visits > MAX_VISITS || depth > MAX_DEPTH {
            return None;
        }
        let object = self.cos.get(reference);
        if let Some(dict) = object.as_dict()
            && matches!(dict.name(b"Type"), Some(b"Page" | b"Pages"))
        {
            return Some((token(b"page", reference.num), NONE_OPEN));
        }
        let position = self.open.len();
        self.open.push(reference.num);
        let unchanged = (!self.cos.is_edited(reference.num)).then_some(reference);
        let mut hasher = DefaultHasher::new();
        let reached = self.value(&object, unchanged, &mut hasher, depth + 1);
        self.open.pop();
        let reached = reached?;
        let hash = hasher.finish();
        if reached >= position {
            self.memo.insert(reference.num, hash);
            return Some((hash, NONE_OPEN));
        }
        Some((hash, reached))
    }

    /// Hashes `value` into `hasher`; `unchanged` is the object it is when the file holds it
    /// unchanged (its stream data is then hashed by length only).
    fn value(&mut self, value: &Object, unchanged: Option<ObjRef>, hasher: &mut DefaultHasher, depth: usize) -> Option<usize> {
        if depth > MAX_DEPTH {
            return None;
        }
        let mut reached = NONE_OPEN;
        match value {
            Object::Null => 0u8.hash(hasher),
            Object::Bool(on) => (1u8, on).hash(hasher),
            Object::Int(number) => (2u8, number).hash(hasher),
            Object::Real(number) => (3u8, number.to_bits()).hash(hasher),
            Object::String(text) => (4u8, &text.bytes).hash(hasher),
            Object::Name(name) => (5u8, name).hash(hasher),
            Object::Array(items) => {
                (6u8, items.len()).hash(hasher);
                for item in items {
                    reached = reached.min(self.value(item, None, hasher, depth + 1)?);
                }
            }
            Object::Dict(dict) => reached = self.dict(dict, hasher, depth)?,
            Object::Stream(stream) => {
                8u8.hash(hasher);
                reached = self.dict(&stream.dict, hasher, depth)?;
                match unchanged {
                    Some(reference) => (reference.num, reference.generation, stream.raw.len()).hash(hasher),
                    None => (stream.raw.len(), &*stream.raw).hash(hasher),
                }
            }
            Object::Ref(reference) => {
                let (hash, inner) = self.object(*reference, depth)?;
                (9u8, hash).hash(hasher);
                reached = inner;
            }
        }
        Some(reached)
    }

    fn dict(&mut self, dict: &Dict, hasher: &mut DefaultHasher, depth: usize) -> Option<usize> {
        (7u8, dict.len()).hash(hasher);
        let mut reached = NONE_OPEN;
        for (key, item) in dict.iter() {
            key.hash(hasher);
            reached = reached.min(self.value(item, None, hasher, depth + 1)?);
        }
        Some(reached)
    }
}

fn token(kind: &[u8], num: u32) -> u64 {
    let mut hasher = DefaultHasher::new();
    (kind, num).hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    /// A file of `objects` (object 1 first), numbered from 1.
    fn file(objects: &[&str]) -> Cos {
        let mut bytes = b"%PDF-1.7\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(bytes.len());
            bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
        }
        let start = bytes.len();
        bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for offset in offsets {
            bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        bytes.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n", objects.len() + 1).as_bytes());
        Cos::open(Arc::new(bytes)).unwrap()
    }

    /// Three pages sharing one font; page 2 has an annotation that points back at it and at page 3.
    fn three_pages() -> Cos {
        file(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 /MediaBox [0 0 200 200] /Resources << /Font << /F1 6 0 R >> >> >>",
            "<< /Type /Page /Parent 2 0 R /Contents 7 0 R >>",
            "<< /Type /Page /Parent 2 0 R /Contents 8 0 R /Annots [9 0 R] >>",
            "<< /Type /Page /Parent 2 0 R /Contents 7 0 R >>",
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
            "<< /Length 0 >>\nstream\n\nendstream",
            "<< /Length 0 >>\nstream\n\nendstream",
            "<< /Type /Annot /Subtype /Link /Rect [0 0 10 10] /P 4 0 R /Dest [5 0 R /Fit] /Popup 10 0 R >>",
            "<< /Type /Annot /Subtype /Popup /Rect [0 0 10 10] /Parent 9 0 R >>",
        ])
    }

    fn stamps(cos: &Cos) -> Vec<u64> {
        page_stamps(cos).unwrap().pages
    }

    #[test]
    fn an_edit_changes_only_the_stamps_of_the_pages_it_reaches() {
        let cos = three_pages();
        let before = stamps(&cos);
        assert_eq!(before.len(), 3);
        // Pages 1 and 3 draw the same: same contents, same inherited resources.
        assert_eq!(before[0], before[2]);
        assert_ne!(before[0], before[1]);
        assert_eq!(stamps(&cos), before, "stamps are stable");

        // An annotation on page 2 changes its stamp only, though it names pages 2 and 3.
        let mut edited = cos.clone();
        edited.update_dict(ObjRef::new(9, 0), |annot| annot.set(b"C".to_vec(), Object::Array(vec![Object::Real(1.0)]))).unwrap();
        let after = stamps(&edited);
        assert_eq!((after[0], after[2]), (before[0], before[2]));
        assert_ne!(after[1], before[1]);

        // A popup's change reaches its annotation through the cycle between them.
        let mut popup = cos.clone();
        popup.update_dict(ObjRef::new(10, 0), |annot| annot.set(b"Open".to_vec(), Object::Bool(true))).unwrap();
        let after = stamps(&popup);
        assert_eq!((after[0], after[2]), (before[0], before[2]));
        assert_ne!(after[1], before[1]);

        // Turning page 3 changes page 3 only.
        let mut turned = cos.clone();
        turned.update_dict(ObjRef::new(5, 0), |page| page.set(b"Rotate".to_vec(), Object::Int(90))).unwrap();
        let after = stamps(&turned);
        assert_eq!((after[0], after[1]), (before[0], before[1]));
        assert_ne!(after[2], before[2]);
    }

    #[test]
    fn shared_and_inherited_parts_change_every_page_that_draws_them() {
        let cos = three_pages();
        let before = stamps(&cos);
        // The shared font: every page.
        let mut font = cos.clone();
        font.update_dict(ObjRef::new(6, 0), |font| font.set(b"BaseFont".to_vec(), Object::name("Courier"))).unwrap();
        assert!(stamps(&font).iter().zip(&before).all(|(a, b)| a != b));
        // An inherited attribute of the page tree: every page.
        let mut tree = cos.clone();
        tree.update_dict(ObjRef::new(2, 0), |pages| pages.set(b"Rotate".to_vec(), Object::Int(180))).unwrap();
        assert!(stamps(&tree).iter().zip(&before).all(|(a, b)| a != b));
        // The form's default resources: every page.
        let mut form = cos.clone();
        let fonts = form.add(Object::Dict(Dict::new()));
        form.update_dict(ObjRef::new(1, 0), |catalog| {
            let mut acro = Dict::new();
            acro.set(b"DR".to_vec(), Object::Ref(fonts));
            catalog.set(b"AcroForm".to_vec(), Object::Dict(acro));
        })
        .unwrap();
        assert!(stamps(&form).iter().zip(&before).all(|(a, b)| a != b));
        // Something no page draws (a bookmark tree): no page.
        let mut outline = cos.clone();
        let outlines = outline.add(Object::Dict(Dict::new()));
        outline.update_dict(ObjRef::new(1, 0), |catalog| catalog.set(b"Outlines".to_vec(), Object::Ref(outlines))).unwrap();
        assert_eq!(stamps(&outline), before);
    }

    #[test]
    fn a_new_page_order_moves_the_stamps_with_the_pages() {
        let cos = three_pages();
        let before = stamps(&cos);
        let mut moved = cos.clone();
        moved
            .update_dict(ObjRef::new(2, 0), |pages| {
                pages.set(
                    b"Kids".to_vec(),
                    Object::Array(vec![Object::Ref(ObjRef::new(4, 0)), Object::Ref(ObjRef::new(3, 0)), Object::Ref(ObjRef::new(5, 0))]),
                )
            })
            .unwrap();
        assert_eq!(stamps(&moved), vec![before[1], before[0], before[2]]);
    }

    #[test]
    fn rewritten_stream_data_changes_the_stamp_and_undoing_it_restores_it() {
        let cos = three_pages();
        let before = stamps(&cos);
        let mut drawn = cos.clone();
        drawn.set(
            ObjRef::new(8, 0),
            Object::Stream(kition_pdf_cos::Stream { dict: Dict::new(), raw: kition_pdf_cos::Bytes::from(b"0 0 10 10 re f".to_vec()) }),
        );
        let changed = stamps(&drawn);
        assert_ne!(changed[1], before[1]);
        assert_eq!((changed[0], changed[2]), (before[0], before[2]));
        // The state before the edit, as undo brings it back, has the stamps it had.
        assert_eq!(stamps(&cos.clone()), before);
    }

    #[test]
    fn deep_or_cyclic_graphs_end_without_panicking() {
        // A reference chain deeper than the walk follows gives no stamps.
        let mut objects = vec!["<< /Type /Catalog /Pages 2 0 R >>".to_owned(), "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned()];
        objects.push("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] /Resources 4 0 R >>".to_owned());
        for index in 4..(MAX_DEPTH + 10) {
            objects.push(format!("<< /Next {} 0 R >>", index + 1));
        }
        objects.push("<< >>".to_owned());
        let refs: Vec<&str> = objects.iter().map(String::as_str).collect();
        assert!(page_stamps(&file(&refs)).is_none());
        // A cycle among resources is fine.
        let cycle = file(&[
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 10 10] /Resources 4 0 R >>",
            "<< /A 5 0 R >>",
            "<< /B 4 0 R >>",
        ]);
        assert_eq!(stamps(&cycle).len(), 1);
    }
}
