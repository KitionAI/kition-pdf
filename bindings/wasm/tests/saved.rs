//! In memory == saved (task card C8-land-14-pdf, item 5): after every representative write, on
//! every fixture file it applies to, the document the kit holds is the document its saved file
//! opens as: the same `~state` (the fact the premise guard compares, so a basis names exactly the
//! saved file) and the same answers of the kit's readers (comments, fields, bookmarks, links,
//! content, pages).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::path::Path;

use kition_kit::KitDocument;
use kition_pdf_kit::PdfDocument;
use serde_json::{Value, json};

const READERS: &[&str] = &["comment.list", "form.listFields", "bookmark.list", "link.list", "content.list", "accessibility.listFigures"];

/// The fixture files: the conformance files that open and can be edited, and a built one with
/// comments, fields, bookmarks and links on three pages.
fn fixtures() -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance"));
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "pdf"))
        .collect();
    names.sort();
    for path in names {
        let bytes = std::fs::read(&path).unwrap();
        if PdfDocument::open("f.pdf", &bytes).is_ok_and(|doc| doc.inspect().is_ok_and(|seen| seen["editable"] == true)) {
            out.push((path.file_name().unwrap().to_string_lossy().into_owned(), bytes));
        }
    }
    let mut doc = PdfDocument::create("built.pdf", 612, 792).unwrap();
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
    out.push(("built".into(), doc.save(None).unwrap()));
    out
}

/// Representative writes: a property, a move, a structural write, a creation of each kind, a value.
fn writes() -> Vec<(&'static str, Value)> {
    vec![
        ("comment.add", json!({ "page": 1, "type": "note", "at": [123.456, 77.7], "contents": "added" })),
        ("comment.edit", json!({ "page": 1, "index": 1, "contents": "edited", "color": "blue" })),
        ("comment.edit", json!({ "page": 1, "index": 1, "move": [33.333, 44.444] })),
        ("comment.setStatus", json!({ "page": 1, "index": 1, "status": "accepted" })),
        ("form.fill", json!({ "values": { "F1": "filled" } })),
        ("form.setProps", json!({ "field": "F1", "tooltip": "tip", "required": true })),
        ("page.rotate", json!({ "pages": [1], "degrees": 90 })),
        ("page.insertBlank", json!({ "at": 1 })),
        ("page.move", json!({ "pages": [1], "to": 2 })),
        ("page.delete", json!({ "pages": [1] })),
        ("page.addText", json!({ "page": 1, "at": [72.25, 300.125], "text": "text" })),
        ("bookmark.add", json!({ "page": 1, "title": "Added", "position": 1 })),
        ("link.add", json!({ "page": 1, "rect": [10.1, 10.2, 50.3, 30.4], "url": "https://example.com/x" })),
        ("page.setBox", json!({ "pages": [1], "box": "crop", "margins": [10.5, 10.5, 10.5, 10.5] })),
        ("content.set", json!({ "page": 1, "index": 1, "text": "updated" })),
        ("page.number", json!({ "from": 1, "to": 1, "style": "upper-roman" })),
    ]
}

fn reads(doc: &mut PdfDocument) -> Value {
    let mut out = serde_json::Map::new();
    for reader in READERS {
        let mut answer = doc.execute(reader, json!({})).unwrap_or_else(|error| json!({ "error": error }));
        // When a comment changed is the clock's, not the file's: compare it as stored.
        if let Some(comments) = answer["comments"].as_array_mut() {
            for comment in comments {
                comment.as_object_mut().unwrap().remove("modified");
            }
        }
        out.insert((*reader).to_owned(), answer);
    }
    let pages = doc.execute("file.inspect", json!({})).unwrap()["pages"].clone();
    out.insert("pages".into(), pages);
    Value::Object(out)
}

#[test]
fn every_write_on_every_fixture_is_what_its_saved_file_opens_as() {
    let mut problems = Vec::new();
    let mut ran = 0;
    for (name, bytes) in fixtures() {
        for (command, params) in writes() {
            let mut doc = PdfDocument::open("f.pdf", &bytes).unwrap();
            if doc.execute(command, params.clone()).is_err() {
                // Not applicable to this file (no comment, no field F1, one page to delete).
                continue;
            }
            ran += 1;
            let state = doc.derived(&[], &[]).unwrap();
            let held = reads(&mut doc);
            let saved = doc.save(None).unwrap();
            let mut reopened = PdfDocument::open("f.pdf", &saved).unwrap();
            if reopened.derived(&[], &[]).unwrap() != state {
                problems.push(format!("{name} {command} {params}: ~state in memory differs from the saved file's"));
            }
            let read = reads(&mut reopened);
            if read != held {
                for (key, value) in held.as_object().unwrap() {
                    if read[key] != *value {
                        problems.push(format!("{name} {command} {params}: {key}: in memory {value} != saved {}", read[key]));
                    }
                }
            }
        }
    }
    println!("in memory == saved: {ran} writes run");
    assert!(ran > 40, "{ran}");
    assert!(problems.is_empty(), "{problems:#?}");
}

/// The typed objects a replica holds, by tool id: each one's fields (lists left out) and where it
/// is (its holder's tool id and its place there).
fn replica_objects(snapshot: &[u8]) -> std::collections::BTreeMap<String, Value> {
    use kition_kit::session::KitModel;
    let replica = kition_kit::kdm::Replica::load(kdm_model::PdfModel::schema(), snapshot, kition_kit::kdm::ActorId::random()).unwrap();
    let mut out = std::collections::BTreeMap::new();
    let fields_of = |id: &kition_kit::kdm::ObjectId, ty: &str| -> serde_json::Map<String, Value> {
        let def = &replica.schema().types[ty];
        def.fields
            .iter()
            .filter(|(_, field)| !field.kind.is_container())
            .map(|(name, _)| (name.clone(), replica.get(id, name).unwrap_or(Value::Null)))
            .collect()
    };
    let root = kdm_model::root_id();
    for (at, page) in replica.list(&root, "pages").unwrap().iter().enumerate() {
        let tool = format!("page:{}", replica.get(page, "id").unwrap().as_str().unwrap());
        out.insert(tool.clone(), json!({ "fields": fields_of(page, "Page"), "at": ["pdf", at] }));
        for (place, comment) in replica.list(page, "comments").unwrap().iter().enumerate() {
            let id = replica.get(comment, "id").unwrap().as_str().unwrap().to_owned();
            out.insert(id, json!({ "fields": fields_of(comment, "Comment"), "at": [tool, place] }));
        }
        for (place, link) in replica.list(page, "links").unwrap().iter().enumerate() {
            let id = replica.get(link, "id").unwrap().as_str().unwrap().to_owned();
            out.insert(id, json!({ "fields": fields_of(link, "Link"), "at": [tool, "links", place] }));
        }
    }
    for (at, field) in replica.list(&root, "fields").unwrap().iter().enumerate() {
        let tool = format!("field:{}", replica.get(field, "id").unwrap().as_str().unwrap());
        out.insert(tool, json!({ "fields": fields_of(field, "Field"), "at": ["pdf", at] }));
    }
    let mut levels = vec![(root.clone(), "pdf".to_owned(), "bookmarks")];
    while let Some((holder, holder_tool, list)) = levels.pop() {
        for (at, bookmark) in replica.list(&holder, list).unwrap().iter().enumerate() {
            let tool = format!("bookmark:{}", replica.get(bookmark, "id").unwrap().as_str().unwrap());
            out.insert(tool.clone(), json!({ "fields": fields_of(bookmark, "Bookmark"), "at": [holder_tool, at] }));
            levels.push((bookmark.clone(), tool, "children"));
        }
    }
    out
}

/// The typed objects a document reads as (its projection), in [`replica_objects`]' form.
fn projected(doc: &PdfDocument) -> std::collections::BTreeMap<String, Value> {
    use kition_pdf_engine::projection::{bookmark_tool_id, field_tool_id, page_tool_id};
    let objects = doc.objects();
    let mut out = std::collections::BTreeMap::new();
    for (at, page) in objects.pages.iter().enumerate() {
        let tool = page_tool_id(&page.id);
        out.insert(tool.clone(), json!({ "fields": page.fields, "at": ["pdf", at] }));
        for (place, comment) in page.comments.iter().enumerate() {
            out.insert(comment.id.clone(), json!({ "fields": comment.fields, "at": [tool, place] }));
        }
        for (place, link) in page.links.iter().enumerate() {
            out.insert(link.id.clone(), json!({ "fields": link.fields, "at": [tool, "links", place] }));
        }
    }
    for (at, field) in objects.fields.iter().enumerate() {
        out.insert(field_tool_id(&field.id), json!({ "fields": field.fields, "at": ["pdf", at] }));
    }
    let mut levels: Vec<(String, &[kition_pdf_engine::projection::BookmarkView])> = vec![("pdf".to_owned(), &objects.bookmarks)];
    while let Some((holder, list)) = levels.pop() {
        for (at, bookmark) in list.iter().enumerate() {
            let tool = bookmark_tool_id(&bookmark.id);
            out.insert(tool.clone(), json!({ "fields": bookmark.fields, "at": [holder, at] }));
            levels.push((tool, &bookmark.children));
        }
    }
    out
}

/// In memory == saved over receipts (task card C8-pdf-2, item 4): after every write family on
/// every editable fixture file, the typed objects the replica holds (what receipts are told from)
/// are, object for object and field for field, the objects a reread of the saved file projects;
/// and every object the write's receipt names reads in the saved file as the receipt says.
#[test]
fn every_write_on_every_fixture_receipts_what_its_saved_file_projects() {
    let mut problems = Vec::new();
    let mut ran = 0;
    let mut told = 0;
    for (name, bytes) in fixtures() {
        for (command, params) in writes() {
            let mut doc = PdfDocument::open("f.pdf", &bytes).unwrap();
            let before = replica_objects(&doc.snapshot().unwrap());
            if doc.execute(command, params.clone()).is_err() {
                continue;
            }
            ran += 1;
            let held = replica_objects(&doc.snapshot().unwrap());
            let saved = doc.save(None).unwrap();
            let reread = projected(&PdfDocument::open("f.pdf", &saved).unwrap());
            // The receipt: the objects whose fields or place changed, created or deleted.
            let receipt: Vec<&String> = held.keys().chain(before.keys()).filter(|id| held.get(*id) != before.get(*id)).collect();
            told += receipt.len();
            for id in receipt {
                if held.get(id) != reread.get(id) {
                    problems.push(format!(
                        "{name} {command} {params}: receipt says {id} is {:?}, the saved file reads {:?}",
                        held.get(id),
                        reread.get(id)
                    ));
                }
            }
            if held != reread {
                let ids: Vec<&String> = held.keys().chain(reread.keys()).filter(|id| held.get(*id) != reread.get(*id)).collect();
                problems.push(format!("{name} {command} {params}: the replica's objects differ from the saved file's: {ids:?}"));
            }
        }
    }
    println!("receipts == saved: {ran} writes run, {told} objects told");
    assert!(ran > 40, "{ran}");
    assert!(problems.is_empty(), "{problems:#?}");
}
