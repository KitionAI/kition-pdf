//! Regression: `file.flatten` flattens only what it names (naming
//! comments leaves the form fields alone, and naming fields leaves the comments), and a push
//! button's caption takes the text colour, font and size its Appearance tab sets.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kition_kit::KitDocument;
use kition_pdf_kit::PdfDocument;
use serde_json::{Value, json};

fn run(doc: &mut PdfDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

/// A page with a text field, a button and a rectangle comment.
fn form() -> PdfDocument {
    let mut doc = PdfDocument::create("form.pdf", 612, 792).unwrap();
    run(&mut doc, "form.addField", json!({ "page": 1, "type": "text", "rect": [72, 100, 300, 124], "name": "member" }));
    run(&mut doc, "form.addField", json!({ "page": 1, "type": "button", "rect": [72, 200, 232, 240], "name": "send", "caption": "Send" }));
    run(&mut doc, "comment.add", json!({ "page": 1, "type": "rectangle", "rect": [72, 300, 300, 360] }));
    doc
}

fn counts(doc: &mut PdfDocument) -> (usize, usize) {
    let fields = run(doc, "form.listFields", json!({}))["fields"].as_array().map_or(0, Vec::len);
    let comments = run(doc, "comment.list", json!({}))["comments"].as_array().map_or(0, Vec::len);
    (fields, comments)
}

#[test]
fn flatten_flattens_only_what_it_names() {
    let mut doc = form();
    assert_eq!(counts(&mut doc), (2, 1));
    run(&mut doc, "file.flatten", json!({ "comments": true }));
    assert_eq!(counts(&mut doc), (2, 0), "naming comments keeps the fields");

    let mut doc = form();
    run(&mut doc, "file.flatten", json!({ "fields": true }));
    assert_eq!(counts(&mut doc), (0, 1), "naming fields keeps the comments");

    let mut doc = form();
    run(&mut doc, "file.flatten", json!({ "comments": false }));
    assert_eq!(counts(&mut doc), (0, 1), "comments: false flattens the fields");

    let mut doc = form();
    run(&mut doc, "file.flatten", json!({}));
    assert_eq!(counts(&mut doc), (0, 0), "naming neither flattens both");
}

/// The page's pixels (RGBA) and its width.
fn page(doc: &PdfDocument) -> (Vec<u8>, usize, f64, f64) {
    let pages = doc.inspect().unwrap()["pages"].as_array().unwrap().clone();
    let area = &pages[0]["area"];
    let (x, y) = (area["x"].as_f64().unwrap(), area["y"].as_f64().unwrap());
    let (w, h) = (area["width"].as_f64().unwrap() as u32, area["height"].as_f64().unwrap() as u32);
    (doc.render(x, y, 1.0, w, h).unwrap(), w as usize, f64::from(w) / 612.0, f64::from(h) / 792.0)
}

/// Pixels inside the button (top-left-origin points [72, 200, 232, 240]) that are near `rgb`.
fn button_pixels(doc: &PdfDocument, rgb: [u8; 3]) -> usize {
    let (px, w, kx, ky) = page(doc);
    let mut n = 0;
    for y in (200.0 * ky) as usize..(240.0 * ky) as usize {
        for x in (72.0 * kx) as usize..(232.0 * kx) as usize {
            let p = &px[(y * w + x) * 4..(y * w + x) * 4 + 3];
            if p.iter().zip(rgb).all(|(a, b)| a.abs_diff(b) < 40) {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn a_button_caption_takes_its_text_colour() {
    let mut doc = form();
    let black_before = button_pixels(&doc, [0, 0, 0]);
    assert!(black_before > 20, "the caption draws in black at first ({black_before})");
    run(&mut doc, "form.setProps", json!({ "field": "send", "appearance": { "fill": "#5645d4", "border": "none", "text_color": "#ffffff" } }));
    assert_eq!(button_pixels(&doc, [0, 0, 0]), 0, "no black caption is left");
    let white = button_pixels(&doc, [255, 255, 255]);
    assert!(white > 20, "the caption draws in white ({white})");
    // Saved and opened again, it still does.
    let mut again = PdfDocument::open("form.pdf", &doc.save(Some("form.pdf")).unwrap()).unwrap();
    assert!(button_pixels(&again, [255, 255, 255]) > 20);
    // Flattening comments keeps the button and its look.
    run(&mut again, "file.flatten", json!({ "comments": true }));
    assert_eq!(counts(&mut again).0, 2);
    assert!(button_pixels(&again, [255, 255, 255]) > 20);
}
