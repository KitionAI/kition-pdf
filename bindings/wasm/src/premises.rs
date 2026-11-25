//! What each write rests on, for the premise guard (task cards C8-land-14-pdf, C8-pdf-2; kition-sdk
//! `docs/platform/kits/call-context.md`).
//!
//! The kit's object model has typed objects (`kdm_model::projection`): the root `Pdf` holds its
//! `pages` (each holding its `comments`) and its form `fields`, each with a tool id
//! (`page:<id>`, a comment's `/NM`, `field:<name>`). [`KitDocument::under`] answers, per call
//! ([`under`]):
//!
//! - **objects**: what a call names by place (a page number, a comment's page and index) as the
//!   objects there now, with the windows of their lists (`Within`), and a premise on every object
//!   it addresses with the fields it replaces or its result depends on (a text box's rectangle,
//!   a page's geometry, a field's type). The guard then refuses `deleted` (the object is gone),
//!   `relocated` (another object is at the place now) or `changed` (a field held changed), and
//!   applies a write after any other change.
//! - **content** facts that no object carries, as before (C8-land-14-pdf): the root's derived
//!   `~state` (private, `premiseOnly`: never in receipts or `others`) with a `region` listing the
//!   facts ([`region`]): a page's text (a write that finds text), a page's content streams (OCR,
//!   detecting fields), an item of a page's content, an image, a paragraph or line, a link, a
//!   figure, a bookmark level, that a new field name is still free, and the whole document for
//!   writes over all of it. [`KitDocument::content_at`] evaluates them in the state an agent
//!   observed and now ([`facts`]).
//!
//! **Identity across another program's save.** Ids stored in the file (a page's private id, a
//! comment's `/NM`, a field's name) are proven by the file itself. Ids named by object number
//! (a page or comment of another program's file that the kit never rewrote) are proven while
//! the file now shares a saved revision with a state this process observed ([`shares_no_revision`]); a
//! program that wrote the file anew leaves them unproven ([`unproven`]), and the guard refuses a
//! write naming them `unknown`.
use std::collections::VecDeque;
use std::sync::{Arc, LazyLock, Mutex};

use kition_kit::{ContentAsk, ContentPremise, DerivedValues, KitDocument, Premise, Under, Within};
use kition_pdf_engine::projection::{self, Projection};
use serde_json::{Map, Value, json};

use crate::PdfDocument;

mod inputs;

/// The root's derived fact: the document's state.
pub const STATE: &str = "state";
/// The field a content premise names: [`STATE`] as the guard spells a derived fact.
pub const STATE_FIELD: &str = "~state";
/// The `Pdf` object's id (`kdm_model::root_id`).
pub const ROOT: &str = "pdf";
/// How many states this process keeps for [`KitDocument::content_at`].
pub const KEPT: usize = 256;
/// At most this many bytes of kept states.
pub const KEPT_BYTES: usize = 512 << 20;

/// A kept state: its hash and saved bytes.
type Kept = (String, Arc<Vec<u8>>);

/// The states this process has read the fact of, oldest first.
static STATES: LazyLock<Mutex<VecDeque<Kept>>> = LazyLock::new(|| Mutex::new(VecDeque::new()));

/// Keeps `bytes` as the state `hash` (once), dropping the oldest beyond [`KEPT`] / [`KEPT_BYTES`].
fn remember(hash: &str, bytes: &Arc<Vec<u8>>) {
    let Ok(mut states) = STATES.lock() else { return };
    if let Some(at) = states.iter().position(|(kept, _)| kept == hash) {
        // The most recently read last, so eviction drops what was read longest ago.
        if let Some(entry) = states.remove(at) {
            states.push_back(entry);
        }
        return;
    }
    states.push_back((hash.to_owned(), bytes.clone()));
    let mut total: usize = states.iter().map(|(_, bytes)| bytes.len()).sum();
    while states.len() > KEPT || (total > KEPT_BYTES && states.len() > 1) {
        if let Some((_, dropped)) = states.pop_front() {
            total = total.saturating_sub(dropped.len());
        }
    }
}

/// The saved bytes of state `hash`, when this process still has them.
fn recall(hash: &str) -> Option<Arc<Vec<u8>>> {
    STATES.lock().ok()?.iter().find(|(kept, _)| kept == hash).map(|(_, bytes)| bytes.clone())
}

/// Forgets every kept state (tests: a kit process that restarted).
pub fn forget_states() {
    if let Ok(mut states) = STATES.lock() {
        states.clear();
    }
}

impl PdfDocument {
    /// The document as it saves now, and its hash: the value of `~state`.
    pub(crate) fn state(&self) -> Result<(String, Arc<Vec<u8>>), String> {
        let bytes = self.auto.session().save_bytes(self.id).map_err(|error| error.to_string())?;
        Ok((kition_kit::session::sha256_hex(&bytes), bytes))
    }

    /// `KitDocument::derived`: the root's `~state`, kept for [`KitDocument::content_at`].
    pub(crate) fn derived_facts(&self, ids: &[Value], names: &[String]) -> Result<Vec<DerivedValues>, String> {
        let asked = ids.is_empty() || ids.iter().any(|id| id.as_str() == Some(ROOT));
        if !asked || !(names.is_empty() || names.iter().any(|name| name == STATE)) {
            return Ok(Vec::new());
        }
        let (hash, bytes) = self.state()?;
        remember(&hash, &bytes);
        Ok(vec![DerivedValues { id: json!(ROOT), values: Map::from_iter([(STATE.to_owned(), json!(hash))]) }])
    }

    /// The document's typed objects now (empty when it cannot be read for editing).
    pub fn objects(&self) -> Projection {
        self.doc().and_then(kition_pdf_engine::Document::projection).unwrap_or_default()
    }

    /// `KitDocument::under`: the objects a write names and the fields it rests on ([`under`]),
    /// and the facts no object carries ([`region`]) as one content premise.
    pub(crate) fn under_call(&self, command: &str, params: &Value) -> Result<Option<Under>, String> {
        let name = crate::tool_name(command);
        if self.tools().find(|tool| tool.name == name).is_none_or(|tool| tool.read_only) {
            return Ok(None);
        }
        Ok(Some(under(command, params, &self.objects())))
    }

    /// `KitDocument::unproven_identities`: of the addressed ids,
    ///
    /// - those named by object number (`kdm_model::projection::derived`) when the file now shares
    ///   no saved revision with any state this process observed: another program wrote it anew;
    /// - those named by object number whose object another program's incremental update gave to
    ///   another page or annotation ([`reused`]), told against the state this process observed
    ///   last;
    /// - bookmarks whose id is told by order among siblings of one title
    ///   (`BookmarkView::ambiguous`): a reread cannot prove which is which.
    pub(crate) fn unproven(&self, objects: &[Value]) -> Result<Vec<Value>, String> {
        let view = self.objects();
        let ambiguous: Vec<Value> = objects
            .iter()
            .filter(|id| {
                id.as_str()
                    .and_then(|id| id.strip_prefix(projection::BOOKMARK_PREFIX))
                    .is_some_and(|id| view.all_bookmarks().iter().any(|bookmark| bookmark.id == id && bookmark.ambiguous))
            })
            .cloned()
            .collect();
        let derived: Vec<Value> = objects
            .iter()
            .filter(|id| id.as_str().is_some_and(|id| projection::derived(id.strip_prefix(projection::PAGE_PREFIX).unwrap_or(id))))
            .cloned()
            .collect();
        if derived.is_empty() {
            return Ok(ambiguous);
        }
        let (hash, now) = self.state()?;
        let Ok(now) = kition_pdf_cos::Document::open(now) else { return Ok([derived, ambiguous].concat()) };
        let kept: Vec<Arc<Vec<u8>>> =
            STATES.lock().map(|states| states.iter().filter(|(kept, _)| *kept != hash).map(|(_, bytes)| bytes.clone()).collect()).unwrap_or_default();
        // The most recently observed state first.
        let lineage =
            kept.iter().rev().find_map(|bytes| kition_pdf_cos::Document::open(bytes.clone()).ok().filter(|then| !shares_no_revision(then, &now)));
        let Some(then) = lineage else { return Ok([derived, ambiguous].concat()) };
        let mut out = ambiguous;
        for id in derived {
            let Some(name) = id.as_str() else { continue };
            let obj = match name.strip_prefix(projection::PAGE_PREFIX) {
                Some(page) => view.pages.iter().find(|view| view.id == page).map(|view| view.obj),
                None => view
                    .pages
                    .iter()
                    .flat_map(|page| page.comments.iter().map(|c| (&c.id, c.obj)).chain(page.links.iter().map(|l| (&l.id, l.obj))))
                    .find(|(cid, _)| *cid == name)
                    .and_then(|(_, obj)| obj),
            };
            if obj.is_some_and(|obj| reused(&then, &now, obj.num)) {
                out.push(id);
            }
        }
        Ok(out)
    }

    /// `KitDocument::content_at`: whether any fact of the region differs between the state
    /// `ask.before` names and the document now. `None` when that state is not kept here, either
    /// state does not open, or the pages cannot be told apart ([`shares_no_revision`]).
    pub(crate) fn content_changed(&self, ask: &ContentAsk) -> Result<Option<bool>, String> {
        if ask.premise.field != STATE_FIELD || ask.premise.id.as_str() != Some(ROOT) {
            return Ok(None);
        }
        let Some(before) = ask.before.as_str() else { return Ok(None) };
        let (hash, now) = self.state()?;
        if hash == before {
            return Ok(Some(false));
        }
        let Some(then) = recall(before) else { return Ok(None) };
        let (Ok(mut then), Ok(mut now)) = (Reader::open(&then), Reader::open(&now)) else { return Ok(None) };
        let rewritten = shares_no_revision(&then.cos, &now.cos);
        let facts = ask.premise.region.get("facts").and_then(Value::as_array).cloned().unwrap_or_default();
        for fact in &facts {
            // Calculation inputs use names, values and resolved dictionaries, with no page
            // number identity. They remain comparable across an object-renumbering rewrite.
            if rewritten && !matches!(fact.get(0).and_then(Value::as_str), Some("calculations" | "fill-inputs")) {
                return Ok(None);
            }
            // These facts describe content at a positional page address. Its separate page
            // window reports displacement; replacement content is not an in-place edit.
            let page = match fact.get(0).and_then(Value::as_str) {
                Some("entry") => fact.get(2).and_then(|p| p.get("page")).or_else(|| fact.get(4)?.get("page")),
                Some("covered" | "text-overlap" | "tab-mode") => fact.get(1),
                _ => None,
            }
            .and_then(Value::as_u64);
            if page.is_some_and(|page| then.page(page) != now.page(page)) {
                continue;
            }
            let (Ok(a), Ok(b)) = (then.fact(fact), now.fact(fact)) else { return Ok(None) };
            if a != b {
                return Ok(Some(true));
            }
        }
        Ok(Some(false))
    }
}

/// Every fact of `region` evaluated on the document `bytes` save to (tests and checks: the same
/// reader [`KitDocument::content_at`] uses).
pub fn evaluate(bytes: &Arc<Vec<u8>>, region: &Value) -> Result<Vec<Value>, String> {
    let mut reader = Reader::open(bytes)?;
    region.get("facts").and_then(Value::as_array).into_iter().flatten().map(|fact| reader.fact(fact)).collect()
}

/// Whether object numbers cannot prove which page is which between `then` and `now`: the two
/// files share no complete revision. An incremental save (this kit's, a replica's, another
/// program's) appends to the file it opened and never numbers its objects again, so two states
/// that descend from one saved revision (equal bytes up to the end of a revision both have) give
/// each number one lineage. A program that writes the file anew shares no revision with it.
fn shares_no_revision(then: &kition_pdf_cos::Document, now: &kition_pdf_cos::Document) -> bool {
    let (a, b) = (then.bytes(), now.bytes());
    let ends = now.revision_ends();
    !then.revision_ends().iter().any(|end| ends.contains(end) && a.get(..*end).is_some_and(|prefix| b.get(..*end) == Some(prefix)))
}

/// Whether object `num` names another page or annotation in `now` than in `then`, which descend
/// from one saved revision (C8-pdf-3 item 6): an incremental update that frees a number and gives
/// it to a new object must raise its generation (ISO 32000-2 §7.5.4), and one that does not still
/// gives the object other content. The object is the same when its generation is, its kind
/// (`/Type`, `/Subtype`) is, and it is unchanged or keeps a content stream it had (a page), or its
/// `/NM` and `/Subtype` (an annotation). A page without content streams that changed cannot be
/// told: reused.
fn reused(then: &kition_pdf_cos::Document, now: &kition_pdf_cos::Document, num: u32) -> bool {
    use kition_pdf_cos::{ObjRef, Object};
    // An object allocated after the observed revision is a known creation. The place guard
    // reports that it displaced the observed occupant; there is no old identity to disprove.
    if num >= then.next_num() {
        return false;
    }
    if then.generation(num) != now.generation(num) {
        return true;
    }
    let generation = now.generation(num);
    let (Some(a), Some(b)) = (then.get(ObjRef { num, generation }).as_dict().cloned(), now.get(ObjRef { num, generation }).as_dict().cloned()) else {
        return true;
    };
    if a == b {
        return false;
    }
    if a.name(b"Type") != b.name(b"Type") || a.name(b"Subtype") != b.name(b"Subtype") {
        return true;
    }
    if a.name(b"Type") == Some(b"Page") {
        let streams = |doc: &kition_pdf_cos::Document, dict: &kition_pdf_cos::Dict| -> Vec<u32> {
            match dict.get(b"Contents") {
                Some(Object::Ref(r)) => match &*doc.get(*r) {
                    Object::Array(list) => list.iter().filter_map(Object::as_ref).map(|r| r.num).collect(),
                    _ => vec![r.num],
                },
                Some(Object::Array(list)) => list.iter().filter_map(Object::as_ref).map(|r| r.num).collect(),
                _ => Vec::new(),
            }
        };
        let had = streams(then, &a);
        let has = streams(now, &b);
        return had.is_empty() || !had.iter().any(|num| has.contains(num));
    }
    a.get(b"NM") != b.get(b"NM")
}

/// One state opened for reading facts: the kit's document (its list commands) and the COS
/// document (page objects, form fields).
struct Reader {
    doc: PdfDocument,
    cos: kition_pdf_cos::Document,
    /// The pages' object numbers, in order.
    pages: Vec<u32>,
    /// Read-command answers, by command and parameters.
    answers: Vec<(String, Value, Value)>,
}

impl Reader {
    fn open(bytes: &Arc<Vec<u8>>) -> Result<Self, String> {
        let doc = PdfDocument::open_bytes("state.pdf", bytes.to_vec())?;
        let cos = kition_pdf_cos::Document::open(bytes.clone()).map_err(|error| error.to_string())?;
        let pages = kition_pdf_organize::pages(&cos).map_err(|error| error.to_string())?.into_iter().map(|page| page.obj.num).collect();
        Ok(Self { doc, cos, pages, answers: Vec::new() })
    }

    /// The answer of the read command `command` with `params`, asked once.
    fn read(&mut self, command: &str, params: &Value) -> Result<Value, String> {
        if let Some((_, _, answer)) = self.answers.iter().find(|(c, p, _)| c == command && p == params) {
            return Ok(answer.clone());
        }
        let answer = self.doc.execute(command, params.clone())?;
        self.answers.push((command.to_owned(), params.clone(), answer.clone()));
        Ok(answer)
    }

    /// Page `page` (from 1): its object number, `null` past the last page.
    fn page(&self, page: u64) -> Value {
        usize::try_from(page).ok().and_then(|page| page.checked_sub(1)).and_then(|index| self.pages.get(index)).map_or(Value::Null, |num| json!(num))
    }

    /// The dictionary of page `page` (from 1).
    fn page_dict(&self, page: u64) -> Option<kition_pdf_cos::Dict> {
        let num = u32::try_from(self.page(page).as_u64()?).ok()?;
        self.cos.get(kition_pdf_cos::ObjRef { num, generation: self.cos.generation(num) }).as_dict().cloned()
    }

    /// The value of one fact ([`region`] lists them).
    fn fact(&mut self, fact: &Value) -> Result<Value, String> {
        let kind = fact.get(0).and_then(Value::as_str).unwrap_or_default();
        let int = |at: usize| fact.get(at).and_then(Value::as_u64).unwrap_or(0);
        Ok(match kind {
            // Which page an index names.
            "page" => self.page(int(1)),
            // Every page, in order: a write on all pages, or one that counts them.
            "pages" => json!(self.pages),
            // The pages' labels (`/PageLabels`), in order.
            "labels" => {
                let pages = self.read("file.inspect", &json!({}))?;
                json!(pages["pages"].as_array().into_iter().flatten().map(|page| page["label"].clone()).collect::<Vec<_>>())
            }
            // What decides where a point given in page coordinates lands: the page's boxes,
            // rotation and size (as inspect reports them).
            "geometry" => {
                let pages = self.read("file.inspect", &json!({}))?;
                let index = usize::try_from(int(1)).unwrap_or(0).saturating_sub(1);
                let boxes = kition_pdf_organize::page_boxes(&self.cos).ok().and_then(|boxes| boxes.get(index).copied());
                let shown = pages["pages"].get(index).map(|page| json!([page["width"], page["height"], page["rotation"]]));
                json!({ "page": self.page(int(1)), "shown": shown, "boxes": boxes.map(|b| b.iter().map(|r| r.to_vec()).collect::<Vec<_>>()) })
            }
            // The page's text, as `text.extract` reads it (a write that finds text on it).
            "text" => {
                let answer = self.read("text.extract", &json!({ "pages": [int(1)] }))?;
                json!({ "page": self.page(int(1)), "text": answer["pages"].get(0).map(|page| page["text"].clone()) })
            }
            // The page's content: its content streams and resources (OCR, detecting fields).
            "content" => {
                let Some(dict) = self.page_dict(int(1)) else { return Ok(Value::Null) };
                let parts: Vec<_> =
                    [b"Contents".as_slice(), b"Resources"].iter().map(|key| dict.get(key).map(|value| deep(&self.cos, value))).collect();
                json!({ "page": self.page(int(1)), "content": parts })
            }
            "ocr-inputs" => {
                let text = self.read("text.extract", &json!({"pages":[int(1)]}))?;
                let textual = text["pages"].get(0).and_then(|p| p.get("text")).and_then(Value::as_str).is_some_and(|s| !s.trim().is_empty());
                if fact.get(2).and_then(Value::as_bool).unwrap_or(true) && textual {
                    json!({"skipped":true})
                } else {
                    json!([self.fact(&json!(["geometry", int(1)]))?, self.fact(&json!(["content", int(1)]))?])
                }
            }
            // An entry of a list command: `["entry", command, params, list key, match]`, the
            // entry whose members equal `match`'s (or the `n`th when `match` is a number).
            "entry" => {
                let (Some(command), Some(params), Some(list)) =
                    (fact.get(1).and_then(Value::as_str), fact.get(2), fact.get(3).and_then(Value::as_str))
                else {
                    return Err(format!("bad fact {fact}"));
                };
                let answer = self.read(command, params)?;
                let items = answer[list].as_array().cloned().unwrap_or_default();
                let found = match fact.get(4) {
                    Some(Value::Number(n)) => {
                        n.as_u64().and_then(|n| usize::try_from(n).ok()).and_then(|n| n.checked_sub(1)).and_then(|n| items.get(n)).cloned()
                    }
                    Some(Value::Object(wanted)) => items.iter().find(|item| wanted.iter().all(|(key, value)| item.get(key) == Some(value))).cloned(),
                    _ => None,
                };
                let found = found.map(|mut item| {
                    if command == "accessibility.listFigures"
                        && let Some(page) = item["page"].as_u64()
                    {
                        item["page"] = self.page(page);
                    }
                    if command == "page.listImages" {
                        item["$content"] =
                            json!(params["page"].as_u64().zip(item["image"].as_u64()).and_then(|(page, index)| self.image_content(page, index)));
                    }
                    if command == "content.list" {
                        item["$content"] =
                            json!(item["page"].as_u64().zip(item["index"].as_u64()).and_then(|(page, index)| self.added_content(page, index)));
                    }
                    item
                });
                let selected = match (found, fact.get(5).and_then(Value::as_array)) {
                    (Some(item), Some(keys)) => {
                        let id = ["id", "name"].iter().find_map(|key| item.get(*key).cloned());
                        let mut out = Map::new();
                        out.insert("$identity".into(), id.unwrap_or(Value::Null));
                        for key in keys.iter().filter_map(Value::as_str) {
                            out.insert(key.to_owned(), item.get(key).cloned().unwrap_or(Value::Null));
                        }
                        Value::Object(out)
                    }
                    (item, _) => item.unwrap_or(Value::Null),
                };
                // The page the entry is on, by its object number: an index names another entry
                // when the page there is another page.
                match fact.get(2).and_then(|params| params.get("page")).and_then(Value::as_u64) {
                    Some(page) => json!({ "page": self.page(page), "entry": selected }),
                    None => selected,
                }
            }
            // A comment by its `/NM` (`comment.list`'s `id`): its page by object number and
            // the members listed (all of the entry when none are).
            "comment" => {
                let id = fact.get(1).cloned().unwrap_or(Value::Null);
                let answer = self.read("comment.list", &json!({}))?;
                let item = answer["comments"].as_array().into_iter().flatten().find(|item| item["id"] == id).cloned();
                match item {
                    None => Value::Null,
                    Some(item) => {
                        let page = item["page"].as_u64().map_or(Value::Null, |page| self.page(page));
                        let selected = match fact.get(2).and_then(Value::as_array) {
                            Some(keys) => Value::Object(
                                keys.iter()
                                    .filter_map(Value::as_str)
                                    .map(|key| (key.to_owned(), item.get(key).cloned().unwrap_or(Value::Null)))
                                    .collect(),
                            ),
                            None => item,
                        };
                        json!({ "page": page, "members": selected })
                    }
                }
            }
            // A form field by its fully qualified name: whether it exists and is the same field
            // (object number and kind), and the properties listed.
            "field" => {
                let name = fact.get(1).and_then(Value::as_str).unwrap_or_default();
                let fields = kition_pdf_forms::fields(&self.cos);
                match fields.iter().find(|field| field.name == name) {
                    None => Value::Null,
                    Some(field) => {
                        let mut out = Map::new();
                        out.insert("obj".into(), json!(field.obj.num));
                        out.insert("kind".into(), json!(format!("{:?}", field.kind)));
                        for prop in fact.get(2).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
                            let value = match prop {
                                "value" => json!(field.value),
                                "default" => json!(field.default),
                                "flags" => json!(field.flags),
                                "max_len" => json!(field.max_len),
                                "options" => json!(field.options),
                                "da" => json!(field.da),
                                "quadding" => json!(field.quadding),
                                "tooltip" => json!(field.tooltip),
                                "actions" => json!(format!("{:?} {:?}", field.actions, field.button)),
                                // What a value shows: each widget's appearance state.
                                "states" => json!(field.widgets.iter().map(|widget| json!([widget.on_state, widget.state])).collect::<Vec<_>>()),
                                "widgets" => {
                                    let dicts: Vec<String> =
                                        field.widgets.iter().map(|widget| format!("{:?} {:?}", widget, self.cos.get(widget.obj))).collect();
                                    json!(dicts)
                                }
                                "dict" => json!(format!("{:?}", self.cos.get(field.obj))),
                                _ => Value::Null,
                            };
                            out.insert(prop.to_owned(), value);
                        }
                        Value::Object(out)
                    }
                }
            }
            // Bookmarks: the outline entries at a parent path, with their titles and pages.
            "bookmarks" => {
                let answer = self.read("bookmark.list", &json!({ "limit": 1000 }))?;
                if answer["truncated"] == true {
                    return Err("more than 1000 bookmarks".into());
                }
                let mut level = answer["bookmarks"].clone();
                for step in fact.get(1).and_then(Value::as_array).into_iter().flatten() {
                    let index = step.as_u64().and_then(|n| usize::try_from(n).ok()).and_then(|n| n.checked_sub(1)).unwrap_or(usize::MAX);
                    level = level.get(index).map_or(Value::Null, |entry| entry["children"].clone());
                }
                let entries: Vec<Value> = level
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|entry| json!({ "title": entry["title"], "page": entry["page"].as_u64().map_or(Value::Null, |page| self.page(page)) }))
                    .collect();
                json!(entries)
            }
            // What the page's readers show under a rectangle (in page coordinates, or the
            // rectangles of quads): the lines of text and the images it touches, each with its
            // content (C8-pdf-3 item 5).
            "covered" => {
                let rects = rects_of(fact.get(2).unwrap_or(&Value::Null));
                let lines = self.read("text.listLines", &json!({ "page": int(1) }))?;
                let mut images = self.read("page.listImages", &json!({ "page": int(1) }))?;
                for image in images["images"].as_array_mut().into_iter().flatten() {
                    image["$content"] = json!(image["image"].as_u64().and_then(|index| self.image_content(int(1), index)));
                }
                let touched = |item: &Value| rects_of(&item["rect"]).first().is_some_and(|r| rects.iter().any(|q| overlaps(r, q)));
                let pick = |list: &Value, keys: &[&str]| -> Vec<Value> {
                    list.as_array()
                        .into_iter()
                        .flatten()
                        .filter(|item| touched(item))
                        .map(|item| Value::Object(keys.iter().map(|key| ((*key).to_owned(), item[*key].clone())).collect()))
                        .collect()
                };
                json!({
                    "page": self.page(int(1)),
                    "lines": pick(&lines["lines"], &["text", "rect", "font", "size"]),
                    "images": pick(&images["images"], &["rect", "pixels", "name", "kind", "$content"]),
                })
            }
            // The page's viewports (`/VP`): the scales measurements read.
            "viewports" => {
                let Some(dict) = self.page_dict(int(1)) else { return Ok(Value::Null) };
                json!({ "page": self.page(int(1)), "vp": dict.get(b"VP").map(|vp| deep(&self.cos, vp)) })
            }
            // Every form field's name: a new field takes one free of them.
            "names" => {
                let mut names: Vec<String> = kition_pdf_forms::fields(&self.cos).into_iter().map(|field| field.name).collect();
                names.sort();
                json!(names)
            }
            "tab-mode" => json!({
                "page": self.page(int(1)),
                "mode": self.page_dict(int(1)).and_then(|d| d.name(b"Tabs").map(|mode| String::from_utf8_lossy(mode).into_owned()))
            }),
            // The effective order used by move_in_tab_order. Geometry matters only through
            // this order when the current /Tabs mode sorts by rows or columns.
            "tabs" => {
                let all = kition_pdf_forms::fields(&self.cos);
                let page = fact
                    .get(1)
                    .and_then(Value::as_str)
                    .and_then(|name| all.iter().find(|f| f.name == name))
                    .and_then(|f| f.widgets.iter().filter_map(|w| w.page).min());
                let Some(page) = page else { return Ok(Value::Null) };
                let slots: Vec<_> = self
                    .page_dict(page as u64 + 1)
                    .and_then(|d| d.get(b"Annots").map(|o| self.cos.resolve(o)))
                    .as_deref()
                    .and_then(kition_pdf_cos::Object::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(kition_pdf_cos::Object::as_ref)
                    .filter(|r| all.iter().any(|f| f.widgets.iter().any(|w| w.obj == *r)))
                    .map(|r| r.num)
                    .collect();
                let mut order: Vec<_> = all
                    .iter()
                    .filter_map(|f| {
                        let rank = f.widgets.iter().filter(|w| w.page == Some(page)).map(|w| w.tab).min()?;
                        Some((rank, f.name.clone(), f.widgets.iter().filter(|w| w.page == Some(page)).map(|w| w.obj.num).collect::<Vec<_>>()))
                    })
                    .collect();
                order.sort();
                json!({"page":self.page(page as u64 + 1),"slots":slots,"widgets":order.into_iter().map(|(_, name, widgets)| (name,widgets)).collect::<Vec<_>>()})
            }
            "calculations" => self.calculation_inputs(&names(&json!({"fields":fact.get(1)}), "fields")),
            "fill-inputs" => self.fill_inputs(&names(&json!({"fields":fact.get(1)}), "fields")),
            "urls" => json!(self.doc.auto.session().find_urls(self.doc.id)),
            "text-overlap" => self.overlapping_text(int(1), fact.get(2).unwrap_or(&Value::Null)),
            "headings" => self.heading_inputs(),
            "flatten" => {
                self.flatten_inputs(fact.get(1).and_then(Value::as_bool).unwrap_or(true), fact.get(2).and_then(Value::as_bool).unwrap_or(true))?
            }
            "redaction-inputs" => {
                let pages: Vec<_> = fact.get(1).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_u64).collect();
                self.redaction_inputs(&pages)?
            }
            // Every page's header, footer, watermark and background marks (this kit's tagged
            // content streams), by page.
            "marks" => {
                let mut out = Vec::new();
                for (at, &num) in self.pages.iter().enumerate() {
                    if fact.get(1).and_then(Value::as_u64).is_some_and(|page| page != at as u64 + 1) {
                        continue;
                    }
                    let Some(dict) = self.page_dict(at as u64 + 1) else { continue };
                    let contents = dict.get(b"Contents").map(|o| self.cos.resolve(o));
                    let refs: Vec<kition_pdf_cos::ObjRef> = match contents.as_deref() {
                        Some(kition_pdf_cos::Object::Array(list)) => list.iter().filter_map(kition_pdf_cos::Object::as_ref).collect(),
                        _ => dict.get(b"Contents").and_then(kition_pdf_cos::Object::as_ref).into_iter().collect(),
                    };
                    let tags: Vec<String> = refs
                        .iter()
                        .filter_map(|r| match &*self.cos.get(*r) {
                            kition_pdf_cos::Object::Stream(stream) => stream.dict.name(b"PCMark").map(|t| String::from_utf8_lossy(t).into_owned()),
                            _ => None,
                        })
                        .filter(|tag| matches!(tag.as_str(), "HeaderFooter" | "Watermark" | "Background"))
                        .collect();
                    if !tags.is_empty() {
                        out.push(json!([num, tags]));
                    }
                }
                json!(out)
            }
            // Every redaction mark: its page, object and rectangles.
            "redactions" => {
                let mut out = Vec::new();
                for (at, &num) in self.pages.iter().enumerate() {
                    let Some(dict) = self.page_dict(at as u64 + 1) else { continue };
                    let annots = dict.get(b"Annots").map(|o| self.cos.resolve(o));
                    for entry in annots.as_deref().and_then(kition_pdf_cos::Object::as_array).into_iter().flatten() {
                        let Some(annot) = self.cos.dict(entry) else { continue };
                        if annot.name(b"Subtype") == Some(b"Redact") {
                            out.push(json!([num, deep(&self.cos, entry)]));
                        }
                    }
                }
                json!(out)
            }
            // The document's scripts (`/Names /JavaScript`).
            "scripts" => {
                let root = self.cos.root().map(|root| self.cos.get(root));
                let names = root.as_deref().and_then(kition_pdf_cos::Object::as_dict).and_then(|catalog| catalog.get(b"Names").cloned());
                let tree = names.and_then(|names| self.cos.dict(&names)).and_then(|names| names.get(b"JavaScript").cloned());
                json!(tree.map(|tree| deep(&self.cos, &tree)))
            }
            // The document's information dictionary (`/Info`).
            "info" => json!(self.cos.trailer().get(b"Info").map(|info| deep(&self.cos, info))),
            // How the document opens: its page mode and layout, viewer preferences, language and
            // the page and magnification it opens at (the page by its object number).
            "view" => {
                let view = kition_pdf_organize::initial_view(&self.cos);
                let root = self.cos.root().map(|root| self.cos.get(root));
                let open = root.as_deref().and_then(kition_pdf_cos::Object::as_dict).and_then(|catalog| catalog.get(b"OpenAction").cloned());
                json!({ "view": format!("{view:?}"), "open": open.map(|open| deep(&self.cos, &open)) })
            }
            // The content of the page a figure's marked content is on (a decorative figure turns
            // it into an artifact).
            "figure_content" => {
                let figures = self.read("accessibility.listFigures", &json!({}))?;
                let page =
                    figures["figures"].as_array().into_iter().flatten().find(|f| f["figure"] == json!(int(1))).and_then(|f| f["page"].as_u64());
                match page {
                    Some(page) => self.fact(&json!(["content", page]))?,
                    None => Value::Null,
                }
            }
            // Everything: the document's state.
            "all" => json!(kition_kit::session::sha256_hex(self.cos.bytes())),
            _ => return Err(format!("unknown fact {fact}")),
        })
    }
}

/// An object with every reference in it followed (at most 8 deep, each object once), as text.
fn deep(cos: &kition_pdf_cos::Document, object: &kition_pdf_cos::Object) -> String {
    use kition_pdf_cos::Object;
    fn walk(cos: &kition_pdf_cos::Document, object: &Object, depth: u8, seen: &mut std::collections::HashSet<u32>, out: &mut String) {
        match object {
            Object::Ref(r) if depth < 8 && seen.insert(r.num) => walk(cos, &cos.get(*r), depth + 1, seen, out),
            Object::Array(list) => {
                out.push('[');
                for item in list {
                    walk(cos, item, depth, seen, out);
                    out.push(' ');
                }
                out.push(']');
            }
            Object::Dict(dict) => walk_dict(cos, dict, depth, seen, out),
            Object::Stream(stream) => {
                walk_dict(cos, &stream.dict, depth, seen, out);
                out.push_str(&kition_kit::session::sha256_hex(&stream.raw));
            }
            other => out.push_str(&format!("{other:?}")),
        }
    }
    fn walk_dict(
        cos: &kition_pdf_cos::Document,
        dict: &kition_pdf_cos::Dict,
        depth: u8,
        seen: &mut std::collections::HashSet<u32>,
        out: &mut String,
    ) {
        out.push_str("<<");
        for (key, value) in dict.iter() {
            // A parent or page link goes back up: it says where, not what.
            if matches!(key.as_slice(), b"Parent" | b"P") {
                continue;
            }
            out.push_str(&String::from_utf8_lossy(key));
            out.push(' ');
            walk(cos, value, depth, seen, out);
            out.push(' ');
        }
        out.push_str(">>");
    }
    let mut out = String::new();
    walk(cos, object, 0, &mut std::collections::HashSet::new(), &mut out);
    out
}

/// Rectangles in a value: one `[x0, y0, x1, y1]`, or a list of them or of quads (8 numbers each).
fn rects_of(value: &Value) -> Vec<[f64; 4]> {
    let numbers = |list: &[Value]| list.iter().filter_map(Value::as_f64).collect::<Vec<f64>>();
    let one = |n: &[f64]| -> Option<[f64; 4]> {
        match n {
            [a, b, c, d] => Some([a.min(*c), b.min(*d), a.max(*c), b.max(*d)]),
            [x0, y0, x1, y1, x2, y2, x3, y3] => {
                let xs = [*x0, *x1, *x2, *x3];
                let ys = [*y0, *y1, *y2, *y3];
                Some([
                    xs.iter().copied().fold(f64::INFINITY, f64::min),
                    ys.iter().copied().fold(f64::INFINITY, f64::min),
                    xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                    ys.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                ])
            }
            _ => None,
        }
    };
    match value.as_array() {
        Some(list) if list.iter().all(Value::is_number) => one(&numbers(list)).into_iter().collect(),
        Some(list) => list.iter().filter_map(|item| item.as_array().and_then(|item| one(&numbers(item)))).collect(),
        None => Vec::new(),
    }
}

/// Whether two rectangles share area (or an edge).
fn overlaps(a: &[f64; 4], b: &[f64; 4]) -> bool {
    a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
}

/// `params[key]` as a page number list: one number, or a list of them.
fn numbers(params: &Value, key: &str) -> Vec<u64> {
    match params.get(key) {
        Some(Value::Array(list)) => list.iter().filter_map(Value::as_u64).collect(),
        Some(value) => value.as_u64().into_iter().collect(),
        None => Vec::new(),
    }
}

/// What decides where a point given in page coordinates lands: the page's size, rotation and
/// boxes.
pub const GEOMETRY: &[&str] = &["width", "height", "rotation", "boxes"];

/// What a link's style edit starts from: `link.edit` merges the parameters given over the style it
/// reads (links.rs `link_style`).
const LINK_STYLE: &[&str] = &["visible", "color", "width", "highlight"];
/// A link action replaces its URL, page destination or other action as a unit.
const LINK_ACTION: &[&str] = &["url", "toPage", "other"];

/// Comment types that mark the page's text under them (`comment.add` with `rect` or `quads`).
const MARKUP: &[&str] = &["highlight", "underline", "strikeout", "squiggly", "replace"];

/// The object holds of one call, built by command family ([`under`]).
struct Holds<'a> {
    objects: &'a Projection,
    under: Under,
}

impl Holds<'_> {
    fn premise(&mut self, id: String, fields: &[&str], positions: &[&str]) {
        self.under.premises.push(Premise::on(id).fields(fields.iter().copied()).positions(positions.iter().copied()));
    }

    fn within(&mut self, parent: Value, field: &str, from: usize, to: usize) {
        self.under.within.push(Within { parent, field: field.into(), from, to, time: None });
    }

    /// The page a number (from 1) names: the window of the root's `pages` at it, and the page
    /// there now with the fields the write reads or replaces.
    fn page(&mut self, number: u64, fields: &[&str]) {
        let Some(index) = usize::try_from(number).ok().and_then(|n| n.checked_sub(1)) else { return };
        self.within(json!(ROOT), "pages", index, index.saturating_add(1));
        if let Some(page) = self.objects.page(number) {
            let id = projection::page_tool_id(&page.id);
            self.under.objects.push(json!(id));
            self.premise(id, fields, &["pages"]);
        }
    }

    /// Every page (a write over all of them): the whole list, and each page's `fields`.
    fn all_pages(&mut self, fields: &[&str]) {
        let count = self.objects.pages.len();
        self.within(json!(ROOT), "pages", 0, count);
        for page in &self.objects.pages {
            let id = projection::page_tool_id(&page.id);
            self.under.objects.push(json!(id));
            self.under.premises.push(Premise::on(id).fields(fields.iter().copied()).positions(["pages"]));
        }
    }

    /// The pages `pages` names, or every page when it names none.
    fn pages_or_all(&mut self, pages: &[u64], fields: &[&str]) {
        if pages.is_empty() {
            self.all_pages(fields);
        }
        for &number in pages {
            self.page(number, fields);
        }
    }

    /// The pages either side of where pages go at `at` (from 1): the page before and the page
    /// there now.
    fn gap(&mut self, at: u64) {
        let at = usize::try_from(at).unwrap_or(usize::MAX).max(1);
        let from = at.saturating_sub(2);
        let to = at.min(self.objects.pages.len().max(from));
        self.within(json!(ROOT), "pages", from, to.max(from));
        for index in from..to {
            if let Some(page) = self.objects.pages.get(index) {
                let id = projection::page_tool_id(&page.id);
                self.under.objects.push(json!(id));
                self.premise(id, &[], &["pages"]);
            }
        }
    }

    /// `page.move`: the pages moved, and the pages either side of where they go. The engine puts
    /// them at `to` (from 1) in the list without them (organize `move_pages`), so the neighbours
    /// are the pages left before and at that place, wherever they are now.
    fn move_gap(&mut self, moved: &[u64], to: u64) {
        let moved: Vec<usize> = moved.iter().filter_map(|n| usize::try_from(*n).ok()?.checked_sub(1)).collect();
        let rest: Vec<usize> = (0..self.objects.pages.len()).filter(|index| !moved.contains(index)).collect();
        let at = usize::try_from(to).unwrap_or(usize::MAX).saturating_sub(1).min(rest.len());
        let before = at.checked_sub(1).and_then(|i| rest.get(i)).copied();
        let after = rest.get(at).copied();
        let mut span: Vec<usize> = moved.clone();
        span.extend(before);
        span.extend(after);
        if let (Some(from), Some(to)) = (span.iter().min(), span.iter().max()) {
            self.within(json!(ROOT), "pages", *from, to.saturating_add(1));
        }
        for index in before.into_iter().chain(after) {
            if let Some(page) = self.objects.pages.get(index) {
                let id = projection::page_tool_id(&page.id);
                self.under.objects.push(json!(id));
                self.premise(id, &[], &["pages"]);
            }
        }
    }

    /// The page holding an annotation named by `page` and `index` (its place in `/Annots`): its
    /// window, and the window of its `list` (`comments` or `links`) where an annotation of that
    /// kind at the index is or would be (the ones before it). A window that is empty now still
    /// holds the one that was there (deleted since).
    fn annotation_place(&mut self, params: &Value, list: &str, geometry: bool) -> Option<u64> {
        let (page, index) = (params.get("page").and_then(Value::as_u64).unwrap_or(0), params.get("index").and_then(Value::as_u64).unwrap_or(0));
        self.page(page, if geometry { GEOMETRY } else { &[] });
        let view = self.objects.page(page)?;
        let holder = projection::page_tool_id(&view.id);
        let at = match list {
            "links" => view.links.iter().filter(|link| (link.index as u64) < index).count(),
            _ => view.comments.iter().filter(|comment| (comment.index as u64) < index).count(),
        };
        self.within(json!(holder), list, at, at + 1);
        // The index counts every annotation before it: the comments and links there now must be
        // where they are (one before it gone or come moves the one at the index), the ones after
        // it may come and go.
        let before: Vec<String> = view
            .comments
            .iter()
            .filter(|comment| (comment.index as u64) < index)
            .map(|comment| comment.id.clone())
            .chain(view.links.iter().filter(|link| (link.index as u64) < index).map(|link| link.id.clone()))
            .collect();
        for id in before {
            self.premise(id, &[], &["index"]);
        }
        Some(page)
    }

    /// A comment as a command names it: by `id`, wherever it is; or by `page` and `index` (its
    /// place among the page's annotations), the comment there now in the window of its page's
    /// `comments`. `geometry`: the write converts page coordinates (a move, a new rectangle).
    fn comment(&mut self, params: &Value, fields: &[&str], geometry: bool) {
        if let Some(id) = params.get("id").and_then(Value::as_str) {
            self.premise(id.to_owned(), fields, &[]);
            if geometry && let Some((page, _, _)) = self.objects.comment(id) {
                let page = self.objects.pages.get(page).map(|page| projection::page_tool_id(&page.id));
                if let Some(page) = page {
                    self.premise(page, GEOMETRY, &[]);
                }
            }
            return;
        }
        let Some(page) = self.annotation_place(params, "comments", geometry) else { return };
        let index = params.get("index").and_then(Value::as_u64).unwrap_or(0);
        if let Some((_, comment)) = self.objects.comment_at(page, index) {
            let id = comment.id.clone();
            self.under.objects.push(json!(id));
            self.premise(id, fields, &["index", "comments"]);
        }
    }

    /// A link by `page` and `index` (its place among the page's annotations, as `link.list`
    /// numbers it): the link there now, in the window of its page's `links`.
    fn link(&mut self, params: &Value, fields: &[&str], geometry: bool) {
        let Some(page) = self.annotation_place(params, "links", geometry) else { return };
        let index = params.get("index").and_then(Value::as_u64).unwrap_or(0);
        if let Some((_, link)) = self.objects.link_at(page, index) {
            let id = link.id.clone();
            self.under.objects.push(json!(id));
            self.premise(id, fields, &["index", "links"]);
        }
    }

    /// A bookmark list a path (from 1, as commands take it) names, through every bookmark on the
    /// way: each one's window in its list and a premise on it (it, not another, is there). Answers
    /// the holder of the list at the end (`pdf` for the top level) and its field.
    fn bookmark_path(&mut self, path: &[u64]) -> Option<(Value, &'static str, usize)> {
        let mut holder = json!(ROOT);
        let mut field = "bookmarks";
        let mut level = &self.objects.bookmarks;
        for step in path {
            let at = usize::try_from(*step).ok()?.checked_sub(1)?;
            self.within(holder.clone(), field, at, at + 1);
            let item = level.get(at)?;
            let id = projection::bookmark_tool_id(&item.id);
            self.under.objects.push(json!(id));
            self.premise(id.clone(), &[], &[field]);
            holder = json!(id);
            field = "children";
            level = &item.children;
        }
        Some((holder, field, level.len()))
    }

    /// The siblings either side of where a bookmark goes at `position` (from 1; past the end or
    /// none: the end) of the list `bookmark_path` answered.
    fn bookmark_gap(&mut self, holder: Value, field: &str, count: usize, position: Option<u64>, path: &[u64]) {
        let at = position.and_then(|p| usize::try_from(p).ok()).map_or(count, |p| p.max(1) - 1).min(count);
        let from = at.saturating_sub(1);
        let to = (at + 1).min(count).max(from);
        self.within(holder, field, from, to);
        let mut level = &self.objects.bookmarks;
        for step in path {
            let Some(item) = usize::try_from(*step).ok().and_then(|s| s.checked_sub(1)).and_then(|s| level.get(s)) else { return };
            level = &item.children;
        }
        for index in from..to {
            if let Some(item) = level.get(index) {
                let id = projection::bookmark_tool_id(&item.id);
                self.under.objects.push(json!(id));
                self.premise(id, &[], &[field]);
            }
        }
    }

    /// A form field by its fully qualified name: its tool id (its stored id, else its name) and
    /// `fields` with its `name` (a rename refuses).
    fn field(&mut self, name: &str, fields: &[&str]) {
        let mut fields = fields.to_vec();
        if !fields.is_empty() && !fields.contains(&"name") {
            fields.push("name");
        }
        let id = self.objects.field(name).map_or_else(|| projection::field_tool_id(name), |view| projection::field_tool_id(&view.id));
        self.premise(id, &fields, &[]);
    }

    /// Every form field: the root's whole `fields` list and each field's `fields`.
    fn all_fields(&mut self, fields: &[&str]) {
        self.within(json!(ROOT), "fields", 0, self.objects.fields.len());
        let names: Vec<String> = self.objects.fields.iter().map(|field| field.name.clone()).collect();
        for name in names {
            self.field(&name, fields);
        }
    }

    /// The page of a form field's first widget, as the projection names it.
    fn widget_page(&self, name: &str) -> Option<String> {
        self.objects.field(name)?.fields.get("widgets")?.get(0)?["page"].as_str().map(str::to_owned)
    }
}

/// The fields of a comment `comment.edit` depends on, by its parameters (its handler:
/// `kition-pdf-automation` `comments.rs` `comment.edit`; annot `set_contents`, `set_style`,
/// `set_fill`, `set_rect`, `move_annotation`). New text in a text box refits the box to it: it reads
/// the box's rectangle and border.
fn comment_edit_fields(params: &Value, free_text: bool) -> Vec<&'static str> {
    let mut fields = Vec::new();
    for (param, field) in [
        ("contents", "contents"),
        ("contents", "type"),
        ("color", "color"),
        ("opacity", "opacity"),
        ("width", "width"),
        ("fill", "fill"),
        ("move", "rect"),
        ("rect", "rect"),
    ] {
        if params.get(param).is_some() && !fields.contains(&field) {
            fields.push(field);
        }
    }
    if free_text && params.get("contents").is_some() {
        for field in ["rect", "width"] {
            if !fields.contains(&field) {
                fields.push(field);
            }
        }
    }
    fields
}

/// The fields of a form field `form.setProps` reads or replaces, by its parameters (its handler:
/// `kition-pdf-automation` `forms.rs` `form.setProps`).
fn field_props(params: &Value) -> Vec<&'static str> {
    let mut fields = Vec::new();
    for (param, field) in [
        ("align", "align"),
        ("appearance", "appearance"),
        ("appearance", "widgets"),
        ("calculate", "actions"),
        ("format", "actions"),
        ("validate", "actions"),
        ("check_style", "widgets"),
        ("default", "default"),
        ("flags", "flags"),
        ("read_only", "flags"),
        ("required", "flags"),
        ("multiline", "flags"),
        ("locked", "widgets"),
        ("font_size", "appearance"),
        ("max_length", "maxLength"),
        ("options", "options"),
        ("rect", "widgets"),
        ("rotation", "widgets"),
        ("tooltip", "tooltip"),
        ("name", "name"),
    ] {
        if params.get(param).is_some() && !fields.contains(&field) {
            fields.push(field);
        }
    }
    fields
}

/// The fields of a link `link.edit` reads or replaces (links.rs `link.edit`, annot `links::set`).
fn link_edit_fields(params: &Value) -> Vec<&'static str> {
    let mut fields = Vec::new();
    if params.get("rect").is_some() {
        fields.push("rect");
    }
    if params.get("url").is_some() || params.get("to_page").is_some() {
        fields.extend(LINK_ACTION);
    }
    if ["visible", "color", "width", "highlight"].iter().any(|key| params.get(*key).is_some()) {
        fields.extend(LINK_STYLE);
    }
    fields
}

/// Strings in `params[key]` (one, or a list).
fn names(params: &Value, key: &str) -> Vec<String> {
    match params.get(key) {
        Some(Value::Array(list)) => list.iter().filter_map(Value::as_str).map(str::to_owned).collect(),
        Some(Value::String(name)) => vec![name.clone()],
        _ => Vec::new(),
    }
}

/// What the write `command` with `params` rests on, given the document's objects now: the
/// objects it names by place, their windows, the fields its result depends on for each object
/// ([`Holds`]), and the facts no object carries ([`region`]) as a content premise on the root's
/// `~state`.
///
/// Each command was declared from its handler in `kition-pdf-automation` and the engine edit it
/// makes (C8-pdf-3 item 1; the readings are the rows of `bindings/wasm/tests/declarations_independent.rs`);
/// `agent/guide.md` lists what each rests on.
pub fn under(command: &str, params: &Value, objects: &Projection) -> Under {
    let mut h = Holds { objects, under: Under::default() };
    let page = |key: &str| numbers(params, key);
    match command {
        // ---- Writes at a point or rectangle of a page: the page and its geometry.
        "comment.add" | "page.addText" | "fillSign.add" | "measure.distance" | "measure.perimeter" | "measure.area" | "measure.setScale" => {
            for number in page("page") {
                h.page(number, GEOMETRY);
            }
        }
        "redaction.mark" => {
            let whole = params.get("whole_pages").and_then(Value::as_bool) == Some(true);
            if params.get("rect").is_some() {
                // A rectangle: on `page` (`pages` is not read).
                for number in page("page") {
                    h.page(number, GEOMETRY);
                }
            } else if whole {
                // Each page's crop box (inspect's `crop`).
                h.pages_or_all(&page("pages"), GEOMETRY);
            } else {
                // Text found on `pages`, or every page.
                h.pages_or_all(&page("pages"), &[]);
            }
        }
        "redaction.apply" => {
            for number in page("pages") {
                h.page(number, &[]);
            }
        }
        "link.add" => {
            for number in page("page") {
                h.page(number, GEOMETRY);
            }
            for number in page("to_page") {
                h.page(number, &[]);
            }
        }
        // ---- Writes on whole pages by number.
        // Rotates by `degrees` from the rotation each page has; `orientation` picks pages by their
        // size (lib.rs `page.rotate`, `filter_pages`).
        "page.rotate" => {
            let fields: &[&str] = if params.get("orientation").is_some() { &["rotation", "width", "height"] } else { &["rotation"] };
            h.pages_or_all(&page("pages"), fields);
        }
        "page.setBox" | "file.setWatermark" | "file.setBackground" => h.pages_or_all(&page("pages"), GEOMETRY),
        "file.setHeaderFooter" => h.pages_or_all(&page("pages"), GEOMETRY),
        "file.removeMarks" => {
            for number in page("pages") {
                h.page(number, &[]);
            }
        }
        "page.delete" | "page.duplicate" => {
            for number in page("pages") {
                h.page(number, &[]);
            }
        }
        "page.move" => {
            let pages = page("pages");
            for &number in &pages {
                h.page(number, &[]);
            }
            if let Some(to) = params.get("to").and_then(Value::as_u64) {
                h.move_gap(&pages, to);
            }
        }
        // Without a size, the new page takes the size of the page before (lib.rs `insert_blank`).
        "page.insertBlank" => {
            let at = params.get("at").and_then(Value::as_u64).unwrap_or(1);
            h.gap(at);
            if (params.get("width").is_none() || params.get("height").is_none())
                && let Some(near) = h.objects.page(at.saturating_sub(1).max(1))
            {
                let id = projection::page_tool_id(&near.id);
                h.premise(id, &["width", "height"], &[]);
            }
        }
        // The assigned range names pages by number. Only the following page's prior label
        // determines the restoration range; all other label ranges are preserved as read now.
        "page.number" => {
            let from = params.get("from").and_then(Value::as_u64).unwrap_or(0);
            let to = params.get("to").and_then(Value::as_u64).unwrap_or(0);
            for number in from..=to.min(objects.pages.len() as u64) {
                h.page(number, &[]);
            }
            if objects.page(to.saturating_add(1)).is_some() {
                h.page(to.saturating_add(1), &["label"]);
            }
        }
        // The content fact holds the skip predicate, or raster inputs when not skipped.
        "ocr.recognize" => h.pages_or_all(&page("pages"), &[]),
        // Detection converts the words' places through the page's geometry and skips places of
        // the fields' widgets there.
        "form.detectFields" => {
            let pages = page("pages");
            h.pages_or_all(&pages, GEOMETRY);
            let wanted: Vec<String> = if pages.is_empty() {
                objects.pages.iter().map(|view| projection::page_tool_id(&view.id)).collect()
            } else {
                pages.iter().filter_map(|n| objects.page(*n)).map(|view| projection::page_tool_id(&view.id)).collect()
            };
            let on: Vec<String> = objects
                .fields
                .iter()
                .filter(|field| {
                    field
                        .fields
                        .get("widgets")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .any(|w| w["page"].as_str().is_some_and(|p| wanted.iter().any(|x| x == p)))
                })
                .map(|field| field.name.clone())
                .collect();
            for name in on {
                h.field(&name, &["widgets"]);
            }
        }
        // ---- Items of a page's content by index (no id): the page; the item is a fact.
        "content.set" => {
            // A new rectangle is flipped through the page's displayed height (content.rs `to_display`).
            let fields: &[&str] = if params.get("rect").is_some() { GEOMETRY } else { &[] };
            for number in page("page") {
                h.page(number, fields);
            }
        }
        "content.delete" => {
            for number in page("page") {
                h.page(number, &[]);
            }
        }
        // The offset turns into page space through the page's rotation (lib.rs `view_vector`).
        "object.move" | "image.edit" => {
            for number in page("page") {
                h.page(number, GEOMETRY);
            }
        }
        // A paragraph without a width wraps at the page's crop box.
        "text.edit" => {
            let fields: &[&str] = if params.get("paragraph").is_some() && params.get("width").is_none() { GEOMETRY } else { &[] };
            for number in page("page") {
                h.page(number, fields);
            }
        }
        // ---- Links by page and index among the page's annotations.
        "link.edit" => {
            let fields = link_edit_fields(params);
            h.link(params, &fields, params.get("rect").is_some());
            for number in page("to_page") {
                h.page(number, &[]);
            }
        }
        "link.delete" => h.link(params, &[], false),
        // The candidate fact includes link-overlap filtering. Blank pages and unrelated
        // existing links do not decide which links this command creates.
        "link.addFromUrls" => {}
        // ---- Comments, by id or by page and index.
        "comment.edit" => {
            let free_text = match params.get("id").and_then(Value::as_str) {
                Some(id) => objects.comment(id).is_some_and(|(_, _, c)| c.fields.get("type").and_then(Value::as_str) == Some("FreeText")),
                None => {
                    let (page, index) =
                        (params.get("page").and_then(Value::as_u64).unwrap_or(0), params.get("index").and_then(Value::as_u64).unwrap_or(0));
                    objects.comment_at(page, index).is_some_and(|(_, c)| c.fields.get("type").and_then(Value::as_str) == Some("FreeText"))
                }
            };
            let fields = comment_edit_fields(params, free_text);
            let geometry = params.get("move").is_some() || params.get("rect").is_some();
            h.comment(params, &fields, geometry);
        }
        // A reply (status and check replies too) copies the comment's rectangle (and colour).
        "comment.reply" => h.comment(params, &["rect", "color"], false),
        "comment.setStatus" => h.comment(params, &["status", "rect", "color"], false),
        "comment.mark" => h.comment(params, &["marked", "rect"], false),
        "comment.lock" => h.comment(params, &["locked"], false),
        "comment.delete" => h.comment(params, &[], false),
        // ---- Form fields, by their fully qualified names.
        // `form.fill` converts the value by the field's type; the engine checks it against the
        // flags (read-only), maximum length, choices, check boxes' on states, and runs the
        // field's format and validation (forms `set_value_with`, `write_value`).
        "form.fill" => {
            let names: Vec<String> =
                params.get("values").and_then(Value::as_object).map(|values| values.keys().cloned().collect()).unwrap_or_default();
            for name in &names {
                let mut fields = vec!["value", "type", "maxLength", "actions", "appearance", "align"];
                if objects.field(name).is_some_and(|f| f.fields.get("type").and_then(Value::as_str) != Some("text")) {
                    fields.push("options");
                }
                h.field(name, &fields);
            }
        }
        // A reset writes each field's default (forms `reset`); none named: every field.
        "form.reset" => {
            let names = names(params, "fields");
            if params.get("fields").is_none() {
                h.all_fields(&["value", "default", "widgets"]);
            }
            for name in &names {
                h.field(name, &["value", "default", "widgets"]);
            }
        }
        "form.setProps" => {
            let props = field_props(params);
            let mut names = names(params, "field");
            names.extend(self::names(params, "fields"));
            for name in &names {
                h.field(name, &props);
                // A new rectangle is in the coordinates of the page of the field's first widget
                // (forms.rs `field_props_arg`).
                if params.get("rect").is_some()
                    && let Some(page) = h.widget_page(name)
                {
                    h.premise(page, GEOMETRY, &[]);
                }
            }
        }
        "form.deleteField" => {
            for name in names(params, "field") {
                h.field(&name, &[]);
            }
        }
        "form.setScript" => {
            for name in names(params, "field") {
                h.field(&name, &["actions"]);
            }
        }
        // Actions that go to a page name it by number (forms `set_field_actions`).
        "form.setActions" => {
            for name in names(params, "field") {
                h.field(&name, &["actions"]);
            }
            for action in params.get("actions").and_then(Value::as_array).into_iter().flatten() {
                if let Some(number) = action.get("page").and_then(Value::as_u64) {
                    h.page(number.max(1), &[]);
                }
            }
        }
        // `order` assigns /Tabs; `field` with `move` consumes its page's effective order and
        // the widget slots it rewrites.
        "form.setTabOrder" => {
            if let Some(name) = params.get("field").and_then(Value::as_str) {
                h.field(name, &["name"]);
            } else {
                h.pages_or_all(&page("pages"), &[]);
            }
        }
        "form.addField" => {
            for number in page("page") {
                h.page(number, GEOMETRY);
            }
            // Joining an existing radio group checks its type. Existing widget geometry is
            // preserved while their appearances are refreshed from the current file.
            if let Some(group) = params.get("group").and_then(Value::as_str)
                && objects.field(group).is_some()
            {
                h.field(group, &["type"]);
            }
        }
        // Scripts read and set any field; they also read the document's scripts, its
        // information and its number of pages (engine `run_javascript`).
        "script.run" => {
            h.all_fields(&[]);
            h.within(json!(ROOT), "pages", 0, objects.pages.len());
        }
        "bookmark.add" => {
            for number in page("page") {
                h.page(number, &[]);
            }
            let path = numbers(params, "parent");
            if let Some((holder, field, count)) = h.bookmark_path(&path) {
                h.bookmark_gap(holder, field, count, params.get("position").and_then(Value::as_u64), &path);
            }
        }
        // `tab-order`: every page's `/Tabs`.
        "accessibility.fix" if params.get("rule").and_then(Value::as_str) == Some("tab-order") => h.all_pages(&[]),
        // The page the document opens at, by number.
        "file.setInitialView" => {
            for number in page("page") {
                h.page(number.max(1), &[]);
            }
        }
        _ => {}
    }
    let mut under = h.under;
    if let Some(region) = region(command, params, objects) {
        under.content.push(ContentPremise { id: json!(ROOT), field: STATE_FIELD.to_owned(), region });
    }
    under
}

/// The facts no object carries that the write `command` with `params` rests on, as the region of
/// its content premise (`{ "facts": [...] }`); `None` when its objects carry everything.
pub fn region(command: &str, params: &Value, objects: &Projection) -> Option<Value> {
    let page = |key: &str| numbers(params, key);
    let at = |key: &str| params.get(key).and_then(Value::as_u64).unwrap_or(0);
    let every_page = || (1..=objects.pages.len() as u64).collect::<Vec<u64>>();
    let or_all = |pages: Vec<u64>| if pages.is_empty() { every_page() } else { pages };
    let rect = params.get("rect").cloned().unwrap_or(Value::Null);
    let mut facts: Vec<Value> = match command {
        // Writes that find text on the page (`all`: every match there, not every page).
        "comment.add" if params.get("find").is_some() => page("page").iter().map(|page| json!(["text", page])).collect(),
        // A mark over the page's text by a rectangle: the content it covers.
        "comment.add"
            if params.get("type").and_then(Value::as_str).is_some_and(|t| MARKUP.contains(&t))
                && (params.get("rect").is_some() || params.get("quads").is_some()) =>
        {
            let area = if params.get("rect").is_some() { rect.clone() } else { params.get("quads").cloned().unwrap_or(Value::Null) };
            page("page").iter().map(|page| json!(["covered", page, area])).collect()
        }
        "redaction.mark" => {
            let finds = ["find", "pattern", "words"].iter().any(|key| params.get(*key).is_some());
            if params.get("rect").is_some() {
                page("page").iter().map(|page| json!(["covered", page, rect])).collect()
            } else if finds {
                or_all(page("pages")).iter().map(|page| json!(["text", page])).collect()
            } else {
                Vec::new()
            }
        }
        // Measurements read the page's scale where the first point is (its viewports).
        "measure.distance" | "measure.perimeter" | "measure.area" => page("page").iter().map(|page| json!(["viewports", page])).collect(),
        // A new field's name: free now as it was; without one, a name made free of every name; a
        // radio button's `group`: the field it joins, or free.
        "form.addField" => {
            let joins = params.get("group").and_then(Value::as_str);
            let mut facts: Vec<Value> = joins.map(|group| json!(["field", group])).into_iter().collect();
            match params.get("name").and_then(Value::as_str) {
                Some(name) if joins.is_none_or(|group| objects.field(group).is_none()) => facts.push(json!(["field", name])),
                Some(_) => {}
                None if joins.is_none() => facts.push(json!(["names"])),
                None => {}
            }
            facts
        }
        // A field by name whose id is its name: that the name still names the same field.
        // Another program's rename reads again as the field's `name` changed under its new tool
        // id; the guard ties a premise on the old one to it (`relocated`), and the name is held
        // as a fact too (`a_rename_by_another_program_refuses_by_tool_id`,
        // `bindings/headless/tests/outside.rs`). A field with a stored id keeps it across a
        // rename, so its premise holds.
        "form.fill" | "form.reset" | "form.setProps" | "form.deleteField" | "form.setScript" | "form.setActions" | "form.setTabOrder" => {
            let mut held: Vec<String> = match command {
                "form.fill" => params.get("values").and_then(Value::as_object).map(|values| values.keys().cloned().collect()).unwrap_or_default(),
                "form.reset" => names(params, "fields"),
                _ => names(params, "field"),
            };
            if command == "form.setProps" {
                held.extend(names(params, "fields"));
            }
            let mut facts: Vec<Value> =
                held.iter().filter(|name| objects.field(name).is_none_or(|view| !view.stored)).map(|name| json!(["field", name])).collect();
            if matches!(command, "form.fill" | "form.reset") {
                let written = if command == "form.reset" && params.get("fields").is_none() {
                    objects.fields.iter().map(|f| f.name.clone()).collect()
                } else {
                    held.clone()
                };
                facts.push(json!(["calculations", written]));
                if command == "form.fill" {
                    facts.push(json!(["fill-inputs", held]));
                }
            }
            if command == "form.setProps"
                && let Some(name) = params.get("name").and_then(Value::as_str)
            {
                let leaf = name.trim();
                let prefix = params.get("field").and_then(Value::as_str).and_then(|field| field.rsplit_once('.')).map(|(prefix, _)| prefix);
                let renamed = prefix.map_or_else(|| leaf.to_owned(), |prefix| format!("{prefix}.{leaf}"));
                facts.push(json!(["field", renamed]));
            }
            if command == "form.setTabOrder"
                && let Some(name) = params.get("field").and_then(Value::as_str)
            {
                facts.push(json!(["tabs", name]));
            } else if command == "form.setTabOrder" {
                facts.extend(or_all(page("pages")).iter().map(|number| json!(["tab-mode", number])));
            }
            facts
        }
        // The page's content (its content streams and resources), the fields' names (a new field
        // takes a name free of them).
        "form.detectFields" => {
            let mut facts: Vec<Value> = or_all(page("pages")).iter().map(|page| json!(["content", page])).collect();
            facts.push(json!(["names"]));
            facts
        }
        "ocr.recognize" => or_all(page("pages"))
            .iter()
            .map(|page| json!(["ocr-inputs", page, params.get("skip_text_pages").and_then(Value::as_bool).unwrap_or(true)]))
            .collect(),
        // Items of a page's content by index: no item has an id but its content, so the item at
        // the place, with its drawing stream as well as the editing metadata `content.list`
        // shows (another program may change only the drawing). Not its rectangle as listed: that is
        // turned with the page, which a change of the item's text does not read (a new `rect`
        // holds the page's geometry instead).
        "content.set" | "content.delete" => {
            vec![
                json!(["entry", "content.list", {}, "items", { "page": at("page"), "index": at("index") }, ["type", "text", "font", "size", "bold", "italic", "$content"]]),
            ]
        }
        "image.edit" => vec![json!(["entry", "page.listImages", { "page": at("page") }, "images", at("image")])],
        // The selected entry and text removed along with it, including coincident copies.
        "text.edit" => {
            let item = match (params.get("paragraph").and_then(Value::as_u64), params.get("line").and_then(Value::as_u64)) {
                (Some(paragraph), _) => {
                    Some(json!(["entry", "text.listParagraphs", { "page": at("page") }, "paragraphs", { "paragraph": paragraph }]))
                }
                (None, Some(line)) => Some(json!(["entry", "text.listLines", { "page": at("page") }, "lines", line])),
                _ => None,
            };
            item.into_iter().chain(std::iter::once(json!(["text-overlap", at("page"), params]))).collect()
        }
        "object.move" => params
            .get("objects")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|object| json!(["entry", "object.list", { "page": at("page") }, "objects", { "index": object["index"], "kind": object["kind"] }]))
            .collect(),
        "link.addFromUrls" => vec![json!(["urls"])],
        // A figure by its number: `figure` is the structure element's object number, as
        // `accessibility.listFigures` lists it, not a place in that list. Decorative: the content of
        // its page too (its marked content becomes an artifact).
        "accessibility.setAlt" => {
            let mut facts = vec![json!(["entry", "accessibility.listFigures", {}, "figures", { "figure": at("figure") }])];
            if params.get("decorative").and_then(Value::as_bool) == Some(true) {
                facts.push(json!(["figure_content", at("figure")]));
            } else if let Some(parts) = facts.first_mut().and_then(Value::as_array_mut) {
                // Hold the replaced Alt and Figure eligibility without display page/bounds.
                parts.push(json!(["figure", "alt"]));
            }
            facts
        }
        // Bookmarks made of the tagged headings: the whole document (the structure tree and the
        // text of the pages).
        "bookmark.addFromStructure" => vec![json!(["headings"])],
        "redaction.apply" => vec![json!(["redaction-inputs", page("pages")])],
        "file.flatten" => {
            let (comments, fields) = match (params.get("comments").and_then(Value::as_bool), params.get("fields").and_then(Value::as_bool)) {
                (None, None) => (true, true),
                (Some(c), None) => (c, !c),
                (None, Some(f)) => (!f, f),
                (Some(c), Some(f)) => (c, f),
            };
            vec![json!(["flatten", comments, fields])]
        }
        // A header or footer with the number of pages (`<<n>>`) counts every page.
        "file.setHeaderFooter" => {
            let counts = ["header_left", "header_center", "header_right", "footer_left", "footer_center", "footer_right"]
                .iter()
                .any(|key| params.get(*key).and_then(Value::as_str).is_some_and(|text| text.contains("<<n>>")));
            if counts { vec![json!(["pages"])] } else { Vec::new() }
        }
        // ---- Writes that rest on one setting of the document, not all of it.
        "file.removeMarks" => {
            if params.get("pages").is_some() {
                page("pages").iter().map(|page| json!(["marks", page])).collect()
            } else {
                vec![json!(["marks"])]
            }
        }
        "redaction.clear" => vec![json!(["redactions"])],
        "script.set" => vec![json!(["scripts"])],
        "script.run" => vec![json!(["scripts"]), json!(["info"])],
        "file.setInitialView" => vec![json!(["view"])],
        "accessibility.fix" => match params.get("rule").and_then(Value::as_str) {
            Some("title") => vec![json!(["view"]), json!(["info"])],
            Some("tab-order") => Vec::new(),
            _ => vec![json!(["view"])],
        },
        // ---- Writes that change nothing of the document: a view setting (`comment.hide`), a
        // preference (`fillSign.setDateFormat`, `script.enabled`), another document (`file.new`), a
        // read (`file.listRevisions`).
        "comment.hide" | "fillSign.setDateFormat" | "script.enabled" | "file.new" | "file.listRevisions" => Vec::new(),
        // Writes whose objects carry all they rest on.
        "comment.add" | "page.addText" | "measure.setScale" | "fillSign.add" | "link.add" | "link.edit" | "link.delete" | "link.removeAll"
        | "page.rotate" | "page.setBox" | "file.setWatermark" | "file.setBackground" | "page.delete" | "page.duplicate" | "page.move"
        | "page.insertBlank" | "page.number" | "comment.edit" | "comment.setStatus" | "comment.mark" | "comment.lock" | "comment.reply"
        | "comment.delete" | "bookmark.add" => Vec::new(),
        // ---- Every other write rests on the whole document: a full rewrite (security, removing
        // hidden data, applying redactions, converting to PDF/A), flattening every comment or
        // field, comparing with another document, undo and redo.
        _ => vec![json!(["all"])],
    };
    facts.dedup();
    (!facts.is_empty()).then(|| json!({ "facts": facts }))
}
