//! # kition-pdf-kit
//!
//! The PDF engine for Kition's in-page editor, on the Kition SDK. [`PdfDocument`] implements
//! [`KitDocument`] for a PDF opened in a Kition PDF session: its pages, rendered by Kition PDF's own
//! renderer, and its edits, through Kition PDF's automation tools (pages, text, comments, forms,
//! bookmarks, links, redaction, measuring, metadata, and more), the same table Kition PDF's MCP
//! server offers agents. Tools that open or write other files are left out; the document is
//! filled in.
//!
//! Document pixels are points, with the pages stacked top to bottom on one canvas, centred, on a
//! light neutral pasteboard.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod check;
mod collab;
mod describe;
#[cfg(feature = "cache-diagnostics")]
mod diagnostics;
mod files;
pub mod hosting;
mod page_cache;
pub mod premises;
mod rendering;

use std::cell::{Cell, RefCell};
use std::sync::{Arc, LazyLock};

use kition_kit::{KitCommand, KitDocument, MAX_RENDER_SIDE};
use kition_pdf_automation::{Automation, Content, ToolDef};
use kition_pdf_engine::{DocId, Document};
use kition_pdf_render::{PageRenderer, RenderConfig, RenderRequest, RenderedPage, RequestKind, Tile};
use serde_json::{Map, Value, json};

use page_cache::{PageCache, PageImage};

/// Canvas around the pages, in points.
pub const MARGIN: f64 = 40.0;
/// Space between two pages, in points.
pub const GAP: f64 = 24.0;
const PASTEBOARD: [u8; 4] = [238, 238, 242, 255];
/// The editor state `inspect()` reports: none (`editor_state` is `null`). kition-headless takes it
/// as its volatile keys, and `kit.json` declares the same list as `engine.editorState`.
pub const EDITOR_STATE: &[&str] = &[];

/// Automation tools a document in the page cannot use: they open, list, close, save, combine, or
/// split files, which the host does; reach the system's printers, keychains, or trust store; or run any registry command,
/// which would be a way around the rest of this list.
const LEFT_OUT: &[&str] = &[
    "doc_open",
    "doc_list",
    "doc_close",
    "doc_save",
    "doc_combine",
    "doc_create_multiple",
    "doc_split",
    "page_insert_file",
    "page_extract",
    "doc_open_revision",
    "doc_print",
    "printers",
    "printer_options",
    "sign_trust",
    "sign_keychain_ids",
    "sign_windows_ids",
    "command_list",
    "command_run",
    "command_batch",
];

/// The automation tools a document in the page uses, built once.
static TOOLS: LazyLock<Vec<ToolDef>> = LazyLock::new(|| kition_pdf_automation::tools().into_iter().filter(usable).collect());

/// The menu a tool goes in, by its name's first word.
const MENUS: &[(&str, &str)] = &[
    ("edit", "Edit"),
    ("doc", "Document"),
    ("page", "Pages"),
    ("text", "Text"),
    ("content", "Edit PDF"),
    ("image", "Edit PDF"),
    ("comment", "Comment"),
    ("form", "Forms"),
    ("bookmark", "Bookmarks"),
    ("link", "Links"),
    ("redact", "Redact"),
    ("measure", "Measure"),
    ("sign", "Protect"),
    ("signature", "Protect"),
    ("security", "Protect"),
    ("a11y", "Accessibility"),
    ("accessibility", "Accessibility"),
    ("object", "Edit PDF"),
    ("links", "Links"),
    ("comments", "Comment"),
    ("fill", "Forms"),
    ("js", "Tools"),
    ("pdfa", "Tools"),
    ("action", "Tools"),
    ("ocr", "Tools"),
];

/// A command id for a tool: the frame's own `edit.undo` and `edit.redo`, the tool name otherwise.
fn command_id(tool: &str) -> &str {
    match tool {
        "edit_undo" => "edit.undo",
        "edit_redo" => "edit.redo",
        other => other,
    }
}

fn tool_name(command: &str) -> &str {
    match command {
        "edit.undo" => "edit_undo",
        "edit.redo" => "edit_redo",
        other => other,
    }
}

/// Marks what a command's parameters address ([`premises`]): a comment's `id` is its target, a
/// `Comment`'s tool id; a page number (`page`, `pages`, `to_page`, a move's `to`, an insertion's
/// `at`) and a comment's or an item's `index` are places, which [`KitDocument::under`] answers
/// with the objects there now.
fn mark_addresses(command: &mut KitCommand) {
    const COMMENT: &[&str] = &["page", "index"];
    const PAGE: &[&str] = &["page"];
    const PAGES: &[&str] = &["pages"];
    const PAGE_AND_PAGES: &[&str] = &["page", "pages"];
    const PAGE_INDEX: &[&str] = &["page", "index"];
    const LINK: &[&str] = &["page", "index", "to_page"];
    const LINK_ADD: &[&str] = &["page", "to_page"];
    const MOVE: &[&str] = &["pages", "to"];
    const INSERT: &[&str] = &["at"];
    let id = command.id.as_str();
    if !command.mutates {
        return;
    }
    if id.starts_with("comment_") && id != "comment_add" {
        command.targets = Some("id");
        command.object_types = &[("id", &["Comment"])];
        command.places = COMMENT;
        return;
    }
    let has = |key: &str| command.schema.as_ref().and_then(|schema| schema.get(key)).is_some();
    command.places = match id {
        "page_move" => MOVE,
        "page_insert_blank" => INSERT,
        "link_edit" | "link_delete" => LINK,
        "link_add" => LINK_ADD,
        "content_update" | "content_delete" => PAGE_INDEX,
        _ if has("page") && has("pages") => PAGE_AND_PAGES,
        _ if has("page") => PAGE,
        _ if has("pages") => PAGES,
        _ => &[],
    };
}

/// Parameters that name files on the system. A tool that needs one is left out; elsewhere they are
/// dropped from the schema and refused.
const FILE_PARAMS: &[&str] = &["path", "paths", "out", "output", "file", "files", "folder", "dir", "certificate", "key"];

/// Parameters of single tools that name files although their names do not say so (elsewhere the
/// same name is no file): dropped from the tool's schema and refused like [`FILE_PARAMS`] (KHP
/// A28). `content_update`'s `image` is the file of a replacement picture; in `image_edit` and
/// `image_export` an `image` is a number.
const FILE_ARGS: &[(&str, &str)] = &[("content_update", "image")];

/// Whether `key` of `tool`'s arguments names a file ([`FILE_PARAMS`], [`FILE_ARGS`]).
fn names_a_file(tool: &str, key: &str) -> bool {
    FILE_PARAMS.contains(&key) || FILE_ARGS.contains(&(tool, key))
}

/// A tool's parameter schema as the kit offers it: [`parameter_schema`] without the tool's
/// [`FILE_ARGS`].
fn tool_schema(tool: &ToolDef) -> Value {
    let mut schema = parameter_schema(&tool.input_schema);
    if let Some(properties) = schema.as_object_mut() {
        properties.retain(|key, _| !FILE_ARGS.contains(&(tool.name, key.as_str())));
    }
    schema
}

/// Whether a tool works on the document alone: no other documents, and no file it must name.
fn usable(tool: &ToolDef) -> bool {
    if LEFT_OUT.contains(&tool.name) {
        return false;
    }
    // A tool with an `out` writes its result to a file or opens it as another document.
    if tool.input_schema.get("properties").and_then(|properties| properties.get("out")).is_some() {
        return false;
    }
    let required = tool.input_schema.get("required").and_then(Value::as_array);
    !required.is_some_and(|list| list.iter().filter_map(Value::as_str).any(|key| FILE_PARAMS.contains(&key)))
}

/// A tool's JSON Schema (draft 2020-12 subset) as the standard's parameter schema, without the
/// `doc` argument, which the document fills in.
fn parameter_schema(input: &Value) -> Value {
    let required: Vec<&str> =
        input.get("required").and_then(Value::as_array).map(|list| list.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
    let mut out = Map::new();
    if let Some(properties) = input.get("properties").and_then(Value::as_object) {
        for (key, node) in properties {
            if key == "doc" || FILE_PARAMS.contains(&key.as_str()) {
                continue;
            }
            let mut converted = value_node(node);
            if required.contains(&key.as_str())
                && let Some(object) = converted.as_object_mut()
            {
                object.insert("required".into(), Value::Bool(true));
            }
            out.insert(key.clone(), converted);
        }
    }
    Value::Object(out)
}

/// One JSON Schema node in the standard's value schema form: its type (or `json` when it has
/// none the standard knows), description, enum, items, and properties; other keywords (bounds,
/// formats) go into the description.
fn value_node(node: &Value) -> Value {
    let Some(object) = node.as_object() else { return json!({ "type": "json" }) };
    let branches = object.get("oneOf").or_else(|| object.get("anyOf")).and_then(Value::as_array);
    if let Some(branches) = branches.filter(|list| list.len() >= 2) {
        let mut out = json!({ "oneOf": branches.iter().map(value_node).collect::<Vec<_>>() });
        if let Some(description) = object.get("description") {
            out["description"] = description.clone();
        }
        return out;
    }
    let kind = match object.get("type") {
        Some(Value::String(kind)) if matches!(kind.as_str(), "string" | "number" | "integer" | "boolean" | "null" | "array" | "object") => {
            kind.clone()
        }
        _ => "json".into(),
    };
    let mut out = Map::new();
    out.insert("type".into(), Value::String(kind.clone()));
    let mut notes: Vec<String> = Vec::new();
    for (key, value) in object {
        match key.as_str() {
            "description" | "title" | "default" | "examples" | "const" => {
                out.insert(key.clone(), value.clone());
            }
            "enum" if value.as_array().is_some_and(|list| !list.is_empty() && list.iter().all(|choice| enum_fits(&kind, choice))) => {
                out.insert(key.clone(), value.clone());
            }
            "items" if kind == "array" => {
                out.insert(key.clone(), value_node(value));
            }
            "properties" if kind == "object" => {
                let inner = parameter_schema(node);
                out.insert(key.clone(), inner);
            }
            "minimum" | "maximum" | "minItems" | "maxItems" | "maxLength" | "minLength" | "pattern" | "format" => {
                notes.push(format!("{key} {value}"))
            }
            _ => {}
        }
    }
    if kind == "object" {
        let additional = object.get("additionalProperties").and_then(Value::as_bool).unwrap_or(true);
        out.insert("additionalProperties".into(), Value::Bool(additional));
    }
    if !notes.is_empty() {
        let description = out.get("description").and_then(Value::as_str).map_or_else(String::new, |text| format!("{text} "));
        out.insert("description".into(), Value::String(format!("{description}({})", notes.join(", "))));
    }
    Value::Object(out)
}

fn enum_fits(kind: &str, choice: &Value) -> bool {
    match kind {
        "string" => choice.is_string(),
        "number" => choice.is_number(),
        "integer" => choice.is_i64() || choice.is_u64(),
        "boolean" => choice.is_boolean(),
        _ => false,
    }
}

/// Where a page lies on the canvas, in points.
#[derive(Clone, Copy, Debug)]
struct PageArea {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

/// A renderer for one version of the displayed file.
struct Renderer {
    display: Arc<Vec<u8>>,
    pages: PageRenderer,
    /// 1:1 page images (premultiplied RGBA, width, height).
    full: PageCache,
}

/// One PDF open in a Kition PDF session.
pub struct PdfDocument {
    auto: Automation,
    id: DocId,
    renderer: RefCell<Option<Renderer>>,
    /// The bytes the last save produced, which [`KitDocument::mark_saved`] rebases on.
    saved: RefCell<Option<Arc<Vec<u8>>>>,
    /// Only the prepared revision may become clean when the host acknowledges its write.
    saved_revision: Cell<Option<u64>>,
    /// The canvas around the pages (an editor's canvas color).
    pasteboard: [u8; 4],
    /// The object model's scratch replica (`modelDescribe`, `modelBatch`), caught up with the
    /// engine's on every call.
    exchange: kition_kit::session::model::Exchange,
}

impl PdfDocument {
    fn open_bytes(name: &str, bytes: Vec<u8>) -> Result<Self, String> {
        let mut auto = Automation::new();
        let id = auto.session_mut().open(name.to_owned(), None, Arc::new(bytes), None).map_err(|error| error.to_string())?;
        Ok(Self {
            auto,
            id,
            renderer: RefCell::new(None),
            saved: RefCell::new(None),
            saved_revision: Cell::new(None),
            pasteboard: PASTEBOARD,
            exchange: Default::default(),
        })
    }

    /// Takes the time from `clock` (seconds since the Unix epoch) instead of the system clock:
    /// the session's fixed-clock seam ([`kition_pdf_engine::Session::with_clock`]), for tests.
    pub fn with_clock(mut self, clock: fn() -> i64) -> Self {
        self.auto = std::mem::take(&mut self.auto).with_clock(clock);
        self
    }

    fn doc(&self) -> Option<&Document> {
        self.auto.session().get(self.id)
    }

    fn areas(&self) -> Vec<PageArea> {
        let Some(doc) = self.doc() else { return Vec::new() };
        let widest = doc.info.pages.iter().map(|page| f64::from(page.width)).fold(0.0, f64::max);
        let mut y = MARGIN;
        doc.info
            .pages
            .iter()
            .map(|page| {
                let (w, h) = (f64::from(page.width), f64::from(page.height));
                let area = PageArea { x: MARGIN + (widest - w) / 2.0, y, w, h };
                y += h + GAP;
                area
            })
            .collect()
    }

    /// The renderer for the file as it is displayed now.
    fn with_renderer<T>(&self, f: impl FnOnce(&mut Renderer) -> T) -> Option<T> {
        let doc = self.doc()?;
        let mut slot = self.renderer.borrow_mut();
        if slot.as_ref().is_none_or(|renderer| !Arc::ptr_eq(&renderer.display, &doc.display)) {
            #[cfg(feature = "cache-diagnostics")]
            let invalidated = slot.is_some();
            // The SDK places full-page pixels at exact 1:1 document coordinates. A silently
            // reduced raster would shift and truncate content; reject it and retain tiled access.
            let config = RenderConfig { password: doc.password.as_deref().map(Arc::from), reject_oversize: true, ..Default::default() };
            *slot =
                Some(Renderer { display: doc.display.clone(), pages: PageRenderer::new(doc.display.clone(), config), full: PageCache::default() });
            #[cfg(feature = "cache-diagnostics")]
            diagnostics::renderer_created(doc.edit_generation(), invalidated);
        }
        slot.as_mut().map(f)
    }

    fn tools(&self) -> impl Iterator<Item = &'static ToolDef> {
        TOOLS.iter()
    }

    fn enabled(&self, tool: &ToolDef) -> bool {
        match tool.command.and_then(kition_pdf_engine::commands::command) {
            Some(spec) => kition_pdf_engine::commands::is_enabled(spec, self.auto.session(), Some(self.id)),
            None => true,
        }
    }
}

/// A partial or failed page must not become a successful pasteboard-only frame or cache entry.
fn check_rendered_page(page: &RenderedPage) -> Result<(), String> {
    let reason = page.error.as_deref().or_else(|| (!page.warnings.is_empty()).then_some("page content exceeded the rendering budget"));
    if let Some(reason) = reason {
        return Err(format!(
            "Could not render PDF page {} at scale {} (tile {:?}): {reason}. Retry the page or reopen the document.",
            page.request.page.saturating_add(1),
            page.request.scale,
            page.request.tile
        ));
    }
    Ok(())
}

/// Copies a premultiplied `w` × `h` image over `out` (`width` wide) at (`x`, `y`).
fn blit(out: &mut [u8], width: u32, src: &[u8], w: u32, h: u32, x: u32, y: u32) {
    let row_len = w as usize * 4;
    for row in 0..h as usize {
        let to = ((y as usize + row) * width as usize + x as usize) * 4;
        if let (Some(source), Some(target)) = (src.get(row * row_len..(row + 1) * row_len), out.get_mut(to..to + row_len)) {
            over(target, source);
        }
    }
}

/// `src` over `out`, both premultiplied.
fn over(out: &mut [u8], src: &[u8]) {
    for (dst, src) in out.as_chunks_mut::<4>().0.iter_mut().zip(src.as_chunks::<4>().0) {
        let keep = 255 - u32::from(src[3]);
        for channel in 0..4 {
            dst[channel] = (u32::from(src[channel]) + (u32::from(dst[channel]) * keep + 127) / 255).min(255) as u8;
        }
    }
}

/// Premultiplied to straight alpha, in place.
fn straight(pixels: &mut [u8]) {
    for px in pixels.as_chunks_mut::<4>().0 {
        let a = u32::from(px[3]);
        if a != 0 && a != 255 {
            for channel in px.iter_mut().take(3) {
                *channel = ((u32::from(*channel) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}

impl KitDocument for PdfDocument {
    fn open(name: &str, bytes: &[u8]) -> Result<Self, String> {
        Self::open_bytes(name, bytes.to_vec())
    }

    /// A host serving this document to an agent owns every file (KHP A29): turn on the engine's
    /// host-only mode ([`hosting`]).
    fn hosted(&mut self, hosting: &kition_kit::host::Hosting) {
        if hosting.files_only {
            hosting::set_files_only();
        }
    }

    /// The root's `~state` ([`premises`]).
    fn derived(&self, ids: &[Value], names: &[String]) -> Result<Vec<kition_kit::DerivedValues>, String> {
        self.derived_facts(ids, names)
    }

    /// What a write rests on: the objects it names and the fields of each it reads or replaces,
    /// and the facts no object carries ([`premises::under`]).
    fn under(&self, command: &str, params: &Value) -> Result<Option<kition_kit::Under>, String> {
        self.under_call(command, params)
    }

    /// The addressed ids named by object number that another program's rewrite left unproven
    /// ([`premises`]).
    fn unproven_identities(&self, objects: &[Value]) -> Result<Vec<Value>, String> {
        self.unproven(objects)
    }

    /// Reads the file again without dropping this clean session's undo histories: the update
    /// another program appended is written onto the replica as one import, outside both
    /// histories, and identities and blobs stay (`Session::reconcile_file`). `false`, changing
    /// nothing, for a document never edited (the host reads the file afresh), a file written
    /// anew or protected, or a history the replica does not hold.
    fn reconcile_reread(&mut self, fresh: &Self) -> Result<bool, String> {
        let file = fresh.doc().ok_or("the reread document is not open")?.bytes.clone();
        if !self.auto.session_mut().reconcile_file(self.id, file).map_err(|error| error.to_string())? {
            return Ok(false);
        }
        self.saved.borrow_mut().take();
        self.saved_revision.set(None);
        Ok(true)
    }

    /// Whether a fact a write rests on changed since the agent's basis ([`premises`]).
    fn content_at(&self, ask: &kition_kit::ContentAsk, _history: &dyn kition_kit::HistoricalBlobs) -> Result<Option<bool>, String> {
        self.content_changed(ask)
    }

    /// What is wrong with the PDF (KHP A21): see [`check`].
    fn check(&self) -> Vec<kition_kit::diagnostics::Diagnostic> {
        self.doc().map(|doc| check::check(doc.bytes.clone(), doc.password.as_deref(), &doc.form)).unwrap_or_default()
    }

    /// The words of every page in reading order, one part per page (`Page N`), for hosts that
    /// index them (`KitText 1.0`, `document.text`). The engine's text extraction of the working
    /// file, as `text_extract` reads it: no OCR, so a scanned page without a text layer has no
    /// words, and display-only comment appearances are not text. A page the engine cannot read
    /// is left out rather than failing the whole document.
    fn text(&self, limit: usize) -> Result<kition_kit::text::DocumentText, String> {
        let doc = self.doc().ok_or("the document is not open")?;
        let config = RenderConfig { password: doc.password.as_deref().map(Arc::from), ..Default::default() };
        let mut pages = PageRenderer::new(doc.bytes.clone(), config);
        let mut text = kition_kit::text::TextBuilder::new(limit);
        for page in 0..doc.info.pages.len() {
            let rendered = pages.render(RenderRequest { page, kind: RequestKind::Text, scale: 1.0, ..Default::default() });
            let Some(words) = rendered.text.filter(|_| rendered.error.is_none()) else { continue };
            if !text.part(format!("Page {}", page + 1), words.plain_text().trim()) {
                break;
            }
        }
        Ok(text.finish())
    }

    /// A new PDF with one blank page of `width` × `height` points (US Letter when not given).
    fn create(name: &str, width: u32, height: u32) -> Result<Self, String> {
        let (width, height) = if width == 0 || height == 0 { (612.0, 792.0) } else { (f64::from(width), f64::from(height)) };
        let bytes = Automation::new().session().create_blank(width, height, 1).map_err(|error| error.to_string())?;
        Self::open_bytes(name, bytes.to_vec())
    }

    fn width(&self) -> u32 {
        let widest = self.doc().map_or(612.0, |doc| doc.info.pages.iter().map(|page| f64::from(page.width)).fold(1.0, f64::max));
        (widest + 2.0 * MARGIN).round() as u32
    }

    fn height(&self) -> u32 {
        self.areas().last().map_or(2.0 * MARGIN + 792.0, |area| area.y + area.h + MARGIN).round() as u32
    }

    fn revision(&self) -> u64 {
        self.doc().map_or(0, Document::edit_generation)
    }

    fn dirty(&self) -> bool {
        self.doc().is_some_and(|doc| doc.dirty)
    }

    /// Kition PDF renders vectors and text at any scale: drawn at the zoom at 1:1 and above;
    /// below, the exact reduction of 1:1.
    fn render(&self, x: f64, y: f64, scale: f64, width: u32, height: u32) -> Result<Vec<u8>, String> {
        if !x.is_finite() || !y.is_finite() {
            return Err("PDF view coordinates must be finite. Retry with a smaller area.".into());
        }
        let (width, height) = (width.clamp(1, MAX_RENDER_SIDE), height.clamp(1, MAX_RENDER_SIDE));
        let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
        if scale < 1.0 {
            let (x0, y0) = (x.floor() as i32, y.floor() as i32);
            let span_x = ((f64::from(width) / scale).ceil() as i32).max(1);
            let span_y = ((f64::from(height) / scale).ceil() as i32).max(1);
            return self.render_area(x0, y0, x0.saturating_add(span_x), y0.saturating_add(span_y), width, height);
        }
        if scale == 1.0 && x.fract() == 0.0 && y.fract() == 0.0 {
            return self.full(i64::from(x as i32), i64::from(y as i32), width, height);
        }
        let mut out = self.compose(x, y, scale, width, height)?;
        straight(&mut out);
        Ok(out)
    }

    fn render_area(&self, x0: i32, y0: i32, x1: i32, y1: i32, width: u32, height: u32) -> Result<Vec<u8>, String> {
        self.reduce_area(x0, y0, x1, y1, width, height)
    }

    /// Runs an automation tool on this document (`doc` is filled in; a caller's `doc` is refused); images in the answer
    /// become their sizes.
    fn execute(&mut self, command: &str, params: Value) -> Result<Value, String> {
        let name = tool_name(command);
        let tool = self.tools().find(|tool| tool.name == name).ok_or_else(|| format!("unknown command `{command}`"))?;
        let mut args = match params {
            Value::Object(map) => map,
            Value::Null => Map::new(),
            _ => return Err(format!("{command}: parameters must be an object")),
        };
        if let Some(key) = args.keys().find(|key| names_a_file(tool.name, key.as_str())) {
            return Err(format!("{command}: `{key}` names a file, which this document cannot use"));
        }
        // `doc` is not a published parameter: refuse it like any unknown key rather than replace it,
        // so a caller that meant another document is told instead of editing this one.
        if args.contains_key("doc") {
            let schema = parameter_schema(&tool.input_schema);
            let known: Vec<&str> = schema.as_object().map(|names| names.keys().map(String::as_str).collect()).unwrap_or_default();
            return Err(format!(
                "{command}: unknown argument \"doc\": a command runs on the document it is called on, not one a parameter names; leave `doc` out (expected: {})",
                known.join(", ")
            ));
        }
        // Tools of the whole session (action_list, ocr_status) take no `doc` and refuse one.
        if tool.input_schema.get("properties").and_then(|properties| properties.get("doc")).is_some() {
            args.insert("doc".into(), json!(self.id.0));
        }
        let contents = self.auto.call(tool.name, &Value::Object(args)).map_err(|error| error.to_string())?;
        let mut out: Vec<Value> = contents
            .into_iter()
            .map(|content| match content {
                Content::Json(value) => value,
                Content::Png { width, height, .. } => json!({ "image": { "width": width, "height": height } }),
            })
            .collect();
        Ok(if out.len() == 1 { out.remove(0) } else { Value::Array(out) })
    }

    /// The document: its pages (with where each lies on the canvas), and whether it can be
    /// edited, undone, and redone.
    fn inspect(&self) -> Result<Value, String> {
        let doc = self.doc().ok_or("the document is not open")?;
        let pages: Vec<Value> = self
            .areas()
            .iter()
            .zip(&doc.info.pages)
            .enumerate()
            .map(|(index, (area, page))| json!({ "index": index, "label": page.label, "area": { "x": area.x, "y": area.y, "width": area.w, "height": area.h } }))
            .collect();
        Ok(json!({
            "name": doc.display_name(),
            "pages": pages,
            "editable": doc.editable(),
            "readOnlyReason": doc.read_only_reason,
            "formFields": doc.form.len(),
            "canUndo": doc.can_undo().is_some(),
            "canRedo": doc.can_redo().is_some(),
        }))
    }

    fn commands(&self) -> Vec<KitCommand> {
        let mut commands: Vec<KitCommand> = self
            .tools()
            .map(|tool| {
                let menu = tool
                    .name
                    .split('_')
                    .next()
                    .and_then(|first| MENUS.iter().find(|(prefix, _)| *prefix == first))
                    .map(|(_, menu)| vec![(*menu).to_owned()]);
                KitCommand {
                    id: command_id(tool.name).to_owned(),
                    title: tool.title.to_owned(),
                    description: tool.description.to_owned(),
                    menu,
                    shortcut: match tool.name {
                        "edit_undo" => Some("Cmd+Z".into()),
                        "edit_redo" => Some("Cmd+Shift+Z".into()),
                        _ => None,
                    },
                    params: Some(tool.description.to_owned()),
                    schema: Some(tool_schema(tool)),
                    // The tools document their results in prose (`description`), not as a schema.
                    output_schema: None,
                    keywords: Vec::new(),
                    mutates: !tool.read_only,
                    enabled: match tool.name {
                        "edit_undo" => self.doc().is_some_and(|doc| doc.can_undo().is_some()),
                        "edit_redo" => self.doc().is_some_and(|doc| doc.can_redo().is_some()),
                        _ => self.enabled(tool),
                    },
                    ..KitCommand::default()
                }
            })
            .collect();
        kition_kit::files::declare(&mut commands, files::FILES);
        describe::describe(&mut commands);
        for command in &mut commands {
            mark_addresses(command);
        }
        commands
    }

    fn disabled_commands(&self) -> Vec<String> {
        let (undo, redo) = self.doc().map_or((false, false), |doc| (doc.can_undo().is_some(), doc.can_redo().is_some()));
        self.tools()
            .filter(|tool| match tool.name {
                "edit_undo" => !undo,
                "edit_redo" => !redo,
                _ => !self.enabled(tool),
            })
            .map(|tool| command_id(tool.name).to_owned())
            .collect()
    }

    /// The working file with its edits appended (an incremental save, as Kition PDF saves).
    /// Callers serialize save/acknowledgement pairs: the shared acknowledgement has no save ID.
    fn save(&self, _name: Option<&str>) -> Result<Vec<u8>, String> {
        let bytes = self.auto.session().save_bytes(self.id).map_err(|error| error.to_string())?;
        *self.saved.borrow_mut() = Some(bytes.clone());
        self.saved_revision.set(Some(self.revision()));
        Ok(bytes.to_vec())
    }

    /// Acknowledge only the last successfully prepared, still-current revision.
    /// With no preparation this is a no-op; a failed rebase retains it for retry.
    fn mark_saved(&mut self) {
        let prepared_revision = self.saved_revision.get();
        let Some(bytes) = self.saved.borrow().as_ref().cloned() else { return };
        // A host write can finish after another local edit, undo, or remote import.
        // Reopening that older preparation would discard the newer working state.
        // Without a matching preparation, no bytes are known to have reached storage.
        if prepared_revision != Some(self.revision()) {
            self.saved.borrow_mut().take();
            self.saved_revision.set(None);
            return;
        }
        if self.auto.session_mut().mark_saved(self.id, bytes, None).is_ok() {
            self.saved.borrow_mut().take();
            self.saved_revision.set(None);
        }
    }
}

kition_kit::export_kit_document!(PdfDocument as "PdfDocument");

/// The tools whose edit rewrites the whole file: applying redactions, sanitizing (removing
/// hidden information), and adding or removing security renumber every object, so the replica
/// records them as a rebase, which drops the overlay entries collaborators write against the old
/// base meanwhile (`kdm-model`'s `validate`). `pdfa_convert` rewrites only an encrypted file (it
/// removes the encryption).
const REWRITES_IN_FULL: &[&str] = &["redact_apply", "doc_remove_hidden", "doc_protect", "doc_unprotect"];

/// The lease scope of the whole document (Kition platform rule M21).
pub const WHOLE_DOCUMENT: &str = "*";

impl PdfDocument {
    /// The lease the editor takes before running `command` (Kition platform rule M21; Kit UI
    /// protocol section 7b): [`WHOLE_DOCUMENT`] for one that rewrites the file in full (see
    /// [`REWRITES_IN_FULL`]), and for an undo or redo that crosses such a rewrite (it moves the
    /// replica onto other bytes too); empty for every other command, whose edits merge.
    pub fn lease_scope(&self, command: &str) -> String {
        let Some(doc) = self.doc() else { return String::new() };
        let rewrites = match tool_name(command) {
            "edit_undo" => doc.history_rewrites(true, self.auto.session().undo_scope()),
            "edit_redo" => doc.history_rewrites(false, self.auto.session().undo_scope()),
            "pdfa_convert" => doc.info.encrypted,
            name => REWRITES_IN_FULL.contains(&name),
        };
        if rewrites { WHOLE_DOCUMENT.to_owned() } else { String::new() }
    }

    /// The canvas around the pages in `#rrggbb` (the editor's canvas color).
    pub fn set_backdrop(&mut self, color: &str) -> Result<(), String> {
        let hex = color.trim().trim_start_matches('#');
        let channel = |at: usize| hex.get(at..at + 2).and_then(|part| u8::from_str_radix(part, 16).ok());
        match (hex.len(), channel(0), channel(2), channel(4)) {
            (6, Some(r), Some(g), Some(b)) => {
                self.pasteboard = [r, g, b, 255];
                Ok(())
            }
            _ => Err(format!("{color} is not a #rrggbb color")),
        }
    }
}

/// PDF's own additions to the exported class, for its embedded editor.
/// Adds a font file (standard E9: a web build has no system fonts, so its editor gives it its
/// faces before any document opens): a document face that added text is drawn from
/// (`page_add_text`), and the face for CID fonts a PDF names but does not embed, for the CJK
/// scripts it covers (common in Chinese PDFs). Returns how many faces or CID collections it
/// serves: 0 for bytes that are no font, or a face added before.
pub fn add_font(bytes: &[u8]) -> usize {
    let collections = kition_pdf_render::add_cjk_font(bytes.to_vec());
    let faces = kition_pdf_fonts::add_document_font(bytes.to_vec());
    collections.max(faces)
}

#[cfg(target_arch = "wasm32")]
pub mod web {
    use kition_kit::wasm_bindgen::prelude::*;
    use serde_json::json;

    use crate::__kition_kit_export::WebDocument;

    fn error(message: String) -> JsError {
        JsError::new(&message)
    }

    /// An origin in the KDM's commit form, the JSON array `[kind, principal, label]`.
    fn origin_of(origin: &str) -> Result<kition_pdf_engine::Origin, JsError> {
        kition_pdf_engine::Origin::decode(origin).ok_or_else(|| error(format!("origin is not [kind, principal, label]: {origin}")))
    }

    /// Adds a font file for CID fonts a PDF does not embed; returns the collections it serves.
    #[wasm_bindgen(js_name = addFont, wasm_bindgen = kition_kit::wasm_bindgen)]
    pub fn add_font(bytes: &[u8]) -> u32 {
        crate::add_font(bytes) as u32
    }

    #[wasm_bindgen(js_class = "PdfDocument", wasm_bindgen = kition_kit::wasm_bindgen)]
    impl WebDocument {
        /// The canvas around the pages in `#rrggbb` (the editor's canvas color).
        #[wasm_bindgen(js_name = setBackdrop)]
        pub fn set_backdrop(&mut self, color: &str) -> Result<(), JsError> {
            self.0.set_backdrop(color).map_err(error)
        }

        /// The lease to take before running `command`: `*` when it rewrites the whole file,
        /// else empty (Kition platform rule M21).
        #[wasm_bindgen(js_name = leaseScope)]
        pub fn lease_scope(&self, command: &str) -> String {
            self.0.lease_scope(command)
        }

        // Kition phase 3g: the collaboration members (engine contract 2). Shapes mirror
        // `kition-sdk`'s `session::web` (`export_kit_session!` does not apply here; see
        // `crate::collab`).

        pub fn version(&mut self) -> Result<Vec<u8>, JsError> {
            self.0.version().map_err(error)
        }

        #[wasm_bindgen(js_name = exportChanges)]
        pub fn export_changes(&mut self, since: &[u8]) -> Result<Vec<u8>, JsError> {
            self.0.export_changes(since).map_err(error)
        }

        #[wasm_bindgen(js_name = importChanges)]
        pub fn import_changes(&mut self, bytes: &[u8]) -> Result<String, JsError> {
            self.0.import_changes(bytes).map(|damage| serde_json::to_string(&damage).unwrap_or_else(|_| "null".to_owned())).map_err(error)
        }

        pub fn snapshot(&mut self) -> Result<Vec<u8>, JsError> {
            self.0.snapshot().map_err(error)
        }

        pub fn undo(&mut self) -> Result<String, JsError> {
            self.0.undo().map(|(done, damage)| json!({ "done": done, "damage": damage }).to_string()).map_err(error)
        }

        pub fn redo(&mut self) -> Result<String, JsError> {
            self.0.redo().map(|(done, damage)| json!({ "done": done, "damage": damage }).to_string()).map_err(error)
        }

        #[wasm_bindgen(js_name = canUndo)]
        pub fn can_undo(&self) -> bool {
            self.0.can_undo()
        }

        #[wasm_bindgen(js_name = canRedo)]
        pub fn can_redo(&self) -> bool {
            self.0.can_redo()
        }

        /// `origin` is the KDM's commit form, the JSON array `[kind, principal, label]`.
        #[wasm_bindgen(js_name = executeAs)]
        pub fn execute_as(&mut self, command: &str, params: &str, origin: &str) -> Result<String, JsError> {
            let params = kition_kit::parse_params(params).map_err(error)?;
            let origin =
                kition_pdf_engine::Origin::decode(origin).ok_or_else(|| error(format!("origin is not [kind, principal, label]: {origin}")))?;
            self.0.execute_as(command, params, &origin).map(|value| value.to_string()).map_err(error)
        }

        /// Every blob the replica references or can still bring back, for a peer's `addBlobs`.
        pub fn blobs(&mut self) -> Result<Vec<u8>, JsError> {
            self.0.blobs().map_err(error)
        }

        /// Adds a peer's blobs (from their `blobs()`): kept until the next `importChanges`.
        #[wasm_bindgen(js_name = addBlobs)]
        pub fn add_blobs(&mut self, bytes: &[u8]) -> Result<(), JsError> {
            self.0.add_blobs(bytes).map_err(error)
        }

        /// The blobs newly needed since `since` (empty bytes: everything the current state
        /// references), encoded like `blobs()`: for live sync, so a change ships only the blobs
        /// it needs.
        #[wasm_bindgen(js_name = blobsSince)]
        pub fn blobs_since(&mut self, since: &[u8]) -> Result<Vec<u8>, JsError> {
            self.0.blobs_since(since).map_err(error)
        }

        #[wasm_bindgen(js_name = kdmResets)]
        pub fn kdm_resets(&self) -> u32 {
            self.0.kdm_resets()
        }

        /// Collaborators' edits dropped because another peer rewrote the file in full meanwhile.
        #[wasm_bindgen(js_name = kdmDropped)]
        pub fn kdm_dropped(&self) -> u32 {
            self.0.kdm_dropped()
        }

        /// A draft's changes taken in as one undo step, recorded with `origin` (`[kind, principal,
        /// label]`): `{"damage", "summary"}`. A refusal fails with the KDM's message.
        #[wasm_bindgen(js_name = mergeChanges)]
        pub fn merge_changes(&mut self, changes: &[u8], origin: &str) -> Result<String, JsError> {
            let origin = origin_of(origin)?;
            self.0.merge_changes(changes, &origin).map(|answer| answer.to_string()).map_err(error)
        }

        /// What merging a draft's changes would change now, as `StateDiff` JSON; changes nothing.
        #[wasm_bindgen(js_name = compareChanges)]
        pub fn compare_changes(&mut self, changes: &[u8]) -> Result<String, JsError> {
            self.0.compare_changes(changes).map(|answer| answer.to_string()).map_err(error)
        }

        /// Writes the state a snapshot holds over the document as one change of `origin`; answers
        /// the damage JSON.
        #[wasm_bindgen(js_name = restoreState)]
        pub fn restore_state(&mut self, snapshot: &[u8], origin: &str) -> Result<String, JsError> {
            let origin = origin_of(origin)?;
            self.0.restore_state(snapshot, &origin).map(|damage| serde_json::to_string(&damage).unwrap_or_else(|_| "null".to_owned())).map_err(error)
        }

        /// An agent's reply (JSON text) with its object ids named as the tool interface names them.
        #[wasm_bindgen(js_name = modelReply)]
        pub fn model_reply(&self, value: &str) -> Result<String, JsError> {
            self.0.model_reply(value).map_err(error)
        }

        /// `model/describe` as JSON text.
        #[wasm_bindgen(js_name = modelDescribe)]
        pub fn model_describe(&mut self) -> Result<String, JsError> {
            self.0.model_describe().map(|answer| answer.to_string()).map_err(error)
        }

        /// `model/batch`: the request and the answer as JSON text; a failure's message is the
        /// error's JSON (`{"code", "index", "message"}`).
        #[wasm_bindgen(js_name = modelBatch)]
        pub fn model_batch(&mut self, request: &str) -> Result<String, JsError> {
            self.0.model_batch(request).map_err(error)
        }

        /// What changed since the replica version `since`: `{"changes": StateDiff}`, or
        /// `{"changes": null, "unavailable"}` when the replica's log does not hold it.
        #[wasm_bindgen(js_name = changesSince)]
        pub fn changes_since(&self, since: &[u8]) -> String {
            self.0.changes_since(since)
        }

        /// Opens a file with the replica a host stored for it (the Kit UI protocol's `replica`
        /// capability); a replica that does not open is ignored and the file opens plainly.
        #[wasm_bindgen(js_name = openWithReplica)]
        pub fn open_with_replica(name: &str, bytes: &[u8], snapshot: &[u8], blobs: &[u8]) -> Result<WebDocument, JsError> {
            crate::PdfDocument::open_with_replica(name, bytes, snapshot, blobs).map(Self).map_err(error)
        }

        /// Whether the document has made its replica yet (it makes one on the first edit).
        #[wasm_bindgen(getter, js_name = hasReplica)]
        pub fn has_replica(&self) -> bool {
            self.0.has_replica()
        }

        /// The replica's own document id, for the Kit UI protocol's `replica/info` message.
        /// `undefined` before the document has made a replica.
        #[wasm_bindgen(getter, js_name = docId)]
        pub fn doc_id(&self) -> Option<String> {
            self.0.doc_id()
        }
    }
}

pub use describe::NO_OUTPUT;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod save_tests;

#[cfg(test)]
mod cache_tests;

#[cfg(test)]
mod rendering_tests;

#[cfg(all(test, feature = "cache-diagnostics"))]
mod diagnostics_tests;
