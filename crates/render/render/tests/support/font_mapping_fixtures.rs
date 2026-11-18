//! Original embedded-font mapping matrix; malformed probes are labeled explicitly.
#[path = "font_programs.rs"]
pub mod programs;
use std::path::PathBuf;

pub struct Case {
    pub id: &'static str,
    pub classification: &'static str,
    pub expectation_basis: &'static str,
    pub font_kind: &'static str,
    pub descendant: &'static str,
    pub font_file: &'static str,
    pub stream_subtype: &'static str,
    pub map: &'static str,
    pub code: u16,
    pub expected_gid: u32,
    pub assert_mapping: bool,
    pub unicode_swapped: bool,
    pub secondary: Option<Vec<u8>>,
    pub font: Vec<u8>,
}

pub fn cases() -> Vec<Case> {
    let tt = programs::opentype(None);
    let raw = programs::cff(true, true);
    let otf = programs::opentype(Some(raw.clone()));
    let mut all = Vec::new();
    let base = |id, font_kind, font: Vec<u8>, descendant, font_file, stream_subtype, map, expected_gid, classification, assert_mapping| Case {
        id,
        classification,
        expectation_basis: if assert_mapping { "format mapping control" } else { "CID charset compatibility hypothesis; malformed wrapper" },
        font_kind,
        descendant,
        font_file,
        stream_subtype,
        map,
        code: 1,
        expected_gid,
        assert_mapping,
        unicode_swapped: false,
        secondary: None,
        font,
    };
    all.push(base("truetype-identity", "truetype", tt.clone(), "CIDFontType2", "FontFile2", "", "Identity", 1, "format-control", true));
    all.push(base("truetype-explicit-map", "truetype", tt.clone(), "CIDFontType2", "FontFile2", "", "swapped", 2, "format-control", true));
    let mut unicode =
        base("truetype-unicode-swapped", "truetype", tt.clone(), "CIDFontType2", "FontFile2", "", "Identity", 1, "format-control", true);
    unicode.unicode_swapped = true;
    all.push(unicode);
    let mut missing = base("truetype-missing-glyph", "truetype", tt.clone(), "CIDFontType2", "FontFile2", "", "Identity", 3, "format-control", true);
    missing.code = 3;
    all.push(missing);
    all.push(base(
        "raw-cid-identity-charset",
        "cid-cff",
        programs::cff(true, false),
        "CIDFontType0",
        "FontFile3",
        "CIDFontType0C",
        "absent",
        1,
        "format-control",
        true,
    ));
    all.push(base(
        "raw-cid-swapped-charset",
        "cid-cff",
        raw.clone(),
        "CIDFontType0",
        "FontFile3",
        "CIDFontType0C",
        "absent",
        2,
        "format-control",
        true,
    ));
    let mut missing =
        base("raw-cid-missing-cid", "cid-cff", raw.clone(), "CIDFontType0", "FontFile3", "CIDFontType0C", "absent", 0, "format-control", true);
    missing.code = 65535;
    all.push(missing);
    all.push(base("raw-cid-type2-file2", "cid-cff", raw, "CIDFontType2", "FontFile2", "", "Identity", 2, "malformed-wrapper-compatibility", true));
    all.push(base(
        "opentype-cid-identity-charset",
        "opentype-cid-cff",
        programs::opentype(Some(programs::cff(true, false))),
        "CIDFontType0",
        "FontFile3",
        "OpenType",
        "absent",
        1,
        "format-control",
        true,
    ));
    all.push(base(
        "opentype-cid-swapped-charset",
        "opentype-cid-cff",
        otf.clone(),
        "CIDFontType0",
        "FontFile3",
        "OpenType",
        "absent",
        2,
        "format-control",
        true,
    ));
    let mut unicode = base(
        "opentype-cid-unicode-swapped",
        "opentype-cid-cff",
        otf.clone(),
        "CIDFontType0",
        "FontFile3",
        "OpenType",
        "absent",
        2,
        "format-control",
        true,
    );
    unicode.unicode_swapped = true;
    all.push(unicode);
    let mut missing = base(
        "opentype-cid-missing-cid",
        "opentype-cid-cff",
        otf.clone(),
        "CIDFontType0",
        "FontFile3",
        "OpenType",
        "absent",
        0,
        "format-control",
        true,
    );
    missing.code = 65535;
    all.push(missing);
    all.push(base(
        "opentype-cid-type2-file2",
        "opentype-cid-cff",
        otf.clone(),
        "CIDFontType2",
        "FontFile2",
        "",
        "Identity",
        1,
        "malformed-wrapper-compatibility",
        true,
    ));
    all.push(base(
        "opentype-cid-type2-file3",
        "opentype-cid-cff",
        otf.clone(),
        "CIDFontType2",
        "FontFile3",
        "OpenType",
        "Identity",
        2,
        "malformed-wrapper-mapping-probe",
        false,
    ));
    all.push(base(
        "opentype-name-type2-file2",
        "opentype-name-cff",
        programs::opentype(Some(programs::cff(false, false))),
        "CIDFontType2",
        "FontFile2",
        "",
        "Identity",
        1,
        "malformed-wrapper-compatibility",
        true,
    ));
    all.push(base(
        "raw-name-composite",
        "name-cff",
        programs::cff(false, false),
        "CIDFontType0",
        "FontFile3",
        "CIDFontType0C",
        "absent",
        1,
        "format-control",
        true,
    ));
    all.push(base(
        "opentype-name-file3",
        "opentype-name-cff",
        programs::opentype(Some(programs::cff(false, false))),
        "CIDFontType0",
        "FontFile3",
        "OpenType",
        "absent",
        1,
        "format-control",
        true,
    ));
    let mut dual =
        base("descriptor-file2-precedence", "truetype", tt, "CIDFontType2", "FontFile2", "", "Identity", 1, "malformed-multiple-programs", true);
    dual.secondary = Some(otf);
    all.push(dual);
    all
}

fn stream(bytes: &[u8], extra: &str) -> Vec<u8> {
    let mut out = format!("<< /Length {} {extra} >>\nstream\n", bytes.len()).into_bytes();
    out.extend_from_slice(bytes);
    out.extend_from_slice(b"\nendstream");
    out
}

pub fn pdf(case: &Case) -> Vec<u8> {
    let code = case.code;
    let unicode = if case.unicode_swapped { "<0001> <0042>\n<0002> <0041>" } else { "<0001> <0041>\n<0002> <0042>" };
    let to_unicode = format!(
        "/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def /CMapName /KitionGeometryUnicode def /CMapType 2 def 1 begincodespacerange <0000> <ffff> endcodespacerange 2 beginbfchar {unicode} endbfchar endcmap CMapName currentdict /CMap defineresource pop end end"
    );
    let map = match case.map {
        "absent" => "",
        "swapped" => "/CIDToGIDMap 10 0 R",
        _ => "/CIDToGIDMap /Identity",
    };
    let subtype = if case.stream_subtype.is_empty() { String::new() } else { format!("/Subtype /{}", case.stream_subtype) };
    let secondary = if case.secondary.is_some() { "/FontFile3 11 0 R" } else { "" };
    let mut objects = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 40 40] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_vec(),
        stream(format!("0 0 0 rg BT /F1 20 Tf 10 10 Td <{code:04X}> Tj ET 1 0 0 rg 0 0 4 4 re f").as_bytes(), ""),
        b"<< /Type /Font /Subtype /Type0 /BaseFont /KitionGeometry /Encoding /Identity-H /DescendantFonts [6 0 R] /ToUnicode 9 0 R >>".to_vec(),
        format!("<< /Type /Font /Subtype /{} /BaseFont /KitionGeometry /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor 7 0 R /DW 1000 {map} >>",case.descendant).into_bytes(),
        format!("<< /Type /FontDescriptor /FontName /KitionGeometry /Flags 4 /FontBBox [0 0 1000 700] /ItalicAngle 0 /Ascent 800 /Descent -200 /CapHeight 700 /StemV 80 /{} 8 0 R {secondary} >>",case.font_file).into_bytes(),
        stream(&case.font, &subtype),
        stream(to_unicode.as_bytes(), ""),
        stream(&[0,0,0,2,0,1,0,0], ""),
    ];
    if let Some(font) = &case.secondary {
        objects.push(stream(font, "/Subtype /OpenType"));
    }
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(object);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    out
}

pub fn export(cases: &[Case]) {
    let Some(requested) = std::env::var_os("KITION_PDF_FONT_OUT") else { return };
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../target").canonicalize().unwrap();
    let out = PathBuf::from(requested);
    assert!(out.is_absolute() && out.starts_with(&target));
    assert!(!out.components().any(|p| p == std::path::Component::ParentDir));
    std::fs::create_dir_all(&out).unwrap();
    let out = out.canonicalize().unwrap();
    assert!(out.starts_with(target));
    let write = |name: &str, bytes: &[u8]| {
        let path = out.join(name);
        if path.exists() {
            assert_eq!(std::fs::read(path).unwrap(), bytes, "refuse different fixture bytes");
        } else {
            std::fs::write(path, bytes).unwrap();
        }
    };
    let mut rows = Vec::new();
    for case in cases {
        write(&format!("{}.pdf", case.id), &pdf(case));
        write(&format!("{}.font", case.id), &case.font);
        rows.push(format!(r#"{{"id":"{}","file":"{}.pdf","fontFile":"{}.font","classification":"{}","expectationBasis":"{}","fontKind":"{}","descendant":"{}","descriptorEntry":"{}","streamSubtype":"{}","cidToGidMap":"{}","code":{},"candidateGlyphId":{},"assertMapping":{},"toUnicodeSwapped":{},"hasSecondaryProgram":{}}}"#,
            case.id,case.id,case.id,case.classification,case.expectation_basis,case.font_kind,case.descendant,case.font_file,case.stream_subtype,case.map,case.code,case.expected_gid,case.assert_mapping,case.unicode_swapped,case.secondary.is_some()));
    }
    write("manifest.raw.json",format!(r#"{{"schema":1,"origin":"Contributor-original geometric font programs, Apache-2.0","pageWidth":40,"pageHeight":40,"scales":[1,2],"coordinateSpace":"pdf-user-space-y-up","cases":[{}]}}"#,rows.join(",")).as_bytes());
}
