//! Neutral native characterization. Passing means controls and rendering are valid;
//! it does not declare either competing exact-boundary hypothesis PDF-correct.
#[path = "support/boundary_dash_fixtures.rs"]
mod fixtures;
use kition_pdf_render::{PageRenderer, RenderConfig, RenderRequest};
use std::sync::Arc;

fn mass(rgba: &[u8], width: u32, height: u32, point: [f64; 2]) -> f64 {
    let mut out = 0.0;
    for y in 0..height {
        for x in 0..width {
            if (f64::from(x) + 0.5 - point[0]).hypot(f64::from(height - y) - 0.5 - point[1]) <= 2.0 {
                out += 1.0 - f64::from(rgba[((y * width + x) * 4) as usize]) / 255.0;
            }
        }
    }
    out
}
fn diff(a: &[u8], b: &[u8]) -> (usize, u8) {
    a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0.iter()).fold((0, 0), |(count, max), (a, b)| {
        let delta = a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
        (count + usize::from(delta > 2), max.max(delta))
    })
}
#[test]
fn boundary_diagnostics_validate_alternative_controls_and_record_current_pixels() {
    let cases = fixtures::cases();
    let out = fixtures::output();
    if let Some(out) = &out {
        fixtures::export(&cases, out);
    }
    let mut observations = Vec::new();
    for case in &cases {
        for scale in [1_u32, 2, 4] {
            let pages = fixtures::VARIANTS.map(|variant| {
                let page = PageRenderer::new(Arc::new(fixtures::pdf(case, variant)), RenderConfig::default())
                    .render(RenderRequest { scale: scale as f32, ..Default::default() });
                assert!(page.error.is_none() && page.warnings.is_empty(), "{} {variant}: {:?} {:?}", case.id, page.error, page.warnings);
                assert_eq!((page.width, page.height), (128 * scale, 128 * scale));
                assert_eq!(page.rgba.len(), (page.width * page.height * 4) as usize);
                assert!(page.rgba.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 255));
                for y in 5 * scale..7 * scale {
                    for x in 5 * scale..7 * scale {
                        let at = (((page.height - 1 - y) * page.width + x) * 4) as usize;
                        assert_eq!(&page.rgba[at..at + 4], &[255, 0, 0, 255]);
                    }
                }
                if let Some(out) = &out {
                    fixtures::write(out, &format!("{}-{variant}-{scale}.rgba", case.id), &page.rgba);
                }
                page
            });
            let alternatives = diff(&pages[1].rgba, &pages[2].rgba);
            assert!(alternatives.0 > 0, "{} controls must discriminate", case.id);
            let mut boundary_masses = Vec::new();
            for line in 0..if case.separated { 2 } else { 1 } {
                let point = fixtures::point(line, f64::from(case.boundary)).map(|v| v * f64::from(scale));
                let masses = pages.each_ref().map(|page| mass(&page.rgba, page.width, page.height, point));
                assert_eq!(masses[1], 0.0, "{} absent boundary control", case.id);
                assert!((0.5..=1.3).contains(&masses[2]), "{} present boundary control {}", case.id, masses[2]);
                boundary_masses.push(masses);
                for &dot in &case.dots {
                    let point = fixtures::point(line, f64::from(dot)).map(|v| v * f64::from(scale));
                    for page in [&pages[1], &pages[2]] {
                        assert!((0.5..=1.3).contains(&mass(&page.rgba, page.width, page.height, point)));
                    }
                }
                for &(a, b) in &case.runs {
                    let point = fixtures::point(line, f64::from(a + b) / 2.0).map(|v| v * f64::from(scale));
                    for page in [&pages[1], &pages[2]] {
                        assert!(mass(&page.rgba, page.width, page.height, point) > 1.5);
                    }
                }
            }
            let without = diff(&pages[0].rgba, &pages[1].rgba);
            let with = diff(&pages[0].rgba, &pages[2].rgba);
            let row = format!(
                r#"{{"id":"{}","scale":{scale},"withoutBoundary":{{"changedPixels":{},"maxDelta":{}}},"withBoundary":{{"changedPixels":{},"maxDelta":{}}},"alternativeChangedPixels":{},"boundaryMasses":{:?}}}"#,
                case.id, without.0, without.1, with.0, with.1, alternatives.0, boundary_masses
            );
            println!("boundary_diagnostic {row}");
            observations.push(row);
        }
    }
    if let Some(out) = &out {
        fixtures::write(
            out,
            "native-observations.json",
            format!(r#"{{"purpose":"Diagnostic comparisons, not normative assertions","tolerance":2,"observations":[{}]}}"#, observations.join(","))
                .as_bytes(),
        );
    }
}
