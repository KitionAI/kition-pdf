//! The kit's declarations for the premise guard (task cards C8-land-14-pdf items 2-3, C8-pdf-2
//! items 1-2), checked against the commands' schemas and their effects:
//!
//! - every integer parameter that says it takes ids is marked (a comment's `page` and `index` are
//!   places), `kition_kit::testing::check_id_params`; the one allowed is a read's;
//! - every parameter that addresses something (a page, a comment, a field, an item of a list)
//!   changes what the write holds when it names something else (`premises::under`), or is
//!   allowed with why;
//! - the kit keeps no editor state, and no command reads one (`testing::check_context`);
//! - a seeded sample of writes, run on a fixture: every object a write changes is held, with the
//!   fields it changed, and a write that changes no object holds a fact of the content;
//! - a seeded independent check (item 2): 30 commands drawn with seed `0x9e3779b97f4a7c15`, each
//!   with the holds written down from reading its handler, compared with `premises::under`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use kition_kit::{KitDocument, Under};
use kition_pdf_kit::PdfDocument;
use kition_pdf_kit::premises::under;
use serde_json::{Value, json};

/// Parameters that address what a write acts on, as the scanner looks for them.
const ADDRESSES: &[&str] = &[
    "page",
    "pages",
    "index",
    "id",
    "field",
    "fields",
    "values",
    "figure",
    "image",
    "paragraph",
    "line",
    "objects",
    "parent",
    "position",
    "to",
    "to_page",
    "at",
    "from",
    "name",
];

/// Address parameters whose other value changes nothing a write holds, and why.
const ADDRESS_ALLOW: &[(&str, &str, &str)] = &[
    ("bookmark_add", "position", "the write holds every child of the parent (titles and pages), so any position among them"),
    ("page_add_text", "at", "a point in page coordinates: held through the page's geometry with `page`"),
    ("measure_scale", "at", "a point in page coordinates: held through the page's geometry with `page`"),
    ("measure_scale", "name", "the new scale's own name: it names nothing in the document"),
    ("comment_add", "at", "a point in page coordinates: held through the page's geometry with `page`"),
    ("comment_add", "from", "a point in page coordinates (a line's start): held through the page's geometry"),
    ("comment_add", "to", "a point in page coordinates (a line's end): held through the page's geometry"),
    ("fill_sign_add", "at", "a point in page coordinates: held through the page's geometry with `page`"),
    ("measure_distance", "points", "points in page coordinates: held through the page's geometry"),
    ("redact_mark", "page", "read only with `rect` (C8-pdf-3): without, `pages` (or every page) is searched or marked"),
    (
        "link_edit",
        "index",
        "an index naming no link holds the window of the page's links where one at it would be, the same for every place between two links",
    ),
    (
        "link_delete",
        "index",
        "an index naming no link holds the window of the page's links where one at it would be, the same for every place between two links",
    ),
    ("js_run", "field", "only the event's target, which is not looked up: the write holds every field"),
    ("js_set_document_script", "name", "the write holds every document script: it rewrites the whole tree of them"),
    ("doc_create", "from", "makes another document: the write holds nothing of this one"),
    ("doc_create", "name", "makes another document: the write holds nothing of this one"),
    ("doc_create", "pages", "makes another document: the write holds nothing of this one"),
];

pub fn fixture() -> PdfDocument {
    let mut doc = PdfDocument::create("Report.pdf", 612, 792).unwrap();
    for at in 2..=3 {
        doc.execute("page_insert_blank", json!({ "at": at })).unwrap();
    }
    for n in 1..=3 {
        doc.execute("page_add_text", json!({ "page": n, "at": [72, 60], "text": format!("Page {n}") })).unwrap();
        doc.execute("comment_add", json!({ "page": n, "type": "note", "at": [100, 100], "contents": format!("note {n}") })).unwrap();
        doc.execute("form_add_field", json!({ "page": n, "type": "text", "rect": [72, 600, 300, 624], "name": format!("F{n}") })).unwrap();
        doc.execute("bookmark_add", json!({ "page": n, "title": format!("B{n}") })).unwrap();
        doc.execute("link_add", json!({ "page": n, "rect": [100, 300, 200, 320], "url": format!("https://example.com/{n}") })).unwrap();
    }
    let bytes = doc.save(None).unwrap();
    PdfDocument::open("Report.pdf", &bytes).unwrap()
}

#[test]
fn every_integer_id_parameter_is_marked() {
    let commands = fixture().commands();
    // Reads hold nothing, so they are not marked (a place on a read would hold the objects there).
    let allow = [("comment_image_preview", "page")];
    kition_kit::testing::check_id_params(&commands, &allow).unwrap_or_else(|problems| panic!("{problems:#?}"));
}

/// Two values of an address parameter that name two different things on [`fixture`], with the
/// parameters it needs beside it.
fn two_values(doc: &PdfDocument, command: &str, name: &str, node: &Value) -> Option<(Value, Value)> {
    let comments = doc.objects();
    let comment = |page: usize| json!(comments.pages[page].comments[0].id);
    let with = |key: &str, a: Value, b: Value, base: Value| {
        let (mut x, mut y) = (base.clone(), base);
        x[key] = a;
        y[key] = b;
        Some((x, y))
    };
    let page_base = json!({ "page": 1 });
    match name {
        "page" | "to_page" => with(name, json!(1), json!(2), json!({})),
        "pages" => with(name, json!([1]), json!([2]), json!({})),
        "index" | "figure" | "image" | "paragraph" | "line" => with(name, json!(1), json!(2), page_base),
        "id" => with(name, comment(0), comment(1), json!({})),
        "field" => with(name, json!("F1"), json!("F2"), json!({})),
        "fields" if node["type"] == "boolean" => with(name, json!(true), json!(false), json!({})),
        "fields" => with(name, json!(["F1"]), json!(["F2"]), json!({})),
        "values" => with(name, json!({ "F1": "v" }), json!({ "F2": "v" }), json!({})),
        "objects" => with(name, json!([{ "index": 1, "kind": "text" }]), json!([{ "index": 2, "kind": "text" }]), page_base),
        "parent" => with(name, json!([]), json!([1]), json!({})),
        "from" if command == "page_number" => with(name, json!(1), json!(2), json!({"to":3})),
        "to" if command == "page_number" => with(name, json!(1), json!(2), json!({"from":1})),
        "to" if command == "page_move" => with(name, json!(1), json!(3), json!({ "pages": [2] })),
        "at" if command == "page_insert_blank" => with(name, json!(1), json!(3), json!({})),
        "name" => with(name, json!("A"), json!("B"), json!({})),
        _ => match node["type"].as_str() {
            Some("integer") => with(name, json!(1), json!(2), page_base),
            Some("array") => with(name, json!([1, 1]), json!([2, 2]), page_base),
            _ => with(name, json!("a"), json!("b"), page_base),
        },
    }
}

fn spelled(under: &Under) -> String {
    serde_json::to_string(&json!({
        "objects": under.objects,
        "within": under.within.iter().map(|w| json!([w.parent, w.field, w.from, w.to])).collect::<Vec<_>>(),
        "premises": under.premises.iter().map(|p| json!([p.id, p.fields, p.positions])).collect::<Vec<_>>(),
        "content": under.content.iter().map(|c| c.region.clone()).collect::<Vec<_>>(),
    }))
    .unwrap()
}

#[test]
fn every_address_parameter_changes_what_its_write_holds() {
    let doc = fixture();
    let objects = doc.objects();
    let mut problems = Vec::new();
    for command in doc.commands().iter().filter(|command| command.mutates) {
        let Some(schema) = command.schema.as_ref().and_then(Value::as_object) else { continue };
        for (name, node) in schema.iter().filter(|(name, _)| ADDRESSES.contains(&name.as_str())) {
            if ADDRESS_ALLOW.iter().any(|(c, p, _)| *c == command.id && p == name) {
                continue;
            }
            let Some((a, b)) = two_values(&doc, &command.id, name, node) else { continue };
            let (x, y) = (spelled(&under(&command.id, &a, &objects)), spelled(&under(&command.id, &b, &objects)));
            // A write over the whole document holds every address.
            if x == y && !x.contains("[\"all\"]") {
                problems.push(format!("{}: `{name}` = {} or {} holds the same: {x}", command.id, a[name], b[name]));
            }
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_kit_keeps_no_editor_state_and_no_command_reads_one() {
    let mut doc = fixture();
    assert_eq!(doc.editor_state(), Value::Null);
    kition_kit::testing::check_context(&mut doc, &kition_kit::CallContext::default(), kition_pdf_kit::EDITOR_STATE).unwrap();
    let reading: Vec<String> = doc.commands().into_iter().filter(|command| !command.reads.is_empty()).map(|command| command.id).collect();
    assert!(reading.is_empty(), "{reading:?}");
}

/// Writes with parameters valid on [`fixture`].
fn writes() -> Vec<(&'static str, Value)> {
    vec![
        ("comment_edit", json!({ "page": 2, "index": 1, "contents": "x" })),
        ("comment_edit", json!({ "page": 2, "index": 1, "color": "blue" })),
        ("comment_edit", json!({ "page": 2, "index": 1, "move": [10, 10] })),
        ("comment_set_status", json!({ "page": 1, "index": 1, "status": "accepted" })),
        ("comment_mark", json!({ "page": 1, "index": 1, "marked": true })),
        ("comment_lock", json!({ "page": 3, "index": 1, "locked": true })),
        ("comment_delete", json!({ "page": 3, "index": 1 })),
        ("comment_reply", json!({ "page": 2, "index": 1, "text": "r" })),
        ("form_fill", json!({ "values": { "F2": "x" } })),
        ("form_set_props", json!({ "field": "F3", "tooltip": "t" })),
        ("form_set_props", json!({ "field": "F1", "required": true })),
        ("form_delete_field", json!({ "field": "F1" })),
        ("page_rotate", json!({ "pages": [2], "degrees": 90 })),
        ("page_delete", json!({ "pages": [1] })),
        ("page_move", json!({ "pages": [3], "to": 1 })),
        ("page_set_box", json!({ "pages": [2], "box": "crop", "margins": [10, 10, 10, 10] })),
        ("page_duplicate", json!({ "pages": [2] })),
        ("page_insert_blank", json!({ "at": 2 })),
        ("content_update", json!({ "page": 1, "index": 1, "text": "y" })),
        ("content_delete", json!({ "page": 2, "index": 1 })),
        ("link_edit", json!({ "page": 1, "index": 3, "url": "https://example.com/z" })),
        ("link_delete", json!({ "page": 2, "index": 3 })),
        ("bookmark_add", json!({ "page": 1, "title": "New", "position": 1 })),
        ("comment_add", json!({ "page": 2, "type": "note", "at": [300, 300], "contents": "n" })),
        ("page_add_text", json!({ "page": 3, "at": [72, 200], "text": "t" })),
        ("form_add_field", json!({ "page": 1, "type": "text", "rect": [72, 400, 300, 424], "name": "New" })),
        ("doc_initial_view", json!({ "layout": "single" })),
        ("page_number", json!({ "from": 1, "to": 2, "style": "upper-roman" })),
    ]
}

/// Every object of a projection by tool id: its fields, and its holder's id and place.
fn flat(objects: &kition_pdf_engine::projection::Projection) -> BTreeMap<String, (serde_json::Map<String, Value>, String, usize)> {
    use kition_pdf_engine::projection::{field_tool_id, page_tool_id};
    let mut out = BTreeMap::new();
    for (at, page) in objects.pages.iter().enumerate() {
        out.insert(page_tool_id(&page.id), (page.fields.clone(), "pdf".to_owned(), at));
        for (place, comment) in page.comments.iter().enumerate() {
            out.insert(comment.id.clone(), (comment.fields.clone(), page_tool_id(&page.id), place));
        }
    }
    for (at, field) in objects.fields.iter().enumerate() {
        out.insert(field_tool_id(&field.id), (field.fields.clone(), "pdf".to_owned(), at));
    }
    out
}

/// A SplitMix64 step: the seeded order of the samples.
fn next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn shuffled<T>(mut all: Vec<T>) -> Vec<T> {
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    for i in (1..all.len()).rev() {
        let j = usize::try_from(next(&mut seed) % (i as u64 + 1)).unwrap();
        all.swap(i, j);
    }
    all
}

/// Fields of an object that changed between two projections (`None`: it is gone).
fn changed_fields(before: &serde_json::Map<String, Value>, after: Option<&serde_json::Map<String, Value>>) -> Option<Vec<String>> {
    let after = after?;
    Some(before.iter().filter(|(key, value)| after.get(*key) != Some(value)).map(|(key, _)| key.clone()).collect())
}

/// Fields that follow from a write's own change and say no more than it does: a comment's
/// modification date (every edit stamps it), its `/Annots` place after a sibling went, a page's
/// label (a page's number), a field's widgets' page after a page move.
const CONSEQUENCES: &[&str] = &["modified", "index"];

#[test]
fn a_seeded_sample_of_writes_holds_every_object_it_changes() {
    let mut problems = Vec::new();
    for (command, params) in shuffled(writes()).into_iter().take(20) {
        println!("sampled: {command} {params}");
        let mut doc = fixture();
        let mut params = params;
        if command.starts_with("link_") {
            // A link's index counts every annotation of its page: read it.
            let links = doc.execute("link_list", json!({})).unwrap();
            let link = links["links"].as_array().unwrap().iter().find(|link| link["page"] == params["page"]).unwrap().clone();
            params["index"] = link["index"].clone();
        }
        let before = doc.objects();
        let held = under(command, &params, &before);
        doc.execute(command, params.clone()).unwrap_or_else(|error| panic!("{command} {params}: {error}"));
        let after = doc.objects();
        let (was, now) = (flat(&before), flat(&after));
        let mut changed_any = false;
        for (id, (fields, holder, place)) in &was {
            let premise = held.premises.iter().find(|premise| premise.id.as_str() == Some(id));
            let window = held.within.iter().any(|w| w.parent.as_str() == Some(holder.as_str()) && (w.from..w.to.max(w.from + 1)).contains(place));
            let holder_held = held.premises.iter().any(|premise| premise.id.as_str() == Some(holder.as_str()));
            if let Some((_, now_holder, now_place)) = now.get(id)
                && (now_holder != holder || now_place != place)
                && holder == "pdf"
            {
                // A page that moved in the list: its order with most other pages flipped (one that
                // only shifted while another moved past it, or after an insertion or a deletion, is
                // told by the window the write holds).
                let flips = was
                    .iter()
                    .filter(|(other, (_, h, p))| {
                        *other != id && h == "pdf" && now.get(*other).is_some_and(|(_, _, q)| (p < place) != (q < now_place))
                    })
                    .count();
                let pages = was.values().filter(|(_, h, _)| h == "pdf").count();
                if flips * 2 > pages.saturating_sub(1) && premise.is_none() && !window {
                    problems.push(format!("{command} {params}: moves {id} ({place} -> {now_place}), which it does not hold"));
                }
                changed_any = true;
            }
            match changed_fields(fields, now.get(id).map(|(fields, _, _)| fields)) {
                None => {
                    changed_any = true;
                    // A comment goes with its page.
                    if premise.is_none() && !window && !holder_held {
                        problems.push(format!("{command} {params}: deletes {id}, which it does not hold"));
                    }
                }
                Some(fields) => {
                    let fields: Vec<String> = fields.into_iter().filter(|f| !CONSEQUENCES.contains(&f.as_str())).collect();
                    if fields.is_empty() {
                        continue;
                    }
                    changed_any = true;
                    let holds = premise.and_then(|premise| premise.fields.clone());
                    // Check the supplied output as well as the hold on the value replaced.
                    if command == "comment_edit"
                        && id
                            == before
                                .comment_at(params["page"].as_u64().unwrap_or(0), params["index"].as_u64().unwrap_or(0))
                                .map(|(_, c)| c.id.as_str())
                                .unwrap_or_default()
                        && params.get("contents").is_some()
                    {
                        assert_eq!(now[id].0["contents"], params["contents"]);
                    }
                    let unheld: Vec<&String> = match (premise, &holds) {
                        (None, _) => fields.iter().collect(),
                        (Some(_), None) => Vec::new(),
                        (Some(_), Some(holds)) => {
                            fields.iter().filter(|f| !holds.iter().any(|h| h == *f || h.starts_with(&format!("{f}.")))).collect()
                        }
                    };
                    if !unheld.is_empty() {
                        problems.push(format!("{command} {params}: changes {unheld:?} of {id}, which it does not hold ({holds:?})"));
                    }
                }
            }
        }
        let created: Vec<&String> = now.keys().filter(|id| !was.contains_key(*id)).collect();
        // A write that changes no object writes page content: it holds the page it writes on, or
        // a fact of the content.
        if !changed_any && created.is_empty() && held.content.is_empty() && held.premises.is_empty() {
            problems.push(format!("{command} {params}: changes no object and holds nothing"));
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

/// What one write holds, as the independent check writes it down from the handler: for each
/// object (`page:N` for the page at N now, `comment:P/I` for the comment at page P, annotation I,
/// `field:<name>`), the fields held (`*`: the object only, no field), and the content facts by
/// kind.
type Expected = (&'static str, Value, &'static [(&'static str, &'static [&'static str])], &'static [&'static str]);

/// 41 commands with parameters valid on [`fixture`], each with what its handler (in
/// `kition-pdf-automation`, `lib.rs`, `comments.rs`, `forms.rs`, `links.rs`, `content.rs`,
/// `measure.rs`, `redact.rs`, and the engine edit each makes) resolves and reads, written down
/// from the handler, not from `premises::under`.
fn expected() -> Vec<Expected> {
    const G: &[&str] = &["width", "height", "rotation", "boxes"];
    vec![
        // comments.rs comment_edit: comment_target (id, or page + index), then each set member.
        ("comment_edit", json!({ "page": 2, "index": 1, "contents": "x" }), &[("page:2", &["*"]), ("comment:2/1", &["contents", "type"])], &[]),
        ("comment_edit", json!({ "page": 2, "index": 1, "move": [5, 5] }), &[("page:2", G), ("comment:2/1", &["rect"])], &[]),
        (
            "comment_edit",
            json!({ "page": 1, "index": 1, "opacity": 0.5, "width": 2 }),
            &[("page:1", &["*"]), ("comment:1/1", &["opacity", "width"])],
            &[],
        ),
        (
            "comment_set_status",
            json!({ "page": 1, "index": 1, "status": "accepted" }),
            &[("page:1", &["*"]), ("comment:1/1", &["status", "rect", "color"])],
            &[],
        ),
        ("comment_mark", json!({ "page": 3, "index": 1, "marked": true }), &[("page:3", &["*"]), ("comment:3/1", &["marked", "rect"])], &[]),
        ("comment_lock", json!({ "page": 3, "index": 1, "locked": true }), &[("page:3", &["*"]), ("comment:3/1", &["locked"])], &[]),
        ("comment_delete", json!({ "page": 3, "index": 1 }), &[("page:3", &["*"]), ("comment:3/1", &["*"])], &[]),
        ("comment_reply", json!({ "page": 2, "index": 1, "text": "r" }), &[("page:2", &["*"]), ("comment:2/1", &["rect", "color"])], &[]),
        // comments.rs comment_add: the page, a point converted through its geometry; `find` reads its text.
        ("comment_add", json!({ "page": 2, "type": "note", "at": [300, 300], "contents": "n" }), &[("page:2", G)], &[]),
        ("comment_add", json!({ "page": 1, "type": "highlight", "find": "Page" }), &[("page:1", G)], &["text"]),
        // forms.rs form_fill: each named field's value (and the check box states showing it).
        (
            "form_fill",
            json!({ "values": { "F2": "x" } }),
            &[("field:F2", &["value", "type", "maxLength", "actions", "appearance", "align", "name"])],
            &["calculations", "fill-inputs"],
        ),
        ("form_reset", json!({ "fields": ["F1"] }), &[("field:F1", &["value", "default", "widgets", "name"])], &["calculations"]),
        (
            "form_reset",
            json!({}),
            &[
                ("field:F1", &["value", "default", "widgets", "name"]),
                ("field:F2", &["value", "default", "widgets", "name"]),
                ("field:F3", &["value", "default", "widgets", "name"]),
            ],
            &["calculations"],
        ),
        ("form_set_props", json!({ "field": "F3", "tooltip": "t" }), &[("field:F3", &["tooltip", "name"])], &[]),
        ("form_set_props", json!({ "field": "F1", "required": true }), &[("field:F1", &["flags", "name"])], &[]),
        ("form_set_props", json!({ "field": "F2", "max_length": 5 }), &[("field:F2", &["maxLength", "name"])], &[]),
        ("form_delete_field", json!({ "field": "F1" }), &[("field:F1", &["*"])], &[]),
        ("form_add_field", json!({ "page": 1, "type": "text", "rect": [72, 400, 300, 424], "name": "New" }), &[("page:1", G)], &["field"]),
        ("form_set_script", json!({ "field": "F1", "event": "calculate", "script": "1" }), &[("field:F1", &["actions", "name"])], &[]),
        // lib.rs page tools: the pages named by number.
        ("page_rotate", json!({ "pages": [2], "degrees": 90 }), &[("page:2", &["rotation"])], &[]),
        ("page_delete", json!({ "pages": [1] }), &[("page:1", &["*"])], &[]),
        ("page_duplicate", json!({ "pages": [2] }), &[("page:2", &["*"])], &[]),
        ("page_move", json!({ "pages": [3], "to": 1 }), &[("page:3", &["*"]), ("page:1", &["*"])], &[]),
        // lib.rs insert_blank: without a size, the page before's size.
        ("page_insert_blank", json!({ "at": 2 }), &[("page:1", &["*", "width", "height"]), ("page:2", &["*"])], &[]),
        ("page_set_box", json!({ "pages": [2], "box": "crop", "margins": [10, 10, 10, 10] }), &[("page:2", G)], &[]),
        ("page_add_text", json!({ "page": 3, "at": [72, 200], "text": "t" }), &[("page:3", G)], &[]),
        (
            "page_number",
            json!({ "from": 1, "to": 2, "style": "upper-roman" }),
            &[("page:1", &["*"]), ("page:2", &["*"]), ("page:3", &["label"])],
            &[],
        ),
        ("doc_watermark", json!({ "text": "W", "pages": [1] }), &[("page:1", G)], &[]),
        ("doc_header_footer", json!({ "header_left": "H" }), &[("page:1", G), ("page:2", G), ("page:3", G)], &[]),
        // content.rs / links.rs: items of a page by index, no id: the page and the entry.
        ("content_update", json!({ "page": 1, "index": 1, "text": "y" }), &[("page:1", &["*"])], &["entry"]),
        ("content_delete", json!({ "page": 2, "index": 1 }), &[("page:2", &["*"])], &["entry"]),
        (
            "link_edit",
            json!({ "page": 1, "index": 4, "url": "https://example.com/z" }),
            &[("page:1", &["*"]), ("link:1/4", &["url", "toPage", "other"]), ("comment:1/1", &["*"])],
            &[],
        ),
        ("link_delete", json!({ "page": 2, "index": 4 }), &[("page:2", &["*"]), ("link:2/4", &["*"]), ("comment:2/1", &["*"])], &[]),
        ("link_add", json!({ "page": 1, "rect": [1, 1, 20, 20], "to_page": 2 }), &[("page:1", G), ("page:2", &["*"])], &[]),
        ("bookmark_add", json!({ "page": 1, "title": "New", "position": 1 }), &[("page:1", &["*"]), ("bookmark:1", &["*"])], &[]),
        ("measure_distance", json!({ "page": 2, "points": [[10, 10], [50, 50]] }), &[("page:2", G)], &["viewports"]),
        ("redact_mark", json!({ "page": 1, "rect": [10, 10, 50, 50] }), &[("page:1", G)], &["covered"]),
        ("redact_mark", json!({ "find": "Page" }), &[("page:1", &["*"]), ("page:2", &["*"]), ("page:3", &["*"])], &["text"]),
        ("ocr_recognize", json!({ "pages": [2] }), &[("page:2", &["*"])], &["ocr-inputs"]),
        // Writes over the whole document.
        ("doc_initial_view", json!({ "layout": "single" }), &[], &["view"]),
        ("redact_apply", json!({}), &[], &["redaction-inputs"]),
    ]
}

/// `under` in the words of [`Expected`].
fn as_expected(held: &Under, objects: &kition_pdf_engine::projection::Projection) -> (BTreeMap<String, BTreeSet<String>>, BTreeSet<String>) {
    use kition_pdf_engine::projection::page_tool_id;
    let name_of = |id: &str| -> String {
        for (number, page) in objects.pages.iter().enumerate() {
            if page_tool_id(&page.id) == id {
                return format!("page:{}", number + 1);
            }
            if let Some(comment) = page.comments.iter().find(|comment| comment.id == id) {
                return format!("comment:{}/{}", number + 1, comment.index);
            }
            if let Some(link) = page.links.iter().find(|link| link.id == id) {
                return format!("link:{}/{}", number + 1, link.index);
            }
        }
        if let Some(field) = objects.fields.iter().find(|field| kition_pdf_engine::projection::field_tool_id(&field.id) == id) {
            return format!("field:{}", field.name);
        }
        for bookmark in objects.all_bookmarks() {
            if kition_pdf_engine::projection::bookmark_tool_id(&bookmark.id) == id {
                return format!("bookmark:{}", bookmark.path.iter().map(|p| (p + 1).to_string()).collect::<Vec<_>>().join("/"));
            }
        }
        id.to_owned()
    };
    let mut holds: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for premise in &held.premises {
        let entry = holds.entry(name_of(premise.id.as_str().unwrap_or_default())).or_default();
        match &premise.fields {
            Some(fields) if fields.is_empty() => {
                entry.insert("*".into());
            }
            Some(fields) => entry.extend(fields.iter().map(|f| f.split('.').next().unwrap_or_default().to_owned())),
            None => {
                entry.insert("all fields".into());
            }
        }
    }
    let kinds = held
        .content
        .iter()
        .flat_map(|content| content.region["facts"].as_array().cloned().unwrap_or_default())
        .filter_map(|fact| fact[0].as_str().map(str::to_owned))
        .collect();
    (holds, kinds)
}

/// The independent check (item 2): 30 of [`expected`] drawn with the seed, each compared with
/// what `premises::under` declares. Prints the error rate; fails above 5 %.
#[test]
fn an_independent_seeded_check_of_thirty_declarations() {
    let doc = fixture();
    let objects = doc.objects();
    let mut wrong = Vec::new();
    let sample: Vec<Expected> = shuffled(expected()).into_iter().take(30).collect();
    for (command, params, holds, kinds) in &sample {
        let (got, got_kinds) = as_expected(&under(command, params, &objects), &objects);
        let want: BTreeMap<String, BTreeSet<String>> =
            holds.iter().map(|(id, fields)| ((*id).to_owned(), fields.iter().map(|f| (*f).to_owned()).collect())).collect();
        let want_kinds: BTreeSet<String> = kinds.iter().map(|k| (*k).to_owned()).collect();
        let ok = got == want && got_kinds == want_kinds;
        println!("check {command} {params}: {}", if ok { "agrees" } else { "differs" });
        if !ok {
            wrong.push(format!("{command} {params}: expected {want:?} {want_kinds:?}, declared {got:?} {got_kinds:?}"));
        }
    }
    let rate = wrong.len() as f64 / sample.len() as f64;
    println!("independent check: {} of {} differ ({:.1} %)", wrong.len(), sample.len(), rate * 100.0);
    assert!(rate <= 0.05, "{wrong:#?}");
}

/// SDK defect (kition-sdk 1b9d5ec): with `derived.state.premiseOnly: true`, a content premise on
/// `~state` is refused `unknown` ("Another program renumbered objects") after any outside reread,
/// even an unrelated one (reproduction: set it to `true` in `kdm-model/model/schema.json` and run
/// `bindings/headless/tests/premises.rs`: `document_settings_hold_the_whole_document`,
/// `links_hold_the_link_at_the_index`, `content_items_hold_the_item_at_the_index` and
/// `bookmarks_hold_the_parents_children` fail with `unknown` where they apply or refuse
/// `changed`). Until the SDK keeps private facts across rereads, `~state` stays public: it shows in
/// receipts and `others`. This test pins that choice.
#[test]
fn waiting_for_sdk_state_stays_public_until_private_facts_hold_across_rereads() {
    use kition_kit::session::KitModel;
    let schema = kdm_model::PdfModel::schema();
    assert!(!schema.types["Pdf"].derived["state"].premise_only);
}
