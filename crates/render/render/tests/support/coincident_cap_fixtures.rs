//! Contributor-original neutral square-cap overlap diagnostics; no external assets.
use std::{fmt::Write, path::PathBuf};

pub const PAGE_SIDE: u32 = 64;
pub const LENGTH: f64 = 1.0 / 8192.0;
pub const LAYOUTS: [&str; 5] = ["single", "duplicate-one-paint", "duplicate-two-paints", "parallel-overlap", "orthogonal-overlap"];

#[derive(Clone)]
pub struct Case {
    pub id: String,
    pub layout: &'static str,
    pub line_width: f64,
    pub scale: u32,
    pub origin: [f64; 2],
    pub opacity: f64,
}
impl Case {
    pub fn width(&self) -> f64 {
        if self.line_width == 0.0 { 1.0 / f64::from(self.scale) } else { self.line_width }
    }
    pub fn rectangles(&self) -> Vec<[f64; 4]> {
        let [x, y] = self.origin;
        let w = self.width();
        let r = w / 2.0;
        let horizontal = [x - r, y - r, x + LENGTH + r, y + r];
        match self.layout {
            "parallel-overlap" => vec![horizontal, [x + w / 4.0 - r, y - r, x + w / 4.0 + LENGTH + r, y + r]],
            "orthogonal-overlap" => vec![horizontal, [x - r, y - r, x + r, y + LENGTH + r]],
            _ => vec![horizontal],
        }
    }
    pub fn union_outline(&self) -> Vec<[f64; 2]> {
        let [x, y] = self.origin;
        let w = self.width();
        let r = w / 2.0;
        if self.layout == "orthogonal-overlap" {
            vec![[x - r, y - r], [x + LENGTH + r, y - r], [x + LENGTH + r, y + r], [x + r, y + r], [x + r, y + LENGTH + r], [x - r, y + LENGTH + r]]
        } else {
            let right = x + LENGTH + r + if self.layout == "parallel-overlap" { w / 4.0 } else { 0.0 };
            vec![[x - r, y - r], [right, y - r], [right, y + r], [x - r, y + r]]
        }
    }
    pub fn union_area(&self) -> f64 {
        let w = self.width();
        match self.layout {
            "parallel-overlap" => w * (1.25 * w + LENGTH),
            "orthogonal-overlap" => w * w + 2.0 * w * LENGTH,
            _ => w * (w + LENGTH),
        }
    }
    pub fn variants(&self) -> Vec<&'static str> {
        if self.layout == "duplicate-two-paints" { vec!["sample", "union-fill", "repeated-fill"] } else { vec!["sample", "union-fill"] }
    }
}

pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for layout in LAYOUTS {
        for (width_name, line_width) in [("hairline", 0.0), ("width-two", 2.0)] {
            for (phase_name, phase) in [("quarter", 0.25), ("half", 0.5)] {
                for (alpha_name, opacity) in [("opaque", 1.0), ("half-alpha", 0.5)] {
                    for scale in [1, 2, 4] {
                        cases.push(Case {
                            id: format!("{layout}-{width_name}-{phase_name}-{alpha_name}-s{scale}"),
                            layout,
                            line_width,
                            scale,
                            origin: [32.0 + phase, 32.0 + phase],
                            opacity,
                        });
                    }
                }
            }
        }
    }
    cases
}
fn segment(content: &mut String, start: [f64; 2], end: [f64; 2]) {
    for coordinate in start.into_iter().chain(end) {
        assert_eq!(f64::from(coordinate as f32), coordinate, "coordinates must parse exactly");
    }
    writeln!(content, "{:.12} {:.12} m {:.12} {:.12} l", start[0], start[1], end[0], end[1]).unwrap();
}
fn fill_union(content: &mut String, case: &Case) {
    let outline = case.union_outline();
    writeln!(content, "{:.12} {:.12} m", outline[0][0], outline[0][1]).unwrap();
    for point in &outline[1..] {
        assert!(point.iter().all(|&value| f64::from(value as f32) == value));
        writeln!(content, "{:.12} {:.12} l", point[0], point[1]).unwrap();
    }
    content.push_str("h f\n");
}
pub fn pdf(case: &Case, variant: &str) -> Vec<u8> {
    let mut content = format!("q /GS gs 0 G 0 g {} w 2 J 0 j 4 M\n", case.line_width);
    let [x, y] = case.origin;
    if variant == "sample" {
        segment(&mut content, [x, y], [x + LENGTH, y]);
        match case.layout {
            "duplicate-one-paint" => segment(&mut content, [x, y], [x + LENGTH, y]),
            "duplicate-two-paints" => {
                content.push_str("S\n");
                segment(&mut content, [x, y], [x + LENGTH, y]);
            }
            "parallel-overlap" => {
                let shift = case.width() / 4.0;
                segment(&mut content, [x + shift, y], [x + shift + LENGTH, y]);
            }
            "orthogonal-overlap" => segment(&mut content, [x, y], [x, y + LENGTH]),
            "single" => {}
            _ => panic!("unknown original layout"),
        }
        content.push_str("S\n");
    } else {
        assert!(variant == "union-fill" || variant == "repeated-fill");
        fill_union(&mut content, case);
        if variant == "repeated-fill" {
            assert_eq!(case.layout, "duplicate-two-paints");
            fill_union(&mut content, case);
        }
    }
    content.push_str("Q\n1 0 0 rg 4 4 4 4 re f\n");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 64 64] /Resources << /ExtGState << /GS 5 0 R >> >> /Contents 4 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        format!("<< /Type /ExtGState /CA {} /ca {} >>", case.opacity, case.opacity),
    ];
    let mut out = String::from("%PDF-1.7\n");
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        writeln!(out, "{} 0 obj\n{object}\nendobj", i + 1).unwrap();
    }
    let xref = out.len();
    out.push_str("xref\n0 6\n0000000000 65535 f \n");
    for offset in offsets {
        writeln!(out, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(out, "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF").unwrap();
    out.into_bytes()
}
pub fn output() -> Option<PathBuf> {
    let out = PathBuf::from(std::env::var_os("KITION_PDF_COINCIDENT_CAP_OUT")?);
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../target").canonicalize().unwrap();
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
        assert_eq!(std::fs::read(path).unwrap(), bytes, "refuse changed coincident-cap evidence");
    } else {
        std::fs::write(path, bytes).unwrap();
    }
}
pub fn export(cases: &[Case], out: &std::path::Path) {
    let mut rows = Vec::new();
    for case in cases {
        for variant in case.variants() {
            write(out, &format!("{}-{variant}.pdf", case.id), &pdf(case, variant));
        }
        let repeated = if case.layout == "duplicate-two-paints" { format!("\"{}-repeated-fill.pdf\"", case.id) } else { "null".to_string() };
        let device_origin = case.origin.map(|coordinate| coordinate * f64::from(case.scale));
        let device_phase = device_origin.map(|coordinate| coordinate.fract());
        rows.push(format!(
            r#"{{"id":"{}","layout":"{}","scale":{},"lineWidth":{},"effectiveWidth":{},"origin":{:?},"deviceOrigin":{device_origin:?},"deviceOriginFraction":{device_phase:?},"length":{LENGTH},"opacity":{},"sample":"{}-sample.pdf","unionControl":"{}-union-fill.pdf","repeatedControl":{repeated},"rectangles":{:?},"unionOutline":{:?},"unionArea":{}}}"#,
            case.id, case.layout, case.scale, case.line_width, case.width(), case.origin, case.opacity, case.id, case.id,
            case.rectangles(), case.union_outline(), case.union_area()
        ));
    }
    write(out, "diagnostic-manifest.json", format!(
        r#"{{"schema":"coincident-cap-diagnostic/1","origin":"Contributor-original axis-aligned geometry, Apache-2.0","purpose":"Neutral overlap and compositing observations; no expectedMatch","pageWidth":{PAGE_SIDE},"pageHeight":{PAGE_SIDE},"coordinateSpace":"pdf-user-space-y-up","controlChannelTolerance":2,"analyticControlModel":"Axis-aligned rectangle union under pixel box-filter coverage; not a universal PDF AA rule","parallelOverlapOrientation":"Collinear horizontal segments; displacement equals effectiveWidth/4 along x, zero along y","rows":[{}]}}"#,
        rows.join(",")
    ).as_bytes());
}
