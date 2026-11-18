//! Original geometry for comparing competing exact-boundary cap hypotheses.
//! These controls are diagnostic alternatives, not PDF conformance verdicts.
use std::{fmt::Write, path::PathBuf};

pub struct Case {
    pub id: String,
    pub dashes: Vec<u32>,
    pub phase: u32,
    pub length: u32,
    pub runs: Vec<(u32, u32)>,
    pub dots: Vec<u32>,
    pub boundary: u32,
    pub cap: u32,
    pub separated: bool,
}
pub fn cases() -> Vec<Case> {
    let tables = [
        ("initial-even", vec![0, 12, 6, 12], 18, 50, vec![(24, 30)], vec![12, 42], 0),
        ("initial-odd", vec![0, 8, 4], 20, 30, vec![(12, 16), (16, 24)], vec![4, 28], 0),
        ("terminal-even", vec![0, 12, 6, 12], 0, 30, vec![(12, 18)], vec![0], 30),
        ("terminal-odd", vec![0, 8, 4], 3, 21, vec![(5, 9), (9, 17)], vec![], 21),
        ("terminal-zero-chain", vec![6, 12, 0, 0, 0, 12], 0, 18, vec![(0, 6)], vec![], 18),
        ("terminal-positive-start", vec![0, 12, 6, 12], 0, 12, vec![], vec![0], 12),
    ];
    let mut out = Vec::new();
    for (id, dashes, phase, length, runs, dots, boundary) in tables {
        for cap in [1, 2] {
            for separated in [false, true] {
                out.push(Case {
                    id: format!("boundary-{id}-{}-{}", if cap == 1 { "round" } else { "square" }, if separated { "separated" } else { "single" }),
                    dashes: dashes.clone(),
                    phase,
                    length,
                    runs: runs.clone(),
                    dots: dots.clone(),
                    boundary,
                    cap,
                    separated,
                });
            }
        }
    }
    out
}
pub const VARIANTS: [&str; 3] = ["sample", "without-boundary", "with-boundary"];
pub fn point(line: usize, distance: f64) -> [f64; 2] {
    if line == 0 { [24.5 + distance, 40.5] } else { [100.5 - distance, 80.5] }
}
fn segment(out: &mut String, start: [f64; 2], end: [f64; 2]) {
    writeln!(out, "{:.12} {:.12} m {:.12} {:.12} l", start[0], start[1], end[0], end[1]).unwrap();
}
pub fn pdf(case: &Case, variant: &str) -> Vec<u8> {
    assert!(VARIANTS.contains(&variant));
    let mut content = format!("q 0 G 0 w {} J 0 j 4 M\n", case.cap);
    if variant == "sample" {
        let pattern = case.dashes.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ");
        writeln!(content, "[{pattern}] {} d", case.phase).unwrap();
    }
    for line in 0..if case.separated { 2 } else { 1 } {
        if variant == "sample" {
            segment(&mut content, point(line, 0.0), point(line, f64::from(case.length)));
        } else {
            for &(start, end) in &case.runs {
                segment(&mut content, point(line, f64::from(start)), point(line, f64::from(end)));
            }
            let mut dots = case.dots.clone();
            if variant == "with-boundary" {
                dots.push(case.boundary);
            }
            for dot in dots {
                // A tiny positive solid segment gives an independent direction-bearing
                // cap without relying on zero-length subpath or dash behavior.
                let distance = f64::from(dot);
                segment(&mut content, point(line, distance), point(line, distance + 1.0 / 8192.0));
            }
        }
    }
    content.push_str("S Q\n1 0 0 rg 4 4 4 4 re f\n");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 128] /Resources << >> /Contents 4 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
    ];
    let mut out = String::from("%PDF-1.7\n");
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        writeln!(out, "{} 0 obj\n{object}\nendobj", i + 1).unwrap();
    }
    let xref = out.len();
    out.push_str("xref\n0 5\n0000000000 65535 f \n");
    for offset in offsets {
        writeln!(out, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(out, "trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF").unwrap();
    out.into_bytes()
}
pub fn output() -> Option<PathBuf> {
    let requested = std::env::var_os("KITION_PDF_BOUNDARY_DASH_OUT")?;
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../target").canonicalize().unwrap();
    let out = PathBuf::from(requested);
    assert!(out.is_absolute() && out.starts_with(&target));
    assert!(!out.components().any(|part| part == std::path::Component::ParentDir));
    std::fs::create_dir_all(&out).unwrap();
    let out = out.canonicalize().unwrap();
    assert!(out.starts_with(target));
    Some(out)
}
pub fn write(out: &std::path::Path, name: &str, bytes: &[u8]) {
    let path = out.join(name);
    if path.exists() {
        assert_eq!(std::fs::read(path).unwrap(), bytes, "refuse changed boundary evidence");
    } else {
        std::fs::write(path, bytes).unwrap();
    }
}
pub fn export(cases: &[Case], out: &std::path::Path) {
    let mut rows = Vec::new();
    for case in cases {
        for variant in VARIANTS {
            write(out, &format!("{}-{variant}.pdf", case.id), &pdf(case, variant));
        }
        rows.push(format!(r#"{{"id":"{}","dashes":{:?},"phase":{},"length":{},"runs":{:?},"dots":{:?},"boundary":{},"cap":{},"separated":{},"sample":"{}-sample.pdf","alternatives":["{}-without-boundary.pdf","{}-with-boundary.pdf"]}}"#,
            case.id, case.dashes, case.phase, case.length,
            case.runs.iter().map(|&(a,b)| [a,b]).collect::<Vec<_>>(),
            case.dots, case.boundary, case.cap, case.separated, case.id, case.id, case.id));
    }
    write(out, "diagnostic-manifest.json", format!(r#"{{"schema":"boundary-diagnostic/1","origin":"Contributor-original geometry, Apache-2.0","purpose":"Competing cap hypotheses; no normative expectedMatch selected","pageWidth":128,"pageHeight":128,"scales":[1,2,4],"variants":["sample","without-boundary","with-boundary"],"cases":[{}]}}"#, rows.join(",")).as_bytes());
}
