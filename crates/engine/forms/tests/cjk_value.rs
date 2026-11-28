//! Regression (`regressions/issue-hand-h5-cjk-field`): a Chinese value in a field whose font is
//! Helvetica showed on the page as `???`: the appearance wrote it in WinAnsi. Now the characters
//! the field's font can't draw come from a document face (here Droid Sans Fallback, added the way
//! the web editor's host adds it) as Type 3 glyphs of their outlines, with a ToUnicode map, and
//! the Latin text around them stays in the field's font. A test binary of its own: added faces are
//! the process's.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::path::PathBuf;
use std::sync::Arc;

use kition_pdf_cos::{Document, Object, SaveOptions, write_incremental};
use kition_pdf_forms::{FieldValue, fields, set_value};

/// Droid Sans Fallback from the public kition-fonts: `KITION_FONTS_DIR`, or the checkout next to
/// the repository. A missing checkout fails the test (the fonts are public, so it never skips).
fn add_chinese_face() {
    let dir = std::env::var_os("KITION_FONTS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../kition-fonts")));
    let path = dir.join("fonts/droid-sans-fallback/DroidSansFallbackFull.ttf");
    let bytes = std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "{}: {error}. This test needs Droid Sans Fallback from the public kition-fonts: clone \
             https://github.com/KitionAI/kition-fonts next to this repository, or set KITION_FONTS_DIR to a checkout",
            path.display()
        )
    });
    assert_eq!(kition_pdf_fonts::add_document_font(bytes), 1, "the face is added");
}

/// One page with a Helvetica text field `city`, a multiline one `notes` and a drop-down
/// `country` with a Chinese option, as the hand fixtures write them (no appearances, `/NeedAppearances`).
fn form() -> Document {
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [5 0 R 6 0 R 7 0 R] /NeedAppearances true /DA (/F1 12 Tf 0 g) /DR << /Font << /F1 4 0 R >> >> >> >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] >>".into(),
        "<< /Type /Page /Parent 2 0 R /Annots [5 0 R 6 0 R 7 0 R] >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (city) /P 3 0 R /Rect [380 712 540 734] /F 4 /DA (/F1 12 Tf 0 g) /MK << /BC [0.4 0.4 0.4] >> >>".into(),
        "<< /Type /Annot /Subtype /Widget /FT /Tx /T (notes) /Ff 4096 /P 3 0 R /Rect [380 600 460 700] /F 4 /DA (/F1 10 Tf 0 g) >>".into(),
        "<< /Type /Annot /Subtype /Widget /FT /Ch /Ff 131072 /T (country) /Opt [[(NO) <FEFF632A5A01>] [(IS) (Iceland)]] /P 3 0 R /Rect [380 682 540 704] /F 4 /DA (/F1 12 Tf 0 g) >>".into(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    Document::open(Arc::new(out)).unwrap()
}

/// A widget's normal appearance: its content and its fonts (resource name and dictionary).
fn appearance(doc: &Document, name: &str) -> (Vec<u8>, Vec<(String, kition_pdf_cos::Dict)>) {
    let all = fields(doc);
    let field = all.iter().find(|f| f.name == name).unwrap();
    let widget = doc.get(field.widgets[0].obj).as_dict().cloned().unwrap();
    let n = doc.resolve(widget.get(b"AP").unwrap()).as_dict().unwrap().reference(b"N").unwrap();
    let Object::Stream(s) = &*doc.get(n) else { panic!("the normal appearance is a stream") };
    let resources = doc.resolve(s.dict.get(b"Resources").unwrap()).as_dict().cloned().unwrap();
    let fonts = doc.resolve(resources.get(b"Font").unwrap()).as_dict().cloned().unwrap();
    let fonts = fonts.iter().map(|(k, v)| (String::from_utf8_lossy(k).into_owned(), doc.resolve(v).as_dict().cloned().unwrap())).collect();
    (s.decoded().unwrap(), fonts)
}

/// The text a font's ToUnicode map gives for its codes, in code order.
fn to_unicode(doc: &Document, font: &kition_pdf_cos::Dict) -> String {
    let r = font.reference(b"ToUnicode").unwrap();
    let Object::Stream(s) = &*doc.get(r) else { panic!("ToUnicode is a stream") };
    let cmap = String::from_utf8(s.decoded().unwrap()).unwrap();
    let mut inside = false;
    cmap.lines()
        .filter(|line| {
            if line.ends_with("beginbfchar") {
                inside = true;
                return false;
            }
            if line.starts_with("endbfchar") {
                inside = false;
            }
            inside
        })
        .filter_map(|line| {
            let hex = line.split_whitespace().nth(1)?.trim_matches(|c| c == '<' || c == '>');
            let unit = u16::from_str_radix(hex, 16).ok()?;
            char::from_u32(u32::from(unit))
        })
        .collect()
}

#[test]
fn chinese_values_are_drawn_from_a_document_face() {
    add_chinese_face();
    let mut doc = form();
    set_value(&mut doc, "city", &FieldValue::Text("奥斯陆 Oslo".into())).unwrap();
    set_value(&mut doc, "notes", &FieldValue::Text("北极光在冬季的夜晚最常见".into())).unwrap();
    set_value(&mut doc, "country", &FieldValue::Choice(vec!["NO".into()])).unwrap();
    let bytes = write_incremental(&doc, &SaveOptions::default()).unwrap();
    let doc = Document::open(Arc::new(bytes)).unwrap();

    let (content, fonts) = appearance(&doc, "city");
    let text = String::from_utf8_lossy(&content);
    assert!(!content.windows(3).any(|w| w == b"???"), "no WinAnsi question marks: {text}");
    assert!(text.contains("(Oslo) Tj") || text.contains("( Oslo) Tj"), "the Latin text stays in Helvetica: {text}");
    let outlined: Vec<_> = fonts.iter().filter(|(_, f)| f.name(b"Subtype") == Some(&b"Type3"[..])).collect();
    assert_eq!(outlined.len(), 1, "one Type 3 font for the Chinese characters: {:?}", fonts.iter().map(|(k, _)| k).collect::<Vec<_>>());
    let (name, font) = outlined[0];
    assert!(text.contains(&format!("/{name} ")), "the Type 3 font is used: {text}");
    assert_eq!(to_unicode(&doc, font), "奥斯陆", "only the glyphs the value shows, copyable");
    let procs = doc.resolve(font.get(b"CharProcs").unwrap()).as_dict().cloned().unwrap();
    assert_eq!(procs.len(), 3);
    for (_, proc) in procs.iter() {
        let Object::Stream(s) = &*doc.resolve(proc) else { panic!("a glyph is a stream") };
        let glyph = String::from_utf8_lossy(&s.decoded().unwrap()).into_owned();
        assert!(glyph.contains(" d1") && glyph.contains(" m") && glyph.ends_with("f\n"), "the glyph's outline is drawn: {glyph}");
    }
    assert!(fonts.iter().any(|(k, f)| k == "F1" && f.name(b"BaseFont") == Some(&b"Helvetica"[..])));

    // Multiline text wraps by character and is drawn the same way.
    let (content, fonts) = appearance(&doc, "notes");
    assert!(!content.windows(1).any(|w| w == b"?"), "{}", String::from_utf8_lossy(&content));
    assert!(String::from_utf8_lossy(&content).matches(" Tj").count() >= 2, "wrapped over lines");
    assert!(fonts.iter().any(|(_, f)| f.name(b"Subtype") == Some(&b"Type3"[..])));

    // A drop-down's Chinese option shows as itself.
    let (content, fonts) = appearance(&doc, "country");
    assert!(!content.windows(2).any(|w| w == b"??"), "{}", String::from_utf8_lossy(&content));
    let chinese = fonts.iter().find(|(_, f)| f.name(b"Subtype") == Some(&b"Type3"[..])).expect("a Type 3 font");
    assert_eq!(to_unicode(&doc, &chinese.1), "挪威");
}
