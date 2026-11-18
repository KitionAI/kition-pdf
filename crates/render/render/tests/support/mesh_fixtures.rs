//! Contributor-original mesh geometry; all colors and control points are authored here.
use std::{fmt::Write, path::PathBuf};

pub const SIZE: u32 = 40;
pub const BLUE: [u8; 3] = [0, 0, 255];
pub const WHITE: [u8; 3] = [255; 3];
pub const RED: [u8; 3] = [255, 0, 0];

#[derive(Clone)]
pub struct Region {
    pub rect: [f64; 4],
    pub rgb: [u8; 3],
}

pub struct Case {
    pub id: &'static str,
    pub shading_type: u8,
    pub stress: bool,
    pub expected_match: Option<&'static str>,
    pub data: Vec<u8>,
    pub matrix: [f64; 6],
    pub clip: Option<[f64; 4]>,
    pub background: bool,
    pub repeats: usize,
    pub regions: Vec<Region>,
}

pub fn rectangle(x: f64, y: f64, width: f64, height: f64, tensor: bool) -> Vec<[f64; 2]> {
    let mut p = vec![
        [x, y],
        [x, y + height / 3.0],
        [x, y + 2.0 * height / 3.0],
        [x, y + height],
        [x + width / 3.0, y + height],
        [x + 2.0 * width / 3.0, y + height],
        [x + width, y + height],
        [x + width, y + 2.0 * height / 3.0],
        [x + width, y + height / 3.0],
        [x + width, y],
        [x + 2.0 * width / 3.0, y],
        [x + width / 3.0, y],
    ];
    if tensor {
        p.extend([
            [x + width / 3.0, y + height / 3.0],
            [x + width / 3.0, y + 2.0 * height / 3.0],
            [x + 2.0 * width / 3.0, y + 2.0 * height / 3.0],
            [x + 2.0 * width / 3.0, y + height / 3.0],
        ]);
    }
    p
}

// 16-bit fixed-point coordinates at 1/4 point resolution; Decode maps them exactly.
fn patch(points: &[[f64; 2]], flag: u8) -> Vec<u8> {
    let mut out = vec![flag];
    for p in points.iter().skip(if flag == 0 { 0 } else { 4 }) {
        for value in p {
            let encoded = (value * 4.0 + 1024.0).round();
            assert!((0.0..=65535.0).contains(&encoded));
            out.extend_from_slice(&(encoded as u16).to_be_bytes());
        }
    }
    for _ in 0..if flag == 0 { 4 } else { 2 } {
        out.extend_from_slice(&BLUE);
    }
    out
}

fn base(id: &'static str, shading_type: u8, points: Vec<[f64; 2]>) -> Case {
    Case {
        id,
        shading_type,
        stress: false,
        expected_match: None,
        data: patch(&points, 0),
        matrix: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        clip: None,
        background: false,
        repeats: 1,
        regions: vec![Region { rect: [1.0, 1.0, 2.0, 2.0], rgb: RED }],
    }
}

pub fn cases() -> Vec<Case> {
    let mut all = Vec::new();
    for (kind, visible, stress) in [(6, "coons-visible", "coons-stress"), (7, "tensor-visible", "tensor-stress")] {
        let points = rectangle(8.0, 8.0, 24.0, 24.0, kind == 7);
        let mut c = base(visible, kind, points.clone());
        c.regions.extend([Region { rect: [12.0, 12.0, 16.0, 16.0], rgb: BLUE }, Region { rect: [34.0, 10.0, 4.0, 20.0], rgb: WHITE }]);
        let mut s = base(stress, kind, points);
        s.stress = true;
        s.expected_match = Some(visible);
        s.regions = c.regions.clone();
        s.data = patch(&rectangle(100.0, 100.0, 40.0, 40.0, kind == 7), 0).repeat(40_000);
        s.data.extend_from_slice(&c.data);
        all.extend([c, s]);
    }
    let mut points = rectangle(200.0, 0.0, 0.0, 40.0, false);
    for (i, p) in points.iter_mut().enumerate() {
        if ![0, 3, 6, 9].contains(&i) {
            p[0] = 80.0;
        }
    }
    let mut c = base("coons-overshoot", 6, points);
    c.regions.push(Region { rect: [28.0, 18.0, 6.0, 4.0], rgb: BLUE });
    all.push(c);
    let mut points = rectangle(80.0, 0.0, 40.0, 40.0, true);
    for p in points.iter_mut().skip(12) {
        p[0] = -120.0;
    }
    let mut c = base("tensor-interior-bulge", 7, points);
    c.regions.push(Region { rect: [10.0, 18.0, 20.0, 4.0], rgb: BLUE });
    all.push(c);
    let matrix = [1.0, 0.25, 0.5, 1.0, -10.0, -5.0];
    let points = rectangle(8.0, 8.0, 24.0, 24.0, false);
    let mut c = base("coons-affine", 6, points.clone());
    c.matrix = matrix;
    c.expected_match = Some("coons-affine-baked");
    c.regions.push(Region { rect: [17.0, 17.0, 6.0, 6.0], rgb: BLUE });
    let points = points.into_iter().map(|[x, y]| [matrix[0] * x + matrix[2] * y + matrix[4], matrix[1] * x + matrix[3] * y + matrix[5]]).collect();
    let mut baked = base("coons-affine-baked", 6, points);
    baked.regions = c.regions.clone();
    all.extend([c, baked]);
    let mut c = base("tensor-reflected", 7, rectangle(8.0, 8.0, 24.0, 24.0, true));
    c.matrix = [-1.0, 0.0, 0.0, 1.0, 40.0, 0.0];
    c.expected_match = Some("tensor-visible");
    c.regions.push(Region { rect: [12.0, 12.0, 16.0, 16.0], rgb: BLUE });
    all.push(c);
    let mut c = base("coons-small-clip", 6, rectangle(0.0, 0.0, 40.0, 40.0, false));
    c.clip = Some([10.0, 10.0, 4.0, 4.0]);
    c.regions.extend([Region { rect: [11.0, 11.0, 2.0, 2.0], rgb: BLUE }, Region { rect: [16.0, 16.0, 4.0, 4.0], rgb: WHITE }]);
    all.push(c);
    let mut c = base("coons-offpage-background", 6, rectangle(100.0, 100.0, 40.0, 40.0, false));
    c.background = true;
    c.regions.push(Region { rect: [8.0, 8.0, 24.0, 24.0], rgb: [0, 255, 0] });
    all.push(c);
    let mut c = base("tensor-inflated-edge", 7, rectangle(42.0, 0.0, 10000.0, 40.0, true));
    c.regions.push(Region { rect: [38.0, 8.0, 1.0, 24.0], rgb: BLUE });
    all.push(c);
    let prev = rectangle(100.0, 100.0, 40.0, 40.0, false);
    let mut next = vec![prev[3], prev[4], prev[5], prev[6]];
    next.extend([[140.0, 90.0], [140.0, 40.0], [140.0, -10.0], [90.0, -10.0], [40.0, -10.0], [-10.0, -10.0], [-10.0, 40.0], [-10.0, 90.0]]);
    let mut c = base("coons-continuation", 6, prev.clone());
    c.data.extend(patch(&next, 1));
    c.expected_match = Some("coons-continuation-independent");
    c.regions.push(Region { rect: [24.0, 16.0, 8.0, 8.0], rgb: BLUE });
    let mut independent = base("coons-continuation-independent", 6, next);
    independent.regions = c.regions.clone();
    all.extend([c, independent]);
    let mut repeated = base("coons-repeated-offpage", 6, rectangle(100.0, 100.0, 40.0, 40.0, false));
    repeated.data = repeated.data.repeat(40_000);
    repeated.repeats = 1000;
    repeated.stress = true;
    repeated.regions.push(Region { rect: [8.0, 8.0, 24.0, 24.0], rgb: WHITE });
    all.push(repeated);
    all
}

pub fn pdf(case: &Case) -> Vec<u8> {
    let [a, b, c, d, e, f] = case.matrix;
    let mut content = String::from("q\n");
    if let Some([x, y, w, h]) = case.clip {
        writeln!(content, "{x} {y} {w} {h} re W n").unwrap();
    }
    writeln!(content, "{a} {b} {c} {d} {e} {f} cm").unwrap();
    for _ in 0..case.repeats {
        content.push_str("/Sh0 sh\n");
    }
    content.push_str("Q 1 0 0 rg 0 0 4 4 re f\n");
    let mut shading=format!("<< /ShadingType {} /ColorSpace /DeviceRGB /BitsPerCoordinate 16 /BitsPerComponent 8 /BitsPerFlag 8 /Decode [-256 16127.75 -256 16127.75 0 1 0 1 0 1] {} /Length {} >>\nstream\n",case.shading_type,if case.background {"/Background [0 1 0]"} else {""},case.data.len()).into_bytes();
    shading.extend(&case.data);
    shading.extend_from_slice(b"\nendstream");
    let objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_vec(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {SIZE} {SIZE}] /Resources << /Shading << /Sh0 5 0 R >> >> /Contents 4 0 R >>")
            .into_bytes(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()).into_bytes(),
        shading,
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend(obj);
        out.extend_from_slice(b"\nendobj\n");
    }
    let start = out.len();
    out.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n").as_bytes());
    out
}

pub fn export(cases: &[Case]) {
    let Some(requested) = std::env::var_os("KITION_PDF_MESH_OUT") else { return };
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../target").canonicalize().unwrap();
    let out = PathBuf::from(requested);
    assert!(out.is_absolute() && out.starts_with(&target));
    assert!(!out.components().any(|p| p == std::path::Component::ParentDir));
    std::fs::create_dir_all(&out).unwrap();
    let out = out.canonicalize().unwrap();
    assert!(out.starts_with(target));
    let write = |name: &str, data: &[u8]| {
        let p = out.join(name);
        if p.exists() {
            assert!(std::fs::read(p).unwrap() == data, "refuse different fixture bytes");
        } else {
            std::fs::write(p, data).unwrap();
        }
    };
    let mut rows = Vec::new();
    for case in cases {
        write(&format!("{}.pdf", case.id), &pdf(case));
        let regions =
            case.regions.iter().map(|r| format!(r#"{{"rect":{:?},"rgb":{:?},"tolerance":8}}"#, r.rect, r.rgb)).collect::<Vec<_>>().join(",");
        let pair = case.expected_match.map(|id| format!(r#", "expectedMatch":"{id}""#)).unwrap_or_default();
        rows.push(format!(
            r#"{{"id":"{}","file":"{}.pdf","shadingType":{},"isStress":{},"regions":[{regions}]{pair}}}"#,
            case.id, case.id, case.shading_type, case.stress
        ));
    }
    write("manifest.raw.json",format!(r#"{{"schema":1,"origin":"Contributor-original geometry, Apache-2.0","coordinateSpace":"pdf-user-space-y-up","pageWidth":40,"pageHeight":40,"scales":[1,2],"cases":[{}]}}"#,rows.join(",")).as_bytes());
}
