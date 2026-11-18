//! Contributor-original single-round stage isolation; independent dense capsule controls.
use kurbo::{Affine, BezPath, Cap, Join, PathEl, Point, Shape, Stroke, StrokeOpts};
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
    pub fn variants(&self) -> [&'static str; 4] {
        ["sample", "curves", "fine-polygon", "capsule"]
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
    cases.retain(|case| case.cap == 1 && case.layout == "single");
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
    match variant {
        "sample" => {
            for [start, end] in case.segments() {
                segment(&mut content, start, end);
            }
            content.push_str("S\n");
        }
        "capsule" => fill_union(&mut content, case),
        "curves" => {
            write_path(&mut content, &stage(case).parsed_user);
            content.push_str("f\n");
        }
        "fine-polygon" => {
            for (index, point) in stage(case).fine_polygon.iter().enumerate() {
                writeln!(content, "{:.12} {:.12} {}", point[0], point[1], if index == 0 { "m" } else { "l" }).unwrap();
            }
            content.push_str("h f\n");
        }
        _ => panic!("unknown stage"),
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
    let out = PathBuf::from(std::env::var_os("KITION_PDF_ROUND_STAGE_OUT")?);
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
        assert_eq!(std::fs::read(path).unwrap(), bytes, "refuse changed round-stage evidence");
    } else {
        std::fs::write(path, bytes).unwrap();
    }
}

pub struct Stage {
    pub raw_expansion: BezPath,
    pub raw_user: BezPath,
    pub parsed_user: BezPath,
    pub fine_polygon: Vec<[f64; 2]>,
    pub expansion_tolerance: f64,
    pub finer_expansion_tolerance: f64,
    pub expansion_equal: bool,
    pub max_control_displacement: f64,
    pub max_hull_distance: f64,
    pub max_polygon_displacement: f64,
}
fn map_path(path: &BezPath, mut map: impl FnMut(Point) -> Point) -> BezPath {
    BezPath::from_vec(
        path.elements()
            .iter()
            .map(|element| match *element {
                PathEl::MoveTo(p) => PathEl::MoveTo(map(p)),
                PathEl::LineTo(p) => PathEl::LineTo(map(p)),
                PathEl::CurveTo(a, b, c) => PathEl::CurveTo(map(a), map(b), map(c)),
                PathEl::ClosePath => PathEl::ClosePath,
                PathEl::QuadTo(_, _) => panic!("unexpected quadratic in this original single-line expansion"),
            })
            .collect(),
    )
}
fn distance_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let direction = b - a;
    if direction.hypot2() == 0.0 {
        return p.distance(a);
    }
    let fraction = ((p - a).dot(direction) / direction.hypot2()).clamp(0.0, 1.0);
    p.distance(a + fraction * direction)
}
fn split_cubic(points: [Point; 4]) -> ([Point; 4], [Point; 4]) {
    let [a, b, c, d] = points;
    let ab = a.midpoint(b);
    let bc = b.midpoint(c);
    let cd = c.midpoint(d);
    let abc = ab.midpoint(bc);
    let bcd = bc.midpoint(cd);
    let center = abc.midpoint(bcd);
    ([a, ab, abc, center], [center, bcd, cd, d])
}
fn fine_polygon(path: &BezPath, scale: f64) -> (Vec<[f64; 2]>, f64, f64) {
    let mut points = Vec::new();
    let mut current = Point::ORIGIN;
    let mut first = Point::ORIGIN;
    let mut max_hull = 0.0_f64;
    for element in path.elements() {
        match *element {
            PathEl::MoveTo(p) => {
                assert!(points.is_empty());
                first = p;
                current = p;
                points.push(p);
            }
            PathEl::LineTo(p) => {
                current = p;
                points.push(p);
            }
            PathEl::CurveTo(a, b, end) => {
                let mut stack = vec![([current, a, b, end], 0_u32)];
                while let Some((cubic, depth)) = stack.pop() {
                    let bound = distance_to_segment(cubic[1], cubic[0], cubic[3]).max(distance_to_segment(cubic[2], cubic[0], cubic[3])) * scale;
                    if bound <= SAGITTA_LIMIT {
                        max_hull = max_hull.max(bound);
                        points.push(cubic[3]);
                    } else {
                        assert!(depth < 32);
                        let (left, right) = split_cubic(cubic);
                        stack.push((right, depth + 1));
                        stack.push((left, depth + 1));
                    }
                    assert!(points.len() + stack.len() < 10000);
                }
                current = end;
            }
            PathEl::ClosePath => {
                current = first;
                points.push(first);
            }
            PathEl::QuadTo(_, _) => panic!("unexpected quadratic"),
        }
    }
    let mut displacement = 0.0_f64;
    let mut rounded = Vec::new();
    for point in points {
        let p = Point::new(f64::from(point.x as f32), f64::from(point.y as f32));
        displacement = displacement.max(point.distance(p) * scale);
        for coordinate in [p.x, p.y] {
            assert_eq!(format!("{coordinate:.12}").parse::<f32>().unwrap(), coordinate as f32);
        }
        if rounded.last() != Some(&[p.x, p.y]) {
            rounded.push([p.x, p.y]);
        }
    }
    if rounded.last() == rounded.first() {
        rounded.pop();
    }
    assert!(displacement <= VERTEX_LIMIT);
    (rounded, max_hull, displacement)
}
pub fn stage(case: &Case) -> Stage {
    let scale = f64::from(case.scale);
    let transform = Affine::new([scale, 0.0, 0.0, -scale, 0.0, f64::from(PAGE_SIDE) * scale]);
    let mut source = BezPath::new();
    source.move_to((case.origin[0], case.origin[1]));
    source.line_to((case.origin[0] + case.length, case.origin[1]));
    let (source, width, tolerance, fine_tolerance) = if case.line_width == 0.0 {
        (transform * source, 1.0, 0.25, SAGITTA_LIMIT)
    } else {
        (source, case.line_width, 0.25 / scale, SAGITTA_LIMIT / scale)
    };
    let style = Stroke::new(width).with_join(Join::Miter).with_caps(Cap::Round);
    let raw_expansion = kurbo::stroke(&source, &style, &StrokeOpts::default(), tolerance);
    let finer = kurbo::stroke(&source, &style, &StrokeOpts::default(), fine_tolerance);
    let expansion_equal = raw_expansion.elements() == finer.elements();
    let raw_user = if case.line_width == 0.0 { transform.inverse() * raw_expansion.clone() } else { raw_expansion.clone() };
    let mut max_control_displacement = 0.0_f64;
    let parsed_user = map_path(&raw_user, |point| {
        let parsed =
            Point::new(f64::from(format!("{:.12}", point.x).parse::<f32>().unwrap()), f64::from(format!("{:.12}", point.y).parse::<f32>().unwrap()));
        for coordinate in [parsed.x, parsed.y] {
            assert_eq!(format!("{coordinate:.12}").parse::<f32>().unwrap(), coordinate as f32);
        }
        max_control_displacement = max_control_displacement.max(point.distance(parsed) * scale);
        parsed
    });
    assert!(max_control_displacement <= VERTEX_LIMIT);
    let (fine_polygon, max_hull_distance, max_polygon_displacement) = fine_polygon(&parsed_user, scale);
    Stage {
        raw_expansion,
        raw_user,
        parsed_user,
        fine_polygon,
        expansion_tolerance: tolerance,
        finer_expansion_tolerance: fine_tolerance,
        expansion_equal,
        max_control_displacement,
        max_hull_distance,
        max_polygon_displacement,
    }
}
pub fn signed_path_area(path: &BezPath) -> f64 {
    let mut translated = path.clone();
    let origin = match path.elements()[0] {
        PathEl::MoveTo(p) => p,
        _ => panic!("expected move"),
    };
    translated.apply_affine(Affine::translate((-origin.x, -origin.y)));
    translated.close_path();
    translated.area()
}
fn write_path(content: &mut String, path: &BezPath) {
    for element in path.elements() {
        match *element {
            PathEl::MoveTo(p) => writeln!(content, "{:.12} {:.12} m", p.x, p.y).unwrap(),
            PathEl::LineTo(p) => writeln!(content, "{:.12} {:.12} l", p.x, p.y).unwrap(),
            PathEl::CurveTo(a, b, c) => writeln!(content, "{:.12} {:.12} {:.12} {:.12} {:.12} {:.12} c", a.x, a.y, b.x, b.y, c.x, c.y).unwrap(),
            PathEl::ClosePath => content.push_str("h\n"),
            PathEl::QuadTo(_, _) => panic!("unexpected quadratic"),
        }
    }
}
fn path_json(path: &BezPath) -> String {
    let rows = path
        .elements()
        .iter()
        .map(|element| match *element {
            PathEl::MoveTo(p) => format!(r#"["M",{},{}]"#, p.x, p.y),
            PathEl::LineTo(p) => format!(r#"["L",{},{}]"#, p.x, p.y),
            PathEl::CurveTo(a, b, c) => format!(r#"["C",{},{},{},{},{},{}]"#, a.x, a.y, b.x, b.y, c.x, c.y),
            PathEl::ClosePath => r#"["Z"]"#.to_string(),
            PathEl::QuadTo(_, _) => panic!("unexpected quadratic"),
        })
        .collect::<Vec<_>>();
    format!("[{}]", rows.join(","))
}
pub fn export(cases: &[Case], out: &std::path::Path) {
    let mut rows = Vec::new();
    for case in cases {
        let stages = stage(case);
        let dense = case.polygon();
        for variant in case.variants() {
            write(out, &format!("{}-{variant}.pdf", case.id), &pdf(case, variant));
        }
        let space = if case.line_width == 0.0 { "device-y-down" } else { "pdf-user-y-up" };
        rows.push(format!(r#"{{"id":"{}","scale":{},"lineWidth":{},"effectiveWidth":{},"origin":{:?},"length":{},"opacity":{},"calibration":{},"stages":{{"stroke":"{}-sample.pdf","curves":"{}-curves.pdf","finePolygon":"{}-fine-polygon.pdf","capsule":"{}-capsule.pdf"}},"rawExpansionSpace":"{space}","rawExpansionOutline":{},"rawUserOutline":{},"parsedUserOutline":{},"expansionTolerance":{},"finerExpansionTolerance":{},"expansionPathsEqual":{},"maxDeviceControlPointDisplacement":{},"maxDeviceHullDistance":{},"maxDevicePolygonVertexDisplacement":{},"finePolygon":{:?},"capsulePolygon":{:?},"rawUserSignedArea":{},"parsedUserSignedArea":{},"trueArea":{},"truePerimeter":{},"denseDeviceAreaErrorBound":{},"denseMaxDeviceSagitta":{},"denseMaxDeviceVertexDisplacement":{}}}"#,
            case.id,case.scale,case.line_width,case.width(),case.origin,case.length,case.opacity,case.calibration,case.id,case.id,case.id,case.id,
            path_json(&stages.raw_expansion),path_json(&stages.raw_user),path_json(&stages.parsed_user),stages.expansion_tolerance,stages.finer_expansion_tolerance,stages.expansion_equal,
            stages.max_control_displacement,stages.max_hull_distance,stages.max_polygon_displacement,stages.fine_polygon,dense.vertices,
            signed_path_area(&stages.raw_user),signed_path_area(&stages.parsed_user),case.true_area(),case.true_perimeter(),case.area_error_bound(),dense.max_sagitta,dense.max_vertex_displacement));
    }
    write(out,"diagnostic-manifest.json",format!(r#"{{"schema":"round-stage-diagnostic/1","origin":"Contributor-original single-round geometry, Apache-2.0","purpose":"Internal stage observations, not normative sample acceptance","kurboVersion":"0.13.1","velloCommonVersion":"0.0.8","pageWidth":{PAGE_SIDE},"pageHeight":{PAGE_SIDE},"coordinateSpace":"pdf-user-space-y-up except rawExpansionOutline when labelled device-y-down","controlChannelTolerance":2,"deviceHullDistanceLimit":{SAGITTA_LIMIT},"deviceVertexDisplacementLimit":{VERTEX_LIMIT},"stagePolygonModel":"De Casteljau flattening of the same f32-reparsed Kurbo cubic outline serialized into the curves PDF; not an independent capsule","rows":[{}]}}"#,rows.join(",")).as_bytes());
}
