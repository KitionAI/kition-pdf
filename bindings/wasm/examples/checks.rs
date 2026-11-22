//! Writes the conformance fixtures (KHP `spec/conformance.md`): `basic.pdf`, which `pdf_check` finds
//! clean, one `check-<code>.pdf` per code of `agent/checks.json`, on which the check reports that
//! code, and `expected/` (the expected previews of `basic.pdf`).
//! `cargo run -p kition-pdf-kit --example checks -- conformance`. The files are written object by
//! object (the same bytes on every run); the restricted one goes through the kit's own
//! `doc_protect` tool.

use std::path::Path;

use serde_json::json;

/// A PDF of `objects` (object 1 is the catalog), with an xref table.
fn pdf(objects: &[String]) -> Vec<u8> {
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    out
}

fn stream(body: &str) -> String {
    format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len())
}

/// The visible text of every fixture: a heading and a line in Helvetica, one of the standard 14 fonts
/// viewers supply (so it is not a missing font).
const TEXT: &str = "BT /F1 24 Tf 72 720 Td (Quarterly report) Tj /F1 12 Tf 0 -32 Td (Sales rose in every region.) Tj ET";

/// One page with `page` added to its dictionary, the catalog with `catalog` added, and `extra`
/// objects from number 6 on (5 is the page's font).
fn document(catalog: &str, page: &str, extra: &[String]) -> Vec<u8> {
    let mut objects = vec![
        format!("<< /Type /Catalog /Pages 2 0 R {catalog} >>"),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] >>".to_string(),
        format!("<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> {page} >>"),
        stream(TEXT),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_string(),
    ];
    objects.extend_from_slice(extra);
    pdf(&objects)
}

fn fixtures() -> Vec<(&'static str, Vec<u8>)> {
    let link = |dest: &str| format!("<< /Type /Annot /Subtype /Link /Rect [72 690 300 710] /Border [0 0 0] {dest} >>");
    let text_field = |name: &str, extra: &str, y: u32| {
        format!("<< /Type /Annot /Subtype /Widget /FT /Tx /T ({name}) /Rect [72 {y} 300 {}] /P 3 0 R /F 4 {extra} >>", y + 20)
    };
    vec![
        (
            "basic.pdf",
            // Shapes only: no font, so nothing (not even a standard-14 note) to report.
            pdf(&[
                "<< /Type /Catalog /Pages 2 0 R >>".into(),
                "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] >>".into(),
                "<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>".into(),
                stream("0.13 0.27 0.55 rg 72 640 468 80 re f 0.85 0.45 0.2 rg 72 480 140 120 re f 0.2 0.6 0.35 rg 236 520 140 80 re f 0.6 0.6 0.6 rg 400 560 140 40 re f"),
            ]),
        ),
        (
            "check-page-count-mismatch.pdf",
            pdf(&[
                "<< /Type /Catalog /Pages 2 0 R >>".into(),
                "<< /Type /Pages /Kids [3 0 R] /Count 3 /MediaBox [0 0 612 792] >>".into(),
                "<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".into(),
                stream(TEXT),
                "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
            ]),
        ),
        (
            "check-missing-content.pdf",
            pdf(&[
                "<< /Type /Catalog /Pages 2 0 R >>".into(),
                "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] >>".into(),
                "<< /Type /Page /Parent 2 0 R /Contents 9 0 R >>".into(),
            ]),
        ),
        (
            "check-damaged-content.pdf",
            pdf(&[
                "<< /Type /Catalog /Pages 2 0 R >>".into(),
                "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] >>".into(),
                "<< /Type /Page /Parent 2 0 R /Contents 4 0 R >>".into(),
                "<< /Length 9 /Filter /FlateDecode >>\nstream\nnot flate\nendstream".into(),
            ]),
        ),
        (
            "check-font-not-embedded.pdf",
            pdf(&[
                "<< /Type /Catalog /Pages 2 0 R >>".into(),
                "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] >>".into(),
                "<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".into(),
                stream(TEXT),
                "<< /Type /Font /Subtype /TrueType /BaseFont /CorporateSans /FontDescriptor 6 0 R /FirstChar 32 /LastChar 126 >>".into(),
                "<< /Type /FontDescriptor /FontName /CorporateSans /Flags 32 /FontBBox [0 -200 1000 900] /ItalicAngle 0 /Ascent 900 /Descent -200 /CapHeight 700 /StemV 80 >>".into(),
            ]),
        ),
        ("check-broken-link.pdf", document("", "/Annots [6 0 R]", &[link("/Dest /summary")])),
        ("check-external-reference.pdf", document("", "/Annots [6 0 R]", &[link("/A << /S /GoToR /F (report-2025.pdf) /D [0 /Fit] >>")])),
        ("check-annotation-off-page.pdf", document("", "/Annots [6 0 R]", &["<< /Type /Annot /Subtype /Text /Rect [700 900 720 920] /Contents (A note) >>".into()])),
        (
            "check-required-field-empty.pdf",
            document("/AcroForm << /Fields [6 0 R] /DA (/Helv 0 Tf 0 g) >>", "/Annots [6 0 R]", &[text_field("name", "/Ff 2 /V ()", 600)]),
        ),
        (
            "check-text-too-long.pdf",
            document("/AcroForm << /Fields [6 0 R] /DA (/Helv 0 Tf 0 g) >>", "/Annots [6 0 R]", &[text_field("code", "/MaxLen 5 /V (ABCDEFG)", 600)]),
        ),
        (
            "check-invalid-choice.pdf",
            document(
                "/AcroForm << /Fields [6 0 R] /DA (/Helv 0 Tf 0 g) >>",
                "/Annots [6 0 R]",
                &["<< /Type /Annot /Subtype /Widget /FT /Ch /Ff 131072 /T (country) /Opt [(France) (Germany)] /V (Atlantis) /Rect [72 600 300 620] /P 3 0 R /F 4 >>".into()],
            ),
        ),
        (
            "check-missing-alt-text.pdf",
            pdf(&[
                "<< /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R /MarkInfo << /Marked true >> >>".into(),
                "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] >>".into(),
                "<< /Type /Page /Parent 2 0 R /Contents 4 0 R /StructParents 0 >>".into(),
                stream("/Figure << /MCID 0 >> BDC 0.2 0.4 0.8 rg 72 500 200 120 re f EMC"),
                "<< /Type /StructTreeRoot /K [6 0 R] >>".into(),
                "<< /Type /StructElem /S /Figure /P 5 0 R /Pg 3 0 R /K 0 >>".into(),
            ]),
        ),
        ("check-truncated.pdf", {
            let annots: Vec<String> = (0..103).map(|i| format!("{} 0 R", i + 6)).collect();
            let extra: Vec<String> = (0..103).map(|_| "<< /Type /Annot /Subtype /Text /Rect [900 900 910 910] >>".to_string()).collect();
            document("", &format!("/Annots [{}]", annots.join(" ")), &extra)
        }),
    ]
}

/// `check-restricted.pdf`: `basic.pdf` with a permissions password that denies printing and
/// copying, written by the kit's own `doc_protect` and `doc_save` tools.
fn restricted(dir: &Path) -> Result<(), String> {
    let mut tools = kition_pdf_automation::Automation::new().with_root(dir).map_err(|error| error.to_string())?;
    let opened = tools.call("doc_open", &json!({ "path": "basic.pdf" })).map_err(|error| format!("{error:?}"))?;
    let doc = match opened.first() {
        Some(kition_pdf_automation::Content::Json(value)) => value["doc"].clone(),
        _ => return Err("doc_open answered no document".into()),
    };
    tools
        .call("doc_protect", &json!({ "doc": doc, "permissions_password": "owner", "printing": "none", "copy": false }))
        .map_err(|error| format!("{error:?}"))?;
    tools.call("doc_save", &json!({ "doc": doc, "path": "check-restricted.pdf" })).map_err(|error| format!("{error:?}"))?;
    println!("wrote check-restricted.pdf");
    Ok(())
}

/// `expected/basic.png` and `expected/basic-scale-2.png`: the engine's own render of `basic.pdf`, at 1
/// and 2 pixels per point, which `<prefix>_inspect { preview }` must match.
fn expected_previews(dir: &Path, bytes: &[u8]) -> Result<(), String> {
    use kition_kit::KitDocument;
    let doc = kition_pdf_kit::PdfDocument::open("basic.pdf", bytes)?;
    let expected = dir.join("expected");
    std::fs::create_dir_all(&expected).map_err(|error| error.to_string())?;
    for (file, scale) in [("basic.png", 1.0), ("basic-scale-2.png", 2.0)] {
        let (width, height) = ((f64::from(doc.width()) * scale) as u32, (f64::from(doc.height()) * scale) as u32);
        let rgba = doc.render(0.0, 0.0, scale, width, height)?;
        let out = std::fs::File::create(expected.join(file)).map_err(|error| error.to_string())?;
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(out), width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
        writer.write_image_data(&rgba).map_err(|error| error.to_string())?;
        println!("wrote expected/{file}");
    }
    Ok(())
}

fn main() -> Result<(), String> {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "conformance".into());
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    for (name, bytes) in fixtures() {
        std::fs::write(dir.join(name), &bytes).map_err(|error| format!("{name}: {error}"))?;
        println!("wrote {name}");
        if name == "basic.pdf" {
            expected_previews(dir, &bytes)?;
        }
    }
    restricted(&std::fs::canonicalize(dir).map_err(|error| error.to_string())?)
}
