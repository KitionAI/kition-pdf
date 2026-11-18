//! Neutral overlap diagnostics with fixed analytic box-filter control checks.
#[path = "support/coincident_cap_fixtures.rs"]
mod fixtures;
use kition_pdf_render::{PageRenderer, RenderConfig, RenderRequest};
use std::sync::Arc;

fn rectangle_area(rect: [f64; 4], pixel: [f64; 4]) -> f64 {
    (rect[2].min(pixel[2]) - rect[0].max(pixel[0])).max(0.0) * (rect[3].min(pixel[3]) - rect[1].max(pixel[1])).max(0.0)
}
fn coverage(case: &fixtures::Case, x: u32, y: u32) -> f64 {
    let scale = f64::from(case.scale);
    let pixel = [f64::from(x) / scale, f64::from(y) / scale, f64::from(x + 1) / scale, f64::from(y + 1) / scale];
    let rects = case.rectangles();
    let mut area = rectangle_area(rects[0], pixel);
    if let Some(second) = rects.get(1) {
        area += rectangle_area(*second, pixel);
        let first = rects[0];
        let overlap = [first[0].max(second[0]), first[1].max(second[1]), first[2].min(second[2]), first[3].min(second[3])];
        area -= rectangle_area(overlap, pixel);
    }
    let out = area * scale * scale;
    assert!((0.0..=1.0).contains(&out));
    out
}
fn model_gray(alpha: f64, coverage: f64, model: &str) -> f64 {
    match model {
        "single" => 255.0 * (1.0 - alpha * coverage),
        "repeated-raster-coverage" => 255.0 * (1.0 - alpha * coverage).powi(2),
        "repeated-pointwise-coverage" => 255.0 * (1.0 - coverage * (2.0 * alpha - alpha * alpha)),
        _ => panic!("unknown diagnostic model"),
    }
}
fn observation(case: &fixtures::Case, rgba: &[u8], model: &str) -> (f64, f64, usize) {
    let side = fixtures::PAGE_SIDE * case.scale;
    let (mut mass, mut max_residual, mut outside_bound) = (0.0, 0.0_f64, 0);
    for y in 0..side {
        for x in 0..side {
            if (4 * case.scale..8 * case.scale).contains(&x) && (4 * case.scale..8 * case.scale).contains(&y) {
                continue;
            }
            let offset = (((side - 1 - y) * side + x) * 4) as usize;
            let expected = model_gray(case.opacity, coverage(case, x, y), model);
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
fn square_cap_diagnostics_validate_fill_controls_and_record_overlap_disagreements() {
    let cases = fixtures::cases();
    let out = fixtures::output();
    if let Some(out) = &out {
        fixtures::export(&cases, out);
    }
    let mut observations = Vec::new();
    for case in &cases {
        let side = fixtures::PAGE_SIDE * case.scale;
        let mut sum_coverage = 0.0;
        for y in 0..side {
            for x in 0..side {
                sum_coverage += coverage(case, x, y);
            }
        }
        let analytic_area = case.union_area() * f64::from(case.scale * case.scale);
        assert!((sum_coverage - analytic_area).abs() < 1e-9, "{} independent area", case.id);
        let mut images = Vec::new();
        for variant in case.variants() {
            let page = PageRenderer::new(Arc::new(fixtures::pdf(case, variant)), RenderConfig::default())
                .render(RenderRequest { scale: case.scale as f32, ..Default::default() });
            assert!(page.error.is_none() && page.warnings.is_empty(), "{} {variant}: {:?} {:?}", case.id, page.error, page.warnings);
            assert_eq!((page.width, page.height), (side, side));
            assert_eq!(page.rgba.len(), (side * side * 4) as usize);
            assert!(page.rgba.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 255));
            for y in 5 * case.scale..7 * case.scale {
                for x in 5 * case.scale..7 * case.scale {
                    let offset = (((side - 1 - y) * side + x) * 4) as usize;
                    assert_eq!(&page.rgba[offset..offset + 4], &[255, 0, 0, 255]);
                }
            }
            if let Some(out) = &out {
                fixtures::write(out, &format!("{}-{variant}.rgba", case.id), &page.rgba);
            }
            images.push(page);
        }
        let control = observation(case, &images[1].rgba, "single");
        assert_eq!(control.2, 0, "{} union-fill analytic residual {}", case.id, control.1);
        let sample = observation(case, &images[0].rgba, "single");
        let sample_diff = difference(&images[0].rgba, &images[1].rgba);
        let repeated = if let Some(page) = images.get(2) {
            let raster_model = observation(case, &page.rgba, "repeated-raster-coverage");
            let pointwise_model = observation(case, &page.rgba, "repeated-pointwise-coverage");
            let sample_repeated = difference(&images[0].rgba, &page.rgba);
            // The two AA/compositing models are competing observations, not acceptance rules.
            format!(
                r#"{{"mass":{},"rasterCoverageModel":{{"maxResidual":{},"outsideTolerancePixels":{}}},"pointwiseCoverageModel":{{"maxResidual":{},"outsideTolerancePixels":{}}},"sampleDifference":{{"exactChangedPixels":{},"outsideTolerancePixels":{},"maxDelta":{}}}}}"#,
                raster_model.0,
                raster_model.1,
                raster_model.2,
                pointwise_model.1,
                pointwise_model.2,
                sample_repeated.0,
                sample_repeated.1,
                sample_repeated.2
            )
        } else {
            "null".to_string()
        };
        let row = format!(
            r#"{{"id":"{}","scale":{},"analyticCoverageArea":{analytic_area},"analyticSingleInkMass":{},"unionControl":{{"mass":{},"maxResidual":{},"outsideTolerancePixels":{}}},"sample":{{"mass":{},"singleCoverageModelMaxResidual":{},"singleCoverageModelOutsideTolerancePixels":{}}},"sampleUnionDifference":{{"exactChangedPixels":{},"outsideTolerancePixels":{},"maxDelta":{}}},"repeatedFill":{repeated}}}"#,
            case.id,
            case.scale,
            analytic_area * case.opacity,
            control.0,
            control.1,
            control.2,
            sample.0,
            sample.1,
            sample.2,
            sample_diff.0,
            sample_diff.1,
            sample_diff.2
        );
        println!("coincident_cap {row}");
        observations.push(row);
    }
    if let Some(out) = &out {
        fixtures::write(out, "native-observations.json", format!(
            r#"{{"purpose":"Neutral sample observations; passing validates filled-union controls only","controlChannelTolerance":2,"observations":[{}]}}"#,
            observations.join(",")
        ).as_bytes());
    }
}
