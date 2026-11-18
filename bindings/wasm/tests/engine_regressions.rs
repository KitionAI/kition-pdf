//! C8-pdf-4: synthetic engine regressions, exercised through the shipped tool table.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::sync::Arc;

use kition_kit::KitDocument;
use kition_pdf_cos::{Document, Object, PdfString};
use kition_pdf_kit::PdfDocument;
use serde_json::json;

fn hierarchical_form() -> PdfDocument {
    let mut pdf = PdfDocument::create("form.pdf", 612, 792).unwrap();
    for name in ["first", "second"] {
        pdf.execute("form_add_field", json!({"page": 1, "name": name, "type": "text", "rect": [10, 10, 100, 30]})).unwrap();
    }
    let mut cos = Document::open(Arc::new(pdf.save(None).unwrap())).unwrap();
    let fields = kition_pdf_forms::fields(&cos);
    let mut parent = kition_pdf_cos::Dict::new();
    parent.set(b"T".to_vec(), PdfString::text("group"));
    parent.set(b"Kids".to_vec(), Object::Array(fields.iter().map(|f| Object::Ref(f.obj)).collect()));
    let parent = cos.add(parent);
    for field in fields {
        cos.update_dict(field.obj, |d| d.set(b"Parent".to_vec(), Object::Ref(parent))).unwrap();
    }
    let root = cos.root().unwrap();
    let af = cos.get(root).as_dict().unwrap().reference(b"AcroForm").unwrap();
    cos.update_dict(af, |d| d.set(b"Fields".to_vec(), Object::Array(vec![Object::Ref(parent)]))).unwrap();
    PdfDocument::open("form.pdf", &kition_pdf_cos::write_full(&cos, &Default::default()).unwrap()).unwrap()
}

#[test]
fn rename_rejects_a_fully_qualified_sibling_collision() {
    let mut pdf = hierarchical_form();
    let before = pdf.execute("form_fields", json!({})).unwrap();
    let result = pdf.execute("form_set_props", json!({"field": "group.first", "name": "second"}));
    assert!(result.is_err(), "renaming group.first to second must not create two group.second fields: {result:?}");
    let mut reopened = PdfDocument::open("form.pdf", &pdf.save(None).unwrap()).unwrap();
    assert_eq!(reopened.execute("form_fields", json!({})).unwrap(), before);
}

#[test]
fn rename_allows_a_leaf_name_in_another_namespace() {
    let mut pdf = hierarchical_form();
    pdf.execute("form_delete_field", json!({"field": "group.second"})).unwrap();
    pdf.execute("form_add_field", json!({"page": 1, "name": "second", "type": "text", "rect": [10, 40, 100, 60]})).unwrap();
    pdf.execute("form_set_props", json!({"field": "group.first", "name": "second"})).unwrap();
    let mut reopened = PdfDocument::open("form.pdf", &pdf.save(None).unwrap()).unwrap();
    let fields = reopened.execute("form_fields", json!({})).unwrap();
    let names: Vec<_> = fields["fields"].as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["group.second", "second"]);
}

#[test]
fn delete_field_removes_only_its_calculation_order_entry() {
    for direct_form in [false, true] {
        for indirect_order in [false, true] {
            let mut pdf = hierarchical_form();
            for field in ["group.first", "group.second"] {
                pdf.execute("form_set_props", json!({"field": field, "calculate": {"notation": "1 + 2"}})).unwrap();
            }
            let mut cos = Document::open(Arc::new(pdf.save(None).unwrap())).unwrap();
            let root = cos.root().unwrap();
            let af = cos.get(root).as_dict().unwrap().reference(b"AcroForm").unwrap();
            if indirect_order {
                let order = cos.get(af).as_dict().unwrap().get(b"CO").unwrap().clone();
                let order = cos.add(order);
                cos.update_dict(af, |d| d.set(b"CO".to_vec(), Object::Ref(order))).unwrap();
            }
            if direct_form {
                let form = (*cos.get(af)).clone();
                cos.update_dict(root, |d| d.set(b"AcroForm".to_vec(), form)).unwrap();
            }
            let mut pdf = PdfDocument::open("form.pdf", &kition_pdf_cos::write_full(&cos, &Default::default()).unwrap()).unwrap();
            pdf.execute("form_delete_field", json!({"field": "group.first"})).unwrap();
            let saved = Document::open(Arc::new(pdf.save(None).unwrap())).unwrap();
            let catalog = saved.get(saved.root().unwrap());
            let form = saved.resolve(catalog.as_dict().unwrap().get(b"AcroForm").unwrap());
            let co = saved.resolve(form.as_dict().unwrap().get(b"CO").unwrap());
            let names: Vec<_> = co
                .as_array()
                .unwrap()
                .iter()
                .map(|o| saved.resolve(o).as_dict().unwrap().get(b"T").unwrap().as_string().unwrap().to_text())
                .collect();
            assert_eq!(names, ["second"], "direct form {direct_form}, indirect order {indirect_order}");
        }
    }
}
