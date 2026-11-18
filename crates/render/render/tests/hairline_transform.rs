//! Contributor-original solid strokes with invariant final centerlines; no external assets.

use kition_pdf_render::{PageRenderer, RenderConfig, RenderRequest};
use std::{fmt::Write, path::PathBuf, sync::Arc};

const SIZE: u32 = 128;
const MATRICES: [(&str, [f64; 4]); 7] = [
    ("identity", [1.0, 0.0, 0.0, 1.0]),
    ("uniform", [10.0, 0.0, 0.0, 10.0]),
    ("uniform-large", [1000.0, 0.0, 0.0, 1000.0]),
    ("wide", [100.0, 0.0, 0.0, 0.01]),
    ("tall", [0.01, 0.0, 0.0, 100.0]),
    ("shear", [1.0, 0.0, 100.0, 1.0]),
    ("rotated-wide", [0.0, 100.0, -0.01, 0.0]),
];
const LINES: [(&str, [f64; 4]); 3] =
    [("horizontal", [16.0, 64.5, 112.0, 64.5]), ("vertical", [64.5, 16.0, 64.5, 112.0]), ("diagonal", [24.5, 24.5, 103.5, 103.5])];

fn local_delta([a, b, c, d]: [f64; 4], [x0, y0, x1, y1]: [f64; 4]) -> [f64; 2] {
    let determinant = a * d - b * c;
    let result = [(d * (x1 - x0) - c * (y1 - y0)) / determinant, (-b * (x1 - x0) + a * (y1 - y0)) / determinant];
    assert!((a * result[0] + c * result[1] - (x1 - x0)).abs() < 1e-8);
    assert!((b * result[0] + d * result[1] - (y1 - y0)).abs() < 1e-8);
    result
}

fn pdf(matrix: [f64; 4], points: [f64; 4], width: f64) -> Vec<u8> {
    let [a, b, c, d] = matrix;
    let [x, y, _, _] = points;
    let [dx, dy] = local_delta(matrix, points);
    let content = format!("q 0 G 0 J {a} {b} {c} {d} {x} {y} cm {width} w 0 0 m {dx:.12} {dy:.12} l S Q\n");
    content_pdf(&content)
}

fn content_pdf(content: &str) -> Vec<u8> {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_string(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {SIZE} {SIZE}] /Resources << >> /Contents 4 0 R >>"),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
    ];
    let mut output = String::from("%PDF-1.7\n");
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(output.len());
        writeln!(output, "{} 0 obj\n{object}\nendobj", index + 1).unwrap();
    }
    let start = output.len();
    output.push_str("xref\n0 5\n0000000000 65535 f \n");
    for offset in offsets {
        writeln!(output, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(output, "trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{start}\n%%EOF").unwrap();
    assert!(output.is_ascii());
    output.into_bytes()
}

fn export_root() -> Option<PathBuf> {
    let requested = std::env::var_os("KITION_PDF_HAIRLINE_OUT")?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../target");
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let output = PathBuf::from(requested);
    assert!(output.is_absolute() && output.starts_with(&root));
    assert!(!output.components().any(|p| p == std::path::Component::ParentDir));
    std::fs::create_dir_all(&output).unwrap();
    let output = output.canonicalize().unwrap();
    assert!(output.starts_with(root));
    Some(output)
}

fn write_new(root: &Option<PathBuf>, name: &str, bytes: &[u8]) {
    if let Some(root) = root {
        let path = root.join(name);
        if path.exists() {
            assert!(std::fs::read(path).unwrap() == bytes, "refuse to replace different fixture evidence");
        } else {
            std::fs::write(path, bytes).unwrap();
        }
    }
}

#[test]
fn zero_width_strokes_keep_device_minimum_under_equivalent_transforms() {
    let output = export_root();
    let mut cases = Vec::new();
    let mut failures = Vec::new();
    for width in [0.0, 0.001] {
        for (matrix_id, matrix) in MATRICES {
            for (line_id, points) in LINES {
                let id = format!("{}-{matrix_id}-{line_id}", if width == 0.0 { "hairline" } else { "thin" });
                let bytes = pdf(matrix, points, width);
                write_new(&output, &format!("{id}.pdf"), &bytes);
                let local = local_delta(matrix, points);
                cases.push(format!(
                    r#"{{"id":"{id}","file":"{id}.pdf","lineWidth":{width},"matrix":{matrix:?},"translation":[{},{}],"localStart":[0,0],"localEnd":{local:?},"pagePoints":{points:?},"normativeHairline":{}}}"#,
                    points[0], points[1], width == 0.0
                ));
                let mut renderer = PageRenderer::new(Arc::new(bytes), RenderConfig::default());
                for scale in [1_u32, 2, 4] {
                    let page = renderer.render(RenderRequest { scale: scale as f32, ..Default::default() });
                    assert!(page.error.is_none() && page.warnings.is_empty(), "{id}: {:?} {:?}", page.error, page.warnings);
                    assert_eq!((page.width, page.height), (SIZE * scale, SIZE * scale));
                    let [x0, y0, x1, y1] = points.map(|p| p * f64::from(scale));
                    let length = (x1 - x0).hypot(y1 - y0);
                    let (tx, ty) = ((x1 - x0) / length, (y1 - y0) / length);
                    let mut bins = vec![0.0; length.floor() as usize];
                    let (mut mass, mut moment, mut outside) = (0.0, 0.0, 0.0_f64);
                    for y in 0..page.height {
                        for x in 0..page.width {
                            let p = &page.rgba[((y * page.width + x) * 4) as usize..][..4];
                            assert!(p[0] == p[1] && p[1] == p[2] && p[3] == 255);
                            let ink = 1.0 - f64::from(p[0]) / 255.0;
                            let (dx, dy) = (f64::from(x) + 0.5 - x0, f64::from(page.height) - f64::from(y) - 0.5 - y0);
                            let (along, across) = (dx * tx + dy * ty, -dx * ty + dy * tx);
                            if along >= 3.0 && along < bins.len() as f64 - 3.0 {
                                bins[along.floor() as usize] += ink;
                                mass += ink;
                                moment += ink * across;
                            }
                            if across.abs() > 3.0 || along < -3.0 || along > length + 3.0 {
                                outside = outside.max(ink);
                            }
                        }
                    }
                    let interior = &bins[3..bins.len() - 3];
                    let equivalent_width = mass / interior.len() as f64;
                    let empty_bins = interior.iter().filter(|v| **v <= 0.01).count();
                    let centroid = if mass > 0.0 { moment / mass } else { 0.0 };
                    println!(
                        "hairline id={id} dpi={} equivalent_width={equivalent_width:.6} empty_bins={empty_bins} intervals={} centroid={centroid:.6} outside={outside:.6}",
                        scale * 72,
                        interior.len()
                    );
                    // Zero width is device-minimum, regardless of user-space reparameterization.
                    // The broad bounds tolerate coverage differences; nonzero widths are exploratory.
                    if width == 0.0
                        && (!(0.75..=1.25).contains(&equivalent_width) || empty_bins != 0 || centroid.abs() > 1.0 || outside > 5.0 / 255.0)
                    {
                        failures.push(format!(
                            "{id}@{}dpi: width={equivalent_width:.6}, empty={empty_bins}, centroid={centroid:.6}, outside={outside:.6}",
                            scale * 72
                        ));
                    }
                }
            }
        }
    }
    let manifest = format!(
        r#"{{"schema":1,"origin":"Contributor-original geometry, Apache-2.0","pageWidth":{SIZE},"pageHeight":{SIZE},"coordinateSpace":"pdf-user-space-y-up","scales":[1,2,4],"cases":[{}]}}"#,
        cases.join(",")
    );
    write_new(&output, "manifest.json", manifest.as_bytes());
    assert!(failures.is_empty(), "zero-width minimum/continuity violations:\n{}", failures.join("\n"));
}

fn control_pdf(matrix: [f64; 4], style: u32, clipped: bool) -> Vec<u8> {
    let [a, b, c, d] = matrix;
    let mut content = String::from("q\n");
    if clipped {
        content.push_str("40 8 48 112 re W n\n");
    }
    writeln!(content, "q {a} {b} {c} {d} 0 0 cm 0 G 0 w {style} J {style} j 4 M").unwrap();
    for (points, closed) in [
        (vec![[16.0, 40.0], [64.0, 72.0], [112.0, 40.0]], false),
        (vec![[24.0, 88.0], [104.0, 88.0], [104.0, 112.0], [24.0, 112.0]], true),
        (vec![[32.0, 20.0], [96.0, 20.0]], false),
    ] {
        for (index, [x, y]) in points.into_iter().enumerate() {
            let [x, y] = local_delta(matrix, [0.0, 0.0, x, y]);
            writeln!(content, "{x:.12} {y:.12} {}", if index == 0 { "m" } else { "l" }).unwrap();
        }
        content.push_str(if closed { "h S\n" } else { "S\n" });
    }
    // A later ordinary filled mark must retain its page-space placement after stroking.
    content.push_str("Q Q 0 g 4 4 8 8 re f\n");
    content_pdf(&content)
}

#[test]
fn solid_hairline_caps_joins_closed_paths_and_existing_clips_are_transform_invariant() {
    let mut comparisons = 0;
    for style in [0, 1, 2] {
        for clipped in [false, true] {
            for scale in [1, 2, 4] {
                let request = RenderRequest { scale: scale as f32, ..Default::default() };
                let reference = PageRenderer::new(Arc::new(control_pdf(MATRICES[0].1, style, clipped)), RenderConfig::default()).render(request);
                assert!(reference.error.is_none() && reference.warnings.is_empty());
                for (id, matrix) in MATRICES.into_iter().skip(1) {
                    let page = PageRenderer::new(Arc::new(control_pdf(matrix, style, clipped)), RenderConfig::default())
                        .render(RenderRequest { scale: scale as f32, ..Default::default() });
                    assert!(page.error.is_none() && page.warnings.is_empty());
                    assert_eq!((page.width, page.height), (reference.width, reference.height));
                    let changed = page.rgba.iter().zip(reference.rgba.iter()).filter(|(a, b)| a.abs_diff(**b) > 2).count();
                    assert_eq!(changed, 0, "{id}, cap/join={style}, clipped={clipped}, scale={scale}");
                    comparisons += 1;
                }
            }
        }
    }
    println!("hairline compound control comparisons={comparisons} max_channel_tolerance=2");
}
