//! Regression `regressions/issue-control-characters-from-faces`: a control character in text
//! added to a page (a carriage return) is refused, never drawn from a document face. Arimo, Lora
//! and Noto Sans Arabic map U+000D to a blank glyph, so before, a page drew "A\r\nB" with no
//! error once the host added Arimo (the item's own face) or a build had Noto Sans Arabic built in
//! (the fallback face). A test binary of its own: added fonts are the process's.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::path::PathBuf;

use kition_kit::KitDocument;
use kition_pdf_kit::{PdfDocument, add_font};
use serde_json::json;

/// The fonts of a kition-fonts checkout: `KITION_FONTS_DIR`, or the one next to the repository.
fn kition_fonts() -> PathBuf {
    std::env::var_os("KITION_FONTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../kition-fonts")))
        .join("fonts")
}

#[test]
fn control_characters_are_never_drawn_from_a_face() {
    // Both faces map U+000D. Noto Sans Arabic is already there when the build has it built in, so
    // its count may be 0.
    for file in ["arimo/Arimo-Variable.ttf", "noto-sans-arabic/NotoSansArabic-Variable.ttf"] {
        let path = kition_fonts().join(file);
        let bytes = std::fs::read(&path).unwrap_or_else(|error| {
            panic!(
                "{}: {error}. This test needs {file} from the public kition-fonts: clone \
                 https://github.com/KitionAI/kition-fonts next to this repository, or set KITION_FONTS_DIR to a checkout",
                path.display()
            )
        });
        add_font(&bytes);
    }
    let mut doc = PdfDocument::create("notice.pdf", 612, 792).unwrap();
    // Helvetica is drawn from Arimo (the item's own face); Courier has no face here, so a
    // character it lacks falls back to any face that has it.
    for font in ["helvetica", "courier"] {
        let refused = doc.execute("page_add_text", json!({ "page": 1, "at": [72, 72], "width": 400, "text": "A\r\nB", "font": font }));
        let err = refused.map(|_| ()).unwrap_err().to_string();
        assert!(err.contains("U+000D"), "{font}: {err}");
        doc.execute("page_add_text", json!({ "page": 1, "at": [72, 120], "width": 400, "text": "A\nB", "font": font })).unwrap();
    }
}
