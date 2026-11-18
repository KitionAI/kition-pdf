//! What [`KitDocument::check`](kition_kit::KitDocument::check) reports on a PDF (KHP
//! A21): a file that does not parse, a page tree that counts pages it does not hold, pages
//! whose content is missing or damaged, fonts the file does not embed, links to pages or named
//! destinations the file lacks, annotations outside their page, form fields that are required but
//! empty, hold a choice their list does not offer, or exceed their length, tagged figures without
//! alternative text, permissions an encrypted file withholds, and actions and streams that name
//! another file, which the kit never opens, runs or reads. Targets name the page (from 1)
//! and, for annotations and fields, a path: `annot:<page>/<index in the page's /Annots>`,
//! `field:<fully qualified name>`, `font:<name>`, `object:<number> <generation>`.
//!
//! Codes: `pdf.unreadable`, `pdf.page-count-mismatch`, `pdf.missing-content`,
//! `pdf.damaged-content`, `pdf.font-not-embedded`, `pdf.broken-link`, `pdf.annotation-off-page`,
//! `pdf.required-field-empty`, `pdf.invalid-choice`, `pdf.text-too-long`, `pdf.missing-alt-text`,
//! `pdf.restricted`, `pdf.external-reference`, and `pdf.truncated` when a code is found more often
//! than [`PER_CODE`] times.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use kition_kit::diagnostics::{Diagnostic, Severity, Target};
use kition_pdf_cos::{Dict, Document, ObjRef, Object};
use kition_pdf_engine::{FormField, FormFieldKind, field_flags};

/// At most this many diagnostics of one code; the rest are counted in one `pdf.truncated`.
pub const PER_CODE: usize = 100;

/// How deep name trees and nested form XObjects are followed.
const MAX_DEPTH: usize = 32;

/// The fonts every PDF viewer has (ISO 32000-2 §9.6.2.2), by base name.
const STANDARD_14: [&str; 14] = [
    "Times-Roman",
    "Times-Bold",
    "Times-Italic",
    "Times-BoldItalic",
    "Helvetica",
    "Helvetica-Bold",
    "Helvetica-Oblique",
    "Helvetica-BoldOblique",
    "Courier",
    "Courier-Bold",
    "Courier-Oblique",
    "Courier-BoldOblique",
    "Symbol",
    "ZapfDingbats",
];

/// Everything worth fixing in the PDF `bytes` (opened with `password`), errors first. `form` is
/// the engine's reading of the form fields.
pub fn check(bytes: std::sync::Arc<Vec<u8>>, password: Option<&str>, form: &[FormField]) -> Vec<Diagnostic> {
    match Document::open_with_password(bytes, password) {
        Ok(doc) => check_document(&doc, form),
        Err(error) => vec![Diagnostic::new("pdf.unreadable", Severity::Error, format!("The file cannot be read as a PDF: {error}."))],
    }
}

/// [`check`] on a parsed document.
pub fn check_document(doc: &Document, form: &[FormField]) -> Vec<Diagnostic> {
    let mut found = Found::default();
    let pages = kition_pdf_model::pages(doc);
    let refs: Vec<ObjRef> = pages.iter().map(|page| page.obj).collect();

    page_tree(doc, pages.len(), &mut found);
    for (index, page) in pages.iter().enumerate() {
        content(doc, index, &page.dict, &mut found);
    }
    fonts(doc, &pages, &mut found);
    let named = named_destinations(doc);
    for (index, page) in pages.iter().enumerate() {
        annotations(doc, index, page, &refs, &named, &mut found);
    }
    fields(form, &mut found);
    figures(doc, &mut found);
    restrictions(doc, &mut found);
    external_references(doc, &mut found);
    found.finish()
}

#[derive(Default)]
struct Found {
    list: Vec<Diagnostic>,
    counts: BTreeMap<String, usize>,
}

impl Found {
    fn push(&mut self, diagnostic: Diagnostic) {
        let count = self.counts.entry(diagnostic.code.clone()).or_default();
        *count += 1;
        if *count <= PER_CODE {
            self.list.push(diagnostic);
        }
    }

    fn finish(mut self) -> Vec<Diagnostic> {
        for (code, count) in &self.counts {
            if *count > PER_CODE {
                self.list.push(Diagnostic::new(
                    "pdf.truncated",
                    Severity::Info,
                    format!("{} more `{code}` diagnostics are not listed.", count - PER_CODE),
                ));
            }
        }
        self.list.sort_by_key(|diagnostic| diagnostic.severity);
        self.list
    }
}

fn on_page(index: usize) -> Target {
    Target { page: u32::try_from(index + 1).ok(), ..Target::default() }
}

fn at(index: Option<usize>, path: String) -> Target {
    Target { page: index.and_then(|i| u32::try_from(i + 1).ok()), path: Some(path), ..Target::default() }
}

/// The page tree's `/Count` against the pages it holds.
fn page_tree(doc: &Document, held: usize, found: &mut Found) {
    let count = doc
        .root()
        .and_then(|root| doc.get(root).as_dict().and_then(|catalog| catalog.get(b"Pages").cloned()))
        .and_then(|pages| doc.dict(&pages))
        .and_then(|pages| pages.get(b"Count").and_then(|count| doc.resolve(count).as_int()));
    if let Some(count) = count
        && usize::try_from(count).ok() != Some(held)
    {
        found.push(Diagnostic::new(
            "pdf.page-count-mismatch",
            Severity::Warning,
            format!("The page tree says it holds {count} pages, but {held} can be found: viewers may disagree on the page count."),
        ));
    }
}

/// A page's `/Contents`: present streams that decode.
fn content(doc: &Document, index: usize, page: &Dict, found: &mut Found) {
    let Some(contents) = page.get(b"Contents") else { return };
    let resolved = doc.resolve(contents);
    let parts: Vec<Object> = match &*resolved {
        Object::Array(items) => items.clone(),
        _ => vec![contents.clone()],
    };
    for part in parts {
        let object = doc.resolve(&part);
        match &*object {
            Object::Stream(stream) => {
                if let Err(error) = stream.decoded_strict() {
                    found.push(
                        Diagnostic::new(
                            "pdf.damaged-content",
                            Severity::Warning,
                            format!("Page {} has damaged content ({error}): part of it may not show.", index + 1),
                        )
                        .on(on_page(index)),
                    );
                }
            }
            _ => found.push(
                Diagnostic::new(
                    "pdf.missing-content",
                    Severity::Error,
                    format!("Page {} names content the file does not hold: the page shows blank or partly blank.", index + 1),
                )
                .on(on_page(index)),
            ),
        }
    }
}

/// Fonts used by pages and the forms they draw, without a font program in the file.
fn fonts(doc: &Document, pages: &[kition_pdf_model::Page], found: &mut Found) {
    let mut reported = BTreeSet::new();
    let mut seen_fonts = HashSet::new();
    for (index, page) in pages.iter().enumerate() {
        let mut seen_forms = HashSet::new();
        let resources = page.dict.get(b"Resources").and_then(|r| doc.dict(r));
        let mut stack = vec![(resources, 0usize)];
        while let Some((resources, depth)) = stack.pop() {
            let Some(resources) = resources else { continue };
            if let Some(fonts) = resources.get(b"Font").and_then(|f| doc.dict(f)) {
                for (_, font) in fonts.iter() {
                    if let Some(r) = font.as_ref()
                        && !seen_fonts.insert(r)
                    {
                        continue;
                    }
                    let Some(font) = doc.dict(font) else { continue };
                    let Some(name) = unembedded(doc, &font) else { continue };
                    if !reported.insert(name.clone()) {
                        continue;
                    }
                    let standard = STANDARD_14.contains(&name.as_str());
                    let (severity, message) = if standard {
                        (Severity::Info, format!("The font {name} is not embedded; it is one of the standard 14 fonts, which viewers supply."))
                    } else {
                        (Severity::Warning, format!("The font {name} is not embedded: viewers without it draw its text with another font."))
                    };
                    found.push(Diagnostic::new("pdf.font-not-embedded", severity, message).on(at(Some(index), format!("font:{name}"))));
                }
            }
            if depth >= MAX_DEPTH {
                continue;
            }
            if let Some(xobjects) = resources.get(b"XObject").and_then(|x| doc.dict(x)) {
                for (_, xobject) in xobjects.iter() {
                    if let Some(r) = xobject.as_ref()
                        && !seen_forms.insert(r)
                    {
                        continue;
                    }
                    let object = doc.resolve(xobject);
                    let Object::Stream(stream) = &*object else { continue };
                    if stream.dict.name(b"Subtype") == Some(b"Form") {
                        stack.push((stream.dict.get(b"Resources").and_then(|r| doc.dict(r)), depth + 1));
                    }
                }
            }
        }
    }
}

/// The base name of a font whose program the file lacks; `None` when it is embedded or a Type 3
/// font (whose glyphs are in the file).
fn unembedded(doc: &Document, font: &Dict) -> Option<String> {
    let subtype = font.name(b"Subtype")?;
    if subtype == b"Type3" {
        return None;
    }
    let base = font.name(b"BaseFont").map(|n| String::from_utf8_lossy(n).into_owned()).unwrap_or_else(|| "(unnamed)".into());
    let described = if subtype == b"Type0" {
        let descendants = font.get(b"DescendantFonts").map(|d| doc.resolve(d))?;
        let first = descendants.as_array()?.first()?.clone();
        doc.dict(&first)?
    } else {
        font.clone()
    };
    let embedded = described
        .get(b"FontDescriptor")
        .and_then(|d| doc.dict(d))
        .is_some_and(|descriptor| [b"FontFile".as_slice(), b"FontFile2", b"FontFile3"].iter().any(|key| descriptor.contains(key)));
    // A subset prefix (`ABCDEF+Name`) is part of an embedded font's name only.
    let name = base.split_once('+').map_or(base.as_str(), |(_, rest)| rest).to_string();
    (!embedded).then_some(name)
}

/// The names in the catalog's `/Dests` and the `/Names` `/Dests` name tree.
fn named_destinations(doc: &Document) -> BTreeSet<Vec<u8>> {
    let mut names = BTreeSet::new();
    let Some(catalog) = doc.root().and_then(|root| doc.get(root).as_dict().cloned()) else { return names };
    if let Some(dests) = catalog.get(b"Dests").and_then(|d| doc.dict(d)) {
        names.extend(dests.iter().map(|(name, _)| name.clone()));
    }
    let tree = catalog.get(b"Names").and_then(|n| doc.dict(n)).and_then(|n| n.get(b"Dests").cloned());
    let mut stack = tree.map(|t| vec![(t, 0usize)]).unwrap_or_default();
    let mut seen = HashSet::new();
    while let Some((node, depth)) = stack.pop() {
        if let Some(r) = node.as_ref()
            && !seen.insert(r)
        {
            continue;
        }
        let Some(node) = doc.dict(&node) else { continue };
        if let Some(pairs) = node.get(b"Names").map(|n| doc.resolve(n))
            && let Some(pairs) = pairs.as_array()
        {
            for key in pairs.iter().step_by(2) {
                if let Some(key) = doc.resolve(key).as_string() {
                    names.insert(key.bytes.clone());
                }
            }
        }
        if depth < MAX_DEPTH
            && let Some(kids) = node.get(b"Kids").map(|k| doc.resolve(k))
            && let Some(kids) = kids.as_array()
        {
            stack.extend(kids.iter().map(|kid| (kid.clone(), depth + 1)));
        }
    }
    names
}

/// Why a destination leads nowhere, or `None` when it reaches a page or a defined name.
fn dead_destination(doc: &Document, dest: &Object, pages: &[ObjRef], named: &BTreeSet<Vec<u8>>) -> Option<String> {
    match &*doc.resolve(dest) {
        Object::Array(items) => match items.first() {
            Some(Object::Ref(r)) if pages.contains(r) => None,
            Some(Object::Int(i)) if usize::try_from(*i).is_ok_and(|i| i < pages.len()) => None,
            Some(Object::Ref(_)) => Some("a page that is not in the document".into()),
            Some(Object::Int(i)) => Some(format!("page {} of {}", i + 1, pages.len())),
            _ => Some("a destination with no page".into()),
        },
        Object::Name(name) => {
            (!named.contains(name)).then(|| format!("the named destination \"{}\", which is not defined", String::from_utf8_lossy(name)))
        }
        Object::String(name) => (!named.contains(&name.bytes)).then(|| format!("the named destination \"{}\", which is not defined", name.to_text())),
        _ => Some("a destination that cannot be read".into()),
    }
}

/// Link destinations and annotation placement on one page.
fn annotations(doc: &Document, index: usize, page: &kition_pdf_model::Page, pages: &[ObjRef], named: &BTreeSet<Vec<u8>>, found: &mut Found) {
    let Some(annots) = page.dict.get(b"Annots").map(|a| doc.resolve(a)) else { return };
    let Some(annots) = annots.as_array() else { return };
    let crop = page.crop(doc);
    // The 1-based number `link_list` gives each link of the page, for the fix's `link_delete`.
    let mut link = 0;
    for (k, annot) in annots.iter().enumerate() {
        let Some(annot) = doc.dict(annot) else { continue };
        let path = format!("annot:{}/{k}", index + 1);
        let subtype = annot.name(b"Subtype").unwrap_or_default();
        if subtype == b"Link" {
            link += 1;
            let dest = annot.get(b"Dest").cloned().or_else(|| {
                let action = annot.get(b"A").and_then(|a| doc.dict(a))?;
                (action.name(b"S") == Some(b"GoTo")).then(|| action.get(b"D").cloned()).flatten()
            });
            if let Some(dest) = dest
                && let Some(why) = dead_destination(doc, &dest, pages, named)
            {
                found.push(
                    Diagnostic::new("pdf.broken-link", Severity::Error, format!("A link on page {} goes to {why}.", index + 1))
                        .on(at(Some(index), path.clone()))
                        .fix("link_delete", Some(serde_json::json!({ "page": index + 1, "index": link }))),
                );
            }
        }
        if subtype == b"Popup" {
            continue;
        }
        let rect: Vec<f64> = annot
            .get(b"Rect")
            .map(|r| doc.resolve(r))
            .and_then(|r| r.as_array().map(|a| a.iter().filter_map(|x| doc.resolve(x).as_f64()).collect()))
            .unwrap_or_default();
        if let [x0, y0, x1, y1] = rect[..] {
            let (left, right, bottom, top) = (x0.min(x1), x0.max(x1), y0.min(y1), y0.max(y1));
            let outside = right < crop[0] || left > crop[2] || top < crop[1] || bottom > crop[3];
            if outside {
                let kind = String::from_utf8_lossy(subtype).into_owned();
                found.push(
                    Diagnostic::new(
                        "pdf.annotation-off-page",
                        Severity::Warning,
                        format!("A {kind} annotation on page {} lies outside the page: nobody sees it.", index + 1),
                    )
                    .on(at(Some(index), path)),
                );
            }
        }
    }
}

/// Form fields that are required but empty, hold a choice the list does not offer, or are too long.
fn fields(form: &[FormField], found: &mut Found) {
    for field in form {
        let page = field.widgets.iter().find_map(|w| w.page);
        let target = || at(page, format!("field:{}", field.name));
        let empty = field.value.iter().all(|value| value.trim().is_empty());
        if field.has(field_flags::REQUIRED) && empty && !matches!(field.kind, FormFieldKind::PushButton | FormFieldKind::Signature) {
            found.push(
                Diagnostic::new("pdf.required-field-empty", Severity::Warning, format!("The required field \"{}\" is empty.", field.name))
                    .on(target()),
            );
        }
        let editable = field.has(field_flags::EDIT);
        if matches!(field.kind, FormFieldKind::Combo | FormFieldKind::List) && !editable && !field.options.is_empty() {
            for value in &field.value {
                if !value.is_empty() && !field.options.iter().any(|(export, _)| export == value) {
                    found.push(
                        Diagnostic::new(
                            "pdf.invalid-choice",
                            Severity::Warning,
                            format!("The field \"{}\" holds \"{value}\", which its list does not offer.", field.name),
                        )
                        .on(target()),
                    );
                }
            }
        }
        if matches!(field.kind, FormFieldKind::Text)
            && let Some(max) = field.max_len
            && let Some(value) = field.value.first()
            && value.chars().count() > max
        {
            found.push(
                Diagnostic::new(
                    "pdf.text-too-long",
                    Severity::Warning,
                    format!("The field \"{}\" holds {} characters; it allows {max}.", field.name, value.chars().count()),
                )
                .on(target()),
            );
        }
    }
}

/// Tagged figures with no alternative text.
fn figures(doc: &Document, found: &mut Found) {
    for figure in kition_pdf_engine::a11y::figures(doc) {
        if figure.alt.as_deref().is_none_or(|alt| alt.trim().is_empty()) {
            let target = Target {
                page: figure.page.and_then(|i| u32::try_from(i + 1).ok()),
                path: Some(format!("figure:{} {}", figure.obj.num, figure.obj.generation)),
                ..Target::default()
            };
            found.push(
                Diagnostic::new(
                    "pdf.missing-alt-text",
                    Severity::Info,
                    "A tagged figure has no alternative text: screen readers cannot describe it.",
                )
                .on(target),
            );
        }
    }
}

/// Actions and streams that name another file (KHP A29, A42; section 10.6): a launch action (a
/// program or file to open), a go-to into another PDF, form data to import from a file, and a
/// stream whose data lies in another file (`/F` on a stream). Kition PDF never opens, runs or reads
/// any of them, in the page or headless: the diagnostic says so and names the file as the document
/// gives it (data, never an instruction). Found in every object and the dictionaries written
/// inside it, to [`MAX_DEPTH`].
fn external_references(doc: &Document, found: &mut Found) {
    for (r, object) in doc.scan_objects() {
        let target = || at(None, format!("object:{} {}", r.num, r.generation));
        if let Object::Stream(stream) = &*object
            && stream.dict.get(b"F").is_some()
        {
            let file = file_named(doc, stream.dict.get(b"F"));
            found.push(
                Diagnostic::new(
                    "pdf.external-reference",
                    Severity::Warning,
                    format!("A stream's data is in another file{file}, which Kition PDF never reads: what it holds does not show."),
                )
                .on(target()),
            );
        }
        let mut actions = Vec::new();
        refer_in(&object, 0, &mut actions);
        for (kind, file) in actions {
            let file = file_named(doc, file.as_ref());
            let what = match kind {
                Refers::Launch => format!("A launch action asks to open a program or file{file}"),
                Refers::OtherPdf => format!("A link goes into another PDF{file}"),
                Refers::ImportData => format!("An action imports form data from a file{file}"),
            };
            found.push(
                Diagnostic::new(
                    "pdf.external-reference",
                    Severity::Warning,
                    format!("{what}; Kition PDF never opens, runs or reads another file, so it does nothing here."),
                )
                .on(target()),
            );
        }
    }
}

/// What an action that names another file does.
#[derive(Clone, Copy)]
enum Refers {
    Launch,
    OtherPdf,
    ImportData,
}

/// The actions in `object` and the dictionaries and arrays written inside it (not those it
/// refers to: each is an object of its own), with the file each names.
fn refer_in(object: &Object, depth: usize, out: &mut Vec<(Refers, Option<Object>)>) {
    if depth > MAX_DEPTH {
        return;
    }
    match object {
        Object::Dict(dict) => {
            let kind = match dict.name(b"S") {
                Some(b"Launch") => Some(Refers::Launch),
                Some(b"GoToR") => Some(Refers::OtherPdf),
                // A go-to into an embedded file names no other file unless it has `/F`.
                Some(b"GoToE") if dict.get(b"F").is_some() => Some(Refers::OtherPdf),
                Some(b"ImportData") => Some(Refers::ImportData),
                _ => None,
            };
            if let Some(kind) = kind {
                let file = dict.get(b"F").cloned().or_else(|| {
                    [&b"Win"[..], b"Mac", b"Unix"]
                        .into_iter()
                        .find_map(|platform| dict.get(platform).and_then(Object::as_dict).and_then(|d| d.get(b"F").cloned()))
                });
                out.push((kind, file));
            }
            for (_, value) in dict.iter() {
                refer_in(value, depth + 1, out);
            }
        }
        Object::Array(items) => {
            for item in items {
                refer_in(item, depth + 1, out);
            }
        }
        _ => {}
    }
}

/// ` ("<file>")` for a file specification (a string, or a dictionary's `/UF` or `/F`), at most 200
/// characters; empty when there is none to show.
fn file_named(doc: &Document, spec: Option<&Object>) -> String {
    let Some(spec) = spec else { return String::new() };
    let spec = doc.resolve(spec);
    let bytes = match &*spec {
        Object::String(text) => Some(text.bytes.clone()),
        Object::Dict(dict) => {
            let file = dict.get(b"UF").or_else(|| dict.get(b"F"));
            file.and_then(|file| doc.resolve(file).as_string().map(|text| text.bytes.clone()))
        }
        _ => None,
    };
    match bytes {
        Some(bytes) if !bytes.is_empty() => format!(" (\"{}\")", String::from_utf8_lossy(&bytes).chars().take(200).collect::<String>()),
        _ => String::new(),
    }
}

/// What an encrypted file withholds from whoever opened it.
fn restrictions(doc: &Document, found: &mut Found) {
    let Some(permissions) = doc.permissions() else { return };
    let denied: Vec<&str> = [
        (permissions.print(), "printing"),
        (permissions.modify(), "editing"),
        (permissions.copy(), "copying text"),
        (permissions.annotate(), "commenting"),
        (permissions.fill_forms(), "filling forms"),
        (permissions.assemble(), "organizing pages"),
    ]
    .into_iter()
    .filter(|(allowed, _)| !allowed)
    .map(|(_, what)| what)
    .collect();
    if !denied.is_empty() {
        found.push(Diagnostic::new("pdf.restricted", Severity::Info, format!("The file is encrypted and does not allow {}.", denied.join(", "))));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kition_kit::diagnostics::check_diagnostics;

    use super::*;

    /// A PDF of `objects` (object 1 is the catalog), with an xref table.
    fn pdf(objects: &[String]) -> Arc<Vec<u8>> {
        let mut out = b"%PDF-1.7\n".to_vec();
        let mut offsets = Vec::new();
        for (i, object) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
        }
        let xref = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for offset in offsets {
            out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
        Arc::new(out)
    }

    fn stream(body: &str) -> String {
        format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len())
    }

    /// One page with `page` added to its dictionary, plus `extra` objects from number 5 on.
    fn one_page(page: &str, extra: &[String]) -> Arc<Vec<u8>> {
        let mut objects = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 600 800] >>".to_string(),
            format!("<< /Type /Page /Parent 2 0 R /Contents 4 0 R {page} >>"),
            stream("BT ET"),
        ];
        objects.extend_from_slice(extra);
        pdf(&objects)
    }

    fn run(bytes: Arc<Vec<u8>>) -> Vec<Diagnostic> {
        let found = check(bytes, None, &[]);
        check_diagnostics(&found).expect("valid diagnostics");
        found
    }

    fn codes(found: &[Diagnostic]) -> Vec<&str> {
        found.iter().map(|d| d.code.as_str()).collect()
    }

    #[test]
    fn a_plain_page_has_nothing_to_report() {
        assert!(run(one_page("", &[])).is_empty());
    }

    #[test]
    fn a_file_that_does_not_parse() {
        let found = run(Arc::new(b"not a pdf at all".to_vec()));
        assert_eq!(codes(&found), ["pdf.unreadable"]);
    }

    #[test]
    fn a_page_tree_that_miscounts() {
        let bytes = pdf(&[
            "<< /Type /Catalog /Pages 2 0 R >>".into(),
            "<< /Type /Pages /Kids [3 0 R] /Count 4 /MediaBox [0 0 600 800] >>".into(),
            "<< /Type /Page /Parent 2 0 R >>".into(),
        ]);
        assert_eq!(codes(&run(bytes)), ["pdf.page-count-mismatch"]);
    }

    #[test]
    fn content_the_file_lacks_or_cannot_decode() {
        let bytes = pdf(&[
            "<< /Type /Catalog /Pages 2 0 R >>".into(),
            "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 600 800] >>".into(),
            "<< /Type /Page /Parent 2 0 R /Contents 9 0 R >>".into(),
            "<< /Type /Page /Parent 2 0 R /Contents 5 0 R >>".into(),
            "<< /Length 9 /Filter /FlateDecode >>\nstream\nnot flate\nendstream".into(),
        ]);
        let found = run(bytes);
        assert_eq!(codes(&found), ["pdf.missing-content", "pdf.damaged-content"]);
        assert_eq!(found[0].target.page, Some(1));
        assert_eq!(found[1].target.page, Some(2));
    }

    #[test]
    fn fonts_without_their_program() {
        let bytes = one_page(
            "/Resources << /Font << /F1 5 0 R /F2 6 0 R /F3 7 0 R /F4 8 0 R >> >>",
            &[
                "<< /Type /Font /Subtype /TrueType /BaseFont /CorporateSans /FontDescriptor 9 0 R >>".into(),
                "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
                "<< /Type /Font /Subtype /TrueType /BaseFont /ABCDEF+Embedded /FontDescriptor 10 0 R >>".into(),
                "<< /Type /Font /Subtype /Type3 /FontMatrix [1 0 0 1 0 0] /CharProcs << >> >>".into(),
                "<< /Type /FontDescriptor /FontName /CorporateSans >>".into(),
                "<< /Type /FontDescriptor /FontName /ABCDEF+Embedded /FontFile2 11 0 R >>".into(),
                stream("font"),
            ],
        );
        let found = run(bytes);
        assert_eq!(codes(&found), ["pdf.font-not-embedded", "pdf.font-not-embedded"]);
        assert_eq!(found[0].severity, Severity::Warning);
        assert_eq!(found[0].target.path.as_deref(), Some("font:CorporateSans"));
        assert_eq!(found[1].severity, Severity::Info);
        assert!(found[1].message.contains("Helvetica"));
    }

    #[test]
    fn fonts_inside_form_xobjects() {
        let bytes = one_page(
            "/Resources << /XObject << /X1 5 0 R >> >>",
            &[
                "<< /Type /XObject /Subtype /Form /BBox [0 0 10 10] /Resources << /Font << /F1 6 0 R >> >> /Length 0 >>\nstream\n\nendstream".into(),
                "<< /Type /Font /Subtype /Type1 /BaseFont /InnerFont >>".into(),
            ],
        );
        assert_eq!(codes(&run(bytes)), ["pdf.font-not-embedded"]);
    }

    #[test]
    fn links_to_missing_pages_and_names() {
        let bytes = pdf(&[
            "<< /Type /Catalog /Pages 2 0 R /Dests << /intro [3 0 R /Fit] >> >>".into(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 600 800] >>".into(),
            "<< /Type /Page /Parent 2 0 R /Annots [4 0 R 5 0 R 6 0 R 7 0 R 8 0 R] >>".into(),
            "<< /Type /Annot /Subtype /Link /Rect [10 10 50 50] /Dest [3 0 R /Fit] >>".into(),
            "<< /Type /Annot /Subtype /Link /Rect [10 10 50 50] /Dest /intro >>".into(),
            "<< /Type /Annot /Subtype /Link /Rect [10 10 50 50] /Dest /missing >>".into(),
            "<< /Type /Annot /Subtype /Link /Rect [10 10 50 50] /A << /S /GoTo /D [7] >> >>".into(),
            "<< /Type /Annot /Subtype /Link /Rect [10 10 50 50] /A << /S /URI /URI (https://example.com) >> >>".into(),
        ]);
        let found = run(bytes);
        assert_eq!(codes(&found), ["pdf.broken-link", "pdf.broken-link"]);
        assert!(found[0].message.contains("\"missing\""));
        assert_eq!(found[0].target.path.as_deref(), Some("annot:1/2"));
        assert!(found[1].message.contains("page 8 of 1"));
    }

    #[test]
    fn named_destinations_in_a_name_tree() {
        let bytes = pdf(&[
            "<< /Type /Catalog /Pages 2 0 R /Names << /Dests << /Kids [5 0 R] >> >> >>".into(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 600 800] >>".into(),
            "<< /Type /Page /Parent 2 0 R /Annots [4 0 R] >>".into(),
            "<< /Type /Annot /Subtype /Link /Rect [10 10 50 50] /Dest (chapter-2) >>".into(),
            "<< /Names [(chapter-1) [3 0 R /Fit] (chapter-2) [3 0 R /Fit]] >>".into(),
        ]);
        assert!(run(bytes).is_empty());
    }

    #[test]
    fn annotations_outside_their_page() {
        let bytes = one_page(
            "/Annots [5 0 R 6 0 R 7 0 R]",
            &[
                "<< /Type /Annot /Subtype /Text /Rect [700 900 720 920] >>".into(),
                "<< /Type /Annot /Subtype /Text /Rect [590 790 620 820] >>".into(),
                "<< /Type /Annot /Subtype /Popup /Rect [900 900 950 950] >>".into(),
            ],
        );
        let found = run(bytes);
        assert_eq!(codes(&found), ["pdf.annotation-off-page"]);
        assert_eq!(found[0].target.path.as_deref(), Some("annot:1/0"));
    }

    fn field(name: &str, kind: FormFieldKind, value: &[&str], flags: u32) -> FormField {
        FormField {
            name: name.into(),
            obj: ObjRef { num: 1, generation: 0 },
            kind,
            value: value.iter().map(|v| v.to_string()).collect(),
            default: Vec::new(),
            flags,
            max_len: None,
            options: Vec::new(),
            da: String::new(),
            quadding: 0,
            tooltip: None,
            widgets: Vec::new(),
            actions: Default::default(),
            button: None,
        }
    }

    #[test]
    fn fields_required_but_empty_with_a_bad_choice_or_too_long() {
        let mut combo = field("country", FormFieldKind::Combo, &["Atlantis"], 0);
        combo.options = vec![("FR".into(), "France".into()), ("DE".into(), "Germany".into())];
        let mut editable = field("city", FormFieldKind::Combo, &["Paris"], field_flags::EDIT);
        editable.options = vec![("Berlin".into(), "Berlin".into())];
        let mut code = field("code", FormFieldKind::Text, &["ABCDEFG"], 0);
        code.max_len = Some(5);
        let form = [
            field("name", FormFieldKind::Text, &[""], field_flags::REQUIRED),
            field("email", FormFieldKind::Text, &["a@b.c"], field_flags::REQUIRED),
            combo,
            editable,
            code,
        ];
        let found = check(one_page("", &[]), None, &form);
        check_diagnostics(&found).expect("valid diagnostics");
        assert_eq!(codes(&found), ["pdf.required-field-empty", "pdf.invalid-choice", "pdf.text-too-long"]);
        assert_eq!(found[0].target.path.as_deref(), Some("field:name"));
    }

    #[test]
    fn tagged_figures_without_alternative_text() {
        let bytes = pdf(&[
            "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R /MarkInfo << /Marked true >> >>".into(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 600 800] >>".into(),
            "<< /Type /Page /Parent 2 0 R /Contents 4 0 R /StructParents 0 >>".into(),
            stream("/Figure << /MCID 0 >> BDC 0 0 10 10 re f EMC /Figure << /MCID 1 >> BDC 0 0 10 10 re f EMC"),
            "<< /Type /StructTreeRoot /K [6 0 R 7 0 R] >>".into(),
            "<< /Type /StructElem /S /Figure /P 5 0 R /Pg 3 0 R /K 0 >>".into(),
            "<< /Type /StructElem /S /Figure /P 5 0 R /Pg 3 0 R /K 1 /Alt (A bar chart) >>".into(),
        ]);
        let found = run(bytes);
        assert_eq!(codes(&found), ["pdf.missing-alt-text"]);
        assert_eq!(found[0].target.page, Some(1));
    }

    #[test]
    fn a_code_found_too_often_is_truncated() {
        let annots: Vec<String> = (0..PER_CODE + 3).map(|i| format!("{} 0 R", i + 5)).collect();
        let extra: Vec<String> = (0..PER_CODE + 3).map(|_| "<< /Type /Annot /Subtype /Text /Rect [900 900 910 910] >>".to_string()).collect();
        let found = run(one_page(&format!("/Annots [{}]", annots.join(" ")), &extra));
        assert_eq!(found.iter().filter(|d| d.code == "pdf.annotation-off-page").count(), PER_CODE);
        let last = found.last().expect("truncated");
        assert_eq!(last.code, "pdf.truncated");
        assert!(last.message.starts_with("3 more"));
    }
}
