//! The typed object model of a PDF (task card C8-pdf-2): `Page`, `Comment` and `Field` objects the
//! kit derives from the document and keeps in step with it on every change ([`sync`]).
//!
//! The overlay stays what replicates the document; commands stay the only way to write it. The
//! projection is what agents read and what the premise guard holds: every object has a tool id
//! ([`page_id`], [`comment_id`], a field's fully qualified name), its fields are read from the
//! file as it stores them (rectangles rounded to [`QUANTUM`] points), and every type is read-only
//! through the object model (`model/schema.json`).
//!
//! **Identity.**
//!
//! - A page's id is the one stored in its dictionary's private data
//!   (`/PieceInfo /KitionPDF /Private /Id`, [`PAGE_ID_KEY`]) when it has one: the kit stores one in
//!   every page it makes and, before a full rewrite of its own, in every page ([`stamp`]). A page
//!   without one is named by its object number (`p<number>`), which every incremental save keeps
//!   (this kit's, a replica's, another program's) and a program that writes the file anew does
//!   not.
//! - A comment's id is its `/NM`: every comment this kit makes has one, and before a full rewrite
//!   of its own the kit writes one into every comment without ([`stamp`]). A comment without one is
//!   named by its object number (`a<number>`), or, held directly in `/Annots`, by its page and
//!   place there (`a<page number>.<index>`).
//! - A field's id is its fully qualified name (`field:<name>`): renaming a field ends one object
//!   and starts another.
//!
//! Ids named by object number are [`derived`]: whether they still name the same page or comment
//! after another program changed the file is what [`renumbered`](crate) answers for the guard.

use std::collections::{BTreeMap, HashMap, HashSet};

use kition_kit::kdm::{ObjectId, Tx};
use kition_pdf_cos::{Dict, Document, ObjRef, Object, PdfString};
use serde_json::{Map, Value, json};

use crate::{KdmResult, root_id};

/// Rectangles and sizes are rounded to this many points: what a file stores is rarely finer.
pub const QUANTUM: f64 = 0.001;
/// The application name of the kit's page private data (`/PieceInfo`).
pub const PIECE: &[u8] = b"KitionPDF";
/// The key under `/Private` holding a page's id.
pub const PAGE_ID_KEY: &[u8] = b"Id";
/// The prefix of a page's tool id.
pub const PAGE_PREFIX: &str = "page:";
/// The prefix of a form field's tool id.
pub const FIELD_PREFIX: &str = "field:";
/// The prefix of a bookmark's tool id.
pub const BOOKMARK_PREFIX: &str = "bookmark:";
/// The private key of a form field's dictionary holding its id (C8-pdf-3 item 3). ISO 32000-2
/// §7.3.7 lets a dictionary hold keys the standard does not define, which readers ignore; this one
/// is not a registered second-class name. The kit's own saves keep it, and so do programs that
/// keep unknown keys when they write a file anew (lopdf does: `bindings/headless/tests/outside.rs`).
pub const FIELD_ID_KEY: &[u8] = b"KitionPDF_Id";

/// One page as the projection reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct PageView {
    /// The page's id (its tool id without [`PAGE_PREFIX`]).
    pub id: String,
    /// Whether the id is stored in the file (rather than derived from the object number).
    pub stored: bool,
    pub obj: ObjRef,
    pub fields: Map<String, Value>,
    pub comments: Vec<CommentView>,
    /// The page's links (`/Subtype /Link` annotations), in `/Annots` order.
    pub links: Vec<LinkView>,
}

/// One link as the projection reads it: an annotation, named like a comment (its `/NM`, else
/// its object number).
#[derive(Clone, Debug, PartialEq)]
pub struct LinkView {
    pub id: String,
    pub stored: bool,
    pub obj: Option<ObjRef>,
    /// Its place in the page's `/Annots`, from 1 (what `link_*` commands' `index` names).
    pub index: usize,
    pub fields: Map<String, Value>,
}

/// One bookmark (an outline item) as the projection reads it. A bookmark has no id in the file:
/// its id is its title, under its parent's id, with its place among the siblings of the same
/// title when there are several ([`bookmark_id`]).
#[derive(Clone, Debug, PartialEq)]
pub struct BookmarkView {
    pub id: String,
    pub obj: ObjRef,
    /// Child indices from the top level (from 0), as the outline edits take them.
    pub path: Vec<usize>,
    /// Another bookmark under the same parent has the same title (or an ancestor's has): the id
    /// is told by order among them, which a reread cannot prove.
    pub ambiguous: bool,
    pub fields: Map<String, Value>,
    pub children: Vec<BookmarkView>,
}

/// One comment as the projection reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct CommentView {
    pub id: String,
    pub stored: bool,
    /// The annotation's object, when it is an indirect one.
    pub obj: Option<ObjRef>,
    /// Its place in the page's `/Annots`, from 1 (what `comment_*` commands' `index` names).
    pub index: usize,
    pub fields: Map<String, Value>,
}

/// One form field as the projection reads it.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldView {
    /// The field's id (its tool id without [`FIELD_PREFIX`]): the one stored in its dictionary
    /// ([`FIELD_ID_KEY`]), else its fully qualified name.
    pub id: String,
    /// Whether the id is stored in the file (rather than the name).
    pub stored: bool,
    /// The fully qualified name.
    pub name: String,
    pub obj: ObjRef,
    pub fields: Map<String, Value>,
}

/// The whole projection of one document state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Projection {
    pub pages: Vec<PageView>,
    pub fields: Vec<FieldView>,
    /// The top-level bookmarks, each with its children.
    pub bookmarks: Vec<BookmarkView>,
}

impl Projection {
    /// The page at `number` (from 1).
    pub fn page(&self, number: u64) -> Option<&PageView> {
        usize::try_from(number).ok().and_then(|n| n.checked_sub(1)).and_then(|index| self.pages.get(index))
    }

    /// The comment at `index` (its place in `/Annots`, from 1) of page `number`, with its place
    /// among the page's comments (from 0).
    pub fn comment_at(&self, number: u64, index: u64) -> Option<(usize, &CommentView)> {
        let page = self.page(number)?;
        page.comments.iter().enumerate().find(|(_, comment)| comment.index as u64 == index)
    }

    /// The comment with id `id`: its page (from 0), its place among the page's comments, and it.
    pub fn comment(&self, id: &str) -> Option<(usize, usize, &CommentView)> {
        self.pages
            .iter()
            .enumerate()
            .find_map(|(page, view)| view.comments.iter().enumerate().find(|(_, comment)| comment.id == id).map(|(at, comment)| (page, at, comment)))
    }

    /// The field named `name` (its fully qualified name).
    pub fn field(&self, name: &str) -> Option<&FieldView> {
        self.fields.iter().find(|field| field.name == name)
    }

    /// The link at `index` (its place in `/Annots`, from 1) of page `number`, with its place
    /// among the page's links (from 0).
    pub fn link_at(&self, number: u64, index: u64) -> Option<(usize, &LinkView)> {
        let page = self.page(number)?;
        page.links.iter().enumerate().find(|(_, link)| link.index as u64 == index)
    }

    /// The bookmark at `path` (child indices from the top level, from 1, as commands take them).
    pub fn bookmark_at(&self, path: &[u64]) -> Option<&BookmarkView> {
        let mut level = &self.bookmarks;
        let mut found = None;
        for step in path {
            let at = usize::try_from(*step).ok()?.checked_sub(1)?;
            let item = level.get(at)?;
            level = &item.children;
            found = Some(item);
        }
        found
    }

    /// Every bookmark, depth first.
    pub fn all_bookmarks(&self) -> Vec<&BookmarkView> {
        let mut out = Vec::new();
        let mut stack: Vec<&BookmarkView> = self.bookmarks.iter().rev().collect();
        while let Some(item) = stack.pop() {
            out.push(item);
            stack.extend(item.children.iter().rev());
        }
        out
    }
}

/// A page's tool id.
pub fn page_tool_id(id: &str) -> String {
    format!("{PAGE_PREFIX}{id}")
}

/// A field's tool id, of its id ([`FieldView::id`]).
pub fn field_tool_id(id: &str) -> String {
    format!("{FIELD_PREFIX}{id}")
}

/// A bookmark's tool id.
pub fn bookmark_tool_id(id: &str) -> String {
    format!("{BOOKMARK_PREFIX}{id}")
}

/// A bookmark title as a part of an id: `/`, `#` and `%` escaped.
fn id_part(title: &str) -> String {
    title.replace('%', "%25").replace('/', "%2F").replace('#', "%23")
}

/// A bookmark's id: its title (escaped) after its parent's id and `/`, and `#<n>` when it is the
/// `n`th (from 2) of its siblings with that title. It promises: the bookmark with this title under
/// the parent with that id; a rename of it or of an ancestor makes another bookmark, and among
/// siblings of one title, order tells them apart ([`BookmarkView::ambiguous`]).
pub fn bookmark_id(parent: Option<&str>, title: &str, occurrence: usize) -> String {
    let mut id = match parent {
        Some(parent) => format!("{parent}/{}", id_part(title)),
        None => id_part(title),
    };
    if occurrence > 1 {
        id.push_str(&format!("#{occurrence}"));
    }
    id
}

/// Whether an id (of a page or a comment, without prefix) is derived from an object number
/// rather than stored in the file.
pub fn derived(id: &str) -> bool {
    // A key: an object number, or `<actor in hex>.<n>` for an object added in a replica.
    let key = |s: &str| match s.split_once('.') {
        Some((actor, n)) => {
            !actor.is_empty() && actor.bytes().all(|b| b.is_ascii_hexdigit()) && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())
        }
        None => !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()),
    };
    match id.split_at_checked(1) {
        Some(("p", rest)) => key(rest),
        Some(("a", rest)) => match rest.split_once('/') {
            Some((page, index)) => key(page) && !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()),
            None => key(rest),
        },
        _ => false,
    }
}

fn text(doc: &Document, dict: &Dict, key: &[u8]) -> Option<String> {
    dict.get(key).map(|o| doc.resolve(o)).and_then(|o| o.as_string().map(PdfString::to_text))
}

fn name(doc: &Document, dict: &Dict, key: &[u8]) -> Option<String> {
    dict.get(key).map(|o| doc.resolve(o)).and_then(|o| o.as_name().map(|n| String::from_utf8_lossy(n).into_owned()))
}

fn numbers(doc: &Document, value: Option<&Object>) -> Vec<f64> {
    value
        .map(|o| doc.resolve(o))
        .and_then(|o| o.as_array().map(|a| a.iter().filter_map(|x| doc.resolve(x).as_f64()).filter(|x| x.is_finite()).collect()))
        .unwrap_or_default()
}

/// A number rounded to [`QUANTUM`].
pub fn quantised(x: f64) -> Value {
    let q = (x / QUANTUM).round() * QUANTUM;
    // Whole numbers read as integers, as files usually store them.
    if q.fract() == 0.0 && q.abs() < 9.0e15 { json!(q as i64) } else { json!((q * 1000.0).round() / 1000.0) }
}

fn rect(values: &[f64]) -> Value {
    match values {
        [a, b, c, d] => json!([quantised(a.min(*c)), quantised(b.min(*d)), quantised(a.max(*c)), quantised(b.max(*d))]),
        _ => Value::Null,
    }
}

fn hex(values: &[f64]) -> Value {
    match values {
        [r, g, b] => {
            let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            json!(format!("#{:02x}{:02x}{:02x}", c(*r), c(*g), c(*b)))
        }
        _ => Value::Null,
    }
}

/// A page's id stored in its private data.
pub fn stored_page_id(doc: &Document, page: &Dict) -> Option<String> {
    let piece = doc.dict(page.get(b"PieceInfo")?)?;
    let data = doc.dict(piece.get(PIECE)?)?;
    let private = doc.dict(data.get(b"Private")?)?;
    text(doc, &private, PAGE_ID_KEY).filter(|id| !id.is_empty())
}

/// A comment's `/NM`.
fn stored_comment_id(doc: &Document, annot: &Dict) -> Option<String> {
    text(doc, annot, b"NM").filter(|id| !id.is_empty())
}

/// An annotation's `/F` flags.
fn flags(doc: &Document, annot: &Dict) -> i64 {
    annot.get(b"F").map(|o| doc.resolve(o)).and_then(|o| o.as_int()).unwrap_or(0)
}

const LOCKED: i64 = 1 << 7;

/// The projection of `doc`, objects named by their numbers in it (a document with no replica).
pub fn project(doc: &Document) -> Projection {
    project_keyed(doc, &|num| num.to_string())
}

/// The projection of `doc`, an object without a stored id named by `key` of its number: its
/// overlay key in a replica's view ([`crate::Ids::known_key`]), the same on every replica.
pub fn project_keyed(doc: &Document, key: &dyn Fn(u32) -> String) -> Projection {
    let page_refs: Vec<ObjRef> = kition_pdf_organize::pages(doc).map(|pages| pages.into_iter().map(|page| page.obj).collect()).unwrap_or_default();
    let boxes = kition_pdf_organize::page_boxes(doc).unwrap_or_default();
    // A page's label when the document labels its pages (`/PageLabels`): without, a label is the
    // page's number, which is where it is rather than what it is.
    let labelled = doc.root().and_then(|root| doc.get(root).as_dict().map(|catalog| catalog.contains(b"PageLabels"))).unwrap_or(false);
    let labels = if labelled { kition_pdf_organize::page_labels(doc).unwrap_or_default() } else { Vec::new() };
    let mut seen_pages = HashSet::new();
    let mut seen_comments = HashSet::new();
    let mut pages = Vec::with_capacity(page_refs.len());
    // Comments by their annotation's object number, for replies (`/IRT`).
    let mut by_obj: HashMap<u32, String> = HashMap::new();
    let mut annots_of: Vec<Vec<(Option<ObjRef>, Dict, usize)>> = Vec::with_capacity(page_refs.len());
    for (index, &obj) in page_refs.iter().enumerate() {
        let dict = doc.get(obj).as_dict().cloned().unwrap_or_default();
        let (id, stored) = match stored_page_id(doc, &dict) {
            Some(id) if seen_pages.insert(id.clone()) => (id, true),
            _ => {
                let id = format!("p{}", key(obj.num));
                seen_pages.insert(id.clone());
                (id, false)
            }
        };
        let rotation = kition_pdf_organize::page_rotation(doc, index).unwrap_or(0);
        let page_boxes = boxes.get(index).copied().unwrap_or([[0.0, 0.0, 612.0, 792.0]; 5]);
        let unit = dict.get(b"UserUnit").map(|o| doc.resolve(o)).and_then(|o| o.as_f64()).filter(|u| u.is_finite() && *u > 0.0).unwrap_or(1.0);
        let crop = page_boxes[1];
        let (w, h) = ((crop[2] - crop[0]).abs() * unit, (crop[3] - crop[1]).abs() * unit);
        let (width, height) = if rotation % 180 == 90 { (h, w) } else { (w, h) };
        let mut fields = Map::new();
        fields.insert("id".into(), json!(id));
        fields.insert("label".into(), json!(labels.get(index)));
        fields.insert("width".into(), quantised(width));
        fields.insert("height".into(), quantised(height));
        fields.insert("rotation".into(), json!(rotation));
        let named = ["media", "crop", "bleed", "trim", "art"];
        fields.insert("boxes".into(), Value::Object(named.iter().zip(page_boxes).map(|(name, b)| ((*name).to_owned(), rect(&b))).collect()));
        let mut annots = Vec::new();
        if let Some(list) = dict.get(b"Annots").map(|o| doc.resolve(o)) {
            for (at, entry) in list.as_array().into_iter().flatten().enumerate() {
                let Some(annot) = doc.dict(entry) else { continue };
                let Some(subtype) = name(doc, &annot, b"Subtype") else { continue };
                if !kition_pdf_annot::is_comment_subtype(&subtype) {
                    continue;
                }
                annots.push((entry.as_ref(), annot, at + 1));
            }
        }
        let mut comments = Vec::with_capacity(annots.len());
        for (obj_ref, annot, at) in &annots {
            let (id, stored) = match stored_comment_id(doc, annot) {
                Some(id) if seen_comments.insert(id.clone()) => (id, true),
                _ => {
                    let id = match obj_ref {
                        Some(r) => format!("a{}", key(r.num)),
                        None => format!("a{}/{at}", key(obj.num)),
                    };
                    seen_comments.insert(id.clone());
                    (id, false)
                }
            };
            if let Some(r) = obj_ref {
                by_obj.insert(r.num, id.clone());
            }
            comments.push(CommentView { id, stored, obj: *obj_ref, index: *at, fields: Map::new() });
        }
        annots_of.push(annots);
        pages.push(PageView { id, stored, obj, fields, comments, links: Vec::new() });
    }
    // Links: annotations too, named like comments (one namespace of annotation ids).
    let page_ids: Vec<String> = pages.iter().map(|page| page_tool_id(&page.id)).collect();
    for link in kition_pdf_annot::links::list(doc) {
        let Some(page) = pages.get_mut(link.page) else { continue };
        let at = link.index.saturating_add(1);
        let dict = link.obj.and_then(|r| doc.get(r).as_dict().cloned()).or_else(|| {
            doc.get(page.obj)
                .as_dict()
                .and_then(|p| p.get(b"Annots").map(|o| doc.resolve(o)))
                .and_then(|list| list.as_array().and_then(|list| list.get(link.index)).and_then(|entry| doc.dict(entry)))
        });
        let (id, stored) = match dict.as_ref().and_then(|dict| stored_comment_id(doc, dict)) {
            Some(id) if seen_comments.insert(id.clone()) => (id, true),
            _ => {
                let id = match link.obj {
                    Some(r) => format!("a{}", key(r.num)),
                    None => format!("a{}/{at}", key(page.obj.num)),
                };
                seen_comments.insert(id.clone());
                (id, false)
            }
        };
        let mut f = Map::new();
        f.insert("id".into(), json!(id));
        f.insert("index".into(), json!(at));
        f.insert("rect".into(), rect(&link.rect));
        let (url, to_page, other) = match &link.action {
            kition_pdf_annot::links::LinkAction::Uri(url) => (json!(url), Value::Null, Value::Null),
            kition_pdf_annot::links::LinkAction::Page(to) => (Value::Null, json!(page_ids.get(*to)), Value::Null),
            kition_pdf_annot::links::LinkAction::Other(other) => (Value::Null, Value::Null, json!(other)),
        };
        f.insert("url".into(), url);
        f.insert("toPage".into(), to_page);
        f.insert("other".into(), other);
        f.insert("visible".into(), json!(link.style.visible));
        f.insert("color".into(), hex(&link.style.color));
        f.insert("width".into(), quantised(link.style.width));
        f.insert("highlight".into(), json!(link.style.highlight.label()));
        page.links.push(LinkView { id, stored, obj: link.obj, index: at, fields: f });
    }
    // The comments' fields, now that every comment has its id (a reply names its parent's).
    let mut parents: HashMap<String, Option<String>> = HashMap::new();
    let mut states: Vec<(String, String)> = Vec::new();
    for (page, annots) in pages.iter_mut().zip(&annots_of) {
        for (comment, (_, annot, _)) in page.comments.iter_mut().zip(annots) {
            let reply_to = annot.get(b"IRT").and_then(|o| o.as_ref()).and_then(|r| by_obj.get(&r.num).cloned());
            let state = text(doc, annot, b"State");
            let width = annot
                .get(b"BS")
                .and_then(|o| doc.dict(o))
                .and_then(|bs| bs.get(b"W").map(|o| doc.resolve(o)).and_then(|o| o.as_f64()))
                .or_else(|| numbers(doc, annot.get(b"Border")).get(2).copied());
            let f = &mut comment.fields;
            f.insert("id".into(), json!(comment.id));
            f.insert("type".into(), json!(name(doc, annot, b"Subtype")));
            f.insert("index".into(), json!(comment.index));
            f.insert("contents".into(), json!(text(doc, annot, b"Contents")));
            f.insert("author".into(), json!(text(doc, annot, b"T")));
            f.insert("modified".into(), json!(text(doc, annot, b"M")));
            f.insert("rect".into(), rect(&numbers(doc, annot.get(b"Rect"))));
            f.insert("color".into(), hex(&numbers(doc, annot.get(b"C"))));
            f.insert("fill".into(), hex(&numbers(doc, annot.get(b"IC"))));
            f.insert("opacity".into(), annot.get(b"CA").map(|o| doc.resolve(o)).and_then(|o| o.as_f64()).map_or(Value::Null, quantised));
            f.insert("width".into(), width.map_or(Value::Null, quantised));
            f.insert("locked".into(), json!(flags(doc, annot) & LOCKED != 0));
            f.insert("replyTo".into(), json!(reply_to));
            f.insert("state".into(), json!(state));
            parents.insert(comment.id.clone(), reply_to);
            if let Some(state) = state {
                states.push((comment.id.clone(), state));
            }
        }
    }
    // A comment's review status and check mark: the last state reply in its thread (at any depth),
    // as `comment.list` reads them.
    let root_of = |id: &str| -> Option<String> {
        let mut at = parents.get(id).cloned().flatten()?;
        for _ in 0..64 {
            match parents.get(&at).cloned().flatten() {
                Some(up) => at = up,
                None => return Some(at),
            }
        }
        None
    };
    let mut status: HashMap<String, String> = HashMap::new();
    let mut marked: HashMap<String, bool> = HashMap::new();
    for (id, state) in &states {
        let Some(root) = root_of(id) else { continue };
        if matches!(state.as_str(), "Marked" | "Unmarked") {
            marked.insert(root, state == "Marked");
        } else {
            status.insert(root, state.clone());
        }
    }
    for page in &mut pages {
        for comment in &mut page.comments {
            let root = comment.fields.get("replyTo").is_none_or(Value::is_null);
            let f = &mut comment.fields;
            f.insert("status".into(), if root { json!(status.get(&comment.id)) } else { Value::Null });
            f.insert("marked".into(), json!(root && marked.get(&comment.id).copied().unwrap_or(false)));
        }
    }
    let page_of: HashMap<usize, String> = pages.iter().enumerate().map(|(index, page)| (index, page_tool_id(&page.id))).collect();
    let mut fields = Vec::new();
    let mut seen_names = HashSet::new();
    let mut seen_fields = HashSet::new();
    for field in kition_pdf_forms::fields(doc) {
        if !seen_names.insert(field.name.clone()) {
            continue;
        }
        let dict = doc.get(field.obj).as_dict().cloned().unwrap_or_default();
        let (id, stored) = match text(doc, &dict, FIELD_ID_KEY).filter(|id| !id.is_empty()) {
            Some(id) if seen_fields.insert(id.clone()) => (id, true),
            _ if seen_fields.insert(field.name.clone()) => (field.name.clone(), false),
            // Another field's stored id is this one's name: the field cannot be named.
            _ => continue,
        };
        let mut f = Map::new();
        f.insert("id".into(), json!(id));
        f.insert("name".into(), json!(field.name));
        f.insert("type".into(), json!(kind_name(field.kind)));
        f.insert("value".into(), json!(field.value));
        f.insert("default".into(), json!(field.default));
        f.insert("flags".into(), json!(field.flags));
        f.insert("maxLength".into(), json!(field.max_len));
        f.insert("options".into(), json!(field.options));
        f.insert("tooltip".into(), json!(field.tooltip));
        f.insert("appearance".into(), json!(field.da));
        f.insert("align".into(), json!(field.quadding));
        f.insert("actions".into(), json!(kition_kit::session::sha256_hex(format!("{:?} {:?}", field.actions, field.button).as_bytes())));
        let widgets: Vec<Value> = field
            .widgets
            .iter()
            .map(|widget| {
                json!({
                    "page": widget.page.and_then(|page| page_of.get(&page).cloned()),
                    "rect": rect(&widget.rect),
                    "state": widget.state,
                    "onState": widget.on_state,
                    "locked": widget.locked,
                    "hidden": widget.hidden,
                    "rotation": widget.rotation,
                })
            })
            .collect();
        f.insert("widgets".into(), json!(widgets));
        fields.push(FieldView { id, stored, name: field.name, obj: field.obj, fields: f });
    }
    let bookmarks = project_bookmarks(doc, &page_ids);
    Projection { pages, fields, bookmarks }
}

/// The document's bookmarks as a tree ([`bookmark_id`] names them), as the outline listing reads
/// them (bookmarks deeper than its depth are left out).
fn project_bookmarks(doc: &Document, page_ids: &[String]) -> Vec<BookmarkView> {
    let listed = kition_pdf_organize::bookmarks(doc);
    // Listing order is depth first: a bookmark's parent is the last one listed at its path's
    // parent. Built as a flat arena, then nested.
    let mut flat: Vec<(BookmarkView, Option<usize>)> = Vec::with_capacity(listed.len());
    let mut last_at_depth: Vec<usize> = Vec::new();
    for item in listed {
        let depth = item.path.len().saturating_sub(1);
        last_at_depth.truncate(depth);
        let parent = if depth == 0 { None } else { last_at_depth.get(depth - 1).copied() };
        if depth > 0 && parent.is_none() {
            continue;
        }
        let view = BookmarkView {
            id: String::new(),
            obj: item.obj,
            path: item.path.clone(),
            ambiguous: false,
            fields: Map::from_iter([
                ("title".to_owned(), json!(item.title)),
                ("page".to_owned(), json!(item.page.and_then(|page| page_ids.get(page)))),
                ("open".to_owned(), json!(item.open)),
            ]),
            children: Vec::new(),
        };
        last_at_depth.push(flat.len());
        flat.push((view, parent));
    }
    // Ids, in listing order (a parent before its children): by title among the siblings.
    let mut seen: HashMap<(Option<usize>, String), usize> = HashMap::new();
    let mut titles: HashMap<(Option<usize>, String), usize> = HashMap::new();
    for (view, parent) in &flat {
        let title = view.fields.get("title").and_then(Value::as_str).unwrap_or_default().to_owned();
        *titles.entry((*parent, title)).or_default() += 1;
    }
    for at in 0..flat.len() {
        let parent = flat[at].1;
        let title = flat[at].0.fields.get("title").and_then(Value::as_str).unwrap_or_default().to_owned();
        let occurrence = {
            let n = seen.entry((parent, title.clone())).or_default();
            *n += 1;
            *n
        };
        let (parent_id, parent_ambiguous) = match parent.and_then(|p| flat.get(p)) {
            Some((view, _)) => (Some(view.id.clone()), view.ambiguous),
            None => (None, false),
        };
        let shared = titles.get(&(parent, title.clone())).copied().unwrap_or(1) > 1;
        let id = bookmark_id(parent_id.as_deref(), &title, occurrence);
        let view = &mut flat[at].0;
        view.ambiguous = parent_ambiguous || shared;
        view.fields.insert("id".into(), json!(id));
        view.id = id;
    }
    // Nested: children last to first, each into its parent.
    let mut slots: Vec<Option<BookmarkView>> = Vec::with_capacity(flat.len());
    let mut parents = Vec::with_capacity(flat.len());
    for (view, parent) in flat {
        slots.push(Some(view));
        parents.push(parent);
    }
    for at in (0..slots.len()).rev() {
        let Some(parent) = parents[at] else { continue };
        if let Some(view) = slots[at].take()
            && let Some(Some(holder)) = slots.get_mut(parent)
        {
            holder.children.insert(0, view);
        }
    }
    slots.into_iter().flatten().collect()
}

fn kind_name(kind: kition_pdf_forms::FieldKind) -> &'static str {
    use kition_pdf_forms::FieldKind;
    match kind {
        FieldKind::Text => "text",
        FieldKind::CheckBox => "checkbox",
        FieldKind::Radio => "radio",
        FieldKind::PushButton => "button",
        FieldKind::Combo => "combo",
        FieldKind::List => "list",
        FieldKind::Signature => "signature",
    }
}

/// A fresh id for a page: random, so two replicas never make the same one.
fn fresh_id() -> String {
    kition_kit::kdm::DocId::new().to_string()
}

/// Writes ids into `doc`'s pages, comments, links and form fields so that they survive another
/// program's rewrite: every page, link and field the edit made (not in `before`) gets a fresh id;
/// with `all` (before a full rewrite of the kit's own) every page, comment, link and field without
/// one gets the id the projection gives it now. Comments the edit made have their `/NM` already.
pub fn stamp(doc: &mut Document, before: &Document, all: bool, key: &dyn Fn(u32) -> String) {
    let had: HashSet<u32> = kition_pdf_organize::pages(before).map(|pages| pages.into_iter().map(|page| page.obj.num).collect()).unwrap_or_default();
    // An object of `before` with this number that is of the same kind: not one the edit made.
    let existed = |num: u32, kind: &[u8], key: &[u8]| {
        before.get(ObjRef { num, generation: before.generation(num) }).as_dict().is_some_and(|dict| dict.name(key) == Some(kind))
    };
    let view = project_keyed(doc, key);
    for page in &view.pages {
        let new = !had.contains(&page.obj.num);
        if !page.stored && (new || all) {
            let id = if new { fresh_id() } else { page.id.clone() };
            let _ = doc.update_dict(page.obj, |dict| set_page_id(dict, &id));
        }
        for link in page.links.iter().filter(|link| !link.stored) {
            let Some(obj) = link.obj else { continue };
            let new = !existed(obj.num, b"Link", b"Subtype");
            if new || all {
                let id = if new { fresh_id() } else { link.id.clone() };
                let _ = doc.update_dict(obj, |dict| dict.set(b"NM".to_vec(), PdfString::text(&id)));
            }
        }
        for comment in page.comments.iter().filter(|comment| !comment.stored) {
            let Some(obj) = comment.obj else { continue };
            // A copied annotation may carry an original's /NM. The projection detects the
            // duplicate; stamp the new object with its own identity before saving it.
            let subtype = comment.fields.get("type").and_then(Value::as_str).unwrap_or_default();
            let new = !existed(obj.num, subtype.as_bytes(), b"Subtype");
            if new || all {
                let id = if new { fresh_id() } else { comment.id.clone() };
                let _ = doc.update_dict(obj, |dict| dict.set(b"NM".to_vec(), PdfString::text(&id)));
            }
        }
    }
    let fields_before: HashSet<u32> = kition_pdf_forms::fields(before).into_iter().map(|field| field.obj.num).collect();
    for field in view.fields.iter().filter(|field| !field.stored) {
        let new = !fields_before.contains(&field.obj.num);
        if new || all {
            let id = if new { fresh_id() } else { field.id.clone() };
            let _ = doc.update_dict(field.obj, |dict| dict.set(FIELD_ID_KEY.to_vec(), PdfString::text(&id)));
        }
    }
}

/// Puts `id` into a page dictionary's private data (`/PieceInfo /KitionPDF`). The data
/// dictionary's `/LastModified` (required) is a fixed date: the entry is an identity, never
/// content that changes.
fn set_page_id(page: &mut Dict, id: &str) {
    let mut private = Dict::new();
    private.set(PAGE_ID_KEY.to_vec(), PdfString::text(id));
    let mut data = Dict::new();
    data.set(b"LastModified".to_vec(), PdfString::literal(b"D:20260101000000Z".to_vec()));
    data.set(b"Private".to_vec(), Object::Dict(private));
    let mut piece = match page.get(b"PieceInfo") {
        Some(Object::Dict(d)) => d.clone(),
        _ => Dict::new(),
    };
    piece.set(PIECE.to_vec(), Object::Dict(data));
    page.set(b"PieceInfo".to_vec(), Object::Dict(piece));
    if !page.contains(b"LastModified") {
        page.set(b"LastModified".to_vec(), PdfString::literal(b"D:20260101000000Z".to_vec()));
    }
}

/// Whether the replica's objects differ from the projection of `doc` (an old replica without
/// them, or one whose merge left them behind its view).
pub fn differs(replica: &kition_kit::kdm::Replica, doc: &Document, ids: &crate::Ids) -> bool {
    let view = project_keyed(doc, &keyer(ids));
    let root = root_id();
    let fields_equal =
        |object: &ObjectId, fields: &Map<String, Value>| fields.iter().all(|(field, value)| replica.get(object, field).ok().as_ref() == Some(value));
    let Ok(pages) = replica.list(&root, "pages") else { return true };
    if pages.len() != view.pages.len() {
        return true;
    }
    for (object, page) in pages.iter().zip(&view.pages) {
        if !fields_equal(object, &page.fields) {
            return true;
        }
        let Ok(comments) = replica.list(object, "comments") else { return true };
        if comments.len() != page.comments.len()
            || comments.iter().zip(&page.comments).any(|(object, comment)| !fields_equal(object, &comment.fields))
        {
            return true;
        }
        let Ok(links) = replica.list(object, "links") else { return true };
        if links.len() != page.links.len() || links.iter().zip(&page.links).any(|(object, link)| !fields_equal(object, &link.fields)) {
            return true;
        }
    }
    let Ok(fields) = replica.list(&root, "fields") else { return true };
    if fields.len() != view.fields.len() || fields.iter().zip(&view.fields).any(|(object, field)| !fields_equal(object, &field.fields)) {
        return true;
    }
    // Bookmarks, level by level.
    let mut levels: Vec<(ObjectId, &[BookmarkView])> = vec![(root.clone(), &view.bookmarks)];
    while let Some((holder, wanted)) = levels.pop() {
        let field = if holder == root { "bookmarks" } else { "children" };
        let Ok(held) = replica.list(&holder, field) else { return true };
        if held.len() != wanted.len() {
            return true;
        }
        for (object, bookmark) in held.iter().zip(wanted) {
            if !fields_equal(object, &bookmark.fields) {
                return true;
            }
            levels.push((object.clone(), &bookmark.children));
        }
    }
    false
}

// ---- Keeping the replica's objects in step ----

/// Objects of a list by tool id, in order.
type Named = Vec<(String, ObjectId)>;
/// Wanted objects of a list: tool id and fields, in order.
type Wanted<'a> = Vec<(String, &'a Map<String, Value>)>;

/// The objects of one type the replica holds now: tool id → object id, first one kept.
fn held(tx: &Tx<'_>, owner: &ObjectId, field: &str, key: &str) -> KdmResult<(Vec<ObjectId>, Named)> {
    let items = tx.list_items(owner, field)?;
    let mut named = Vec::with_capacity(items.len());
    for item in &items {
        // An object a merge left in the list after another replica deleted it reads as no id: it
        // is taken out.
        let id = if tx.exists(item) { tx.get(item, key)?.as_str().unwrap_or_default().to_owned() } else { String::new() };
        named.push((id, item.clone()));
    }
    Ok((items, named))
}

/// Sets every register of `fields` that differs.
fn set_fields(tx: &mut Tx<'_>, id: &ObjectId, fields: &Map<String, Value>) -> KdmResult<()> {
    for (field, value) in fields {
        if tx.get(id, field)? != *value {
            tx.set(id, field, value.clone())?;
        }
    }
    Ok(())
}

/// Makes `owner.field` hold, in order, one object per entry of `wanted` (by tool id `key`),
/// creating, moving, updating and deleting objects as needed. `reuse` gives an object of the
/// same tool id held elsewhere (a comment whose page changed), taken out of there first. Answers
/// the objects in order.
#[allow(clippy::too_many_arguments)]
fn sync_list(
    tx: &mut Tx<'_>,
    owner: &ObjectId,
    field: &str,
    ty: &str,
    key: &str,
    wanted: &[(String, &Map<String, Value>)],
    reuse: &mut HashMap<String, ObjectId>,
    gone: &mut Vec<ObjectId>,
) -> KdmResult<Vec<ObjectId>> {
    let (_, named) = held(tx, owner, field, key)?;
    let want: HashSet<&str> = wanted.iter().map(|(id, _)| id.as_str()).collect();
    // Out of the list: what is no longer wanted here, and every second object of one tool id.
    let mut seen = HashSet::new();
    let mut keep: Vec<(String, ObjectId)> = Vec::new();
    let mut remove = Vec::new();
    for (at, (id, object)) in named.iter().enumerate() {
        if want.contains(id.as_str()) && seen.insert(id.clone()) {
            keep.push((id.clone(), object.clone()));
        } else {
            remove.push((at, object.clone(), id.clone()));
        }
    }
    for (at, object, id) in remove.into_iter().rev() {
        tx.list_remove(owner, field, at)?;
        // Kept for another list (a comment on another page now) unless nothing wants it.
        if !reuse.contains_key(&id) && !id.is_empty() {
            reuse.insert(id, object);
        } else {
            gone.push(object);
        }
    }
    // In order: each wanted entry at its place, moved, taken from elsewhere or made.
    let mut order: Vec<String> = keep.iter().map(|(id, _)| id.clone()).collect();
    let mut objects: HashMap<String, ObjectId> = keep.into_iter().collect();
    let mut out = Vec::with_capacity(wanted.len());
    for (index, (id, fields)) in wanted.iter().enumerate() {
        let object = match order.iter().position(|held| held == id) {
            Some(at) => {
                if at != index {
                    tx.list_move(owner, field, at, index)?;
                    let moved = order.remove(at);
                    order.insert(index, moved);
                }
                objects.get(id).cloned().ok_or_else(|| kition_kit::kdm::KdmError::Engine(format!("{ty} {id} is not held")))?
            }
            None => {
                let object = match reuse.remove(id) {
                    Some(object) => object,
                    None => tx.create(ty, (*fields).clone())?,
                };
                tx.list_insert(owner, field, index, &object)?;
                order.insert(index, id.clone());
                objects.insert(id.clone(), object.clone());
                object
            }
        };
        set_fields(tx, &object, fields)?;
        out.push(object);
    }
    Ok(out)
}

/// Keeps the replica's `Page`, `Comment`, `Link`, `Field` and `Bookmark` objects in step with `doc`.
pub fn sync(tx: &mut Tx<'_>, doc: &Document, ids: &crate::Ids) -> KdmResult<()> {
    sync_projection(tx, &project_keyed(doc, &keyer(ids)))
}

/// Names an object number by its overlay key in `ids` (its number when it has none).
pub fn keyer(ids: &crate::Ids) -> impl Fn(u32) -> String + '_ {
    move |num| ids.known_key(num).unwrap_or_else(|| num.to_string())
}

/// [`sync`] with the projection read already.
pub fn sync_projection(tx: &mut Tx<'_>, view: &Projection) -> KdmResult<()> {
    let root = root_id();
    let mut reuse: HashMap<String, ObjectId> = HashMap::new();
    let mut gone = Vec::new();
    // Comments first leave the pages that no longer have them, so one that moved to another page
    // is kept (its id) rather than deleted and made again.
    let wanted_pages: Vec<(String, &Map<String, Value>)> = view.pages.iter().map(|page| (page.id.clone(), &page.fields)).collect();
    let mut comment_pages: BTreeMap<String, Wanted<'_>> = BTreeMap::new();
    for page in &view.pages {
        comment_pages.insert(page.id.clone(), page.comments.iter().map(|comment| (comment.id.clone(), &comment.fields)).collect());
    }
    let mut page_reuse = HashMap::new();
    let mut gone_pages = Vec::new();
    let pages = sync_list(tx, &root, "pages", "Page", "id", &wanted_pages, &mut page_reuse, &mut gone_pages)?;
    // A page that is gone takes its comments out first (held elsewhere now, or gone).
    for page in gone_pages.iter().chain(page_reuse.values()) {
        if !tx.exists(page) {
            continue;
        }
        let items = tx.list_items(page, "comments")?;
        for (at, item) in items.iter().enumerate().rev() {
            tx.list_remove(page, "comments", at)?;
            let id = if tx.exists(item) { tx.get(item, "id")?.as_str().unwrap_or_default().to_owned() } else { String::new() };
            if reuse.contains_key(&id) || id.is_empty() {
                gone.push(item.clone());
            } else {
                reuse.insert(id, item.clone());
            }
        }
        gone.push(page.clone());
    }
    // Comments that left their page: out of every page first.
    let mut wanted_by_page = Vec::new();
    for (object, page) in pages.iter().zip(&view.pages) {
        let list = comment_pages.remove(&page.id).unwrap_or_default();
        let keep: HashSet<&str> = list.iter().map(|(id, _)| id.as_str()).collect();
        let (_, named) = held(tx, object, "comments", "id")?;
        for (at, (id, item)) in named.iter().enumerate().rev() {
            if !keep.contains(id.as_str()) {
                tx.list_remove(object, "comments", at)?;
                if reuse.contains_key(id) || id.is_empty() {
                    gone.push(item.clone());
                } else {
                    reuse.insert(id.clone(), item.clone());
                }
            }
        }
        wanted_by_page.push((object.clone(), list));
    }
    for (object, list) in &wanted_by_page {
        sync_list(tx, object, "comments", "Comment", "id", list, &mut reuse, &mut gone)?;
    }
    // Links: out of every page that no longer has them first, so one that moved is kept.
    let mut link_reuse: HashMap<String, ObjectId> = HashMap::new();
    for (object, page) in pages.iter().zip(&view.pages) {
        let keep: HashSet<&str> = page.links.iter().map(|link| link.id.as_str()).collect();
        let (_, named) = held(tx, object, "links", "id")?;
        for (at, (id, item)) in named.iter().enumerate().rev() {
            if !keep.contains(id.as_str()) {
                tx.list_remove(object, "links", at)?;
                if link_reuse.contains_key(id) || id.is_empty() {
                    gone.push(item.clone());
                } else {
                    link_reuse.insert(id.clone(), item.clone());
                }
            }
        }
    }
    for (object, page) in pages.iter().zip(&view.pages) {
        let list: Wanted<'_> = page.links.iter().map(|link| (link.id.clone(), &link.fields)).collect();
        sync_list(tx, object, "links", "Link", "id", &list, &mut link_reuse, &mut gone)?;
    }
    gone.extend(link_reuse.into_values());
    let wanted_fields: Vec<(String, &Map<String, Value>)> = view.fields.iter().map(|field| (field.id.clone(), &field.fields)).collect();
    let mut field_reuse = HashMap::new();
    sync_list(tx, &root, "fields", "Field", "id", &wanted_fields, &mut field_reuse, &mut gone)?;
    gone.extend(field_reuse.into_values());
    // Bookmarks, level by level from the top: one that moved to another parent has another id.
    let mut bookmark_reuse: HashMap<String, ObjectId> = HashMap::new();
    let mut levels: Vec<(ObjectId, &str, &[BookmarkView])> = vec![(root.clone(), "bookmarks", &view.bookmarks)];
    while let Some((holder, field, wanted)) = levels.pop() {
        let list: Wanted<'_> = wanted.iter().map(|bookmark| (bookmark.id.clone(), &bookmark.fields)).collect();
        let objects = sync_list(tx, &holder, field, "Bookmark", "id", &list, &mut bookmark_reuse, &mut gone)?;
        for (object, bookmark) in objects.into_iter().zip(wanted) {
            levels.push((object, "children", &bookmark.children));
        }
    }
    // A bookmark taken out of a list takes its children with it.
    let mut out: Vec<ObjectId> = bookmark_reuse.into_values().collect();
    while let Some(object) = out.pop() {
        if tx.exists(&object) {
            out.extend(tx.list_items(&object, "children")?);
        }
        gone.push(object);
    }
    gone.extend(reuse.into_values());
    let mut deleted = HashSet::new();
    for object in gone {
        if deleted.insert(object.clone()) && tx.exists(&object) {
            tx.delete(&object)?;
        }
    }
    Ok(())
}
