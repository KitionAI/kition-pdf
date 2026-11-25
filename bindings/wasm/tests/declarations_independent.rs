//! The independent reading of every write command (task card C8-pdf-3, item 1): for each of the
//! 67 commands the kit offers that change the document, what its automation body and the engine
//! edit behind it resolve and read, written down from the code (the rows of [`readings`]) before
//! being compared with the declarations (`premises::under`). Each row names the objects a write
//! holds by their place on [`fixture`] (`page:N`, `comment:P/I` and `link:P/I` for the annotation
//! at `/Annots` place I of page P, `field:<name>`, `bookmark:<path from 1>`), the fields held of
//! each (`*`: only that it is there), the lists whose window it holds, and the kinds of facts no
//! object carries.
//!
//! Where the readings found the declarations of C8-pdf-2 wrong, the row says so (`was:`). Of the
//! 67 commands, 37 were declared differently from what their code reads: 24 held less (a write
//! that can land on what the agent did not observe), 14 held more (a refusal no change needs);
//! `redaction.mark` and `form.setTabOrder` both.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

use kition_kit::{KitDocument, Under};
use kition_pdf_engine::projection::{Projection, bookmark_tool_id, field_tool_id, page_tool_id};
use kition_pdf_kit::PdfDocument;
use kition_pdf_kit::premises::under;
use serde_json::{Value, json};

/// Three Letter pages, each with the text `Page n`, a note (and its pop-up), a text field `Fn`, a
/// bookmark `Bn` to it and a link; page 1 also a radio group `R`, page 3 a text box.
pub fn fixture() -> PdfDocument {
    let mut doc = PdfDocument::create("Report.pdf", 612, 792).unwrap();
    for at in 2..=3 {
        doc.execute("page.insertBlank", json!({ "at": at })).unwrap();
    }
    for n in 1..=3 {
        doc.execute("page.addText", json!({ "page": n, "at": [72, 60], "text": format!("Page {n}") })).unwrap();
        doc.execute("comment.add", json!({ "page": n, "type": "note", "at": [100, 100], "contents": format!("note {n}") })).unwrap();
        doc.execute("form.addField", json!({ "page": n, "type": "text", "rect": [72, 600, 300, 624], "name": format!("F{n}") })).unwrap();
        doc.execute("bookmark.add", json!({ "page": n, "title": format!("B{n}") })).unwrap();
        doc.execute("link.add", json!({ "page": n, "rect": [100, 300, 200, 320], "url": format!("https://example.com/{n}") })).unwrap();
    }
    doc.execute("form.addField", json!({ "page": 1, "type": "radio", "rect": [72, 650, 90, 668], "name": "R" })).unwrap();
    doc.execute("comment.add", json!({ "page": 3, "type": "textbox", "rect": [300, 400, 450, 450], "contents": "box" })).unwrap();
    let bytes = doc.save(None).unwrap();
    PdfDocument::open("Report.pdf", &bytes).unwrap()
}

/// The place in `/Annots` (from 1) of page `page`'s link, as `link.list` numbers it.
fn link(doc: &mut PdfDocument, page: u64) -> u64 {
    let links = doc.execute("link.list", json!({})).unwrap();
    links["links"].as_array().unwrap().iter().find(|l| l["page"] == page).unwrap()["index"].as_u64().unwrap()
}

/// The place in `/Annots` (from 1) of page 3's text box.
fn text_box(doc: &PdfDocument) -> u64 {
    doc.objects().pages[2].comments.iter().find(|c| c.fields["type"] == "FreeText").unwrap().index as u64
}

/// One row: the command, its parameters, the objects held with their fields, the windows held
/// (holder, list), the kinds of facts.
struct Row {
    command: &'static str,
    params: Value,
    holds: Vec<(String, Vec<&'static str>)>,
    windows: Vec<(String, &'static str)>,
    facts: Vec<&'static str>,
}

const G: &[&str] = &["width", "height", "rotation", "boxes"];
const STAR: &[&str] = &["*"];

fn row(command: &'static str, params: Value, holds: &[(&str, &[&'static str])], windows: &[(&str, &'static str)], facts: &[&'static str]) -> Row {
    Row {
        command,
        params,
        holds: holds.iter().map(|(id, fields)| ((*id).to_owned(), fields.to_vec())).collect(),
        windows: windows.iter().map(|(holder, list)| ((*holder).to_owned(), *list)).collect(),
        facts: facts.to_vec(),
    }
}

/// An annotation by `page` and `index` holds where the comments and links before it are: the index
/// counts every annotation of the page (comments.rs `comment_target`, links.rs), so one of them
/// gone or come names another annotation (added after the extended table's first run, C8-pdf-3
/// item 7: a link by index after the comment before it was deleted failed at the kit instead of
/// being refused).
///
/// What each write resolves and reads, from its handler (`kition-pdf-automation`) and the engine
/// edit it makes (`kition-pdf-engine` `run_edit` and the crate it calls), written down before the
/// comparison. Comments give the code read and, where the C8-pdf-2 declaration differed, `was:`.
fn readings(doc: &mut PdfDocument) -> Vec<Row> {
    let (l1, l2) = (link(doc, 1), link(doc, 2));
    let tb = text_box(doc);
    let pages = |fields: &'static [&'static str]| -> Vec<(&'static str, &'static [&'static str])> {
        vec![("page:1", fields), ("page:2", fields), ("page:3", fields)]
    };
    let fields3 = |fields: &'static [&'static str]| -> Vec<(&'static str, &'static [&'static str])> {
        vec![("field:F1", fields), ("field:F2", fields), ("field:F3", fields), ("field:R", fields)]
    };
    let mut rows = vec![
        // ---- Pages (automation lib.rs; organize).
        // page.rotate: `degrees` added to each page's own or inherited /Rotate (organize rotate).
        row("page.rotate", json!({ "pages": [2], "degrees": 90 }), &[("page:2", &["rotation"])], &[("pdf", "pages")], &[]),
        // `orientation` compares the displayed width and height (engine filter_pages); no `pages`: all.
        row("page.rotate", json!({ "degrees": 90, "orientation": "portrait" }), &pages(&["rotation", "width", "height"]), &[("pdf", "pages")], &[]),
        // page.delete: the pages by number; the engine also drops links, bookmarks and named
        // destinations to them: consequences of the deletion, not what it rests on.
        row("page.delete", json!({ "pages": [1] }), &[("page:1", STAR)], &[("pdf", "pages")], &[]),
        // page.move: `to` is a place in the list WITHOUT the moved pages (organize move_pages:
        // splice at min(to, rest.len())): pages [1] to 2 go between pages 2 and 3.
        // was: the pages either side of `to` before the move (pages 1 and 2), not page 3.
        row("page.move", json!({ "pages": [1], "to": 2 }), &[("page:1", STAR), ("page:2", STAR), ("page:3", STAR)], &[("pdf", "pages")], &[]),
        row("page.move", json!({ "pages": [3], "to": 1 }), &[("page:3", STAR), ("page:1", STAR)], &[("pdf", "pages")], &[]),
        // page.insertBlank: without a size the displayed size of the page before (at 1: the first).
        row("page.insertBlank", json!({ "at": 2 }), &[("page:1", &["*", "width", "height"]), ("page:2", STAR)], &[("pdf", "pages")], &[]),
        // page.duplicate: copies the page and what is on it as it is (links to other pages lose
        // their destination: a consequence of the copy).
        row("page.duplicate", json!({ "pages": [2] }), &[("page:2", STAR)], &[("pdf", "pages")], &[]),
        // page.setBox: margins from each page's media box, `rect` through crop and rotation.
        row("page.setBox", json!({ "pages": [2], "box": "crop", "margins": [10, 10, 10, 10] }), &[("page:2", G)], &[("pdf", "pages")], &[]),
        // page.number: the existing label ranges and the number page `to`+1 showed (labels.rs).
        row(
            "page.number",
            json!({ "from": 1, "to": 2, "style": "upper-roman" }),
            &[("page:1", STAR), ("page:2", STAR), ("page:3", &["label"])],
            &[("pdf", "pages")],
            &[],
        ),
        // page.addText: `at`/`rect` flipped through the displayed height, placed through the
        // page's view matrix (crop, rotation, unit).
        row("page.addText", json!({ "page": 3, "at": [72, 200], "text": "t" }), &[("page:3", G)], &[("pdf", "pages")], &[]),
        // ---- Page marks (engine marks; edit stamp): each page's display size and view matrix.
        row("file.setWatermark", json!({ "text": "W", "pages": [1] }), &[("page:1", G)], &[("pdf", "pages")], &[]),
        row("file.setBackground", json!({ "color": "yellow", "pages": [2] }), &[("page:2", G)], &[("pdf", "pages")], &[]),
        row("file.setHeaderFooter", json!({ "headerLeft": "H" }), &pages(G), &[("pdf", "pages")], &[]),
        // `<<n>>` is the number of pages of the document (edit header_footer `all.len()`).
        // was: only the pages marked.
        row("file.setHeaderFooter", json!({ "pages": [2], "footerCenter": "<<1>> of <<n>>" }), &[("page:2", G)], &[("pdf", "pages")], &["pages"]),
        // file.removeMarks: every page's content streams tagged with the kit's marks.
        // was: the whole document.
        row("file.removeMarks", json!({ "kind": "watermark" }), &[], &[], &["marks"]),
        // ---- Bookmarks (organize outline).
        // bookmark.add: the page object for the destination; `parent` a path of places; `position`
        // a place among the parent's children (clamped).
        // was: the parent's children's titles and pages as a fact, the parent by its path only.
        row(
            "bookmark.add",
            json!({ "page": 1, "title": "New", "position": 1 }),
            &[("page:1", STAR), ("bookmark:1", STAR)],
            &[("pdf", "pages"), ("pdf", "bookmarks")],
            &[],
        ),
        row(
            "bookmark.add",
            json!({ "page": 2, "title": "Child", "parent": [1] }),
            &[("page:2", STAR), ("bookmark:1", STAR)],
            &[("pdf", "pages"), ("pdf", "bookmarks"), ("bookmark:1", "children")],
            &[],
        ),
        // bookmark.addFromStructure: the structure tree's headings and their text on every page.
        row("bookmark.addFromStructure", json!({}), &[], &[], &["headings"]),
        // ---- Comments (comments.rs comment_target: `index` is the place in /Annots).
        row("comment.add", json!({ "page": 2, "type": "note", "at": [300, 300], "contents": "n" }), &[("page:2", G)], &[("pdf", "pages")], &[]),
        row("comment.add", json!({ "page": 1, "type": "highlight", "find": "Page" }), &[("page:1", G)], &[("pdf", "pages")], &["text"]),
        // `all`: every match on the page (comments.rs), not every page. was: the whole document.
        row("comment.add", json!({ "page": 1, "type": "highlight", "find": "Page", "all": true }), &[("page:1", G)], &[("pdf", "pages")], &["text"]),
        // A highlight by quads marks the text under them (item 5).
        row(
            "comment.add",
            json!({ "page": 1, "type": "highlight", "quads": [[72, 60, 140, 60, 72, 80, 140, 80]] }),
            &[("page:1", G)],
            &[("pdf", "pages")],
            &["covered"],
        ),
        // annot reply: copies the comment's /Rect and /C. was: only that it is there.
        row(
            "comment.reply",
            json!({ "page": 2, "index": 1, "text": "r" }),
            &[("page:2", STAR), ("comment:2/1", &["rect", "color"])],
            &[("pdf", "pages"), ("page:2", "comments")],
            &[],
        ),
        row(
            "comment.setStatus",
            json!({ "page": 1, "index": 1, "status": "accepted" }),
            &[("page:1", STAR), ("comment:1/1", &["status", "rect", "color"])],
            &[("pdf", "pages"), ("page:1", "comments")],
            &[],
        ),
        // annot set_marked: copies /Rect.
        row(
            "comment.mark",
            json!({ "page": 3, "index": 1, "marked": true }),
            &[("page:3", STAR), ("comment:3/1", &["marked", "rect"])],
            &[("pdf", "pages"), ("page:3", "comments")],
            &[],
        ),
        row(
            "comment.lock",
            json!({ "page": 3, "index": 1, "locked": true }),
            &[("page:3", STAR), ("comment:3/1", &["locked"])],
            &[("pdf", "pages"), ("page:3", "comments")],
            &[],
        ),
        row(
            "comment.edit",
            json!({ "page": 2, "index": 1, "contents": "x" }),
            &[("page:2", STAR), ("comment:2/1", &["contents", "type"])],
            &[("pdf", "pages"), ("page:2", "comments")],
            &[],
        ),
        // New text in a text box refits the box (annot set_contents fitted_box: /Rect, /BS).
        // was: the text only.
        row(
            "comment.edit",
            json!({ "page": 3, "index": tb, "contents": "longer text" }),
            &[
                ("page:3", STAR),
                (&format!("comment:3/{tb}"), &["contents", "type", "rect", "width"]),
                ("comment:3/1", STAR),
                (&format!("link:3/{}", link(doc, 3)), STAR),
            ],
            &[("pdf", "pages"), ("page:3", "comments")],
            &[],
        ),
        row(
            "comment.edit",
            json!({ "page": 2, "index": 1, "move": [5, 5] }),
            &[("page:2", G), ("comment:2/1", &["rect"])],
            &[("pdf", "pages"), ("page:2", "comments")],
            &[],
        ),
        row(
            "comment.delete",
            json!({ "page": 3, "index": 1 }),
            &[("page:3", STAR), ("comment:3/1", STAR)],
            &[("pdf", "pages"), ("page:3", "comments")],
            &[],
        ),
        // comment.hide: a view setting of the session (engine set_hide_comments); the file does
        // not change. was: the whole document.
        row("comment.hide", json!({ "hidden": true }), &[], &[], &[]),
        // ---- Measuring (measure.rs): points through the view matrix; the scale of the page's
        // viewport (/VP) at the first point. was: the geometry only.
        row("measure.distance", json!({ "page": 2, "points": [[10, 10], [50, 50]] }), &[("page:2", G)], &[("pdf", "pages")], &["viewports"]),
        row(
            "measure.perimeter",
            json!({ "page": 2, "points": [[10, 10], [50, 50], [10, 50]] }),
            &[("page:2", G)],
            &[("pdf", "pages")],
            &["viewports"],
        ),
        row("measure.area", json!({ "page": 2, "points": [[10, 10], [50, 50], [10, 50]] }), &[("page:2", G)], &[("pdf", "pages")], &["viewports"]),
        // measure.setScale: the page's size for the default viewport; appends to /VP.
        row("measure.setScale", json!({ "page": 1, "points": [[0, 0], [72, 0]], "distance": 1 }), &[("page:1", G)], &[("pdf", "pages")], &[]),
        // ---- Fill & Sign: `at` through crop and rotation; the date format is a preference.
        row("fillSign.add", json!({ "page": 1, "type": "text", "at": [100, 500], "text": "x" }), &[("page:1", G)], &[("pdf", "pages")], &[]),
        // fillSign.setDateFormat: the session's preference only. was: the whole document.
        row("fillSign.setDateFormat", json!({ "format": "dd.mm.yyyy" }), &[], &[], &[]),
        // ---- Redaction (redact.rs).
        // `rect` on `page` (`pages` not read): the content it covers (item 5). was: geometry only.
        row("redaction.mark", json!({ "page": 1, "rect": [10, 10, 50, 50] }), &[("page:1", G)], &[("pdf", "pages")], &["covered"]),
        // `find` without `pages`: every page's text. was: the whole document.
        row("redaction.mark", json!({ "find": "Page" }), &pages(STAR), &[("pdf", "pages")], &["text"]),
        // `wholePages`: each page's crop box. was: the page only, no geometry.
        row("redaction.mark", json!({ "wholePages": true, "pages": [2] }), &[("page:2", G)], &[("pdf", "pages")], &[]),
        // redaction.apply: every mark, the pages' content, annotations and fields under the marks; a
        // full rewrite.
        row("redaction.apply", json!({}), &[], &[], &["redaction-inputs"]),
        // redaction.clear: every redaction mark (redact clear_marks). was: the whole document.
        row("redaction.clear", json!({}), &[], &[], &["redactions"]),
        // ---- Form fields (forms.rs; forms crate).
        // form.fill: `value_for` reads the type; write_value reads the flags, maximum length,
        // choices, the check boxes' on states and runs format and validation scripts.
        // was: value, type and widget states.
        row(
            "form.fill",
            json!({ "values": { "F2": "x" } }),
            &[("field:F2", &["value", "type", "maxLength", "actions", "appearance", "align", "name"])],
            &[],
            &["calculations", "fill-inputs"],
        ),
        // forms reset: each field's /V and /DV. was: no default.
        row("form.reset", json!({ "fields": ["F1"] }), &[("field:F1", &["value", "default", "widgets", "name"])], &[], &["calculations"]),
        // No `fields`: every field. was: the whole document.
        row("form.reset", json!({}), &fields3(&["value", "default", "widgets", "name"]), &[("pdf", "fields")], &["calculations"]),
        row("form.setProps", json!({ "field": "F3", "tooltip": "t" }), &[("field:F3", &["tooltip", "name"])], &[], &[]),
        row("form.setProps", json!({ "field": "F1", "required": true }), &[("field:F1", &["flags", "name"])], &[], &[]),
        row("form.setProps", json!({ "field": "F2", "maxLength": 5 }), &[("field:F2", &["maxLength", "name"])], &[], &[]),
        // `rect`: in the coordinates of the page of the field's first widget.
        row("form.setProps", json!({ "field": "F1", "rect": [72, 600, 320, 624] }), &[("field:F1", &["widgets", "name"]), ("page:1", G)], &[], &[]),
        // A rename: the new name free (set_props checks it against every name).
        row("form.setProps", json!({ "field": "F1", "name": "G1" }), &[("field:F1", &["name"])], &[], &["field"]),
        row("form.deleteField", json!({ "field": "F1" }), &[("field:F1", STAR)], &[], &[]),
        row("form.setScript", json!({ "field": "F1", "event": "calculate", "script": "1" }), &[("field:F1", &["actions", "name"])], &[], &[]),
        // An action going to a page names it by number (forms set_field_actions). was: the field only.
        row(
            "form.setActions",
            json!({ "field": "F1", "actions": [{ "trigger": "mouse_up", "page": 2 }] }),
            &[("field:F1", &["actions", "name"]), ("page:2", STAR)],
            &[("pdf", "pages")],
            &[],
        ),
        // form.addField: `rect` through crop and rotation; a name given: free.
        row(
            "form.addField",
            json!({ "page": 1, "type": "text", "rect": [72, 400, 300, 424], "name": "New" }),
            &[("page:1", G)],
            &[("pdf", "pages")],
            &["field"],
        ),
        // No name: `free_name` makes one free of every field's name. was: nothing for the name.
        row("form.addField", json!({ "page": 1, "type": "text", "rect": [72, 400, 300, 424] }), &[("page:1", G)], &[("pdf", "pages")], &["names"]),
        // `group` names an existing radio field; existing widget geometry is preserved.
        row(
            "form.addField",
            json!({ "page": 2, "type": "radio", "rect": [72, 400, 90, 418], "group": "R", "export": "Two" }),
            &[("page:2", G), ("field:R", &["type", "name"])],
            &[("pdf", "pages")],
            &["field"],
        ),
        // Same-field rule: hold the replaced /Tabs, without widget geometry.
        row("form.setTabOrder", json!({ "order": "row" }), &pages(STAR), &[("pdf", "pages")], &["tab-mode"]),
        // With `field` (`pages` not read): the order of the widgets on the field's page.
        // was: every page (it holds `pages` when none) and the field, not the order there.
        row("form.setTabOrder", json!({ "field": "F1", "move": "later" }), &[("field:F1", &["name"])], &[], &["tabs"]),
        // form.detectFields: the page's content, its widgets' places, every name, the geometry
        // the words go through. was: the page's content only.
        row(
            "form.detectFields",
            json!({ "pages": [2] }),
            &[("page:2", G), ("field:F2", &["widgets", "name"])],
            &[("pdf", "pages")],
            &["content", "names"],
        ),
        // ---- Scripts.
        // script.run: every field's state, the document's scripts, /Info, the number of pages; it
        // sets fields only. was: the whole document.
        row("script.run", json!({ "script": "1" }), &fields3(STAR), &[("pdf", "fields"), ("pdf", "pages")], &["scripts", "info"]),
        // script.set: /Names /JavaScript only. was: the whole document.
        row("script.set", json!({ "name": "init", "script": "1" }), &[], &[], &["scripts"]),
        // script.enabled: a session preference. was: the whole document.
        row("script.enabled", json!({ "enabled": true }), &[], &[], &[]),
        // ---- Links (links.rs; annot links): `index` is the place in /Annots, as link.list numbers it.
        row("link.add", json!({ "page": 1, "rect": [1, 1, 20, 20], "toPage": 2 }), &[("page:1", G), ("page:2", STAR)], &[("pdf", "pages")], &[]),
        // link.edit replaces the action given; was: the whole link entry as a fact.
        row(
            "link.edit",
            json!({ "page": 1, "index": l1, "url": "https://example.com/z" }),
            &[("page:1", STAR), (&format!("link:1/{l1}"), &["url", "toPage", "other"]), ("comment:1/1", STAR)],
            &[("pdf", "pages"), ("page:1", "links")],
            &[],
        ),
        // A style edit starts from the style it reads.
        row(
            "link.edit",
            json!({ "page": 2, "index": l2, "color": "red" }),
            &[("page:2", STAR), (&format!("link:2/{l2}"), &["visible", "color", "width", "highlight"]), ("comment:2/1", STAR)],
            &[("pdf", "pages"), ("page:2", "links")],
            &[],
        ),
        // `rect` through crop and rotation. was: no geometry.
        row(
            "link.edit",
            json!({ "page": 1, "index": l1, "rect": [10, 10, 60, 30] }),
            &[("page:1", G), (&format!("link:1/{l1}"), &["rect"]), ("comment:1/1", STAR)],
            &[("pdf", "pages"), ("page:1", "links")],
            &[],
        ),
        row(
            "link.delete",
            json!({ "page": 2, "index": l2 }),
            &[("page:2", STAR), (&format!("link:2/{l2}"), STAR), ("comment:2/1", STAR)],
            &[("pdf", "pages"), ("page:2", "links")],
            &[],
        ),
        // link.addFromUrls: every page's text, and the links there (an address over one is
        // skipped). was: the whole document.
        row("link.addFromUrls", json!({}), &[], &[], &["urls"]),
        // link.removeAll: every link wherever it is; nothing it reads decides what it writes.
        // was: the whole document.
        row("link.removeAll", json!({}), &[], &[], &[]),
        // ---- Page content (content.rs; edit added, text, images, objects): items by place.
        // content.set merges the parameters over the item's content and style.
        row("content.set", json!({ "page": 1, "index": 1, "text": "y" }), &[("page:1", STAR)], &[("pdf", "pages")], &["entry"]),
        // `rect` flipped through the displayed height (content.rs to_display). was: no geometry.
        row("content.set", json!({ "page": 1, "index": 1, "rect": [72, 100, 300, 140] }), &[("page:1", G)], &[("pdf", "pages")], &["entry"]),
        row("content.delete", json!({ "page": 2, "index": 1 }), &[("page:2", STAR)], &[("pdf", "pages")], &["entry"]),
        // object.move: the offset through the page's rotation (engine view_vector). was: no geometry.
        row(
            "object.move",
            json!({ "page": 1, "objects": [{ "index": 1, "kind": "added" }], "offset": [5, 5] }),
            &[("page:1", G)],
            &[("pdf", "pages")],
            &["entry"],
        ),
        row("image.edit", json!({ "page": 1, "image": 1, "action": "rotate" }), &[("page:1", G)], &[("pdf", "pages")], &["entry"]),
        // text.edit: the line, and the text overlapping it, which the edit takes out
        // (coincident_ops). was: the line only.
        row("text.edit", json!({ "page": 1, "line": 1, "text": "z" }), &[("page:1", STAR)], &[("pdf", "pages")], &["entry", "text-overlap"]),
        // A paragraph without `width` wraps at the crop box.
        row("text.edit", json!({ "page": 1, "paragraph": 1, "text": "z" }), &[("page:1", G)], &[("pdf", "pages")], &["entry", "text-overlap"]),
        // ---- Accessibility (a11y.rs; engine accessibility_fix).
        // `figure` is the structure element's object number (accessibility.listFigures' `figure`),
        // not a place in the list. was: the n-th entry of the list.
        row("accessibility.setAlt", json!({ "figure": 1, "alt": "a" }), &[], &[], &["entry"]),
        // primary-language: /Lang, rewriting the whole initial view it reads. was: the whole document.
        row("accessibility.fix", json!({ "rule": "primary-language", "value": "en" }), &[], &[], &["view"]),
        row("accessibility.fix", json!({ "rule": "title", "value": "T" }), &[], &[], &["view", "info"]),
        // tab-order: /Tabs on every page.
        row("accessibility.fix", json!({ "rule": "tab-order" }), &pages(STAR), &[("pdf", "pages")], &[]),
        // ---- Document settings.
        // file.setInitialView: the initial view it reads and rewrites; `page` by number to a page
        // object. was: the whole document.
        row("file.setInitialView", json!({ "layout": "single" }), &[], &[], &["view"]),
        row("file.setInitialView", json!({ "page": 2 }), &[("page:2", STAR)], &[("pdf", "pages")], &["view"]),
        // file.listRevisions: reads the revisions; writes nothing. was: the whole document.
        row("file.listRevisions", json!({}), &[], &[], &[]),
        // file.new: makes another document; this one does not change. was: the whole document.
        row("file.new", json!({ "from": "blank" }), &[], &[], &[]),
        // OCR: skip textual pages, otherwise consume raster content and geometry.
        row("ocr.recognize", json!({ "pages": [2] }), &[("page:2", STAR)], &[("pdf", "pages")], &["ocr-inputs"]),
        row("ocr.recognize", json!({}), &pages(STAR), &[("pdf", "pages")], &["ocr-inputs"]),
        // ---- The whole document: full rewrites, every comment or field, another document, history.
        row("file.protect", json!({ "openPassword": "x" }), &[], &[], &["all"]),
        row("file.unprotect", json!({}), &[], &[], &["all"]),
        row("file.removeHidden", json!({}), &[], &[], &["all"]),
        row("pdfa.convert", json!({}), &[], &[], &["all"]),
        row("file.compareMark", json!({ "other": 2 }), &[], &[], &["all"]),
        row("file.flatten", json!({}), &[], &[], &["flatten"]),
        row("edit.undo", json!({}), &[], &[], &["all"]),
        row("edit.redo", json!({}), &[], &[], &["all"]),
    ];
    // Hold a page with only its place: `*`.
    for row in &mut rows {
        for (_, fields) in &mut row.holds {
            if fields.is_empty() {
                fields.push("*");
            }
        }
    }
    rows
}

/// What `under` declares, in the words of [`Row`].
/// Objects with their fields held, windows (holder, list), kinds of facts.
type Holds = (BTreeMap<String, BTreeSet<String>>, BTreeSet<(String, String)>, BTreeSet<String>);

fn as_row(held: &Under, objects: &Projection) -> Holds {
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
        if let Some(field) = objects.fields.iter().find(|field| field_tool_id(&field.id) == id) {
            return format!("field:{}", field.name);
        }
        for bookmark in objects.all_bookmarks() {
            if bookmark_tool_id(&bookmark.id) == id {
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
    let windows = held.within.iter().map(|w| (name_of(w.parent.as_str().unwrap_or_default()), w.field.clone())).collect();
    let kinds = held
        .content
        .iter()
        .flat_map(|content| content.region["facts"].as_array().cloned().unwrap_or_default())
        .filter_map(|fact| fact[0].as_str().map(str::to_owned))
        .collect();
    (holds, windows, kinds)
}

/// Every write command has at least one reading, and every reading agrees with the declaration.
/// Prints each difference and the error rate.
#[test]
fn every_write_command_is_declared_as_its_code_reads() {
    let mut doc = fixture();
    let objects = doc.objects();
    let rows = readings(&mut doc);
    let writes: BTreeSet<String> = doc.commands().into_iter().filter(|command| command.mutates).map(|command| command.id).collect();
    let read: BTreeSet<String> = rows.iter().map(|row| row.command.to_owned()).collect();
    let unread: Vec<&String> = writes.difference(&read).collect();
    let unknown: Vec<&String> = read.difference(&writes).collect();
    println!("write commands: {}; read: {}", writes.len(), writes.intersection(&read).count());
    assert!(unread.is_empty() && unknown.is_empty(), "unread {unread:?}, not commands {unknown:?}");
    let mut wrong = Vec::new();
    let mut wrong_commands = BTreeSet::new();
    for row in &rows {
        let (holds, windows, kinds) = as_row(&under(row.command, &row.params, &objects), &objects);
        let want_holds: BTreeMap<String, BTreeSet<String>> =
            row.holds.iter().map(|(id, fields)| (id.clone(), fields.iter().map(|f| (*f).to_owned()).collect())).collect();
        let want_windows: BTreeSet<(String, String)> = row.windows.iter().map(|(holder, list)| (holder.clone(), (*list).to_owned())).collect();
        let want_kinds: BTreeSet<String> = row.facts.iter().map(|k| (*k).to_owned()).collect();
        if holds != want_holds || windows != want_windows || kinds != want_kinds {
            wrong_commands.insert(row.command);
            wrong.push(format!(
                "{} {}:\n  read     {want_holds:?} {want_windows:?} {want_kinds:?}\n  declared {holds:?} {windows:?} {kinds:?}",
                row.command, row.params
            ));
        }
    }
    for line in &wrong {
        println!("DIFFERS {line}");
    }
    println!(
        "declarations that differ from the readings: {} of {} rows, {} of {} commands",
        wrong.len(),
        rows.len(),
        wrong_commands.len(),
        writes.len()
    );
    assert!(wrong.is_empty(), "{} rows differ", wrong.len());
}
