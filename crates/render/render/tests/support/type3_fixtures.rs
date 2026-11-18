//! Contributor-original Type3 recursion and resource-rebinding controls.
use std::path::PathBuf;

pub struct Region {
    pub rect: [f64; 4],
    pub rgb: [u8; 3],
}

pub struct Case {
    pub id: &'static str,
    pub expected_truncation: bool,
    pub glyph_kind: &'static str,
    pub expected_match: Option<&'static str>,
    pub data: Vec<u8>,
    pub regions: Vec<Region>,
}

fn stream(body: &str, extra: &str) -> String {
    format!("<< /Length {} {extra} >>\nstream\n{body}\nendstream", body.len())
}

fn font(program: u32, resources: &str) -> String {
    format!(
        "<< /Type /Font /Subtype /Type3 /FontBBox [0 0 8 8] /FontMatrix [1 0 0 1 0 0] /FirstChar 65 /LastChar 66 /Widths [8 8] /Encoding << /Differences [65 /a /b] >> /CharProcs << /a {program} 0 R /b 9 0 R >> {resources} >>"
    )
}

fn pdf(programs: [&str; 3], font_resources: &str, proc_resources: &str, content: &str) -> Vec<u8> {
    let resources = "/Resources << /Font << /Root 5 0 R /Next 6 0 R /Last 7 0 R >> >>";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R 11 0 R] /Count 2 >>".to_owned(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 40 40] /Contents 4 0 R {resources} >>"),
        stream(&format!("0 0 1 rg {content} 1 0 0 rg 0 0 4 4 re f"), ""),
        font(8, ""),
        font(9, font_resources),
        font(10, ""),
        stream(programs[0], ""),
        stream(programs[1], proc_resources),
        stream(programs[2], ""),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 40 40] /Contents 12 0 R {resources} >>"),
        stream("0 1 0 rg 10 10 6 6 re f 1 0 0 rg 0 0 4 4 re f", ""),
    ];
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
    out.extend_from_slice(format!("trailer\n<< /Root 1 0 R /Size {} >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    out
}

pub fn cases() -> Vec<Case> {
    let root = "BT /Root 1 Tf 10 10 Td (A) Tj ET";
    let blue = "8 0 d0 0 0 1 rg 0 0 6 6 re f";
    let green = "8 0 d0 0 1 0 rg 0 0 6 6 re f";
    let mut cases = Vec::new();
    let mut add = |id, kind, truncated, expected_match, programs, font_resources, proc_resources, content, color| {
        cases.push(Case {
            id,
            expected_truncation: truncated,
            glyph_kind: kind,
            expected_match,
            data: pdf(programs, font_resources, proc_resources, content),
            regions: vec![
                Region { rect: [1.0, 1.0, 2.0, 2.0], rgb: [255, 0, 0] },
                Region { rect: [11.0, 11.0, 4.0, 4.0], rgb: color },
                Region { rect: [32.0, 32.0, 4.0, 4.0], rgb: [255; 3] },
            ],
        });
    };
    add(
        "direct-fanout-d0",
        "d0",
        true,
        Some("plain-blue"),
        ["8 0 d0 BT /Root 1 Tf (AAAAAAAA) Tj ET 0 0 1 rg 0 0 6 6 re f", blue, green],
        "",
        "",
        root,
        [0, 0, 255],
    );
    add(
        "direct-fanout-d1",
        "d1",
        true,
        Some("plain-blue"),
        ["8 0 0 0 8 8 d1 BT /Root 1 Tf (AAAAAAAA) Tj ET 0 0 6 6 re f", blue, green],
        "",
        "",
        root,
        [0, 0, 255],
    );
    add(
        "indirect-fanout",
        "d0",
        true,
        Some("plain-blue"),
        ["8 0 d0 BT /Next 1 Tf (AAAAAAAA) Tj ET 0 0 1 rg 0 0 6 6 re f", "8 0 d0 BT /Root 1 Tf (AAAAAAAA) Tj ET", green],
        "",
        "",
        root,
        [0, 0, 255],
    );
    add(
        "transformed-cycle",
        "d0",
        true,
        Some("plain-blue"),
        ["8 0 d0 q 0.5 0 0 0.5 1 1 cm BT /Root 1 Tf (A) Tj ET Q 0 0 1 rg 0 0 6 6 re f", blue, green],
        "",
        "",
        root,
        [0, 0, 255],
    );
    for (id, font_resources, proc_resources) in [
        ("finite-font-rebinding", "/Resources << /Font << /Next 7 0 R >> >>", ""),
        ("finite-charproc-rebinding", "", "/Resources << /Font << /Next 7 0 R >> >>"),
    ] {
        add(
            id,
            "d0",
            false,
            Some("plain-green"),
            ["8 0 d0 BT /Next 1 Tf (A) Tj ET", "8 0 d0 BT /Root 1 Tf (A) Tj ET", green],
            font_resources,
            proc_resources,
            root,
            [0, 255, 0],
        );
    }
    add("finite-same-font-other-glyph", "d0", false, Some("plain-green"), ["8 0 d0 BT /Root 1 Tf (B) Tj ET", green, blue], "", "", root, [0, 255, 0]);
    add("finite-cross-font-same-glyph", "d0", false, Some("plain-green"), ["8 0 d0 BT /Next 1 Tf (A) Tj ET", green, blue], "", "", root, [0, 255, 0]);
    add(
        "finite-invalid-font-fallback",
        "d0",
        false,
        Some("plain-green"),
        ["8 0 d0 BT /Next 1 Tf (A) Tj ET", "8 0 d0 BT /Last 1 Tf (A) Tj ET", green],
        "/Resources << /Font << /Last 42 >> >>",
        "",
        root,
        [0, 255, 0],
    );
    add("plain-blue", "d0", false, None, [blue, blue, green], "", "", root, [0, 0, 255]);
    add("plain-green", "d0", false, None, [green, blue, green], "", "", root, [0, 255, 0]);
    let siblings = "BT /Root 1 Tf 10 10 Td (AA) Tj ET q 0 1 -1 0 28 10 cm BT /Root 1 Tf (A) Tj ET Q";
    add("siblings-transforms-d0", "d0", false, None, [blue, blue, green], "", "", siblings, [0, 0, 255]);
    add(
        "siblings-transforms-d1",
        "d1",
        false,
        Some("siblings-transforms-d0"),
        ["8 0 0 0 8 8 d1 0 0 6 6 re f", blue, green],
        "",
        "",
        siblings,
        [0, 0, 255],
    );
    for case in cases.iter_mut().filter(|case| case.id.starts_with("siblings")) {
        case.regions.extend([
            Region { rect: [19.0, 11.0, 2.0, 4.0], rgb: [0, 0, 255] },
            Region { rect: [25.0, 11.0, 2.0, 4.0], rgb: [0, 0, 255] },
            Region { rect: [16.0, 11.0, 2.0, 4.0], rgb: [255; 3] },
        ]);
    }
    cases
}

pub fn export(cases: &[Case]) {
    let Some(requested) = std::env::var_os("KITION_PDF_TYPE3_OUT") else { return };
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../target").canonicalize().unwrap();
    let out = PathBuf::from(requested);
    assert!(out.is_absolute() && out.starts_with(&target));
    assert!(!out.components().any(|p| p == std::path::Component::ParentDir));
    std::fs::create_dir_all(&out).unwrap();
    let out = out.canonicalize().unwrap();
    assert!(out.starts_with(target));
    let write = |name: &str, data: &[u8]| {
        let path = out.join(name);
        if path.exists() {
            assert!(std::fs::read(path).unwrap() == data, "refuse different fixture bytes");
        } else {
            std::fs::write(path, data).unwrap();
        }
    };
    let mut rows = Vec::new();
    for case in cases {
        write(&format!("{}.pdf", case.id), &case.data);
        let regions =
            case.regions.iter().map(|r| format!(r#"{{"rect":{:?},"rgb":{:?},"tolerance":8}}"#, r.rect, r.rgb)).collect::<Vec<_>>().join(",");
        let pair = case.expected_match.map(|id| format!(r#", "expectedMatch":"{id}""#)).unwrap_or_default();
        rows.push(format!(
            r#"{{"id":"{}","file":"{}.pdf","isStress":{},"expectedTruncation":{},"glyphKind":"{}","regions":[{regions}]{pair}}}"#,
            case.id, case.id, case.expected_truncation, case.expected_truncation, case.glyph_kind
        ));
    }
    write("manifest.raw.json",format!(r#"{{"schema":1,"origin":"Contributor-original Type3 controls, Apache-2.0","coordinateSpace":"pdf-user-space-y-up","pageWidth":40,"pageHeight":40,"scales":[1,2],"cases":[{}]}}"#,rows.join(",")).as_bytes());
}
