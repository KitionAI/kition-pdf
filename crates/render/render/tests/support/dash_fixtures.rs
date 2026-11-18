//! Contributor-original dashed centerlines and independently authored solid controls.
use std::{fmt::Write, path::PathBuf};

pub type Point = [f64; 2];
#[derive(Clone)]
pub enum Op {
    Move(Point),
    Line(Point),
    Curve(Point, Point, Point),
    Close,
}
#[derive(Clone)]
pub struct Case {
    pub id: &'static str,
    pub matrix: [f64; 6],
    pub path: Vec<Op>,
    pub reference: Vec<Op>,
    pub dashes: Vec<f64>,
    pub phase: f64,
    pub cap: u32,
    pub join: u32,
    pub opacity: f64,
    pub width: f64,
    pub clip: bool,
    pub analytic_line: Option<(Point, Point)>,
    pub dots: Vec<Point>,
    pub classification: &'static str,
    pub alpha_cross: bool,
}
const RUNS: &[(f64, f64)] = &[(0.0, 12.0), (24.0, 36.0), (48.0, 60.0), (72.0, 84.0)];
const SHIFTED: &[(f64, f64)] = &[(0.0, 6.0), (18.0, 30.0), (42.0, 54.0), (66.0, 78.0), (90.0, 96.0)];
fn runs(p0: Point, p1: Point, spans: &[(f64, f64)]) -> Vec<Op> {
    let length = (p1[0] - p0[0]).hypot(p1[1] - p0[1]);
    let at = |s: f64| [p0[0] + (p1[0] - p0[0]) * s / length, p0[1] + (p1[1] - p0[1]) * s / length];
    spans.iter().flat_map(|&(a, b)| [Op::Move(at(a)), Op::Line(at(b))]).collect()
}
fn line(id: &'static str, matrix: [f64; 6]) -> Case {
    Case {
        id,
        matrix,
        path: vec![Op::Move([0.0, 0.0]), Op::Line([96.0, 0.0])],
        reference: runs([0.0, 0.0], [96.0, 0.0], RUNS),
        dashes: vec![12.0, 12.0],
        phase: 0.0,
        cap: 0,
        join: 0,
        opacity: 1.0,
        width: 0.0,
        clip: false,
        analytic_line: Some(([0.0, 0.0], [96.0, 0.0])),
        dots: vec![],
        classification: "normative-hairline",
        alpha_cross: false,
    }
}
// Keep the frozen PDF decimal serialization exactly reproducible.
#[allow(clippy::approx_constant)]
pub fn cases() -> Vec<Case> {
    let flat = [1.0, 0.0, 0.0, 0.01, 16.0, 64.5];
    let mut cases = vec![
        line("identity", [1.0, 0.0, 0.0, 1.0, 16.0, 64.5]),
        line("flat-normal", flat),
        line("wide-normal", [1.0, 0.0, 0.0, 100.0, 16.0, 64.5]),
        line("sheared-normal", [1.0, 0.0, 5.0, 1.0, 16.0, 64.5]),
        line("rotated-flat", [0.0, 1.0, -0.01, 0.0, 64.5, 16.0]),
        line("reflected-flat", [-1.0, 0.0, 0.0, 0.01, 112.0, 64.5]),
        line("diagonal-flat", [0.707106781187, 0.707106781187, -0.007071067812, 0.007071067812, 24.0, 24.0]),
    ];
    let mut uniform = line("uniform-four", [4.0, 0.0, 0.0, 4.0, 16.0, 64.5]);
    uniform.path = vec![Op::Move([0.0, 0.0]), Op::Line([24.0, 0.0])];
    uniform.reference = runs([0.0, 0.0], [24.0, 0.0], &[(0.0, 3.0), (6.0, 9.0), (12.0, 15.0), (18.0, 21.0)]);
    uniform.dashes = vec![3.0, 3.0];
    uniform.analytic_line = Some(([0.0, 0.0], [24.0, 0.0]));
    cases.push(uniform);
    for (id, cap) in [("round-cap", 1), ("square-cap", 2)] {
        let mut c = line(id, flat);
        c.cap = cap;
        cases.push(c);
    }
    for (id, phase) in [("phase-six", 6.0), ("phase-large", 240006.0)] {
        let mut c = line(id, flat);
        c.phase = phase;
        c.reference = runs([0.0, 0.0], [96.0, 0.0], SHIFTED);
        cases.push(c);
    }
    let mut odd = line("odd-array", flat);
    odd.dashes = vec![8.0, 4.0, 12.0];
    odd.reference = runs([0.0, 0.0], [96.0, 0.0], &[(0.0, 8.0), (12.0, 24.0), (32.0, 36.0), (48.0, 56.0), (60.0, 72.0), (80.0, 84.0)]);
    cases.push(odd);
    for (id, cap) in [("round-dots", 1), ("square-dots", 2)] {
        let mut c = line(id, flat);
        c.cap = cap;
        c.dashes = vec![0.0, 24.0];
        c.analytic_line = None;
        c.dots = vec![[0.0, 0.0], [24.0, 0.0], [48.0, 0.0], [72.0, 0.0]];
        c.reference = c.dots.iter().flat_map(|&p| [Op::Move(p), Op::Line(p)]).collect();
        cases.push(c);
    }
    let mut zero_gap = line("zero-gaps", flat);
    zero_gap.dashes = vec![24.0, 0.0];
    zero_gap.cap = 1;
    zero_gap.analytic_line = None;
    zero_gap.reference = runs([0.0, 0.0], [96.0, 0.0], &[(0.0, 24.0), (24.0, 48.0), (48.0, 72.0), (72.0, 96.0)]);
    cases.push(zero_gap);
    let mut clipped = line("clipped-phase", flat);
    clipped.clip = true;
    clipped.phase = 6.0;
    clipped.analytic_line = None;
    clipped.reference = runs([0.0, 0.0], [96.0, 0.0], SHIFTED);
    cases.push(clipped);
    for (id, phase, reference, join) in [
        (
            "closed-seam-gap",
            0.0,
            vec![
                Op::Move([0.0, 0.0]),
                Op::Line([24.0, 0.0]),
                Op::Line([24.0, 6.0]),
                Op::Move([24.0, 24.0]),
                Op::Line([0.0, 24.0]),
                Op::Line([0.0, 18.0]),
            ],
            0,
        ),
        (
            "closed-seam-join",
            12.0,
            vec![
                Op::Move([0.0, 12.0]),
                Op::Line([0.0, 0.0]),
                Op::Line([18.0, 0.0]),
                Op::Move([24.0, 12.0]),
                Op::Line([24.0, 24.0]),
                Op::Line([6.0, 24.0]),
            ],
            0,
        ),
        (
            "closed-round-join",
            12.0,
            vec![
                Op::Move([0.0, 12.0]),
                Op::Line([0.0, 0.0]),
                Op::Line([18.0, 0.0]),
                Op::Move([24.0, 12.0]),
                Op::Line([24.0, 24.0]),
                Op::Line([6.0, 24.0]),
            ],
            1,
        ),
    ] {
        let mut c = line(id, [3.0, 0.0, 0.0, 0.5, 24.0, 48.0]);
        c.path = vec![Op::Move([0.0, 0.0]), Op::Line([24.0, 0.0]), Op::Line([24.0, 24.0]), Op::Line([0.0, 24.0]), Op::Close];
        c.dashes = vec![30.0, 18.0];
        c.phase = phase;
        c.reference = reference;
        c.join = join;
        c.cap = 2;
        c.analytic_line = None;
        cases.push(c);
    }
    let mut closed = cases.iter().find(|c| c.id == "closed-seam-join").unwrap().clone();
    closed.id = "closed-all-on";
    closed.dashes = vec![120.0, 12.0];
    closed.phase = 0.0;
    closed.reference = closed.path.clone();
    cases.push(closed);
    let mut cross = line("alpha-cross-subpaths", [1.0, 0.0, 0.0, 0.25, 16.0, 64.5]);
    cross.path.extend([Op::Move([48.5, -48.0]), Op::Line([48.5, 48.0])]);
    cross.phase = 6.0;
    cross.opacity = 0.5;
    cross.analytic_line = None;
    cross.alpha_cross = true;
    cross.reference = runs([0.0, 0.0], [96.0, 0.0], SHIFTED);
    cross.reference.extend(runs([48.5, -48.0], [48.5, 48.0], SHIFTED));
    cases.push(cross);
    // This original cubic has speed 72*(2*t*t-2*t+1), so each half has exact
    // user-space length 24. The split controls below are de Casteljau at t=1/2.
    for (id, phase, reference) in [
        ("cubic-first-half", 0.0, vec![Op::Move([0.0, 0.0]), Op::Curve([0.0, 12.0], [6.0, 18.0], [12.0, 18.0])]),
        ("cubic-second-half", 24.0, vec![Op::Move([12.0, 18.0]), Op::Curve([18.0, 18.0], [24.0, 12.0], [24.0, 0.0])]),
    ] {
        let mut c = line(id, [2.0, 0.0, 2.0, 0.5, 16.0, 48.0]);
        c.path = vec![Op::Move([0.0, 0.0]), Op::Curve([0.0, 24.0], [24.0, 24.0], [24.0, 0.0])];
        c.dashes = vec![24.0, 24.0];
        c.phase = phase;
        c.reference = reference;
        c.analytic_line = None;
        cases.push(c);
    }
    for (id, dashes) in [("overbudget-solid", vec![0.000001]), ("invalid-solid", vec![-1.0, -1.0])] {
        let mut c = line(id, flat);
        c.dashes = dashes;
        c.reference = c.path.clone();
        c.analytic_line = None;
        c.classification = "sanitized-fallback";
        cases.push(c);
    }
    cases
}
pub fn map(matrix: [f64; 6], p: Point) -> Point {
    let [a, b, c, d, e, f] = matrix;
    [a * p[0] + c * p[1] + e, b * p[0] + d * p[1] + f]
}
fn path_text(ops: &[Op], matrix: Option<[f64; 6]>) -> String {
    let mut out = String::new();
    let point = |p: Point| matrix.map_or(p, |m| map(m, p));
    for op in ops {
        match *op {
            Op::Move(p) | Op::Line(p) => {
                let [x, y] = point(p);
                writeln!(out, "{x:.12} {y:.12} {}", if matches!(op, Op::Move(_)) { "m" } else { "l" }).unwrap();
            }
            Op::Curve(a, b, c) => {
                let [x1, y1] = point(a);
                let [x2, y2] = point(b);
                let [x3, y3] = point(c);
                writeln!(out, "{x1:.12} {y1:.12} {x2:.12} {y2:.12} {x3:.12} {y3:.12} c").unwrap();
            }
            Op::Close => out.push_str("h\n"),
        }
    }
    out
}
pub fn pdf(case: &Case, control: bool) -> Vec<u8> {
    pdf_with_dash_state(case, control, false)
}
pub fn pdf_with_dash_state(case: &Case, control: bool, ext_gstate: bool) -> Vec<u8> {
    let mut content = String::from("q\n");
    if case.clip {
        content.push_str("40 8 48 112 re W n\n");
    }
    if !control {
        let [a, b, c, d, e, f] = case.matrix;
        writeln!(content, "{a} {b} {c} {d} {e} {f} cm").unwrap();
    }
    writeln!(content, "/GS gs 0 G {} w {} J {} j 4 M", case.width, case.cap, case.join).unwrap();
    if !control && !ext_gstate {
        let pattern = case.dashes.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ");
        writeln!(content, "[{pattern}] {} d", case.phase).unwrap();
    }
    content.push_str(&path_text(if control { &case.reference } else { &case.path }, control.then_some(case.matrix)));
    content.push_str("S Q\n1 0 0 rg 4 4 4 4 re f\n");
    let dash_state = if ext_gstate && !control {
        format!(" /D [[{}] {}]", case.dashes.iter().map(ToString::to_string).collect::<Vec<_>>().join(" "), case.phase)
    } else {
        String::new()
    };
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 128] /Resources << /ExtGState << /GS 5 0 R >> >> /Contents 4 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        format!("<< /Type /ExtGState /CA {} /ca {}{dash_state} >>", case.opacity, case.opacity),
    ];
    let mut out = String::from("%PDF-1.7\n");
    let mut offsets = Vec::new();
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        writeln!(out, "{} 0 obj\n{obj}\nendobj", i + 1).unwrap();
    }
    let xref = out.len();
    out.push_str("xref\n0 6\n0000000000 65535 f \n");
    for offset in offsets {
        writeln!(out, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(out, "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF").unwrap();
    out.into_bytes()
}
pub struct InkProbe {
    pub point: Point,
    pub min_mass: f64,
    pub max_mass: f64,
    pub opacity: f64,
}

pub fn ink_probes(case: &Case) -> Vec<InkProbe> {
    let mut probes: Vec<_> = case
        .dots
        .iter()
        .map(|&point| InkProbe {
            point: map(case.matrix, point),
            min_mass: if case.cap == 1 { 0.5 } else { 0.85 },
            max_mass: if case.cap == 1 { 1.1 } else { 1.15 },
            opacity: 1.0,
        })
        .collect();
    let points: &[Point] = match case.id {
        "zero-gaps" | "overbudget-solid" | "invalid-solid" | "clipped-phase" => &[[64.0, 64.5]],
        "closed-seam-gap" | "closed-all-on" => &[[60.0, 48.0]],
        "closed-seam-join" | "closed-round-join" => &[[54.0, 48.0], [24.0, 48.0]],
        "cubic-first-half" => &[[50.5, 54.75]],
        "cubic-second-half" => &[[83.5, 54.75]],
        "alpha-cross-subpaths" => &[[64.5, 64.5]],
        _ => &[],
    };
    probes.extend(points.iter().map(|&point| InkProbe {
        point,
        min_mass: if case.alpha_cross { 3.0 } else { 1.5 },
        max_mass: if case.alpha_cross { 9.0 } else { 6.0 },
        opacity: case.opacity,
    }));
    probes
}

pub fn empty_regions(case: &Case) -> &'static [[u32; 4]] {
    match case.id {
        "clipped-phase" => &[[34, 64, 2, 1], [50, 64, 2, 1]],
        "zero-gaps" | "overbudget-solid" | "invalid-solid" => &[[114, 64, 2, 1]],
        "closed-seam-gap" => &[[95, 55, 2, 2]],
        "closed-seam-join" | "closed-round-join" => &[[86, 47, 2, 2]],
        "closed-all-on" => &[[56, 53, 4, 2]],
        "cubic-first-half" => &[[83, 54, 1, 1]],
        "cubic-second-half" => &[[50, 54, 1, 1]],
        "alpha-cross-subpaths" => &[[26, 64, 2, 1]],
        _ => &[],
    }
}

pub fn export(cases: &[Case]) {
    let Some(requested) = std::env::var_os("KITION_PDF_DASH_OUT") else { return };
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../target").canonicalize().unwrap();
    let out = PathBuf::from(requested);
    assert!(out.is_absolute() && out.starts_with(&target));
    assert!(!out.components().any(|part| part == std::path::Component::ParentDir));
    std::fs::create_dir_all(&out).unwrap();
    let out = out.canonicalize().unwrap();
    assert!(out.starts_with(target));
    let write = |name: &str, bytes: &[u8]| {
        let p = out.join(name);
        if p.exists() {
            assert_eq!(std::fs::read(p).unwrap(), bytes, "refuse changed dash fixture bytes");
        } else {
            std::fs::write(p, bytes).unwrap();
        }
    };
    let mut rows = Vec::new();
    for case in cases {
        for control in [false, true] {
            let id = if control { format!("{}-control", case.id) } else { case.id.to_string() };
            write(&format!("{id}.pdf"), &pdf(case, control));
            let role = if control { "control" } else { "sample" };
            let mut row = format!(
                r#"{{"id":"{id}","file":"{id}.pdf","role":"{role}","classification":"{}","regions":[{{"id":"sentinel","rect":[5,5,2,2],"rgba":[255,0,0,255],"tolerance":2}},{{"id":"empty-corner","rect":[120,120,2,2],"rgba":[255,255,255,255],"tolerance":2}}"#,
                case.classification
            );
            if case.alpha_cross {
                row.push_str(r#",{"id":"single-paint-alpha","scales":[1],"rect":[64,64,1,1],"rgba":[128,128,128,255],"tolerance":2}"#);
            }
            for (index, rect) in empty_regions(case).iter().enumerate() {
                write!(row, r#",{{"id":"authored-empty-{index}","rect":{rect:?},"rgba":[255,255,255,255],"tolerance":2}}"#).unwrap();
            }
            row.push(']');
            if !control {
                write!(row, r#", "expectedMatch":"{}-control""#, case.id).unwrap();
            }
            if let Some((start, end)) = case.analytic_line {
                let cap = match case.cap {
                    1 => "round",
                    2 => "square",
                    _ => "butt",
                };
                write!(row,r#", "analyticLine":{{"localStart":{start:?},"localEnd":{end:?},"matrix":{:?},"dashArray":{:?},"dashOffset":{},"cap":"{cap}","opacity":{}}}"#,case.matrix,case.dashes,case.phase,case.opacity).unwrap();
            }
            let probes = ink_probes(case);
            if !probes.is_empty() {
                let points = probes
                    .iter()
                    .enumerate()
                    .map(|(i, probe)| {
                        format!(
                            r#"{{"id":"ink-{i}","point":{:?},"radius":2,"minMass":{},"maxMass":{},"centroidTolerance":{},"opacity":{}}}"#,
                            probe.point,
                            probe.min_mass,
                            probe.max_mass,
                            if case.dots.is_empty() { 0.9 } else { 0.75 },
                            probe.opacity
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                write!(row, r#", "pointInk":[{points}]"#).unwrap();
            }
            row.push('}');
            rows.push(row);
        }
    }
    write("manifest.raw.json",format!(r#"{{"schema":1,"origin":"Contributor-original geometry, Apache-2.0","pageWidth":128,"pageHeight":128,"coordinateSpace":"pdf-user-space-y-up","scales":[1,2,4],"cases":[{}]}}"#,rows.join(",")).as_bytes());
}
