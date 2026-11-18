//! Single-round stage diagnostics with independently evaluated polygon control checks.
#[path = "support/round_stage_fixtures.rs"]
mod fixtures;
use kition_pdf_render::{PageRenderer, RenderConfig, RenderRequest};
use std::sync::Arc;

fn signed_area(points: &[[f64; 2]]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    let origin = points[0];
    let mut twice = 0.0;
    for (a, b) in points.iter().zip(points.iter().cycle().skip(1)).take(points.len()) {
        twice += (a[0] - origin[0]) * (b[1] - origin[1]) - (a[1] - origin[1]) * (b[0] - origin[0]);
    }
    twice / 2.0
}
fn clipped(points: &[[f64; 2]], axis: usize, bound: f64, above: bool) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    if points.is_empty() {
        return out;
    }
    for (a, b) in points.iter().zip(points.iter().cycle().skip(1)).take(points.len()) {
        let inside = |p: &[f64; 2]| if above { p[axis] >= bound } else { p[axis] <= bound };
        let (a_in, b_in) = (inside(a), inside(b));
        if a_in != b_in {
            let denominator = b[axis] - a[axis];
            assert_ne!(denominator, 0.0);
            let t = (bound - a[axis]) / denominator;
            let mut crossing = [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])];
            crossing[axis] = bound;
            out.push(crossing);
        }
        if b_in {
            out.push(*b);
        }
    }
    out
}
fn polygon_coverage(case: &fixtures::Case, vertices: Vec<[f64; 2]>) -> (Vec<f64>, f64, f64) {
    let side = fixtures::PAGE_SIDE * case.scale;
    let points = vertices.into_iter().map(|point| point.map(|v| v * f64::from(case.scale))).collect::<Vec<_>>();
    let polygon_area = signed_area(&points).abs();
    assert!(polygon_area > 0.0);
    let low = [points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min), points.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min)];
    let high = [points.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max), points.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max)];
    assert!(low.iter().all(|&v| v >= 0.0) && high.iter().all(|&v| v < f64::from(side)));
    let mut coverage = vec![0.0; (side * side) as usize];
    for y in low[1].floor() as u32..high[1].ceil() as u32 {
        let strip = clipped(&clipped(&points, 1, f64::from(y), true), 1, f64::from(y + 1), false);
        for x in low[0].floor() as u32..high[0].ceil() as u32 {
            let pixel = clipped(&clipped(&strip, 0, f64::from(x), true), 0, f64::from(x + 1), false);
            let area = signed_area(&pixel).abs();
            assert!((-1e-9..=1.0 + 1e-9).contains(&area));
            coverage[(y * side + x) as usize] = area.clamp(0.0, 1.0);
        }
    }
    let sum = coverage.iter().sum::<f64>();
    assert!((sum - polygon_area).abs() < 1e-8);
    (coverage, polygon_area, sum)
}
fn model_gray(alpha: f64, coverage: f64, model: &str) -> f64 {
    match model {
        "single" => 255.0 * (1.0 - alpha * coverage),
        "repeated-raster-coverage" => 255.0 * (1.0 - alpha * coverage).powi(2),
        "repeated-pointwise-coverage" => 255.0 * (1.0 - coverage * (2.0 * alpha - alpha * alpha)),
        _ => panic!("unknown diagnostic model"),
    }
}
fn observation(case: &fixtures::Case, coverage: &[f64], rgba: &[u8], model: &str) -> (f64, f64, usize) {
    let side = fixtures::PAGE_SIDE * case.scale;
    let (mut mass, mut max_residual, mut outside_bound) = (0.0, 0.0_f64, 0);
    for y in 0..side {
        for x in 0..side {
            if (4 * case.scale..8 * case.scale).contains(&x) && (4 * case.scale..8 * case.scale).contains(&y) {
                continue;
            }
            let offset = (((side - 1 - y) * side + x) * 4) as usize;
            let expected = model_gray(case.opacity, coverage[(y * side + x) as usize], model);
            let residual = rgba[offset..offset + 3].iter().map(|&channel| (f64::from(channel) - expected).abs()).fold(0.0_f64, f64::max);
            max_residual = max_residual.max(residual);
            outside_bound += usize::from(residual > 2.0);
            mass += 1.0 - f64::from(rgba[offset]) / 255.0;
        }
    }
    (mass, max_residual, outside_bound)
}
fn difference(a: &[u8], b: &[u8]) -> (usize, usize, u8) {
    a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0.iter()).fold((0, 0, 0), |(exact_changed, outside_tolerance, max), (a, b)| {
        let delta = a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
        (exact_changed + usize::from(delta > 0), outside_tolerance + usize::from(delta > 2), max.max(delta))
    })
}
#[test]
fn single_round_stage_diagnostics_record_expansion_curve_and_polygon_observations() {
    let cases = fixtures::cases();
    let out = fixtures::output();
    if let Some(out) = &out {
        fixtures::export(&cases, out);
    }
    let mut failures = Vec::new();
    let mut observations = Vec::new();
    for case in &cases {
        let side = fixtures::PAGE_SIDE * case.scale;
        let stage = fixtures::stage(case);
        let (dense_coverage, dense_area, _) = polygon_coverage(case, case.polygon().vertices);
        let (fine_coverage, fine_area, _) = polygon_coverage(case, stage.fine_polygon);
        let true_area = case.true_area() * f64::from(case.scale * case.scale);
        if (dense_area - true_area).abs() > case.area_error_bound() + 1e-9 {
            failures.push(format!("{} capsule area budget", case.id));
        }
        let mut images = Vec::new();
        let mut invalid = false;
        for variant in case.variants() {
            let page = PageRenderer::new(Arc::new(fixtures::pdf(case, variant)), RenderConfig::default())
                .render(RenderRequest { scale: case.scale as f32, ..Default::default() });
            if page.error.is_some()
                || !page.warnings.is_empty()
                || (page.width, page.height) != (side, side)
                || page.rgba.len() != (side * side * 4) as usize
            {
                failures.push(format!("{} {variant} render invalid: {:?} {:?}", case.id, page.error, page.warnings));
                invalid = true;
            }
            if let Some(out) = &out {
                fixtures::write(out, &format!("{}-{variant}.rgba", case.id), &page.rgba);
            }
            images.push(page);
        }
        if invalid {
            observations.push(format!(r#"{{"id":"{}","renderingValid":false}}"#, case.id));
            continue;
        }
        for page in &images {
            if !page.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255) {
                failures.push(format!("{} unexpected alpha", case.id));
            }
            for y in 5 * case.scale..7 * case.scale {
                for x in 5 * case.scale..7 * case.scale {
                    let at = (((side - 1 - y) * side + x) * 4) as usize;
                    if page.rgba[at..at + 4] != [255, 0, 0, 255] {
                        failures.push(format!("{} sentinel", case.id));
                    }
                }
            }
        }
        let fine_control = observation(case, &fine_coverage, &images[2].rgba, "single");
        let dense_control = observation(case, &dense_coverage, &images[3].rgba, "single");
        if fine_control.2 > 0 {
            failures.push(format!("{} fine-polygon fixed control bound {}", case.id, fine_control.1));
        }
        if dense_control.2 > 0 {
            failures.push(format!("{} capsule fixed control bound {}", case.id, dense_control.1));
        }
        let mut comparisons = Vec::new();
        for (name, a, b) in
            [("strokeCurves", 0, 1), ("curvesFinePolygon", 1, 2), ("finePolygonCapsule", 2, 3), ("strokeCapsule", 0, 3), ("curvesCapsule", 1, 3)]
        {
            let d = difference(&images[a].rgba, &images[b].rgba);
            comparisons.push(format!(r#""{name}":{{"exactChangedPixels":{},"outsideTolerancePixels":{},"maxDelta":{}}}"#, d.0, d.1, d.2));
        }
        let masses = images.iter().map(|page| observation(case, &dense_coverage, &page.rgba, "single").0).collect::<Vec<_>>();
        let row = format!(
            r#"{{"id":"{}","scale":{},"renderingValid":true,"expansionPathsEqual":{},"finePolygonArea":{fine_area},"capsulePolygonArea":{dense_area},"trueArea":{true_area},"rawUserCurveArea":{},"parsedUserCurveArea":{},"massesStrokeCurvesFineCapsule":{masses:?},"finePolygonControl":{{"maxResidual":{},"outsideTolerancePixels":{}}},"capsuleControl":{{"maxResidual":{},"outsideTolerancePixels":{}}},"comparisons":{{{}}}}}"#,
            case.id,
            case.scale,
            stage.expansion_equal,
            fixtures::signed_path_area(&stage.raw_user) * f64::from(case.scale * case.scale),
            fixtures::signed_path_area(&stage.parsed_user) * f64::from(case.scale * case.scale),
            fine_control.1,
            fine_control.2,
            dense_control.1,
            dense_control.2,
            comparisons.join(",")
        );
        println!("round_stage {row}");
        observations.push(row);
    }
    if let Some(out) = &out {
        fixtures::write(out,"native-observations.json",format!(r#"{{"purpose":"Internal stage observations; samples are not normative assertions","controlChannelTolerance":2,"validationFailureCount":{},"observations":[{}]}}"#,failures.len(),observations.join(",")).as_bytes());
    }
    assert!(failures.is_empty(), "{}", failures.join("; "));
}
