//! Format fidelity scorecard (roadmap phase 7).
//!
//! `KITION_PDF_CORPUS=<dir>`: every PDF in that directory is opened, saved and opened again in
//! four ways (legs), each scored on its own:
//!
//! - `save`: nothing edited, saved as Kition PDF saves (the file as it was);
//! - `annotate`: a comment added on the first page, saved incrementally (Kition PDF's Save);
//! - `fill`: the first fillable field filled in, saved incrementally (files without one skip it);
//! - `rewrite`: nothing edited, written in full (Save As optimized: reachable objects only,
//!   renumbered, packed into object streams; signed files skip it, as the editor refuses it).
//!
//! For each leg the test reports
//!
//! - per feature class (pages, content, fonts, images, graphics, annotations, forms, navigation,
//!   structure, signatures, document), which files keep it: the object graph reachable from the
//!   trailer, as the editor holds it before saving and as the saved file reads back, flattened
//!   into `path=value` leaves (an object is `{kind: dictionary}`, a reference names the kind of
//!   the object it points to, a stream its data by content) and compared per class as multisets;
//! - COS object retention: dictionary keys and streams of every object the file holds (reachable
//!   or not), counted per object kind, the original against the saved file; whether the saved
//!   file starts with the original bytes (an incremental update leaves them untouched); and how
//!   many of the original objects the edit rewrote (the rest stay as they were, byte for byte).
//!
//! `KITION_FIDELITY_LEGS=save,annotate,fill,rewrite` selects structural legs (default all).
//! `KITION_FIDELITY_ORACLE=1` enables a separate Poppler save/rewrite check: external reopen,
//! all-page text and exact pixels at 72 dpi on first/middle/last pages. Set
//! `KITION_FIDELITY_VISUAL_ALL=1` for all pages. Edited legs are deliberately excluded.
//! `KITION_FIDELITY_REQUIRE_CORPUS=1` fails if the corpus variable is missing; any explicitly
//! configured corpus must contain PDFs. Subdirectories are searched recursively.
//! `KITION_FIDELITY_REQUIRE_LOSSLESS=1` fails after writing reports if any model class changed.
//! Without it, structural scoring measures changes and optionally gates against a baseline.
//! Oracle commands time out after `KITION_FIDELITY_TIMEOUT_SECS` (default 60) seconds.
//!
//! `KITION_FIDELITY_OUT=<dir>` writes the full report (`pdf-fidelity.txt`, per file and
//! aggregated) and the aggregate scorecard (`pdf-fidelity.json`, no file names) there.
//! `KITION_FIDELITY_BASELINE=<file>` fails when the corpus scores below such a scorecard (the
//! non-regression gate, per leg). Skipped when the corpus variable is unset. The KDM leg of the
//! product's path (a document through a replica and back) is covered by `kdm-model`'s
//! `tests/roundtrip.rs`.
//!
//! The scorer below the kit-specific part is the one of Kition Word, Slide and Sheet, with the
//! leg passed through and the original-bytes count added.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Arc;

#[path = "fidelity/oracle.rs"]
mod oracle;

use serde_json::{Value, json};

use kition_pdf_cos::{Dict, Document, ObjRef, Object, SaveOptions, write_full, write_incremental};

// ---- Kit-specific -------------------------------------------------------------------------------

const KIT: &str = "pdf";
const CORPUS_VAR: &str = "KITION_PDF_CORPUS";
const EXTENSIONS: &[&str] = &["pdf"];
/// A class the self-test's document must have.
const PRIMARY_CLASS: &str = "pages";

type Model = Document;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Leg {
    #[default]
    Save,
    Annotate,
    Fill,
    Rewrite,
}

const LEGS: [Leg; 4] = [Leg::Save, Leg::Annotate, Leg::Fill, Leg::Rewrite];

impl Leg {
    fn name(self) -> &'static str {
        match self {
            Leg::Save => "save",
            Leg::Annotate => "annotate",
            Leg::Fill => "fill",
            Leg::Rewrite => "rewrite",
        }
    }

    /// Whether the saved file must start with the original bytes.
    fn incremental(self) -> bool {
        self != Leg::Rewrite
    }
}

fn import(bytes: &[u8]) -> Result<Model, String> {
    Document::open(Arc::new(bytes.to_vec())).map_err(|error| error.to_string())
}

/// The document a leg saves, or an explicit reason the leg does not apply.
enum Prepared {
    Ready(Model),
    Skip(&'static str),
}

fn prepare(leg: Leg, bytes: &[u8]) -> Result<Prepared, String> {
    let mut doc = import(bytes)?;
    match leg {
        Leg::Save => {}
        Leg::Annotate => {
            let note = kition_pdf_annot::NewAnnotation {
                page: 0,
                shape: kition_pdf_annot::Shape::Note { at: [36.0, 36.0], icon: kition_pdf_annot::NoteIcon::Comment },
                style: kition_pdf_annot::Style::default(),
                contents: "Fidelity".into(),
                author: "Kition".into(),
            };
            let meta = kition_pdf_annot::Meta { date: Some("D:20261018120000Z".into()), id: "fidelity".into() };
            kition_pdf_annot::add_annotation(&mut doc, &note, &meta).map_err(|error| format!("annotation failed: {error}"))?;
        }
        Leg::Fill => {
            use kition_pdf_forms::{FieldKind, FieldValue};
            let fields = kition_pdf_forms::fields(&doc);
            let value = |f: &kition_pdf_forms::Field| match f.kind {
                FieldKind::Text => Some(FieldValue::Text("Kition".into())),
                FieldKind::CheckBox => Some(FieldValue::Check(f.value.is_empty() || f.value.iter().any(|v| v == "Off"))),
                _ => None,
            };
            let Some((name, value)) = fields.iter().filter(|f| !f.read_only()).find_map(|f| Some((f.name.clone(), value(f)?))) else {
                return Ok(Prepared::Skip("no writable text or checkbox field"));
            };
            kition_pdf_forms::set_value(&mut doc, &name, &value).map_err(|error| error.to_string())?;
        }
        Leg::Rewrite => {
            if signed(&doc) {
                return Ok(Prepared::Skip("signed document cannot be rewritten"));
            }
        }
    }
    Ok(Prepared::Ready(doc))
}

/// Whether a signature field of `doc` holds a signature.
fn signed(doc: &Document) -> bool {
    kition_pdf_forms::fields(doc)
        .iter()
        .filter(|f| f.kind == kition_pdf_forms::FieldKind::Signature)
        .any(|f| doc.get(f.obj).as_dict().is_some_and(|d| d.contains(b"V")))
}

fn export(leg: Leg, doc: &Model) -> Result<Vec<u8>, String> {
    let opts = SaveOptions::default();
    let written = match leg {
        // As the engine saves: an unedited document is the file as it was.
        Leg::Save if !doc.is_modified() => Ok(doc.bytes().to_vec()),
        Leg::Save | Leg::Annotate | Leg::Fill => write_incremental(doc, &opts),
        Leg::Rewrite => write_full(doc, &opts),
    };
    written.map_err(|error| error.to_string())
}

/// The kind of an object: its `/Type` (with the `/Subtype` of XObjects and annotations), a field,
/// action or structure element by its keys, else `@` and the key it was reached by.
fn kind_of(o: &Object, key: &str) -> String {
    let dict = match o {
        Object::Dict(d) => Some(d),
        Object::Stream(s) => Some(&s.dict),
        _ => None,
    };
    let name = |d: &Dict, k: &[u8]| d.name(k).map(|n| String::from_utf8_lossy(n).into_owned());
    if let Some(d) = dict {
        if let Some(t) = name(d, b"Type") {
            return match name(d, b"Subtype") {
                Some(s) if matches!(t.as_str(), "XObject" | "Annot") => format!("{t}/{s}"),
                _ => t,
            };
        }
        if d.contains(b"FT") || d.contains(b"T") && (d.contains(b"Kids") || d.contains(b"Parent")) {
            return "Field".into();
        }
        if d.contains(b"Subtype") && d.contains(b"Rect") {
            return format!("Annot/{}", name(d, b"Subtype").unwrap_or_default());
        }
        if d.name(b"S").is_some() && d.contains(b"P") {
            return "StructElem".into();
        }
        if d.name(b"S").is_some()
            && matches!(key, "A" | "OpenAction" | "Next" | "E" | "X" | "D" | "U" | "Fo" | "Bl" | "PO" | "PC" | "PV" | "PI" | "K" | "F" | "V" | "C")
        {
            return "Action".into();
        }
    }
    format!("@{key}")
}

/// The class of an object kind.
fn class_of(kind: &str) -> &'static str {
    let base = kind.split('/').next().unwrap_or(kind);
    match base {
        "Catalog" | "@Info" | "Metadata" | "@MarkInfo" | "@ViewerPreferences" | "@PageLabels" | "@OCProperties" | "OCG" | "OCMD" | "@Collection" => {
            "document"
        }
        "Pages" | "Page" | "@Resources" | "@Group" | "Group" => "pages",
        "@Contents" | "@Thumb" => "content",
        "Font" | "FontDescriptor" | "@FontFile" | "@FontFile2" | "@FontFile3" | "@ToUnicode" | "Encoding" | "@Encoding" | "@Widths"
        | "@DescendantFonts" | "@CIDSet" | "@CIDToGIDMap" | "@W" | "CMap" | "@CIDSystemInfo" | "@FontBBox" | "@FontMatrix" => "fonts",
        "XObject" if kind == "XObject/Image" => "images",
        "XObject" => "content",
        "@SMask" | "@Mask" | "@Alternates" => "images",
        "ExtGState" | "Pattern" | "Shading" | "@ColorSpace" | "@Shading" | "@Pattern" | "@ExtGState" | "@Function" | "@Functions" | "@TR" | "@BG"
        | "@UCR" => "graphics",
        "Annot" if kind == "Annot/Widget" => "forms",
        "Annot" | "@Popup" | "@AP" | "@IRT" | "@RT" | "@BS" | "@BE" | "@MK" | "Border" => "annotations",
        "@AcroForm" | "Field" | "@DR" | "@Fields" | "@Opt" | "@CO" | "@XFA" => "forms",
        "Outlines" | "@Outlines" | "@First" | "@Last" | "@Next" | "@Prev" | "@Dests" | "@Names" | "Action" | "@Dest" | "@A" | "@OpenAction"
        | "@AA" | "Filespec" | "EmbeddedFile" | "@EmbeddedFiles" | "@JavaScript" | "@URI" | "@D" => "navigation",
        "StructTreeRoot" | "StructElem" | "@ParentTree" | "@K" | "@RoleMap" | "@ClassMap" | "MCR" | "OBJR" | "@IDTree" | "@Nums"
        | "@StructTreeRoot" | "@Pg" => "structure",
        "Sig" | "DocTimeStamp" | "@Perms" | "DSS" | "@DSS" | "@VRI" | "@Certs" | "@CRLs" | "@OCSPs" | "SigRef" | "TransformParams" | "@Reference"
        | "@Lock" | "SigFieldLock" => "signatures",
        _ => "other",
    }
}

/// The object graph reachable from the trailer: `{trailer, version, objects: [{kind: object}]}`.
fn model_value(doc: &Model) -> Value {
    struct Walk<'d> {
        doc: &'d Document,
        kinds: HashMap<ObjRef, String>,
        queue: VecDeque<ObjRef>,
    }
    impl Walk<'_> {
        /// The kind of the object `r` points to (seen first by `key`), queued to be written.
        fn reach(&mut self, r: ObjRef, key: &str) -> String {
            if let Some(kind) = self.kinds.get(&r) {
                return kind.clone();
            }
            let o = self.doc.get(r);
            if matches!(*o, Object::Null) {
                return "null".into();
            }
            let kind = kind_of(&o, key);
            self.kinds.insert(r, kind.clone());
            self.queue.push_back(r);
            kind
        }

        fn value(&mut self, o: &Object, key: &str) -> Value {
            match o {
                Object::Null => Value::Null,
                Object::Bool(b) => json!(b),
                Object::Int(i) => json!(i),
                Object::Real(r) if r.is_finite() => json!(r),
                Object::Real(r) => json!(r.to_string()),
                Object::String(s) => {
                    let text = s.to_text();
                    if s.bytes.len() > 256 || text.chars().any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t')) {
                        json!(format!("<{}>", digest(&s.bytes)))
                    } else {
                        json!(format!("({text})"))
                    }
                }
                Object::Name(n) => json!(format!("/{}", String::from_utf8_lossy(n))),
                // Numbers one by one (keys folded to `#`), so that each is compared within
                // [`close`]'s tolerance.
                Object::Array(items) if items.len() > 1 && items.iter().all(|i| matches!(i, Object::Int(_) | Object::Real(_))) => {
                    Value::Object(items.iter().enumerate().map(|(i, item)| (i.to_string(), self.value(item, key))).collect())
                }
                Object::Array(items) => Value::Array(items.iter().map(|item| self.value(item, key)).collect()),
                Object::Dict(d) => self.dict(d),
                Object::Stream(s) => {
                    let mut v = self.dict(&s.dict);
                    if let Value::Object(map) = &mut v {
                        map.insert("stream".into(), json!(digest(&s.raw)));
                    }
                    v
                }
                Object::Ref(r) => json!(format!("→{}", self.reach(*r, key))),
            }
        }

        fn dict(&mut self, d: &Dict) -> Value {
            let mut entries: Vec<_> = d.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let mut map = serde_json::Map::new();
            for (k, v) in entries {
                let key = String::from_utf8_lossy(k).into_owned();
                let value = self.value(v, &key);
                map.insert(key, value);
            }
            Value::Object(map)
        }
    }
    let mut walk = Walk { doc, kinds: HashMap::new(), queue: VecDeque::new() };
    let trailer = walk.dict(doc.trailer());
    let mut objects = Vec::new();
    while let Some(r) = walk.queue.pop_front() {
        let o = doc.get(r);
        let kind = walk.kinds.get(&r).cloned().unwrap_or_default();
        let key = kind.trim_start_matches('@').to_owned();
        let value = walk.value(&o, &key);
        objects.push(json!({ kind: value }));
    }
    json!({ "trailer": trailer, "version": doc.version(), "objects": objects })
}

/// Keys whose values are ids or caches a save may renumber or recompute without losing anything:
/// the trailer's chain and file identifier, and stream lengths.
fn ignored(key: &str) -> bool {
    matches!(key, "Length" | "Prev" | "XRefStm" | "Size" | "ID")
}

/// Whether two numbers of `class` are the same after a round trip: reals are written with a
/// limited number of digits.
fn close(_class: &str, a: f64, b: f64) -> bool {
    (a - b).abs() <= 0.01_f64.max(a.abs() * 1e-4)
}

/// The feature class of a leaf, by its path: `.objects[].<kind>.…`, `.trailer…`, `.version`.
fn classify(path: &str) -> &'static str {
    if let Some(rest) = path.strip_prefix(".objects[].") {
        let kind = rest.split('.').next().unwrap_or(rest);
        return class_of(kind);
    }
    "document"
}

/// COS objects of a file by kind: the kind, `kind/key` per dictionary key and `kind stream`,
/// over every object the file holds (reachable or not; cross-reference and object streams aside).
fn features(pdf: &[u8]) -> Result<Features, String> {
    let doc = import(pdf)?;
    let mut out = Features::new();
    for num in doc.object_numbers() {
        let Ok(o) = doc.try_get(num) else { continue };
        let dict = match &*o {
            Object::Dict(d) => d,
            Object::Stream(s) => &s.dict,
            _ => continue,
        };
        if matches!(dict.name(b"Type"), Some(b"XRef" | b"ObjStm")) {
            continue;
        }
        let kind = match kind_of(&o, "") {
            k if k == "@" => "(untyped)".to_owned(),
            k => k,
        };
        let counts = out.entry(kind.clone()).or_default();
        *counts.entry(kind.clone()).or_default() += 1;
        for (k, _) in dict.iter() {
            if !ignored(&String::from_utf8_lossy(k)) {
                *counts.entry(format!("{kind}/{}", String::from_utf8_lossy(k))).or_default() += 1;
            }
        }
        if matches!(&*o, Object::Stream(_)) {
            *counts.entry(format!("{kind} stream")).or_default() += 1;
        }
    }
    Ok(out)
}

/// A small document exercising most classes, for the self-test: two pages with text in a font
/// and a form field, written in full.
fn synthetic() -> Vec<u8> {
    use kition_pdf_cos::{PdfString, Stream};
    let mut doc = Document::new_empty();
    let mut font = Dict::new();
    font.set(b"Type".to_vec(), Object::name("Font"));
    font.set(b"Subtype".to_vec(), Object::name("Type1"));
    font.set(b"BaseFont".to_vec(), Object::name("Helvetica"));
    let font = doc.add(font);
    let pages = doc.trailer().reference(b"Root").and_then(|r| doc.get(r).as_dict()?.reference(b"Pages")).unwrap_or_else(|| panic!("no page tree"));
    let mut kids = Vec::new();
    for text in ["First", "Second"] {
        let content = doc.add(Object::Stream(Stream::from_raw(Dict::new(), format!("BT /F1 12 Tf 72 720 Td ({text}) Tj ET").into_bytes())));
        let mut fonts = Dict::new();
        fonts.set(b"F1".to_vec(), Object::Ref(font));
        let mut resources = Dict::new();
        resources.set(b"Font".to_vec(), Object::Dict(fonts));
        let mut page = Dict::new();
        page.set(b"Type".to_vec(), Object::name("Page"));
        page.set(b"Parent".to_vec(), Object::Ref(pages));
        page.set(b"MediaBox".to_vec(), Object::Array(vec![Object::Int(0), Object::Int(0), Object::Real(612.0), Object::Real(792.0)]));
        page.set(b"Resources".to_vec(), Object::Dict(resources));
        page.set(b"Contents".to_vec(), Object::Ref(content));
        kids.push(Object::Ref(doc.add(page)));
    }
    let count = kids.len() as i64;
    doc.update_dict(pages, |d| {
        d.set(b"Kids".to_vec(), Object::Array(kids));
        d.set(b"Count".to_vec(), Object::Int(count));
    })
    .unwrap_or_else(|e| panic!("pages: {e}"));
    let mut info = Dict::new();
    info.set(b"Title".to_vec(), Object::String(PdfString::text("Fidelity")));
    let info = doc.add(info);
    doc.trailer_mut().set(b"Info".to_vec(), Object::Ref(info));
    kition_pdf_forms::add_field(&mut doc, 0, [72.0, 600.0, 300.0, 624.0], &kition_pdf_forms::NewField::Text { multiline: false }, Some("Name"))
        .unwrap_or_else(|e| panic!("field: {e}"));
    write_full(&doc, &SaveOptions::default()).unwrap_or_else(|e| panic!("write: {e}"))
}

// ---- Shared scorer (as in kition-word, kition-slide, kition-sheet, per leg) ----------------------

/// Leaves of a model by class: `path=value` → count.
type Leaves = BTreeMap<&'static str, BTreeMap<String, u64>>;
/// Features of a package by part kind: feature → count.
type Features = BTreeMap<String, BTreeMap<String, u64>>;

fn digest(bytes: &[u8]) -> String {
    // FNV-1a: enough to tell two media items apart in a report.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3);
    }
    format!("{:016x}/{}", hash, bytes.len())
}

fn fold_digits(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut in_digits = false;
    for c in name.chars() {
        if c.is_ascii_digit() {
            if !in_digits {
                out.push('#');
            }
            in_digits = true;
        } else {
            out.push(c);
            in_digits = false;
        }
    }
    out
}

fn leaves(value: &Value) -> Leaves {
    fn walk(value: &Value, path: &mut String, out: &mut Leaves) {
        match value {
            Value::Null => {}
            Value::Object(map) => {
                for (key, item) in map {
                    if ignored(key) {
                        continue;
                    }
                    let len = path.len();
                    path.push('.');
                    path.push_str(if key.chars().all(|c| c.is_ascii_digit() || c == '-') { "#" } else { key });
                    walk(item, path, out);
                    path.truncate(len);
                }
            }
            Value::Array(items) if items.iter().any(|v| v.is_object() || v.is_array()) => {
                let len = path.len();
                path.push_str("[]");
                for item in items {
                    walk(item, path, out);
                }
                path.truncate(len);
            }
            scalar => {
                *out.entry(classify(path)).or_default().entry(format!("{path}={scalar}")).or_default() += 1;
            }
        }
    }
    let mut out = Leaves::new();
    walk(value, &mut String::new(), &mut out);
    out
}

#[derive(Default)]
struct ClassScore {
    files: u64,
    files_kept: u64,
    leaves: u64,
    kept: u64,
    added: u64,
}

#[derive(Default)]
struct Scorecard {
    leg: Leg,
    /// Files the leg does not apply to (no page to comment on, no field to fill, signed).
    skipped: u64,
    skip_reasons: BTreeMap<String, u64>,
    elapsed_ms: u128,
    /// Incremental legs: files saved, and how many of them start with the original bytes.
    original_files: u64,
    original_kept: u64,
    /// Incremental legs: objects of the original files, and how many the edit rewrote.
    objects: u64,
    rewritten: u64,
    files: u64,
    unreadable: u64,
    unwritable: u64,
    unrereadable: u64,
    classes: BTreeMap<&'static str, ClassScore>,
    /// (class, path with digits folded) → (files, leaves lost, an example: lost leaf → a leaf the
    /// second import has instead on that path, if any)
    lost_leaves: BTreeMap<(&'static str, String), (u64, u64, String)>,
    /// (class, path with digits folded) → (files, leaves added, an example)
    added_leaves: BTreeMap<(&'static str, String), (u64, u64, String)>,
    xml_original: u64,
    xml_kept: u64,
    /// (part kind, feature) → (files, count lost)
    lost_features: BTreeMap<(String, String), (u64, u64)>,
    part_kinds: BTreeMap<String, (u64, u64)>,
    lines: Vec<String>,
}

/// The number a leaf holds, if it holds one.
fn number_of(leaf: &str) -> Option<f64> {
    leaf.split_once('=').and_then(|(_, v)| if v.starts_with('"') { None } else { v.parse().ok() })
}

/// Folded path → (count, an example).
type Paths = BTreeMap<String, (u64, String)>;

/// How many of `left`'s leaves `right` keeps (equal, or a number [`close`] to a spare one on the
/// same path), how many it adds, what it lost and what it added.
fn compare(class: &str, left: &BTreeMap<String, u64>, right: &BTreeMap<String, u64>) -> (u64, u64, Paths, Paths) {
    let mut spare: BTreeMap<String, Vec<(f64, &str)>> = BTreeMap::new();
    let mut added = 0;
    for (leaf, m) in right {
        let extra = m.saturating_sub(left.get(leaf).copied().unwrap_or(0));
        added += extra;
        if let Some(v) = number_of(leaf) {
            for _ in 0..extra {
                spare.entry(path_of(leaf)).or_default().push((v, leaf));
            }
        }
    }
    let mut kept = 0;
    let mut lost = Paths::new();
    for (leaf, n) in left {
        let m = right.get(leaf).copied().unwrap_or(0);
        kept += m.min(*n);
        let path = path_of(leaf);
        for _ in m..*n {
            if let (Some(v), Some(list)) = (number_of(leaf), spare.get_mut(&path))
                && let Some(i) = list.iter().position(|(w, _)| close(class, v, *w))
            {
                list.swap_remove(i);
                kept += 1;
                added -= 1;
                continue;
            }
            let entry = lost.entry(fold_digits(&path)).or_default();
            entry.0 += 1;
            if entry.1.is_empty() {
                let instead = right.keys().find(|r| path_of(r) == path && !left.contains_key(*r)).map(String::as_str).unwrap_or("nothing");
                entry.1 = format!("{} → {}", clip(leaf), clip(instead));
            }
        }
    }
    let mut gained = Paths::new();
    let mut note = |leaf: &str, n: u64| {
        let entry = gained.entry(fold_digits(&path_of(leaf))).or_default();
        entry.0 += n;
        if entry.1.is_empty() {
            entry.1 = clip(leaf);
        }
    };
    for (leaf, m) in right {
        let extra = m.saturating_sub(left.get(leaf).copied().unwrap_or(0));
        if extra > 0 && number_of(leaf).is_none() {
            note(leaf, extra);
        }
    }
    for (_, leaf) in spare.values().flatten() {
        note(leaf, 1);
    }
    (kept, added, lost, gained)
}

/// `leaf`, cut to a readable length.
fn clip(leaf: &str) -> String {
    let mut out: String = leaf.chars().take(160).collect();
    if out.len() < leaf.len() {
        out.push('…');
    }
    out
}

fn path_of(leaf: &str) -> String {
    leaf.split_once('=').map(|(p, _)| p.to_owned()).unwrap_or_default()
}

impl Scorecard {
    fn file(&mut self, name: &str, bytes: &[u8]) {
        let started = std::time::Instant::now();
        self.measure_file(name, bytes);
        let elapsed_ms = started.elapsed().as_millis();
        self.elapsed_ms += elapsed_ms;
        self.lines.push(format!("{name}: elapsed_ms={elapsed_ms}"));
    }

    fn measure_file(&mut self, name: &str, bytes: &[u8]) {
        let first = match prepare(self.leg, bytes) {
            Ok(Prepared::Ready(first)) => first,
            Ok(Prepared::Skip(reason)) => {
                self.skipped += 1;
                *self.skip_reasons.entry(reason.into()).or_default() += 1;
                self.lines.push(format!("{name}: skipped: {reason}"));
                return;
            }
            Err(error) => {
                self.files += 1;
                self.unreadable += 1;
                self.lines.push(format!("{name}: import or edit failed: {error}"));
                return;
            }
        };
        self.files += 1;
        let written = match export(self.leg, &first) {
            Ok(written) => written,
            Err(error) => {
                self.unwritable += 1;
                self.lines.push(format!("{name}: export failed: {error}"));
                return;
            }
        };
        if self.leg.incremental() {
            let original: std::collections::HashSet<u32> = import(bytes).map(|d| d.object_numbers().into_iter().collect()).unwrap_or_default();
            self.objects += original.len() as u64;
            self.rewritten += first.modified_objects().iter().filter(|n| original.contains(n)).count() as u64;
            self.original_files += 1;
            if written.starts_with(bytes) {
                self.original_kept += 1;
            } else {
                self.lines.push(format!("{name}: the original bytes are not kept"));
            }
        }
        let second = match import(&written) {
            Ok(second) => second,
            Err(error) => {
                self.unrereadable += 1;
                self.lines.push(format!("{name}: export not re-importable: {error}"));
                return;
            }
        };
        let (a, b) = (leaves(&model_value(&first)), leaves(&model_value(&second)));
        let mut lost_classes = Vec::new();
        let empty = BTreeMap::new();
        for (class, left) in &a {
            let right = b.get(class).unwrap_or(&empty);
            let score = self.classes.entry(class).or_default();
            let total: u64 = left.values().sum();
            let (kept, added, lost_paths, added_paths) = compare(class, left, right);
            score.files += 1;
            score.leaves += total;
            score.kept += kept;
            score.added += added;
            if kept == total && added == 0 {
                score.files_kept += 1;
            } else {
                lost_classes.push(format!("{class} {kept}/{total}+{added}"));
            }
            for (paths, into) in [(lost_paths, &mut self.lost_leaves), (added_paths, &mut self.added_leaves)] {
                for (path, (n, example)) in paths {
                    let entry = into.entry((class, path)).or_default();
                    entry.0 += 1;
                    entry.1 += n;
                    if entry.2.is_empty() {
                        entry.2 = example;
                    }
                }
            }
        }
        for (class, right) in &b {
            if !a.contains_key(class) {
                let score = self.classes.entry(class).or_default();
                score.files += 1;
                score.added += right.values().sum::<u64>();
                lost_classes.push(format!("{class} 0/0+{}", right.values().sum::<u64>()));
            }
        }
        let (Ok(original), Ok(exported)) = (features(bytes), features(&written)) else {
            self.lines.push(format!("{name}: classes changed: {lost_classes:?}; package not comparable"));
            return;
        };
        let (mut total, mut kept) = (0u64, 0u64);
        for (kind, counts) in &original {
            let theirs = exported.get(kind);
            let kind_total: u64 = counts.values().sum();
            let mut kind_kept = 0;
            for (feature, n) in counts {
                let m = theirs.and_then(|t| t.get(feature)).copied().unwrap_or(0);
                kind_kept += m.min(*n);
                if m < *n {
                    let entry = self.lost_features.entry((kind.clone(), feature.clone())).or_default();
                    entry.0 += 1;
                    entry.1 += n - m;
                }
            }
            let entry = self.part_kinds.entry(kind.clone()).or_default();
            entry.0 += kind_total;
            entry.1 += kind_kept;
            total += kind_total;
            kept += kind_kept;
        }
        self.xml_original += total;
        self.xml_kept += kept;
        self.lines.push(format!("{name}: xml {:.1}%; classes changed: {lost_classes:?}", percent(kept, total)));
    }

    fn json(&self) -> Value {
        let classes: serde_json::Map<String, Value> = self
            .classes
            .iter()
            .map(|(class, s)| {
                ((*class).to_owned(), json!({ "files": s.files, "filesKept": s.files_kept, "leaves": s.leaves, "kept": s.kept, "added": s.added }))
            })
            .collect();
        json!({
            "kit": KIT,
            "leg": self.leg.name(),
            "skipped": self.skipped,
            "skipReasons": self.skip_reasons,
            "attempted": self.files + self.skipped,
            "elapsedMs": self.elapsed_ms,
            "original": { "files": self.original_files, "kept": self.original_kept, "objects": self.objects, "rewritten": self.rewritten },
            "files": self.files,
            "unreadable": self.unreadable,
            "unwritable": self.unwritable,
            "unrereadable": self.unrereadable,
            "classes": classes,
            "xml": { "original": self.xml_original, "kept": self.xml_kept },
        })
    }

    fn report(&self) -> String {
        let mut out = String::new();
        let ok = self.files - self.unreadable - self.unwritable - self.unrereadable;
        out.push_str(&format!(
            "{KIT} fidelity, leg `{}`: {} files ({} skipped), {ok} round-tripped ({} unreadable, {} export failed, {} export unreadable)\n",
            self.leg.name(),
            self.files,
            self.skipped,
            self.unreadable,
            self.unwritable,
            self.unrereadable
        ));
        if self.leg.incremental() {
            out.push_str(&format!(
                "original bytes kept: {}/{} files; objects the edit rewrote: {} of {}\n",
                self.original_kept, self.original_files, self.rewritten, self.objects
            ));
        }
        out.push('\n');
        out.push_str("class                        files kept   leaves kept      added\n");
        for (class, s) in &self.classes {
            out.push_str(&format!(
                "{class:<28} {:>4}/{:<4}  {:>6.2}% of {:<9} {}\n",
                s.files_kept,
                s.files,
                percent(s.kept, s.leaves),
                s.leaves,
                s.added
            ));
        }
        out.push_str(&format!(
            "\nCOS retained: {:.2}% of {} objects, keys and streams\n",
            percent(self.xml_kept, self.xml_original),
            self.xml_original
        ));
        let mut kinds: Vec<_> = self.part_kinds.iter().collect();
        kinds.sort_by_key(|(_, (total, kept))| std::cmp::Reverse(total - kept));
        out.push_str("\npart kinds by features lost:\n");
        for (kind, (total, kept)) in kinds.iter().take(25) {
            out.push_str(&format!("  {kind:<50} {:>9} lost of {:<9} ({:.1}% kept)\n", total - kept, total, percent(*kept, *total)));
        }
        let mut lost: Vec<_> = self.lost_features.iter().collect();
        lost.sort_by_key(|(_, (files, count))| std::cmp::Reverse((*files, *count)));
        out.push_str("\ntop lost XML features by files (files, count):\n");
        for ((kind, feature), (files, count)) in lost.iter().take(150) {
            out.push_str(&format!("  {files:>3} {count:>8}  {kind} {feature}\n"));
        }
        lost.sort_by_key(|(_, (files, count))| std::cmp::Reverse((*count, *files)));
        out.push_str("\ntop lost XML features by count (files, count):\n");
        for ((kind, feature), (files, count)) in lost.iter().take(150) {
            out.push_str(&format!("  {files:>3} {count:>8}  {kind} {feature}\n"));
        }
        let mut lost: Vec<_> = self.lost_leaves.iter().collect();
        lost.sort_by_key(|(_, (files, count, _))| std::cmp::Reverse((*files, *count)));
        out.push_str("\ntop lost model leaves (files, count, example):\n");
        for ((class, path), (files, count, example)) in lost.iter().take(300) {
            out.push_str(&format!("  {files:>3} {count:>8}  [{class}] {path}\n               e.g. {example}\n"));
        }
        let mut added: Vec<_> = self.added_leaves.iter().collect();
        added.sort_by_key(|(_, (files, count, _))| std::cmp::Reverse((*files, *count)));
        out.push_str("\ntop added model leaves (files, count, example):\n");
        for ((class, path), (files, count, example)) in added.iter().take(100) {
            out.push_str(&format!("  {files:>3} {count:>8}  [{class}] {path}\n               e.g. {example}\n"));
        }
        out.push_str("\nper file:\n");
        for line in &self.lines {
            out.push_str(&format!("  {line}\n"));
        }
        out
    }
}

fn percent(part: u64, whole: u64) -> f64 {
    if whole == 0 { 100.0 } else { part as f64 * 100.0 / whole as f64 }
}

/// The regressions of `now` against `baseline`, both [`Scorecard::json`] values.
fn regressions(now: &Value, baseline: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let number = |v: &Value, path: &[&str]| path.iter().fold(v, |v, key| &v[*key]).as_u64().unwrap_or(0);
    if number(now, &["files"]) != number(baseline, &["files"]) || number(now, &["skipped"]) != number(baseline, &["skipped"]) {
        out.push("corpus denominator changed (files or skipped); review the baseline".into());
    }
    for key in ["unreadable", "unwritable", "unrereadable"] {
        if number(now, &[key]) > number(baseline, &[key]) {
            out.push(format!("{key}: {} > {}", number(now, &[key]), number(baseline, &[key])));
        }
    }
    if number(now, &["original", "kept"]) < number(baseline, &["original", "kept"]) {
        out.push(format!(
            "original bytes kept by {} files, baseline {}",
            number(now, &["original", "kept"]),
            number(baseline, &["original", "kept"])
        ));
    }
    if number(now, &["original", "rewritten"]) > number(baseline, &["original", "rewritten"]) {
        out.push(format!(
            "the edit rewrote {} objects, baseline {}",
            number(now, &["original", "rewritten"]),
            number(baseline, &["original", "rewritten"])
        ));
    }
    let ratio = |kept: u64, total: u64| if total == 0 { 1.0 } else { kept as f64 / total as f64 };
    if let Some(classes) = baseline["classes"].as_object() {
        for class in classes.keys() {
            let (b, n) = (&baseline["classes"][class], &now["classes"][class]);
            if number(n, &["filesKept"]) < number(b, &["filesKept"]) {
                out.push(format!("{class}: {} files keep it, baseline {}", number(n, &["filesKept"]), number(b, &["filesKept"])));
            }
            // More leaves lost than the class grew by: a model that learns a property may also
            // count it on objects that were lost already, which is not a regression.
            let lost = |v: &Value| number(v, &["leaves"]).saturating_sub(number(v, &["kept"]));
            let grown = number(n, &["leaves"]).saturating_sub(number(b, &["leaves"]));
            if lost(n) > lost(b) + grown {
                out.push(format!("{class}: {} leaves lost, baseline {} (+{grown} leaves)", lost(n), lost(b)));
            }
        }
    }
    let (rb, rn) = (
        ratio(number(baseline, &["xml", "kept"]), number(baseline, &["xml", "original"])),
        ratio(number(now, &["xml", "kept"]), number(now, &["xml", "original"])),
    );
    if rn + 1e-9 < rb {
        out.push(format!("COS retained {:.4}%, baseline {:.4}%", rn * 100.0, rb * 100.0));
    }
    out
}

fn corpus_files_at(dir: &std::path::Path) -> Result<Vec<std::path::PathBuf>, String> {
    let mut pending = vec![dir.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).map_err(|e| format!("{}: {e}", directory.display()))? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let kind = entry.file_type().map_err(|e| format!("{}: {e}", path.display()))?;
            if kind.is_dir() && entry.file_name() == "__MACOSX" {
                continue;
            }
            // Do not follow directory symlinks (which may cycle). File symlinks are useful for
            // preparing a corpus without copying private PDFs into the repository.
            if kind.is_dir() {
                pending.push(path);
            } else if path.is_file()
                && path.extension().is_some_and(|ext| EXTENSIONS.iter().any(|e| ext.eq_ignore_ascii_case(e)))
                && !entry.file_name().to_string_lossy().starts_with("._")
            {
                files.push(path);
            }
        }
    }
    files.sort();
    if files.is_empty() {
        return Err(format!("{}: configured corpus contains no PDFs", dir.display()));
    }
    Ok(files)
}

fn corpus_files() -> Option<(std::path::PathBuf, Vec<std::path::PathBuf>)> {
    let Some(dir) = std::env::var_os(CORPUS_VAR) else {
        assert_ne!(std::env::var("KITION_FIDELITY_REQUIRE_CORPUS").as_deref(), Ok("1"), "{CORPUS_VAR} is required");
        eprintln!("SKIP: {CORPUS_VAR} is unset");
        return None;
    };
    let dir = std::path::PathBuf::from(dir);
    let files = corpus_files_at(&dir).unwrap_or_else(|error| panic!("{error}"));
    Some((dir, files))
}

fn selected_legs() -> Vec<Leg> {
    let Ok(value) = std::env::var("KITION_FIDELITY_LEGS") else { return LEGS.to_vec() };
    let mut selected = Vec::new();
    for name in value.split(',') {
        let leg = LEGS.iter().find(|leg| leg.name() == name.trim()).copied().unwrap_or_else(|| panic!("unknown fidelity leg: {name}"));
        assert!(!selected.contains(&leg), "duplicate fidelity leg: {name}");
        selected.push(leg);
    }
    selected
}

#[test]
fn corpus_fidelity() {
    let Some((dir, files)) = corpus_files() else { return };
    let (mut report, mut legs) = (String::new(), serde_json::Map::new());
    let selected = selected_legs();
    let mut failures = 0;
    let mut changed_class_files = 0;
    for leg in &selected {
        let mut card = Scorecard { leg: *leg, ..Scorecard::default() };
        for (index, path) in files.iter().enumerate() {
            let name = path.strip_prefix(&dir).unwrap_or(path).to_string_lossy();
            let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("{name}: {error}"));
            card.file(&name, &bytes);
            eprintln!("{} {}/{}: {} ({} ms cumulative)", leg.name(), index + 1, files.len(), name, card.elapsed_ms);
        }
        let text = card.report();
        eprintln!("{}", text.split("\npart kinds").next().unwrap_or_default());
        report.push_str(&text);
        report.push_str("\n\n");
        failures += card.unreadable + card.unwritable + card.unrereadable + card.original_files - card.original_kept;
        changed_class_files += card.classes.values().map(|score| score.files - score.files_kept).sum::<u64>();
        legs.insert(leg.name().into(), card.json());
    }
    let now = json!({ "kit": KIT, "corpusFiles": files.len(), "legs": legs });
    if let Ok(out) = std::env::var("KITION_FIDELITY_OUT") {
        let _ = std::fs::create_dir_all(&out);
        std::fs::write(format!("{out}/{KIT}-fidelity.txt"), &report).unwrap_or_else(|error| panic!("{out}: {error}"));
        let pretty = serde_json::to_string_pretty(&now).unwrap_or_default();
        std::fs::write(format!("{out}/{KIT}-fidelity.json"), pretty).unwrap_or_else(|error| panic!("{out}: {error}"));
    }
    assert_eq!(failures, 0, "corpus has import/edit/export/reopen or original-prefix failures; see the fidelity report");
    if std::env::var("KITION_FIDELITY_REQUIRE_LOSSLESS").as_deref() == Ok("1") {
        assert_eq!(changed_class_files, 0, "model classes changed in {changed_class_files} file/class pairs; see the fidelity report");
    } else {
        eprintln!(
            "MEASUREMENT ONLY (not a lossless gate): {changed_class_files} changed file/class pairs; set KITION_FIDELITY_REQUIRE_LOSSLESS=1 to reject changes"
        );
    }
    if let Ok(path) = std::env::var("KITION_FIDELITY_BASELINE") {
        let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"));
        let baseline: Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{path}: {error}"));
        let regressed: Vec<String> = selected
            .iter()
            .flat_map(|leg| {
                regressions(&now["legs"][leg.name()], &baseline["legs"][leg.name()]).into_iter().map(move |r| format!("{}: {r}", leg.name()))
            })
            .collect();
        assert!(regressed.is_empty(), "fidelity regressed against {path}:\n{}", regressed.join("\n"));
    }
}

#[test]
fn scorer_passes_a_lossless_round_trip_and_flags_a_regression() {
    let bytes = synthetic();
    for leg in LEGS {
        let mut card = Scorecard { leg, ..Scorecard::default() };
        card.file("synthetic", &bytes);
        assert_eq!((card.files, card.skipped), (1, 0), "{leg:?}: {}", card.report());
        assert_eq!(card.unreadable + card.unwritable + card.unrereadable, 0, "{leg:?}: {}", card.report());
        assert!(card.classes.values().all(|s| s.files_kept == s.files), "{leg:?}: {}", card.report());
        assert!(card.classes.contains_key(PRIMARY_CLASS), "{leg:?}: {}", card.report());
        assert!(card.xml_original > 0 && card.xml_kept == card.xml_original, "{leg:?}: {}", card.report());
        assert_eq!(card.original_kept, card.original_files, "{leg:?}: {}", card.report());
        // A baseline of this run accepts it; one that kept more flags it.
        let now = card.json();
        assert!(regressions(&now, &now).is_empty());
        let mut better = now.clone();
        better["xml"]["kept"] = json!(card.xml_kept + 1);
        better["xml"]["original"] = json!(card.xml_original);
        assert_eq!(regressions(&now, &better).len(), 1);
    }
}

#[test]
fn checksum_rewrite_is_reported_as_failed_export_not_skipped_or_lossless() {
    let mut source = import(&synthetic()).unwrap();
    source.trailer_mut().set(b"DocChecksum".to_vec(), Object::Name(b"synthetic-checksum".to_vec()));
    let bytes = write_incremental(&source, &SaveOptions::default()).unwrap();
    let mut rewrite = Scorecard { leg: Leg::Rewrite, ..Scorecard::default() };
    rewrite.file("checksum", &bytes);
    assert_eq!((rewrite.files, rewrite.skipped, rewrite.unwritable), (1, 0, 1));
    assert!(rewrite.report().contains("DocChecksum"));
    assert!(rewrite.classes.is_empty(), "no successful output was available to score");
    let mut save = Scorecard { leg: Leg::Save, ..Scorecard::default() };
    save.file("checksum", &bytes);
    assert_eq!((save.files, save.skipped, save.unwritable, save.original_kept), (1, 0, 0, 1));
}

#[test]
fn unknown_trailer_rewrite_is_reported_as_failed_export_not_skipped_or_lossless() {
    let mut source = import(&synthetic()).unwrap();
    source.trailer_mut().set(b"SyntheticTrailer".to_vec(), Object::Int(17));
    let root = source.root().unwrap();
    source.update_dict(root, |dict| dict.set(b"SyntheticEdit".to_vec(), Object::Bool(true))).unwrap();
    let bytes = write_incremental(&source, &SaveOptions::default()).unwrap();
    assert!(bytes.starts_with(source.bytes()));
    assert!(bytes.len() > source.bytes().len());
    let reopened = import(&bytes).unwrap();
    assert_eq!(reopened.trailer().get(b"SyntheticTrailer"), Some(&Object::Int(17)));
    let mut rewrite = Scorecard { leg: Leg::Rewrite, ..Scorecard::default() };
    rewrite.file("unknown-trailer", &bytes);
    assert_eq!((rewrite.files, rewrite.skipped, rewrite.unwritable), (1, 0, 1));
    assert!(rewrite.report().contains(&kition_pdf_cos::CosError::UnsupportedTrailerRewrite.to_string()));
    assert!(!rewrite.report().contains("SyntheticTrailer"), "private keys must not leak into errors");
    assert!(rewrite.classes.is_empty(), "a refused export has no successful output to score");
    let mut save = Scorecard { leg: Leg::Save, ..Scorecard::default() };
    save.file("unknown-trailer", &bytes);
    assert_eq!((save.files, save.skipped, save.unwritable, save.original_kept), (1, 0, 0, 1));
}

/// A leg that loses an object is flagged: a comment written without its page's `/Annots` entry.
#[test]
fn scorer_flags_a_lost_object() {
    let bytes = synthetic();
    let mut first = import(&bytes).unwrap();
    let mut card = Scorecard { leg: Leg::Annotate, ..Scorecard::default() };
    card.file("synthetic", &bytes);
    let kept = card.json();
    // The same edit, saved by a writer that dropped the page's change.
    let note = kition_pdf_annot::NewAnnotation {
        page: 1,
        shape: kition_pdf_annot::Shape::Note { at: [36.0, 36.0], icon: kition_pdf_annot::NoteIcon::Comment },
        style: kition_pdf_annot::Style::default(),
        contents: "Lost".into(),
        author: "Kition".into(),
    };
    kition_pdf_annot::add_annotation(&mut first, &note, &kition_pdf_annot::Meta { date: None, id: "lost".into() }).unwrap();
    let saved = write_incremental(&import(&bytes).unwrap(), &SaveOptions::default()).unwrap();
    let (a, b) = (leaves(&model_value(&first)), leaves(&model_value(&import(&saved).unwrap())));
    let empty = BTreeMap::new();
    let lost: u64 = a.iter().map(|(class, left)| left.values().sum::<u64>() - compare(class, left, b.get(class).unwrap_or(&empty)).0).sum();
    assert!(lost > 0, "the comment and the page's /Annots are lost");
    assert!(regressions(&kept, &kept).is_empty());
}
