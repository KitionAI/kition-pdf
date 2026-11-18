//! Regression `regressions/issue-japanese-faces-for-japanese-text`: the Japanese faces a build has
//! built in (kition-fonts, `KITION_FONTS_DIR`) draw only Japanese text. Han characters of other
//! text, such as "Café 中", are refused until the host adds a font for them, and then come from
//! that font, so Chinese is never drawn in Japanese glyph forms. Before, a build with a Japanese
//! face drew any Han character with it. A test binary of its own: added fonts are the process's.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::path::PathBuf;
use std::sync::Arc;

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

/// The FontName of every font descriptor in a saved PDF.
fn font_names(bytes: &[u8]) -> Vec<String> {
    let doc = kition_pdf_cos::Document::open(Arc::new(bytes.to_vec())).unwrap();
    doc.scan_objects()
        .filter_map(|(_, object)| {
            let dict = object.as_dict()?;
            let name = dict.name(b"FontName").filter(|_| dict.name(b"Type") == Some(b"FontDescriptor".as_slice()))?;
            Some(String::from_utf8_lossy(name).into_owned())
        })
        .collect()
}

fn add_text(doc: &mut PdfDocument, y: u32, text: &str) -> Result<(), String> {
    doc.execute("page_add_text", json!({ "page": 1, "at": [72, y], "width": 400, "text": text })).map(|_| ()).map_err(|e| e.to_string())
}

#[test]
fn built_in_japanese_faces_draw_only_japanese_text() {
    let japanese_built_in = kition_pdf_fonts::document_japanese_font().is_some();
    let mut doc = PdfDocument::create("notice.pdf", 612, 792).unwrap();
    // Han characters without kana are no Japanese text: refused until the host adds a font, with
    // or without a built-in Japanese face.
    let err = add_text(&mut doc, 72, "Café 中").unwrap_err();
    assert!(err.contains("U+4E2D"), "{err}");
    // Japanese text draws with the built-in face, and only when the build has one.
    let japanese = add_text(&mut doc, 120, "日本語のテキスト");
    assert_eq!(japanese.is_ok(), japanese_built_in, "{japanese:?}");

    // The host adds its Chinese font: "Café 中" now draws, from that font.
    let droid = kition_fonts().join("droid-sans-fallback/DroidSansFallbackFull.ttf");
    let bytes = std::fs::read(&droid).unwrap_or_else(|error| {
        panic!(
            "{}: {error}. This test needs Droid Sans Fallback from the public kition-fonts: clone \
             https://github.com/KitionAI/kition-fonts next to this repository, or set KITION_FONTS_DIR to a checkout",
            droid.display()
        )
    });
    assert!(add_font(&bytes) > 0);
    let mut chinese = PdfDocument::create("chinese.pdf", 612, 792).unwrap();
    add_text(&mut chinese, 72, "Café 中").unwrap();
    let names = font_names(&chinese.save(Some("chinese.pdf")).unwrap());
    assert!(names.iter().any(|n| n.starts_with("DroidSansFallback")), "{names:?}");
    let built_in: Vec<String> =
        kition_pdf_fonts::KITION_FONTS.iter().filter(|f| f.covers("Jpan")).map(|f| format!("{}-{}", f.family, f.style).replace(' ', "")).collect();
    assert!(!names.iter().any(|n| built_in.contains(n)), "{names:?} has a built-in Japanese face of {built_in:?}");

    // Japanese text still prefers the built-in Japanese face over the host's Chinese one.
    if japanese_built_in {
        let mut japanese = PdfDocument::create("japanese.pdf", 612, 792).unwrap();
        add_text(&mut japanese, 72, "日本語のテキスト").unwrap();
        let names = font_names(&japanese.save(Some("japanese.pdf")).unwrap());
        assert!(names.iter().any(|n| built_in.contains(n)), "{names:?}");
        assert!(!names.iter().any(|n| n.starts_with("DroidSansFallback")), "{names:?}");
    }
}
