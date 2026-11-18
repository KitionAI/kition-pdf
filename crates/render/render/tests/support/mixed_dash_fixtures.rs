//! Contributor-original mixed zero/positive dash PDFs and hand-authored controls.
use std::{fmt::Write, path::PathBuf};

#[derive(Clone)]
pub struct Case {
    pub id: &'static str,
    pub matrix: [f64; 6],
    pub direction: [f64; 2],
    pub length: f64,
    pub dashes: Vec<f64>,
    pub phase: f64,
    pub cap: u32,
    pub opacity: f64,
    pub clip: bool,
    pub runs: Vec<(f64, f64)>,
    pub dots: Vec<f64>,
    pub dot_delta: [f64; 2],
}
fn base(id: &'static str) -> Case {
    Case {
        id,
        matrix: [1.0, 0.0, 0.0, 0.01, 16.0, 64.5],
        direction: [1.0, 0.0],
        length: 84.0,
        dashes: vec![0.0, 12.0, 6.0, 12.0],
        phase: 0.0,
        cap: 2,
        opacity: 1.0,
        clip: false,
        runs: vec![(12.0, 18.0), (42.0, 48.0), (72.0, 78.0)],
        dots: vec![0.0, 30.0, 60.0],
        dot_delta: [1.0 / 8192.0, 0.0],
    }
}
pub fn cases() -> Vec<Case> {
    let mut round = base("mixed-flat-round");
    round.cap = 1;
    let mut wide = base("mixed-wide");
    wide.matrix[3] = 100.0;
    let mut shear = base("mixed-shear");
    shear.matrix[2] = 5.0;
    shear.matrix[3] = 1.0;
    let mut rotated = base("mixed-rotated");
    rotated.matrix = [0.0, 1.0, -0.01, 0.0, 64.5, 16.0];
    rotated.dot_delta = [0.0, 1.0 / 8192.0];
    let mut reflected = base("mixed-reflected");
    reflected.matrix = [-1.0, 0.0, 0.0, 0.01, 112.0, 64.5];
    reflected.dot_delta = [-1.0 / 8192.0, 0.0];
    let mut diagonal = base("mixed-source-three-four");
    diagonal.matrix = [1.0, 0.0, 0.0, 1.0, 24.0, 24.0];
    diagonal.direction = [0.6, 0.8];
    diagonal.length = 80.0; // Exact 48-by-64 source endpoint preserves the 3:4 tangent.
    diagonal.dot_delta = [3.0 / 32768.0, 4.0 / 32768.0];
    let mut phase = base("mixed-phase-three");
    phase.phase = 3.0;
    phase.runs = vec![(9.0, 15.0), (39.0, 45.0), (69.0, 75.0)];
    phase.dots = vec![27.0, 57.0];
    let mut large = phase.clone();
    large.id = "mixed-phase-large";
    large.phase = 300003.0;
    let mut inside = base("mixed-phase-inside-on");
    inside.phase = 15.0;
    inside.runs = vec![(0.0, 3.0), (27.0, 33.0), (57.0, 63.0)];
    inside.dots = vec![15.0, 45.0, 75.0];
    let mut odd = base("mixed-odd-phase");
    odd.dashes = vec![0.0, 8.0, 4.0];
    odd.phase = 3.0;
    odd.runs = vec![(5.0, 9.0), (9.0, 17.0), (29.0, 33.0), (33.0, 41.0), (53.0, 57.0), (57.0, 65.0), (77.0, 81.0), (81.0, 84.0)];
    odd.dots = vec![21.0, 45.0, 69.0];
    let mut clipped = base("mixed-clipped");
    clipped.clip = true;
    let mut alpha = base("mixed-alpha-overlap");
    alpha.opacity = 0.5;
    alpha.cap = 1;
    alpha.dashes = vec![0.0, 0.0, 6.0, 12.0];
    alpha.runs = vec![(0.0, 6.0), (18.0, 24.0), (36.0, 42.0), (54.0, 60.0), (72.0, 78.0)];
    alpha.dots = vec![0.0, 18.0, 36.0, 54.0, 72.0];
    vec![round, base("mixed-flat-square"), wide, shear, rotated, reflected, diagonal, phase, inside, large, odd, clipped, alpha]
}
pub fn point(case: &Case, distance: f64) -> [f64; 2] {
    let [x, y] = case.direction.map(|v| v * distance);
    let [a, b, c, d, e, f] = case.matrix;
    [a * x + c * y + e, b * x + d * y + f]
}
fn segment(out: &mut String, start: [f64; 2], end: [f64; 2]) {
    writeln!(out, "{:.12} {:.12} m {:.12} {:.12} l", start[0], start[1], end[0], end[1]).unwrap();
}
pub fn pdf(case: &Case, control: bool) -> Vec<u8> {
    let mut content = String::from("q\n");
    if case.clip {
        content.push_str("40 8 40 112 re W n\n");
    }
    if !control {
        let [a, b, c, d, e, f] = case.matrix;
        writeln!(content, "{a} {b} {c} {d} {e} {f} cm").unwrap();
    }
    writeln!(content, "/GS gs 0 G 0 w {} J 0 j 4 M", case.cap).unwrap();
    if control {
        for &(start, end) in &case.runs {
            segment(&mut content, point(case, start), point(case, end));
        }
        for &distance in &case.dots {
            let start = point(case, distance).map(|v| f64::from(v as f32));
            let end = [start[0] + case.dot_delta[0], start[1] + case.dot_delta[1]];
            // Exact PDF float differences keep manual cap direction independent of parser rounding.
            for axis in 0..2 {
                assert_eq!(f64::from(end[axis] as f32) - f64::from(start[axis] as f32), case.dot_delta[axis]);
            }
            segment(&mut content, start, end);
        }
    } else {
        let pattern = case.dashes.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ");
        writeln!(content, "[{pattern}] {} d", case.phase).unwrap();
        segment(&mut content, [0.0, 0.0], case.direction.map(|v| v * case.length));
    }
    content.push_str("S Q\n1 0 0 rg 4 4 4 4 re f\n");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 128 128] /Resources << /ExtGState << /GS 5 0 R >> >> /Contents 4 0 R >>".to_string(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        format!("<< /Type /ExtGState /CA {} /ca {} >>", case.opacity, case.opacity),
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
pub struct Probe {
    pub point: [f64; 2],
    pub min: f64,
    pub max: f64,
    pub opacity: f64,
}
pub fn probes(case: &Case) -> Vec<Probe> {
    let mut probes = Vec::new();
    if case.opacity == 1.0 {
        for &distance in &case.dots {
            let point = point(case, distance);
            if case.clip && !(40.0..80.0).contains(&point[0]) {
                continue;
            }
            probes.push(Probe { point, min: 0.5, max: if case.cap == 1 { 1.1 } else { 1.3 }, opacity: 1.0 });
        }
    }
    let interior = if case.clip {
        44.0
    } else {
        let (a, b) = case.runs[0];
        (a + b) / 2.0
    };
    probes.push(Probe { point: point(case, interior), min: 1.5, max: 6.0, opacity: case.opacity });
    probes
}
pub fn empty_regions(case: &Case) -> &'static [[u32; 4]] {
    if case.clip {
        &[[30, 64, 2, 1], [68, 64, 2, 1]]
    } else if case.opacity < 1.0 {
        &[[26, 64, 2, 1]]
    } else {
        &[]
    }
}
pub fn export(cases: &[Case]) {
    let Some(requested) = std::env::var_os("KITION_PDF_MIXED_DASH_OUT") else { return };
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../target").canonicalize().unwrap();
    let out = PathBuf::from(requested);
    assert!(out.is_absolute() && out.starts_with(&target));
    assert!(!out.components().any(|part| part == std::path::Component::ParentDir));
    std::fs::create_dir_all(&out).unwrap();
    let out = out.canonicalize().unwrap();
    assert!(out.starts_with(target));
    let write = |name: &str, bytes: &[u8]| {
        let path = out.join(name);
        if path.exists() {
            assert_eq!(std::fs::read(path).unwrap(), bytes, "refuse changed mixed fixture");
        } else {
            std::fs::write(path, bytes).unwrap();
        }
    };
    let mut rows = Vec::new();
    for case in cases {
        for control in [false, true] {
            let id = if control { format!("{}-control", case.id) } else { case.id.to_string() };
            write(&format!("{id}.pdf"), &pdf(case, control));
            let role = if control { "control" } else { "sample" };
            let mut row = format!(
                r#"{{"id":"{id}","file":"{id}.pdf","role":"{role}","classification":"normative-hairline","regions":[{{"id":"sentinel","rect":[5,5,2,2],"rgba":[255,0,0,255],"tolerance":2}},{{"id":"empty-corner","rect":[120,120,2,2],"rgba":[255,255,255,255],"tolerance":2}}"#
            );
            for (i, rect) in empty_regions(case).iter().enumerate() {
                write!(row, r#",{{"id":"empty-{i}","rect":{rect:?},"rgba":[255,255,255,255],"tolerance":2}}"#).unwrap();
            }
            if case.opacity < 1.0 {
                row.push_str(r#",{"id":"single-paint-alpha","scales":[1],"rect":[16,64,1,1],"rgba":[128,128,128,255],"tolerance":2}"#);
            }
            row.push(']');
            if !control {
                write!(row, r#", "expectedMatch":"{}-control""#, case.id).unwrap();
            }
            if !case.clip {
                let end = case.direction.map(|v| v * case.length);
                let cap = if case.cap == 1 { "round" } else { "square" };
                write!(row,r#", "analyticLine":{{"localStart":[0,0],"localEnd":{end:?},"matrix":{:?},"dashArray":{:?},"dashOffset":{},"cap":"{cap}","opacity":{}}}"#,case.matrix,case.dashes,case.phase,case.opacity).unwrap();
            }
            let points = probes(case)
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    format!(
                        r#"{{"id":"ink-{i}","point":{:?},"radius":2,"minMass":{},"maxMass":{},"centroidTolerance":0.75,"opacity":{}}}"#,
                        p.point, p.min, p.max, p.opacity
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            write!(row, r#", "pointInk":[{points}]}}"#).unwrap();
            rows.push(row);
        }
    }
    write("manifest.raw.json",format!(r#"{{"schema":1,"origin":"Contributor-original geometry, Apache-2.0","pageWidth":128,"pageHeight":128,"coordinateSpace":"pdf-user-space-y-up","scales":[1,2,4],"cases":[{}]}}"#,rows.join(",")).as_bytes());
}
