//! C8-pdf-4: synthetic inputs, two serializers, and a saved-file merge oracle.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use kition_kit::KitDocument;
use kition_pdf_cos::{Dict, Document, ObjRef, Object, PdfString, Stream};
use kition_pdf_kit::PdfDocument;
use serde_json::{Value, json};

use crate::common::*;

#[derive(Clone, Copy, Debug)]
pub enum Writer {
    Engine,
    Lopdf,
}

pub const WRITERS: [Writer; 2] = [Writer::Engine, Writer::Lopdf];

pub fn dict(entries: &[(&str, Object)]) -> Dict {
    let mut d = Dict::new();
    for (k, v) in entries {
        d.set(k.as_bytes().to_vec(), v.clone());
    }
    d
}

pub fn text(s: &str) -> Object {
    Object::String(PdfString::text(s))
}
pub fn array(n: &[i32]) -> Object {
    Object::Array(n.iter().map(|n| Object::Int(i64::from(*n))).collect())
}
pub fn open(bytes: &[u8]) -> Document {
    Document::open(Arc::new(bytes.to_vec())).unwrap()
}
pub fn page(doc: &Document, n: usize) -> ObjRef {
    kition_pdf_organize::pages(doc).unwrap()[n - 1].obj
}
pub fn dictionary(doc: &Document, r: ObjRef) -> Dict {
    doc.get(r).as_dict().unwrap().clone()
}
pub fn set(doc: &mut Document, r: ObjRef, key: &str, v: impl Into<Object>) {
    doc.update_dict(r, |d| d.set(key.as_bytes().to_vec(), v.into())).unwrap();
}
pub fn list(doc: &Document, r: ObjRef, key: &str) -> Vec<Object> {
    doc.resolve(dictionary(doc, r).get(key.as_bytes()).unwrap()).as_array().unwrap().clone()
}
pub fn field(doc: &Document, n: usize) -> ObjRef {
    kition_pdf_forms::fields(doc).iter().find(|f| f.name == format!("F{n}")).unwrap().obj
}
pub fn annot(doc: &Document, n: usize, subtype: &str) -> ObjRef {
    list(doc, page(doc, n), "Annots")
        .iter()
        .filter_map(Object::as_ref)
        .find(|r| dictionary(doc, *r).name(b"Subtype") == Some(subtype.as_bytes()))
        .unwrap()
}
pub fn figure(doc: &Document, n: usize) -> ObjRef {
    doc.scan_objects()
        .find(|(_, o)| o.as_dict().is_some_and(|d| d.name(b"S") == Some(b"Figure") && d.reference(b"Pg") == Some(page(doc, n))))
        .unwrap()
        .0
}

/// Every page has distinguishable content, a note/popup, a widget, a link, two editable text
/// blocks, a tiny original RGB image and a tagged figure. No binary fixture is committed.
pub fn fixture() -> Vec<u8> {
    let mut doc = fixture_doc(4);
    for n in 1..=4 {
        doc.execute("page_add_text", json!({"page":n,"at":[72,210],"text":format!("Neighbour {n}")})).unwrap();
        doc.execute("link_add", json!({"page":n,"rect":[100,300,200,320],"url":format!("https://example.com/{n}")})).unwrap();
    }
    let mut cos = open(&doc.save(None).unwrap());
    let root = cos.root().unwrap();
    let structure = cos.add(dict(&[("Type", Object::name("StructTreeRoot"))]));
    let mut children = Vec::new();
    for n in 1..=4 {
        let p = page(&cos, n);
        let image = cos.add(Stream::from_raw(
            dict(&[
                ("Type", Object::name("XObject")),
                ("Subtype", Object::name("Image")),
                ("Width", 2.into()),
                ("Height", 2.into()),
                ("BitsPerComponent", 8.into()),
                ("ColorSpace", Object::name("DeviceRGB")),
            ]),
            vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0],
        ));
        let mut resources = cos.dict(dictionary(&cos, p).get(b"Resources").unwrap()).unwrap();
        let mut xo = resources.get(b"XObject").and_then(|v| cos.dict(v)).unwrap_or_default();
        xo.set(b"MatrixImage".to_vec(), Object::Ref(image));
        resources.set(b"XObject".to_vec(), xo);
        set(&mut cos, p, "Resources", resources);
        let content = cos.add(Stream::from_raw(Dict::new(), b"/Figure <</MCID 17>> BDC q 32 0 0 24 360 470 cm /MatrixImage Do Q EMC\n".to_vec()));
        let current = dictionary(&cos, p).get(b"Contents").cloned().unwrap();
        let mut contents = match &*cos.resolve(&current) {
            Object::Array(a) => a.clone(),
            _ => vec![current],
        };
        contents.push(Object::Ref(content));
        set(&mut cos, p, "Contents", Object::Array(contents));
        let f = cos.add(dict(&[
            ("Type", Object::name("StructElem")),
            ("S", Object::name("Figure")),
            ("P", Object::Ref(structure)),
            ("Pg", Object::Ref(p)),
            ("K", 17.into()),
            ("Alt", text(&format!("figure {n}"))),
        ]));
        children.push(Object::Ref(f));
    }
    set(&mut cos, structure, "K", Object::Array(children));
    set(&mut cos, root, "StructTreeRoot", Object::Ref(structure));
    kition_pdf_cos::write_full(&cos, &Default::default()).unwrap()
}

fn lopdf_object(o: &Object) -> lopdf::Object {
    use lopdf::Object as L;
    match o {
        Object::Null => L::Null,
        Object::Bool(b) => L::Boolean(*b),
        Object::Int(n) => L::Integer(*n),
        Object::Real(n) => L::Real(*n as f32),
        Object::Name(n) => L::Name(n.clone()),
        Object::String(s) => L::String(s.bytes.clone(), if s.hex { lopdf::StringFormat::Hexadecimal } else { lopdf::StringFormat::Literal }),
        Object::Ref(r) => L::Reference((r.num, r.generation)),
        Object::Array(a) => L::Array(a.iter().map(lopdf_object).collect()),
        Object::Dict(d) => L::Dictionary(lopdf_dict(d)),
        Object::Stream(s) => L::Stream(lopdf::Stream::new(lopdf_dict(&s.dict), s.raw.to_vec())),
    }
}
fn lopdf_dict(d: &Dict) -> lopdf::Dictionary {
    let mut out = lopdf::Dictionary::new();
    for (k, v) in d.iter() {
        out.set(k.clone(), lopdf_object(v));
    }
    out
}

/// The edit is an ISO dictionary/stream patch, not a call to the premise declarations or the
/// guarded command. COS and lopdf independently parse and serialize it. A full lopdf save
/// renumbers every object; the COS full writer likewise compacts/reassigns object numbers.
pub fn save_patch(bytes: &[u8], doc: &Document, writer: Writer, full: bool) -> Vec<u8> {
    match writer {
        Writer::Engine if full => kition_pdf_cos::write_full(doc, &Default::default()).unwrap(),
        Writer::Engine => kition_pdf_cos::write_incremental(doc, &Default::default()).unwrap(),
        Writer::Lopdf => {
            let prev = lopdf::Document::load_mem(bytes).unwrap();
            let mut update = lopdf::IncrementalDocument::create_from(bytes.to_vec(), prev);
            for (num, generation, obj) in doc.overlay_entries() {
                update.new_document.set_object((num, generation), lopdf_object(&obj.unwrap()));
                update.new_document.max_id = update.new_document.max_id.max(num);
            }
            let mut out = Vec::new();
            if full {
                let mut all = update.get_prev_documents().clone();
                all.objects.extend(update.new_document.objects);
                all.max_id = all.objects.keys().map(|id| id.0).max().unwrap();
                all.renumber_objects_with(1000);
                all.save_to(&mut out).unwrap();
            } else {
                update.save_to(&mut out).unwrap();
                assert!(out.starts_with(bytes));
            }
            out
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Rotate,
    Box,
    MovePage,
    DeletePage,
    InsertPage,
    CommentText,
    CommentColor,
    CommentRect,
    DeleteComment,
    MoveComment,
    FieldValue,
    FieldRename,
    FieldType,
    FieldOptions,
    FieldFlags,
    DeleteField,
    LinkTarget,
    LinkStyle,
    DeleteLink,
    BookmarkRename,
    BookmarkMove,
    Text,
    DrawingText,
    OtherText,
    DeleteText,
    ImagePixels,
    FigureAlt,
    Slot { subtype: &'static str, before: bool, remove: bool },
    Property,
    Rewrite,
    Incremental,
}
#[derive(Clone, Copy, Debug)]
pub struct Change {
    pub kind: Kind,
    pub page: usize,
}

pub fn changes() -> Vec<Change> {
    use Kind::*;
    let mut out = Vec::new();
    for kind in [
        Rotate,
        Box,
        MovePage,
        DeletePage,
        CommentText,
        CommentColor,
        CommentRect,
        DeleteComment,
        MoveComment,
        FieldValue,
        FieldRename,
        FieldType,
        FieldOptions,
        FieldFlags,
        DeleteField,
        LinkTarget,
        LinkStyle,
        DeleteLink,
        BookmarkRename,
        BookmarkMove,
        Text,
        OtherText,
        DeleteText,
        ImagePixels,
        FigureAlt,
    ] {
        for page in 1..=4 {
            out.push(Change { kind, page });
        }
    }
    for page in 1..=5 {
        out.push(Change { kind: InsertPage, page });
    }
    for page in [2, 4] {
        for subtype in ["Text", "Popup", "Widget"] {
            for before in [true, false] {
                for remove in [true, false] {
                    out.push(Change { kind: Slot { subtype, before, remove }, page });
                }
            }
        }
    }
    for kind in [Property, Rewrite, Incremental] {
        out.push(Change { kind, page: 4 });
    }
    out
}

fn pages_tree(doc: &mut Document, pages: &[ObjRef]) {
    let tree = dictionary(doc, doc.root().unwrap()).reference(b"Pages").unwrap();
    set(doc, tree, "Kids", Object::Array(pages.iter().map(|r| Object::Ref(*r)).collect()));
    set(doc, tree, "Count", Object::Int(pages.len() as i64));
    for p in pages {
        set(doc, *p, "Parent", Object::Ref(tree));
    }
}
pub fn annotation_slot(doc: &mut Document, n: usize, subtype: &str, before: bool, remove: bool) {
    let p = page(doc, n);
    let mut a = list(doc, p, "Annots");
    if remove {
        // Dedicated sentinel slots, not a real field widget or a note's linked popup.
        let at = a.iter().position(|r| doc.resolve(r).as_dict().is_some_and(|d| d.get(b"MatrixSlot") == Some(&Object::Bool(true)))).unwrap();
        a.remove(at);
    } else {
        let r = doc.add(dict(&[
            ("Type", Object::name("Annot")),
            ("Subtype", Object::name(subtype)),
            ("Rect", array(&[10, 10, 30, 30])),
            ("NM", text("matrix-slot")),
            ("Contents", text("sentinel")),
            ("MatrixSlot", Object::Bool(true)),
        ]));
        let at = if before { 0 } else { a.len() };
        a.insert(at, Object::Ref(r));
    }
    set(doc, p, "Annots", Object::Array(a));
}
pub fn initial(base: &[u8], change: Change) -> Vec<u8> {
    if let Kind::Slot { subtype, before, remove: true } = change.kind {
        let mut doc = open(base);
        annotation_slot(&mut doc, change.page, subtype, before, false);
        kition_pdf_cos::write_incremental(&doc, &Default::default()).unwrap()
    } else {
        base.to_vec()
    }
}

pub fn mutate(bytes: &[u8], writer: Writer, change: Change) -> Vec<u8> {
    use Kind::*;
    let mut doc = open(bytes);
    let n = change.page;
    let p = page(&doc, n.min(4));
    match change.kind {
        Rotate => set(&mut doc, p, "Rotate", 90),
        Box => set(&mut doc, p, "CropBox", array(&[20, 20, 580, 760])),
        DeletePage | MovePage | InsertPage => {
            let mut pages: Vec<_> = kition_pdf_organize::pages(&doc).unwrap().iter().map(|p| p.obj).collect();
            match change.kind {
                DeletePage => {
                    pages.remove(n - 1);
                }
                MovePage => {
                    let p = pages.remove(n - 1);
                    pages.insert(if n == 4 { 0 } else { 3 }, p);
                }
                InsertPage => {
                    let r = doc.add(dict(&[("Type", Object::name("Page")), ("MediaBox", array(&[0, 0, 612, 792]))]));
                    pages.insert(n - 1, r);
                }
                _ => unreachable!(),
            }
            pages_tree(&mut doc, &pages);
        }
        CommentText | CommentColor | CommentRect => {
            let r = annot(&doc, n, "Text");
            let (key, v) = match change.kind {
                CommentText => ("Contents", text("theirs")),
                CommentColor => ("C", array(&[0, 0, 1])),
                _ => ("Rect", array(&[250, 450, 275, 475])),
            };
            set(&mut doc, r, key, v);
        }
        DeleteComment | DeleteLink | MoveComment => {
            let r = annot(&doc, n, if change.kind == DeleteLink { "Link" } else { "Text" });
            let mut a = list(&doc, p, "Annots");
            a.retain(|a| a.as_ref() != Some(r));
            set(&mut doc, p, "Annots", Object::Array(a));
            if change.kind == MoveComment {
                let dest = page(&doc, if n == 4 { 1 } else { 4 });
                let mut a = list(&doc, dest, "Annots");
                a.push(Object::Ref(r));
                set(&mut doc, dest, "Annots", Object::Array(a));
                set(&mut doc, r, "P", Object::Ref(dest));
            }
        }
        FieldValue | FieldRename | FieldType | FieldOptions | FieldFlags => {
            let r = field(&doc, n);
            let (key, v) = match change.kind {
                FieldValue => ("V", text("theirs")),
                FieldRename => ("T", text(&format!("Renamed{n}"))),
                FieldType => ("FT", Object::name("Ch")),
                FieldOptions => ("Opt", Object::Array(vec![text("one"), text("two")])),
                _ => ("Ff", 2.into()),
            };
            set(&mut doc, r, key, v);
        }
        DeleteField => {
            let r = field(&doc, n);
            let af = dictionary(&doc, doc.root().unwrap()).reference(b"AcroForm").unwrap();
            let mut fields = list(&doc, af, "Fields");
            fields.retain(|o| o.as_ref() != Some(r));
            set(&mut doc, af, "Fields", Object::Array(fields));
            let mut a = list(&doc, p, "Annots");
            a.retain(|o| o.as_ref() != Some(r));
            set(&mut doc, p, "Annots", Object::Array(a));
        }
        LinkTarget | LinkStyle => {
            let r = annot(&doc, n, "Link");
            if change.kind == LinkTarget {
                set(&mut doc, r, "A", dict(&[("S", Object::name("URI")), ("URI", text("https://example.org/theirs"))]));
            } else {
                set(&mut doc, r, "C", array(&[1, 0, 0]));
            }
        }
        BookmarkRename | BookmarkMove => {
            // Outline links are edited directly, so both writers express the same ISO edit.
            let root = dictionary(&doc, doc.root().unwrap()).reference(b"Outlines").unwrap();
            let mut siblings = Vec::new();
            let mut next = dictionary(&doc, root).reference(b"First");
            while let Some(r) = next {
                siblings.push(r);
                next = dictionary(&doc, r).reference(b"Next");
            }
            if change.kind == BookmarkRename {
                set(&mut doc, siblings[n - 1], "Title", text(&format!("Renamed{n}")));
            } else {
                let r = siblings.remove(n - 1);
                siblings.insert(if n == 4 { 0 } else { 3 }, r);
                set(&mut doc, root, "First", Object::Ref(siblings[0]));
                set(&mut doc, root, "Last", Object::Ref(siblings[3]));
                for (i, r) in siblings.iter().enumerate() {
                    doc.update_dict(*r, |d| {
                        d.remove(b"Prev");
                        d.remove(b"Next");
                    })
                    .unwrap();
                    if i > 0 {
                        set(&mut doc, *r, "Prev", Object::Ref(siblings[i - 1]));
                    }
                    if i + 1 < siblings.len() {
                        set(&mut doc, *r, "Next", Object::Ref(siblings[i + 1]));
                    }
                }
            }
        }
        Text | DrawingText | OtherText | DeleteText => {
            // page_add_text stores each block as its own stream; preserve its resource/font.
            let streams = list(&doc, p, "Contents");
            let r = streams[if change.kind == OtherText { 1 } else { 0 }].as_ref().unwrap();
            let Object::Stream(s) = &*doc.get(r) else { panic!("text stream") };
            let original = String::from_utf8(s.decoded().unwrap()).unwrap();
            let old = if change.kind == OtherText { format!("Neighbour {n}") } else { format!("Page {n}") };
            let replacement = if change.kind == DeleteText { String::new() } else { "Theirs".into() };
            assert!(original.contains(&old), "{original}");
            let mut d = s.dict.clone();
            d.remove(b"Filter");
            d.remove(b"DecodeParms");
            if change.kind != DrawingText
                && let Some(Object::Dict(added)) = d.get_mut(b"PCAdded")
            {
                added.set(b"Text".to_vec(), text(&replacement));
            }
            doc.set(r, Stream::from_raw(d, original.replace(&old, &replacement).into_bytes()));
        }
        ImagePixels => {
            let resources = doc.dict(dictionary(&doc, p).get(b"Resources").unwrap()).unwrap();
            let xo = doc.dict(resources.get(b"XObject").unwrap()).unwrap();
            let r = xo.reference(b"MatrixImage").unwrap();
            let Object::Stream(s) = &*doc.get(r) else { panic!("image") };
            let mut d = s.dict.clone();
            d.remove(b"Filter");
            d.remove(b"DecodeParms");
            doc.set(r, Stream::from_raw(d, vec![20; 12]));
        }
        FigureAlt => {
            let r = figure(&doc, n);
            set(&mut doc, r, "Alt", text("theirs"));
        }
        Slot { subtype, before, remove } => annotation_slot(&mut doc, n, subtype, before, remove),
        Property | Incremental => {
            let root = doc.root().unwrap();
            set(&mut doc, root, "Lang", text("fr-FR"));
        }
        Rewrite => {
            // PDF dictionary order is immaterial, but changes the full writer's traversal and
            // thus its object numbering. Assert that this really is a renumbering below.
            let root = doc.root().unwrap();
            let original = dictionary(&doc, root);
            let entries: Vec<_> = original.iter().collect();
            let mut reversed = Dict::new();
            for (k, v) in entries.into_iter().rev() {
                reversed.set(k.clone(), v.clone());
            }
            doc.set(root, reversed);
        }
    }
    let out = save_patch(bytes, &doc, writer, change.kind == Rewrite);
    if change.kind == Rewrite {
        assert_ne!(page(&open(bytes), 2).num, page(&open(&out), 2).num, "rewrite must renumber");
    }
    out
}

fn normalize(v: &mut Value) {
    match v {
        Value::Object(o) => {
            for key in ["modified", "generation", "doc"] {
                o.remove(key);
            }
            for v in o.values_mut() {
                normalize(v);
            }
        }
        Value::Array(a) => {
            for v in a {
                normalize(v);
            }
        }
        _ => {}
    }
}
pub fn reads(bytes: &[u8], stable: &BTreeSet<String>) -> Value {
    let mut doc = PdfDocument::open(FILE, bytes).unwrap();
    let mut out = serde_json::Map::new();
    for command in [
        "comment_list",
        "form_fields",
        "bookmark_list",
        "link_list",
        "content_list",
        "text_extract",
        "doc_inspect",
        "accessibility_figures",
        "measure_list",
    ] {
        let mut v = doc.execute(command, json!({})).unwrap();
        if command == "doc_inspect" {
            v = v["pages"].clone();
        }
        normalize(&mut v);
        out.insert(command.into(), v);
    }
    let mut session = kition_pdf_engine::Session::new();
    let id = session.open(FILE, None, Arc::new(bytes.to_vec()), None).unwrap();
    let engine = session.get(id).unwrap();
    let mut images = Vec::new();
    for page in 0..engine.info.pages.len() {
        let mut listed = doc.execute("page_images", json!({"page":page + 1})).unwrap();
        for (index, image) in engine.page_images(page).iter().enumerate() {
            if !image.is_form {
                let (_, bytes) = engine.page_image_file(page, index).unwrap();
                listed["images"][index]["sha256"] = json!(kition_kit::session::sha256_hex(&bytes));
            }
        }
        normalize(&mut listed);
        images.push(listed);
    }
    out.insert("page_images".into(), json!(images));
    let cos = open(bytes);
    let root = dictionary(&cos, cos.root().unwrap());
    out.insert("language".into(), json!(root.get(b"Lang").and_then(Object::as_string).map(|s| s.to_text())));
    out.insert(
        "headings".into(),
        json!(kition_pdf_engine::a11y::headings(&cos).iter().map(|h| json!([h.level, h.title, h.page])).collect::<Vec<_>>()),
    );
    let slots: Vec<_> = kition_pdf_organize::pages(&cos)
        .unwrap()
        .iter()
        .map(|p| {
            dictionary(&cos, p.obj)
                .get(b"Annots")
                .map(|a| cos.resolve(a))
                .as_deref()
                .and_then(Object::as_array)
                .into_iter()
                .flatten()
                .filter_map(|a| cos.dict(a))
                .map(|a| {
                    json!([
                        a.name(b"Subtype").map(|s| String::from_utf8_lossy(s)),
                        a.get(b"NM").and_then(Object::as_string).map(|s| s.to_text()),
                        a.get(b"MatrixSlot") == Some(&Object::Bool(true))
                    ])
                })
                .collect::<Vec<_>>()
        })
        .collect();
    out.insert("annotation_slots".into(), json!(slots));
    // New UUIDs may differ between the two executions. Existing stored identities must match.
    fn ids(v: &mut Value, stable: &BTreeSet<String>, names: &mut BTreeMap<String, String>) {
        match v {
            Value::String(s) if !stable.contains(s) && (s.starts_with("kition-") || s.len() == 36) => {
                let next = format!("new-id-{}", names.len());
                *s = names.entry(s.clone()).or_insert(next).clone();
            }
            Value::Object(o) => {
                for v in o.values_mut() {
                    ids(v, stable, names);
                }
            }
            Value::Array(a) => {
                for v in a {
                    ids(v, stable, names);
                }
            }
            _ => {}
        }
    }
    let mut out = Value::Object(out);
    ids(&mut out, stable, &mut BTreeMap::new());
    out
}
pub fn stable_ids(bytes: &[u8]) -> BTreeSet<String> {
    let doc = PdfDocument::open(FILE, bytes).unwrap();
    let objects = doc.objects();
    objects
        .pages
        .iter()
        .flat_map(|p| std::iter::once(p.id.clone()).chain(p.comments.iter().map(|c| c.id.clone())).chain(p.links.iter().map(|l| l.id.clone())))
        .chain(objects.fields.iter().map(|f| f.id.clone()))
        .collect()
}

#[derive(Clone, Copy, Debug)]
pub struct Expected {
    pub reason: &'static str,
}
pub const A: Expected = Expected { reason: "A" };
pub const C: Expected = Expected { reason: "changed" };
pub const R: Expected = Expected { reason: "relocated" };
pub const D: Expected = Expected { reason: "deleted" };
pub const U: Expected = Expected { reason: "unknown" };

#[derive(Default, Debug)]
pub struct Totals {
    pub cells: usize,
    pub applied: usize,
    pub refused: usize,
    pub wrong: usize,
    pub false_refusal: usize,
    pub wrong_reason: usize,
    pub errors: usize,
    pub merge: usize,
    pub blocked: usize,
}
impl Totals {
    pub fn clean(&self) -> bool {
        self.wrong + self.false_refusal + self.wrong_reason + self.errors + self.merge == 0
    }
}
pub fn reason_set(s: &str) -> BTreeSet<&str> {
    s.split('+').filter(|s| !s.is_empty()).collect()
}

pub async fn cell(bytes: &[u8], theirs: &[u8], mine: Value, want: Expected, label: &str, totals: &mut Totals) {
    let f = Fixture::from_bytes(bytes, Default::default());
    let (_, basis) = f.inspect().await;
    std::fs::write(f.file(), theirs).unwrap();
    let got = f.call("run", json!({"basis":basis,"steps":[mine.clone()]})).await;
    totals.cells += 1;
    let models_unavailable = matches!(&got,Err(e) if mine["command"]=="ocr_recognize" && e["error"]["code"]=="command-failed"
        && e["error"]["message"].as_str().is_some_and(|s|s.contains("text recognition models are not installed")));
    let fail = match &got {
        Ok(_) if want.reason != "A" => {
            totals.wrong += 1;
            Some("wrong write")
        }
        Ok(_) => {
            totals.applied += 1;
            None
        }
        Err(_) if models_unavailable && want.reason == "A" => {
            totals.blocked += 1;
            None
        }
        Err(_) if models_unavailable => {
            totals.wrong += 1;
            Some("guard permitted related OCR; models unavailable")
        }
        Err(e) if e["error"]["code"] != "conflict" => {
            totals.errors += 1;
            Some("command error")
        }
        Err(_) if want.reason == "A" => {
            totals.false_refusal += 1;
            Some("false refusal")
        }
        Err(e) if reason_set(&reasons(e)) != reason_set(want.reason) => {
            totals.wrong_reason += 1;
            Some("wrong reason")
        }
        Err(_) => {
            totals.refused += 1;
            None
        }
    };
    if let Some(class) = fail {
        let actual = match &got {
            Ok(_) => "A".to_owned(),
            Err(e) if e["error"]["code"] == "conflict" => reasons(e),
            Err(e) => e["error"].to_string(),
        };
        println!("DIFF {label}: {class}; expected {}; got {actual}", want.reason);
    }
    if got.is_err() {
        assert_eq!(std::fs::read(f.file()).unwrap(), theirs, "{label}: refusal preserves bytes");
    }
    if got.is_ok() {
        let mut expected = PdfDocument::open(FILE, theirs).unwrap();
        match expected.execute(mine["command"].as_str().unwrap(), mine["params"].clone()) {
            Err(e) => {
                totals.merge += 1;
                println!("DIFF {label}: sequential command failed {e}");
            }
            Ok(_) => {
                let stable = stable_ids(theirs);
                let actual = reads(&std::fs::read(f.file()).unwrap(), &stable);
                let expected = reads(&expected.save(None).unwrap(), &stable);
                if actual != expected {
                    totals.merge += 1;
                    println!("DIFF {label}: saved merge differs\nactual {actual}\nexpected {expected}");
                }
            }
        }
    }
    f.shutdown().await;
}
