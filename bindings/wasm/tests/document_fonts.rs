//! Regression: text added to a page uses the fonts the host adds
//! (`add_font`). Chinese and other text the standard fonts can't draw is drawn from a face that
//! has it, `font` names any added face, and a standard family is drawn from its metric-compatible
//! face when the host added one (Arimo, Tinos, Cousine), so the page the kit writes embeds every
//! font it uses. A test binary of its own: added fonts are the process's.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::path::PathBuf;

use kition_kit::KitDocument;
use kition_pdf_kit::{PdfDocument, add_font};
use serde_json::{Value, json};

/// The fonts of a kition-fonts checkout: `KITION_FONTS_DIR`, or the one next to the repository.
fn kition_fonts() -> PathBuf {
    std::env::var_os("KITION_FONTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../kition-fonts")))
        .join("fonts")
}

/// Adds a font of the public kition-fonts, the way a host does; a missing checkout fails the test
/// (the fonts are public, so it is never skipped).
fn add(file: &str) {
    let path = kition_fonts().join(file);
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "{}: {error}. This test needs {file} from the public kition-fonts: clone \
             https://github.com/KitionAI/kition-fonts next to this repository, or set KITION_FONTS_DIR to a checkout",
            path.display()
        )
    });
    assert!(add_font(&bytes) > 0, "{file} is added");
}

fn run(doc: &mut PdfDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

fn page_text(doc: &mut PdfDocument) -> String {
    let pages = run(doc, "text_extract", json!({}));
    pages["pages"][0]["text"].as_str().unwrap_or_default().to_owned()
}

#[test]
fn added_text_uses_the_hosts_fonts() {
    let mut doc = PdfDocument::create("notice.pdf", 612, 792).unwrap();
    // Before any font is added, Chinese is refused as before.
    let refused = doc.execute("page_add_text", json!({ "page": 1, "at": [72, 72], "text": "冬季开放时间" }));
    assert!(refused.is_err(), "{refused:?}");

    for file in [
        "droid-sans-fallback/DroidSansFallbackFull.ttf",
        "lora/Lora-Variable.ttf",
        "arimo/Arimo-Variable.ttf",
        "tinos/Tinos-Regular.ttf",
        "tinos/Tinos-Bold.ttf",
        "cousine/Cousine-Regular.ttf",
    ] {
        add(file);
    }
    // A background draws no text, so it brings no font.
    run(&mut doc, "doc_background", json!({ "color": "#fffdf7" }));
    run(&mut doc, "page_add_text", json!({ "page": 1, "at": [72, 72], "width": 460, "text": "图书馆冬季开放时间 Winter hours", "size": 20 }));
    run(
        &mut doc,
        "page_add_text",
        json!({ "page": 1, "at": [72, 120], "width": 460, "text": "Saltmarsh Branch Library", "font": "Lora", "size": 18 }),
    );
    run(&mut doc, "page_add_text", json!({ "page": 1, "at": [72, 160], "width": 460, "text": "Monday to Friday", "font": "times", "bold": true }));
    run(&mut doc, "page_add_text", json!({ "page": 1, "at": [72, 200], "width": 460, "text": "10:00 to 18:00", "font": "courier" }));
    run(&mut doc, "page_add_text", json!({ "page": 1, "at": [72, 240], "width": 460, "text": "Book returns stay open." }));

    // Every character can be copied and searched.
    let text = page_text(&mut doc);
    for expected in ["图书馆冬季开放时间", "Winter hours", "Saltmarsh Branch Library", "Monday to Friday", "10:00 to 18:00", "Book returns"]
    {
        assert!(text.contains(expected), "{expected:?} in {text:?}");
    }
    // The items keep their faces through an edit.
    let list = run(&mut doc, "content_list", json!({}));
    assert_eq!(list["items"][1]["font"], json!("Lora"), "{list}");
    run(&mut doc, "content_update", json!({ "page": 1, "index": 2, "text": "Saltmarsh Library" }));
    assert!(page_text(&mut doc).contains("Saltmarsh Library"));
    // The page embeds every font it uses.
    let unembedded: Vec<String> = doc.check().into_iter().filter(|d| d.code == "pdf.font-not-embedded").map(|d| d.message).collect();
    assert!(unembedded.is_empty(), "{unembedded:#?}");
    // Saved and opened again, the page reads the same.
    let bytes = doc.save(Some("notice.pdf")).unwrap();
    let mut again = PdfDocument::open("notice.pdf", &bytes).unwrap();
    assert!(page_text(&mut again).contains("图书馆冬季开放时间"));
}
