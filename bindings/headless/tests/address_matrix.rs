//! C8-pdf-4: necessity expectations are declared before the first matrix execution.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use common::*;
use kition_kit::KitDocument;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug)]
enum Writer {
    Engine,
    Lopdf,
}

fn with_links() -> Vec<u8> {
    let mut doc = fixture_doc(4);
    for page in 1..=4 {
        doc.execute("link_add", json!({"page": page, "rect": [100, 300, 200, 320], "url": format!("https://example.com/{page}")})).unwrap();
    }
    doc.save(None).unwrap()
}

fn annotation(bytes: &[u8], writer: Writer, subtype: &str, remove: bool, at: usize) -> Vec<u8> {
    match writer {
        Writer::Engine => {
            use kition_pdf_cos::{Dict, Document, Object};
            let mut doc = Document::open(std::sync::Arc::new(bytes.to_vec())).unwrap();
            let page = kition_pdf_organize::pages(&doc).unwrap()[1].obj;
            let mut list = doc.resolve(doc.get(page).as_dict().unwrap().get(b"Annots").unwrap()).as_array().unwrap().clone();
            if remove {
                list.remove(at);
            } else {
                let mut a = Dict::new();
                a.set(b"Type".to_vec(), Object::name("Annot"));
                a.set(b"Subtype".to_vec(), Object::name(subtype));
                a.set(b"Rect".to_vec(), Object::Array(vec![10.into(), 10.into(), 30.into(), 30.into()]));
                let a = doc.add(a);
                list.insert(at, Object::Ref(a));
            }
            doc.update_dict(page, |d| d.set(b"Annots".to_vec(), Object::Array(list))).unwrap();
            kition_pdf_cos::write_incremental(&doc, &Default::default()).unwrap()
        }
        Writer::Lopdf => {
            use lopdf::{Dictionary, Object};
            let prev = lopdf::Document::load_mem(bytes).unwrap();
            let page = prev.get_pages()[&2];
            let mut list = prev.get_dictionary(page).unwrap().get(b"Annots").unwrap().as_array().unwrap().clone();
            let mut update = lopdf::IncrementalDocument::create_from(bytes.to_vec(), prev);
            update.opt_clone_object_to_new_document(page).unwrap();
            if remove {
                list.remove(at);
            } else {
                let mut a = Dictionary::new();
                a.set("Type", Object::Name(b"Annot".to_vec()));
                a.set("Subtype", Object::Name(subtype.as_bytes().to_vec()));
                a.set("Rect", Object::Array(vec![10.into(), 10.into(), 30.into(), 30.into()]));
                let a = update.new_document.add_object(a);
                list.insert(at, Object::Reference(a));
            }
            update.new_document.get_dictionary_mut(page).unwrap().set("Annots", Object::Array(list));
            let mut out = Vec::new();
            update.save_to(&mut out).unwrap();
            assert!(out.starts_with(bytes));
            out
        }
    }
}

fn normalize(value: &mut Value) {
    match value {
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

fn reads(doc: &mut kition_pdf_kit::PdfDocument) -> Value {
    let mut out = serde_json::Map::new();
    for command in ["comment_list", "form_fields", "bookmark_list", "link_list", "content_list", "text_extract", "doc_inspect"] {
        let mut value = doc.execute(command, json!({})).unwrap();
        if command == "doc_inspect" {
            value = value["pages"].clone();
        }
        normalize(&mut value);
        out.insert(command.to_owned(), value);
    }
    Value::Object(out)
}

#[tokio::test(flavor = "multi_thread")]
async fn annotation_slots_include_popups_and_widgets() {
    // A positional index names its observed occupant. Adding/removing before it must refuse
    // relocated; adding/removing after it must apply and preserve the complete saved merge.
    let mut failures = Vec::new();
    let mut cells = 0;
    for writer in [Writer::Engine, Writer::Lopdf] {
        for subtype in ["Popup", "Widget"] {
            for link in [false, true] {
                for before in [false, true] {
                    for remove in [false, true] {
                        let base = with_links();
                        let cos = kition_pdf_cos::Document::open(std::sync::Arc::new(base.clone())).unwrap();
                        let page = kition_pdf_organize::pages(&cos).unwrap()[1].obj;
                        let count = cos.resolve(cos.get(page).as_dict().unwrap().get(b"Annots").unwrap()).as_array().unwrap().len();
                        let slot = if before { 0 } else { count };
                        let initial = if remove { annotation(&base, writer, subtype, false, slot) } else { base };
                        let f = Fixture::from_bytes(&initial, Default::default());
                        let index = if link {
                            let links = read_on_disk(&f.file(), "link_list", json!({}));
                            links["links"].as_array().unwrap().iter().find(|l| l["page"] == 2).unwrap()["index"].clone()
                        } else {
                            read_on_disk(&f.file(), "comment_list", json!({"page": 2}))["comments"][0]["index"].clone()
                        };
                        let mine = if link {
                            step("link_edit", json!({"page": 2, "index": index, "url": "https://example.com/mine"}))
                        } else {
                            step("comment_edit", json!({"page": 2, "index": index, "contents": "mine"}))
                        };
                        let (_, basis) = f.inspect().await;
                        let theirs = annotation(&initial, writer, subtype, remove, slot);
                        std::fs::write(f.file(), &theirs).unwrap();
                        let got = f.call("run", json!({"basis": basis, "steps": [mine.clone()]})).await;
                        let label = format!("{writer:?}/{subtype}/link={link}/before={before}/remove={remove}");
                        cells += 1;
                        if before {
                            match got {
                                Err(refused) if reasons(&refused).split('+').all(|r| r == "relocated") && !reasons(&refused).is_empty() => {}
                                other => failures.push(format!("{label}: expected relocated, got {other:?}")),
                            }
                            assert_eq!(std::fs::read(f.file()).unwrap(), theirs, "{label}: refusal preserves bytes");
                        } else if let Err(error) = got {
                            failures.push(format!("{label}: false refusal/command error {error}"));
                        } else {
                            let mut expected = kition_pdf_kit::PdfDocument::open(FILE, &theirs).unwrap();
                            expected.execute(mine["command"].as_str().unwrap(), mine["params"].clone()).unwrap();
                            let mut expected = kition_pdf_kit::PdfDocument::open(FILE, &expected.save(None).unwrap()).unwrap();
                            assert_eq!(reads(&mut on_disk(&f.file())), reads(&mut expected), "{label}: saved merge");
                            let slots = |bytes: Vec<u8>| {
                                let cos = kition_pdf_cos::Document::open(std::sync::Arc::new(bytes)).unwrap();
                                let page = kition_pdf_organize::pages(&cos).unwrap()[1].obj;
                                let list = cos.resolve(cos.get(page).as_dict().unwrap().get(b"Annots").unwrap());
                                list.as_array()
                                    .unwrap()
                                    .iter()
                                    .map(|a| String::from_utf8_lossy(cos.resolve(a).as_dict().unwrap().name(b"Subtype").unwrap()).into_owned())
                                    .collect::<Vec<_>>()
                            };
                            assert_eq!(slots(std::fs::read(f.file()).unwrap()), slots(theirs), "{label}: preserve invisible annotation slots");
                        }
                        f.shutdown().await;
                    }
                }
            }
        }
    }
    println!("annotation slots: {cells} cells, {} failures", failures.len());
    assert!(failures.is_empty(), "{failures:#?}");
}

fn nested_fields() -> Vec<u8> {
    use kition_pdf_cos::{Dict, Document, Object, PdfString};
    let mut cos = Document::open(std::sync::Arc::new(with_links())).unwrap();
    let fields = kition_pdf_forms::fields(&cos);
    let nested: Vec<_> = fields.iter().filter(|f| ["F2", "F3"].contains(&f.name.as_str())).collect();
    let mut group = Dict::new();
    group.set(b"T".to_vec(), PdfString::text("group"));
    group.set(b"Kids".to_vec(), Object::Array(nested.iter().map(|f| Object::Ref(f.obj)).collect()));
    let group = cos.add(group);
    for f in &nested {
        cos.update_dict(f.obj, |d| d.set(b"Parent".to_vec(), Object::Ref(group))).unwrap();
    }
    let af = cos.get(cos.root().unwrap()).as_dict().unwrap().reference(b"AcroForm").unwrap();
    let mut roots: Vec<_> = fields.iter().filter(|f| !["F2", "F3"].contains(&f.name.as_str())).map(|f| Object::Ref(f.obj)).collect();
    roots.push(Object::Ref(group));
    cos.update_dict(af, |d| d.set(b"Fields".to_vec(), Object::Array(roots))).unwrap();
    kition_pdf_cos::write_full(&cos, &Default::default()).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn nested_rename_holds_the_resulting_name() {
    // The collision is group.free, not the unrelated root name free. Expectations precede
    // the first execution: sibling takes group.free => changed; root takes free => apply.
    let mut failures = Vec::new();
    for sibling in [false, true] {
        let f = Fixture::from_bytes(&nested_fields(), Default::default());
        let (_, basis) = f.inspect().await;
        if sibling {
            f.colleague(&[("form_set_props", json!({"field": "group.F3", "name": "free"}))]);
        } else {
            f.colleague(&[("form_add_field", json!({"page": 4, "name": "free", "type": "text", "rect": [20, 20, 100, 40]}))]);
        }
        let theirs = std::fs::read(f.file()).unwrap();
        let mine = step("form_set_props", json!({"field": "group.F2", "name": "free"}));
        let got = f.call("run", json!({"basis": basis, "steps": [mine.clone()]})).await;
        if sibling {
            match got {
                Err(refused) if reasons(&refused) == "changed" => {}
                other => failures.push(format!("sibling: expected changed, got {other:?}")),
            }
            assert_eq!(std::fs::read(f.file()).unwrap(), theirs);
        } else if let Err(error) = got {
            failures.push(format!("unrelated root: expected saved merge, got {error}"));
        } else {
            let mut expected = kition_pdf_kit::PdfDocument::open(FILE, &theirs).unwrap();
            expected.execute("form_set_props", mine["params"].clone()).unwrap();
            let mut expected = kition_pdf_kit::PdfDocument::open(FILE, &expected.save(None).unwrap()).unwrap();
            assert_eq!(reads(&mut on_disk(&f.file())), reads(&mut expected));
        }
        f.shutdown().await;
    }
    println!("nested rename: 2 cells, {} failures", failures.len());
    assert!(failures.is_empty(), "{failures:#?}");
}
