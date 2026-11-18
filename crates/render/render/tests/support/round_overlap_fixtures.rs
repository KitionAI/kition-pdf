//! Contributor-original round caps and noncollinear parallel overlap diagnostics.
use std::{f64::consts::PI, fmt::Write, path::PathBuf};
pub const PAGE_SIDE: u32 = 64;
pub const LENGTH: f64 = 1.0 / 8192.0;
pub const SAGITTA_LIMIT: f64 = 1.0 / 65536.0;
pub const VERTEX_LIMIT: f64 = 1.0 / 65536.0;
pub const BOUNDARY_LIMIT: f64 = SAGITTA_LIMIT + VERTEX_LIMIT;

#[derive(Clone)]
pub struct Case {
    pub id: String,
    pub layout: &'static str,
    pub line_width: f64,
    pub scale: u32,
    pub origin: [f64; 2],
    pub opacity: f64,
    pub length: f64,
    pub cap: u32,
    pub calibration: bool,
}
pub struct Polygon {
    pub vertices: Vec<[f64; 2]>,
    pub max_sagitta: f64,
    pub max_vertex_displacement: f64,
}
impl Case {
    pub fn width(&self) -> f64 {
        if self.line_width == 0.0 { 1.0 / f64::from(self.scale) } else { self.line_width }
    }
    pub fn segments(&self) -> Vec<[[f64; 2]; 2]> {
        let [x, y] = self.origin;
        let first = [[x, y], [x + self.length, y]];
        match self.layout {
            "duplicate-one-paint" | "duplicate-two-paints" => vec![first, first],
            "parallel-y-shift" => vec![first, [[x, y + self.width() / 2.0], [x + self.length, y + self.width() / 2.0]]],
            "single" => vec![first],
            _ => panic!("unknown original layout"),
        }
    }
    pub fn true_area(&self) -> f64 {
        let r = self.width() / 2.0;
        if self.cap == 2 {
            3.0 * r * (2.0 * r + self.length)
        } else if self.layout == "parallel-y-shift" {
            3.0 * r * self.length + (4.0 * PI / 3.0 + 3.0_f64.sqrt() / 2.0) * r * r
        } else {
            2.0 * r * self.length + PI * r * r
        }
    }
    pub fn true_perimeter(&self) -> f64 {
        let r = self.width() / 2.0;
        if self.cap == 2 {
            10.0 * r + 2.0 * self.length
        } else if self.layout == "parallel-y-shift" {
            2.0 * self.length + 8.0 * PI * r / 3.0
        } else {
            2.0 * self.length + 2.0 * PI * r
        }
    }
    pub fn area_error_bound(&self) -> f64 {
        if self.cap == 2 { 0.0 } else { 2.0 * self.true_perimeter() * f64::from(self.scale) * BOUNDARY_LIMIT + PI * BOUNDARY_LIMIT.powi(2) }
    }
    pub fn polygon(&self) -> Polygon {
        let [x, y] = self.origin;
        let r = self.width() / 2.0;
        let mut ideal = vec![[x, y - r], [x + self.length, y - r]];
        let mut max_sagitta = 0.0_f64;
        if self.cap == 2 {
            ideal = vec![[x - r, y - r], [x + self.length + r, y - r], [x + self.length + r, y + 2.0 * r], [x - r, y + 2.0 * r]];
        } else {
            let mut arc = |center: [f64; 2], from: f64, to: f64, endpoint: [f64; 2], increasing_y: bool| {
                let device_radius = r * f64::from(self.scale);
                let max_angle = 2.0 * (1.0 - SAGITTA_LIMIT / device_radius).acos();
                let count = ((to - from) / max_angle).ceil() as u32;
                assert!(count > 0 && count < 10000);
                let actual_sagitta = device_radius * (1.0 - ((to - from) / f64::from(count) / 2.0).cos());
                assert!(actual_sagitta <= SAGITTA_LIMIT);
                max_sagitta = max_sagitta.max(actual_sagitta);
                for i in 1..=count {
                    let theta = from + (to - from) * f64::from(i) / f64::from(count);
                    let point = if i == count { endpoint } else { [center[0] + r * theta.cos(), center[1] + r * theta.sin()] };
                    let previous = ideal.last().unwrap();
                    assert!(if increasing_y { point[1] >= previous[1] - 1e-12 } else { point[1] <= previous[1] + 1e-12 });
                    ideal.push(point);
                }
            };
            if self.layout == "parallel-y-shift" {
                let crossing_y = y + r / 2.0;
                let right_crossing = [x + self.length + 3.0_f64.sqrt() * r / 2.0, crossing_y];
                arc([x + self.length, y], -PI / 2.0, PI / 6.0, right_crossing, true);
                arc([x + self.length, y + r], -PI / 6.0, PI / 2.0, [x + self.length, y + 2.0 * r], true);
                ideal.push([x, y + 2.0 * r]);
                // Reborrow after the explicit straight top edge.
                append_left_arcs(self, &mut ideal, &mut max_sagitta);
            } else {
                arc([x + self.length, y], -PI / 2.0, PI / 2.0, [x + self.length, y + r], true);
                ideal.push([x, y + r]);
                append_left_arcs(self, &mut ideal, &mut max_sagitta);
            }
        }
        if ideal.last() == ideal.first() {
            ideal.pop();
        }
        let mut max_vertex_displacement = 0.0_f64;
        let mut vertices = Vec::new();
        for point in ideal {
            let rounded = point.map(|coordinate| f64::from(coordinate as f32));
            let displacement = (rounded[0] - point[0]).hypot(rounded[1] - point[1]) * f64::from(self.scale);
            max_vertex_displacement = max_vertex_displacement.max(displacement);
            assert!(displacement <= VERTEX_LIMIT);
            for coordinate in rounded {
                assert_eq!(format!("{coordinate:.12}").parse::<f32>().unwrap(), coordinate as f32);
            }
            if vertices.last() != Some(&rounded) {
                vertices.push(rounded);
            }
        }
        if vertices.last() == vertices.first() {
            vertices.pop();
        }
        assert!(vertices.len() >= 4);
        Polygon { vertices, max_sagitta, max_vertex_displacement }
    }
    pub fn variants(&self) -> Vec<&'static str> {
        if self.layout == "duplicate-two-paints" { vec!["sample", "union-fill", "repeated-fill"] } else { vec!["sample", "union-fill"] }
    }
}
fn append_left_arcs(case: &Case, ideal: &mut Vec<[f64; 2]>, max_sagitta: &mut f64) {
    let [x, y] = case.origin;
    let r = case.width() / 2.0;
    let arcs = if case.layout == "parallel-y-shift" {
        vec![
            ([x, y + r], PI / 2.0, 7.0 * PI / 6.0, [x - 3.0_f64.sqrt() * r / 2.0, y + r / 2.0]),
            ([x, y], 5.0 * PI / 6.0, 3.0 * PI / 2.0, [x, y - r]),
        ]
    } else {
        vec![([x, y], PI / 2.0, 3.0 * PI / 2.0, [x, y - r])]
    };
    for (center, from, to, endpoint) in arcs {
        let radius = r * f64::from(case.scale);
        let count = ((to - from) / (2.0 * (1.0 - SAGITTA_LIMIT / radius).acos())).ceil() as u32;
        assert!(count > 0 && count < 10000);
        let sagitta = radius * (1.0 - ((to - from) / f64::from(count) / 2.0).cos());
        assert!(sagitta <= SAGITTA_LIMIT);
        *max_sagitta = max_sagitta.max(sagitta);
        for i in 1..=count {
            let theta = from + (to - from) * f64::from(i) / f64::from(count);
            let point = if i == count { endpoint } else { [center[0] + r * theta.cos(), center[1] + r * theta.sin()] };
            assert!(point[1] <= ideal.last().unwrap()[1] + 1e-12);
            ideal.push(point);
        }
    }
}
pub fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for layout in ["single", "duplicate-one-paint", "duplicate-two-paints", "parallel-y-shift"] {
        for (width_name, line_width) in [("hairline", 0.0), ("width-two", 2.0)] {
            for (phase_name, phase) in [("quarter", 0.25), ("half", 0.5)] {
                for (alpha_name, opacity) in [("opaque", 1.0), ("half-alpha", 0.5)] {
                    for scale in [1, 2, 4] {
                        cases.push(Case {
                            id: format!("round-{layout}-{width_name}-{phase_name}-{alpha_name}-s{scale}"),
                            layout,
                            line_width,
                            scale,
                            origin: [32.0 + phase, 32.0 + phase],
                            opacity,
                            length: LENGTH,
                            cap: 1,
                            calibration: false,
                        });
                    }
                }
            }
        }
    }
    for (phase_name, phase) in [("quarter", 0.25), ("half", 0.5)] {
        for (alpha_name, opacity) in [("opaque", 1.0), ("half-alpha", 0.5)] {
            for scale in [1, 2, 4] {
                cases.push(Case {
                    id: format!("round-long-single-{phase_name}-{alpha_name}-s{scale}"),
                    layout: "single",
                    line_width: 2.0,
                    scale,
                    origin: [32.0 + phase, 32.0 + phase],
                    opacity,
                    length: 2.0,
                    cap: 1,
                    calibration: true,
                });
            }
        }
    }
    for scale in [1, 2, 4] {
        cases.push(Case {
            id: format!("square-parallel-y-shift-calibration-s{scale}"),
            layout: "parallel-y-shift",
            line_width: 2.0,
            scale,
            origin: [32.25, 32.25],
            opacity: 0.5,
            length: LENGTH,
            cap: 2,
            calibration: true,
        });
    }
    cases
}
fn segment(content: &mut String, start: [f64; 2], end: [f64; 2]) {
    for coordinate in start.into_iter().chain(end) {
        assert_eq!(f64::from(coordinate as f32), coordinate);
    }
    assert!(end[0] as f32 > start[0] as f32);
    writeln!(content, "{:.12} {:.12} m {:.12} {:.12} l", start[0], start[1], end[0], end[1]).unwrap();
}
fn fill_union(content: &mut String, case: &Case) {
    let polygon = case.polygon();
    for (index, point) in polygon.vertices.iter().enumerate() {
        writeln!(content, "{:.12} {:.12} {}", point[0], point[1], if index == 0 { "m" } else { "l" }).unwrap();
    }
    content.push_str("h f\n");
}
pub fn pdf(case: &Case, variant: &str) -> Vec<u8> {
    let mut content = format!("q /GS gs 0 G 0 g {} w {} J 0 j 4 M\n", case.line_width, case.cap);
    if variant == "sample" {
        for [start, end] in case.segments() {
            segment(&mut content, start, end);
            if case.layout == "duplicate-two-paints" {
                content.push_str("S\n");
            }
        }
        if case.layout != "duplicate-two-paints" {
            content.push_str("S\n");
        }
    } else {
        assert!(variant == "union-fill" || variant == "repeated-fill");
        fill_union(&mut content, case);
        if variant == "repeated-fill" {
            assert_eq!(case.layout, "duplicate-two-paints");
            fill_union(&mut content, case);
        }
    }
    content.push_str("Q\n1 0 0 rg 4 4 4 4 re f\n");
    finish_pdf(&content, case.opacity)
}
fn finish_pdf(content: &str, opacity: f64) -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 64 64] /Resources << /ExtGState << /GS 5 0 R >> >> /Contents 4 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        format!("<< /Type /ExtGState /CA {} /ca {} >>", opacity, opacity),
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
    let out = PathBuf::from(std::env::var_os("KITION_PDF_ROUND_OVERLAP_OUT")?);
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
        assert_eq!(std::fs::read(path).unwrap(), bytes, "refuse changed round-overlap evidence");
    } else {
        std::fs::write(path, bytes).unwrap();
    }
}
pub fn export(cases: &[Case], out: &std::path::Path) {
    let mut rows = Vec::new();
    for case in cases {
        let polygon = case.polygon();
        for variant in case.variants() {
            write(out, &format!("{}-{variant}.pdf", case.id), &pdf(case, variant));
        }
        let repeated = if case.layout == "duplicate-two-paints" { format!("\"{}-repeated-fill.pdf\"", case.id) } else { "null".to_string() };
        let device_origin = case.origin.map(|coordinate| coordinate * f64::from(case.scale));
        let device_phase = device_origin.map(|coordinate| coordinate.fract());
        rows.push(format!(
            r#"{{"id":"{}","layout":"{}","scale":{},"lineWidth":{},"effectiveWidth":{},"origin":{:?},"deviceOrigin":{device_origin:?},"deviceOriginFraction":{device_phase:?},"length":{},"opacity":{},"cap":{},"calibration":{},"sample":"{}-sample.pdf","unionControl":"{}-union-fill.pdf","repeatedControl":{repeated},"segments":{:?},"polygon":{:?},"trueArea":{},"truePerimeter":{},"deviceAreaErrorBound":{},"maxDeviceSagitta":{},"maxDeviceVertexDisplacement":{}}}"#,
            case.id, case.layout, case.scale, case.line_width, case.width(), case.origin, case.length, case.opacity, case.cap, case.calibration,
            case.id, case.id, case.segments(), polygon.vertices, case.true_area(), case.true_perimeter(), case.area_error_bound(),
            polygon.max_sagitta, polygon.max_vertex_displacement
        ));
    }
    write(out, "diagnostic-manifest.json", format!(
        r#"{{"schema":"round-overlap-diagnostic/1","origin":"Contributor-original capsule and vertical parallel geometry, Apache-2.0","purpose":"Neutral overlap and compositing observations; no expectedMatch","pageWidth":{PAGE_SIDE},"pageHeight":{PAGE_SIDE},"coordinateSpace":"pdf-user-space-y-up","controlChannelTolerance":2,"analyticControlModel":"Authored-polygon pixel box-filter coverage evaluated in f64; not a universal PDF AA rule","deviceSagittaLimit":{SAGITTA_LIMIT},"deviceVertexDisplacementLimit":{VERTEX_LIMIT},"deviceBoundaryDisplacementLimit":{BOUNDARY_LIMIT},"areaBoundFormula":"2 * trueDevicePerimeter * deviceBoundaryDisplacementLimit + pi * deviceBoundaryDisplacementLimit^2; simple oriented corresponding boundaries","parallelOverlapOrientation":"Separate horizontal centerlines; y displacement effectiveWidth/2, x displacement zero","rows":[{}]}}"#,
        rows.join(",")
    ).as_bytes());
}
