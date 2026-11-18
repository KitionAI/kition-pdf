//! Contributor-original disconnected mixed dash PDFs with explicit independent scalar spans.
use std::{fmt::Write, path::PathBuf};

#[derive(Clone)]
pub struct Line {
    pub start: [f64; 2],
    pub end: [f64; 2],
    pub length: f64,
    pub runs: Vec<(f64, f64)>,
    pub dots: Vec<f64>,
    pub dot_delta: [f64; 2],
    pub positive_probe: f64,
}
#[derive(Clone)]
pub struct Case {
    pub id: &'static str,
    pub matrix: [f64; 6],
    pub lines: Vec<Line>,
    pub dashes: Vec<f64>,
    pub phase: f64,
    pub cap: u32,
    pub opacity: f64,
    pub clip: Option<[u32; 4]>,
    pub gaps: Vec<[u32; 4]>,
    pub alpha_pixel: Option<[u32; 4]>,
}
fn base_line(start: [f64; 2], end: [f64; 2], length: f64, dot_delta: [f64; 2]) -> Line {
    assert!((end[0] - start[0]).hypot(end[1] - start[1]) == length);
    let runs = if length == 44.0 { vec![(12.0, 18.0), (42.0, 44.0)] } else { vec![(12.0, 18.0), (42.0, 48.0)] };
    Line { start, end, length, runs, dots: vec![0.0, 30.0], dot_delta, positive_probe: 15.0 }
}
fn base(id: &'static str) -> Case {
    Case {
        id,
        matrix: [1.0, 0.0, 0.0, 0.5, 0.0, 0.0],
        lines: vec![
            base_line([16.0, 81.0], [70.0, 81.0], 54.0, [1.0 / 8192.0, 0.0]),
            base_line([112.0, 145.0], [68.0, 145.0], 44.0, [-1.0 / 8192.0, 0.0]),
        ],
        dashes: vec![0.0, 12.0, 6.0, 12.0],
        phase: 0.0,
        cap: 2,
        opacity: 1.0,
        clip: None,
        gaps: vec![[38, 40, 2, 1], [88, 72, 2, 1]],
        alpha_pixel: None,
    }
}
pub fn cases() -> Vec<Case> {
    let mut round = base("separated-restart-round");
    round.cap = 1;
    let square = base("separated-restart-square");
    let mut phase = base("separated-phase-three");
    phase.matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    phase.phase = 3.0;
    phase.lines = vec![
        Line {
            start: [16.0, 40.5],
            end: [100.0, 40.5],
            length: 84.0,
            runs: vec![(9.0, 15.0), (39.0, 45.0), (69.0, 75.0)],
            dots: vec![27.0, 57.0],
            dot_delta: [1.0 / 8192.0, 0.0],
            positive_probe: 12.0,
        },
        Line {
            start: [100.5, 24.0],
            end: [100.5, 78.0],
            length: 54.0,
            runs: vec![(9.0, 15.0), (39.0, 45.0)],
            dots: vec![27.0],
            dot_delta: [0.0, 1.0 / 8192.0],
            positive_probe: 12.0,
        },
    ];
    phase.gaps = vec![[34, 40, 2, 1], [100, 42, 1, 2]];
    let mut diagonal = base("separated-direction-three-four");
    diagonal.matrix = phase.matrix;
    diagonal.lines = vec![
        base_line([24.0, 24.0], [54.0, 64.0], 50.0, [3.0 / 32768.0, 4.0 / 32768.0]),
        base_line([112.0, 100.5], [58.0, 100.5], 54.0, [-1.0 / 8192.0, 0.0]),
    ];
    diagonal.gaps = vec![[38, 43, 1, 1], [88, 100, 2, 1]];
    let mut odd = base("separated-odd-phase");
    odd.matrix = phase.matrix;
    odd.dashes = vec![0.0, 8.0, 4.0];
    odd.phase = 3.0;
    odd.lines = vec![
        Line {
            start: [16.0, 40.5],
            end: [70.0, 40.5],
            length: 54.0,
            runs: vec![(5.0, 9.0), (9.0, 17.0), (29.0, 33.0), (33.0, 41.0), (53.0, 54.0)],
            dots: vec![21.0, 45.0],
            dot_delta: [1.0 / 8192.0, 0.0],
            positive_probe: 13.0,
        },
        Line {
            start: [112.0, 72.5],
            end: [74.0, 72.5],
            length: 38.0,
            runs: vec![(5.0, 9.0), (9.0, 17.0), (29.0, 33.0), (33.0, 38.0)],
            dots: vec![21.0],
            dot_delta: [-1.0 / 8192.0, 0.0],
            positive_probe: 13.0,
        },
    ];
    odd.gaps = vec![[34, 40, 1, 1], [93, 72, 1, 1]];
    let mut shear = base("separated-shear");
    shear.matrix = [1.0, 0.0, 0.25, 0.5, 8.0, 16.0];
    shear.lines = vec![
        base_line([0.0, 33.0], [54.0, 33.0], 54.0, [1.0 / 8192.0, 0.0]),
        base_line([80.5, 0.0], [80.5, 54.0], 54.0, [1.0 / 16384.0, 2.0 / 16384.0]),
        base_line([0.0, 120.0], [30.0, 160.0], 50.0, [2.0 / 16384.0, 1.0 / 16384.0]),
    ];
    shear.gaps = vec![[40, 32, 1, 1], [94, 27, 1, 1], [57, 85, 1, 1]];
    let mut clip = base("separated-clipped");
    clip.matrix = phase.matrix;
    clip.clip = Some([40, 8, 40, 112]);
    clip.lines = vec![
        Line {
            start: [16.0, 40.5],
            end: [100.0, 40.5],
            length: 84.0,
            runs: vec![(12.0, 18.0), (42.0, 48.0), (72.0, 78.0)],
            dots: vec![0.0, 30.0, 60.0],
            dot_delta: [1.0 / 8192.0, 0.0],
            positive_probe: 45.0,
        },
        Line {
            start: [112.0, 72.5],
            end: [28.0, 72.5],
            length: 84.0,
            runs: vec![(12.0, 18.0), (42.0, 48.0), (72.0, 78.0)],
            dots: vec![0.0, 30.0, 60.0],
            dot_delta: [-1.0 / 8192.0, 0.0],
            positive_probe: 45.0,
        },
    ];
    clip.gaps = vec![[30, 40, 2, 1], [94, 72, 2, 1], [68, 40, 2, 1], [58, 72, 2, 1]];
    let mut alpha = base("separated-alpha-crossing");
    alpha.matrix = phase.matrix;
    alpha.cap = 1;
    alpha.opacity = 0.5;
    alpha.dashes = vec![0.0, 0.0, 6.0, 12.0];
    alpha.lines = vec![
        Line {
            start: [16.0, 64.5],
            end: [100.0, 64.5],
            length: 84.0,
            runs: vec![(0.0, 6.0), (18.0, 24.0), (36.0, 42.0), (54.0, 60.0), (72.0, 78.0)],
            dots: vec![0.0, 18.0, 36.0, 54.0, 72.0],
            dot_delta: [1.0 / 8192.0, 0.0],
            positive_probe: 21.0,
        },
        Line {
            start: [55.5, 25.0],
            end: [55.5, 109.0],
            length: 84.0,
            runs: vec![(0.0, 6.0), (18.0, 24.0), (36.0, 42.0), (54.0, 60.0), (72.0, 78.0)],
            dots: vec![0.0, 18.0, 36.0, 54.0, 72.0],
            dot_delta: [0.0, 1.0 / 8192.0],
            positive_probe: 21.0,
        },
    ];
    alpha.gaps = vec![[27, 64, 2, 1], [55, 36, 1, 2]];
    alpha.alpha_pixel = Some([55, 64, 1, 1]);
    vec![round, square, phase, diagonal, odd, shear, clip, alpha]
}
pub fn point(case: &Case, line: &Line, distance: f64) -> [f64; 2] {
    let local = [
        line.start[0] + (line.end[0] - line.start[0]) * (distance / line.length),
        line.start[1] + (line.end[1] - line.start[1]) * (distance / line.length),
    ];
    let [a, b, c, d, e, f] = case.matrix;
    [a * local[0] + c * local[1] + e, b * local[0] + d * local[1] + f]
}
fn segment(out: &mut String, start: [f64; 2], end: [f64; 2]) {
    writeln!(out, "{:.12} {:.12} m {:.12} {:.12} l", start[0], start[1], end[0], end[1]).unwrap();
}
pub fn pdf(case: &Case, control: bool) -> Vec<u8> {
    let mut content = String::from("q\n");
    if let Some([x, y, w, h]) = case.clip {
        writeln!(content, "{x} {y} {w} {h} re W n").unwrap();
    }
    if !control {
        let [a, b, c, d, e, f] = case.matrix;
        writeln!(content, "{a} {b} {c} {d} {e} {f} cm").unwrap();
    }
    writeln!(content, "/GS gs 0 G 0 w {} J 0 j 4 M", case.cap).unwrap();
    if control {
        for line in &case.lines {
            for &(start, end) in &line.runs {
                segment(&mut content, point(case, line, start), point(case, line, end));
            }
            for &distance in &line.dots {
                let start = point(case, line, distance).map(|value| f64::from(value as f32));
                let end = [start[0] + line.dot_delta[0], start[1] + line.dot_delta[1]];
                for axis in 0..2 {
                    assert_eq!(f64::from(end[axis] as f32) - start[axis], line.dot_delta[axis]);
                }
                segment(&mut content, start, end);
            }
        }
    } else {
        let pattern = case.dashes.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ");
        writeln!(content, "[{pattern}] {} d", case.phase).unwrap();
        for line in &case.lines {
            segment(&mut content, line.start, line.end);
        }
    }
    // One paint for the entire disconnected path, including alpha-crossing controls.
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
pub struct Probe {
    pub id: String,
    pub point: [f64; 2],
    pub min: f64,
    pub max: f64,
    pub opacity: f64,
}
pub fn probes(case: &Case) -> Vec<Probe> {
    let mut probes = Vec::new();
    for (index, line) in case.lines.iter().enumerate() {
        if case.opacity == 1.0 {
            for (dot, &distance) in line.dots.iter().enumerate() {
                let point = point(case, line, distance);
                if case.clip.is_some_and(|[x, y, w, h]| {
                    point[0] < f64::from(x) || point[0] > f64::from(x + w) || point[1] < f64::from(y) || point[1] > f64::from(y + h)
                }) {
                    continue;
                }
                probes.push(Probe {
                    id: format!("line-{index}-dot-{dot}"),
                    point,
                    min: 0.5,
                    max: if case.cap == 1 { 1.1 } else { 1.3 },
                    opacity: 1.0,
                });
            }
        }
        probes.push(Probe {
            id: format!("line-{index}-positive"),
            point: point(case, line, line.positive_probe),
            min: 1.5,
            max: 6.0,
            opacity: case.opacity,
        });
    }
    probes
}
pub fn export(cases: &[Case]) {
    let Some(requested) = std::env::var_os("KITION_PDF_SEPARATED_DASH_OUT") else { return };
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
            assert_eq!(std::fs::read(path).unwrap(), bytes, "refuse changed separated fixture");
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
            for (i, rect) in case.gaps.iter().enumerate() {
                write!(row, r#",{{"id":"gap-{i}","rect":{rect:?},"rgba":[255,255,255,255],"tolerance":2}}"#).unwrap();
            }
            if let Some(rect) = case.alpha_pixel {
                write!(row, r#",{{"id":"single-paint-alpha","scales":[1],"rect":{rect:?},"rgba":[128,128,128,255],"tolerance":2}}"#).unwrap();
            }
            row.push(']');
            if !control {
                write!(row, r#", "expectedMatch":"{}-control""#, case.id).unwrap();
            }
            let points = probes(case)
                .iter()
                .map(|probe| {
                    format!(
                        r#"{{"id":"{}","point":{:?},"radius":2,"minMass":{},"maxMass":{},"centroidTolerance":0.75,"opacity":{}}}"#,
                        probe.id, probe.point, probe.min, probe.max, probe.opacity
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            write!(row, r#", "pointInk":[{points}]}}"#).unwrap();
            rows.push(row);
        }
    }
    write("manifest.raw.json", format!(r#"{{"schema":1,"origin":"Contributor-original geometry, Apache-2.0","pageWidth":128,"pageHeight":128,"coordinateSpace":"pdf-user-space-y-up","scales":[1,2,4],"cases":[{}]}}"#, rows.join(",")).as_bytes());
}
