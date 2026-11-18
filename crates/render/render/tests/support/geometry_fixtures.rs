//! Contributor-original ASCII geometry; no font program or external asset is used.

use std::{fmt::Write, path::PathBuf};

pub const WIDTH: u32 = 256;
pub const HEIGHT: u32 = 192;
pub const RECTS: [[f64; 4]; 2] = [[0.0, 0.0, 10.0, 40.0], [10.0, 0.0, 30.0, 10.0]];

#[derive(Clone, Copy)]
pub struct Segment {
    pub id: &'static str,
    pub points: [f64; 4],
    pub width: f64,
}

pub const SEGMENTS: [Segment; 11] = [
    Segment { id: "horizontal-hairline-quarter", points: [16.0, 20.25, 112.0, 20.25], width: 0.0 },
    Segment { id: "horizontal-quarter-half", points: [16.0, 40.5, 112.0, 40.5], width: 0.25 },
    Segment { id: "horizontal-half-threequarters", points: [16.0, 60.75, 112.0, 60.75], width: 0.5 },
    Segment { id: "horizontal-one-quarter", points: [16.0, 80.25, 112.0, 80.25], width: 1.0 },
    Segment { id: "horizontal-hairline-half", points: [16.0, 100.5, 112.0, 100.5], width: 0.0 },
    Segment { id: "horizontal-quarter-threequarters", points: [16.0, 120.75, 112.0, 120.75], width: 0.25 },
    Segment { id: "diagonal-quarter", points: [16.0, 150.25, 112.0, 170.25], width: 0.25 },
    Segment { id: "vertical-hairline-quarter", points: [148.25, 16.0, 148.25, 176.0], width: 0.0 },
    Segment { id: "vertical-quarter-half", points: [172.5, 16.0, 172.5, 176.0], width: 0.25 },
    Segment { id: "vertical-half-threequarters", points: [196.75, 16.0, 196.75, 176.0], width: 0.5 },
    Segment { id: "vertical-one-integer", points: [220.0, 16.0, 220.0, 176.0], width: 1.0 },
];

pub const GLYPHS: [(&str, [f64; 6]); 7] = [
    ("fractional", [1.0, 0.0, 0.0, 1.0, 20.25, 20.5]),
    ("translated", [1.0, 0.0, 0.0, 1.0, 75.5, 22.25]),
    ("rotated", [0.0, 1.0, -1.0, 0.0, 150.25, 20.5]),
    ("sheared", [1.0, 0.0, 0.5, 1.0, 185.25, 20.5]),
    ("scaled", [1.5, 0.0, 0.0, 0.75, 20.25, 105.5]),
    ("advance-first", [1.0, 0.0, 0.0, 1.0, 90.25, 100.5]),
    ("advance-second", [1.0, 0.0, 0.0, 1.0, 130.25, 100.5]),
];

fn stream(content: &str) -> String {
    format!("<< /Length {} >>\nstream\n{content}endstream", content.len())
}

fn pdf(content: &str, font: bool) -> Vec<u8> {
    let resources = if font { "/Font << /F1 5 0 R >>" } else { "" };
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_string(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {WIDTH} {HEIGHT}] /Resources << {resources} >> /Contents 4 0 R >>"),
        stream(content),
    ];
    if font {
        objects.push("<< /Type /Font /Subtype /Type3 /FontBBox [0 0 300 400] /FontMatrix [0.001 0 0 0.001 0 0] /FirstChar 65 /LastChar 65 /Widths [400] /Encoding << /Type /Encoding /Differences [65 /originalL] >> /CharProcs << /originalL 6 0 R >> /Resources << >> >>".into());
        objects.push(stream("400 0 0 0 300 400 d1\n0 0 100 400 re 100 0 200 100 re f\n"));
    }
    let mut bytes = String::from("%PDF-1.7\n");
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        writeln!(bytes, "{} 0 obj\n{object}\nendobj", index + 1).unwrap();
    }
    let start = bytes.len();
    writeln!(bytes, "xref\n0 {}\n0000000000 65535 f ", objects.len() + 1).unwrap();
    for offset in offsets {
        writeln!(bytes, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(bytes, "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{start}\n%%EOF", objects.len() + 1).unwrap();
    assert!(bytes.is_ascii());
    bytes.into_bytes()
}

pub fn strokes() -> Vec<u8> {
    let mut content = String::from("0 G 0 J\n");
    for segment in SEGMENTS {
        let [x0, y0, x1, y1] = segment.points;
        writeln!(content, "{} w {x0} {y0} m {x1} {y1} l S", segment.width).unwrap();
    }
    pdf(&content, false)
}

pub fn glyphs() -> Vec<u8> {
    let mut content = String::from("0 g\n");
    for (_, [a, b, c, d, e, f]) in GLYPHS.into_iter().take(5) {
        writeln!(content, "BT /F1 100 Tf {a} {b} {c} {d} {e} {f} Tm (A) Tj ET").unwrap();
    }
    content.push_str("BT /F1 100 Tf 1 0 0 1 90.25 100.5 Tm (AA) Tj ET\n");
    pdf(&content, true)
}

pub fn manifest() -> String {
    let segments = SEGMENTS
        .map(|s| {
            let [x0, y0, x1, y1] = s.points;
            format!(r#"{{"id":"{}","x0":{x0},"y0":{y0},"x1":{x1},"y1":{y1},"width":{}}}"#, s.id, s.width)
        })
        .join(",");
    let glyphs = GLYPHS.map(|(id, matrix)| format!(r#"{{"id":"{id}","matrix":{matrix:?}}}"#)).join(",");
    format!(
        r#"{{"schema":1,"origin":"Contributor-original ASCII geometry, Apache-2.0","coordinateSpace":"pdf-user-space-y-up","pageWidth":{WIDTH},"pageHeight":{HEIGHT},"rotation":0,"dpis":[72,144,288],"ink":[0,0,0,255],"background":[255,255,255,255],"glyphShape":{{"rectFormat":"x0,y0,x1,y1","rects":{RECTS:?}}},"fixtures":[{{"id":"strokes","file":"strokes.pdf","segments":[{segments}],"glyphs":[]}},{{"id":"type3","file":"type3.pdf","segments":[],"glyphs":[{glyphs}]}}]}}
"#
    )
}

/// Export only on explicit request, into this checkout's ignored target directory.
pub fn export() -> Option<PathBuf> {
    static EXPORTED: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    EXPORTED.get_or_init(export_once).clone()
}

fn export_once() -> Option<PathBuf> {
    let requested = std::env::var_os("KITION_PDF_GEOMETRY_OUT")?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..").canonicalize().unwrap();
    let target = root.join("target");
    std::fs::create_dir_all(&target).unwrap();
    let target = target.canonicalize().unwrap();
    let output = PathBuf::from(requested);
    assert!(output.is_absolute(), "KITION_PDF_GEOMETRY_OUT must be explicit and absolute");
    assert!(
        output.starts_with(&target) && !output.components().any(|part| part == std::path::Component::ParentDir),
        "fixture exports must stay inside this checkout target directory"
    );
    std::fs::create_dir_all(&output).unwrap();
    let output = output.canonicalize().unwrap();
    assert!(output.starts_with(&target), "fixture exports must stay inside this checkout's ignored target directory");
    for (name, bytes) in [("strokes.pdf", strokes()), ("type3.pdf", glyphs()), ("manifest.json", manifest().into_bytes())] {
        let path = output.join(name);
        if path.exists() {
            assert_eq!(std::fs::read(path).unwrap(), bytes, "refuse to replace different fixture evidence");
        } else {
            std::fs::write(path, bytes).unwrap();
        }
    }
    Some(output)
}
