//! Original dashed hairline geometry compared with manually segmented solid controls.
#[path = "support/dash_fixtures.rs"]
mod fixtures;
use kition_pdf_render::{PageRenderer, RenderConfig, RenderRequest};
use std::sync::Arc;

#[test]
fn dashed_hairlines_keep_user_space_runs_and_device_width_in_one_paint() {
    let cases = fixtures::cases();
    fixtures::export(&cases);
    let mut failures = Vec::new();
    for case in &cases {
        for scale in [1_u32, 2, 4] {
            let render = |control| {
                PageRenderer::new(Arc::new(fixtures::pdf(case, control)), RenderConfig::default())
                    .render(RenderRequest { scale: scale as f32, ..Default::default() })
            };
            let sample = render(false);
            let reference = render(true);
            assert!(sample.error.is_none() && sample.warnings.is_empty(), "{}: {:?} {:?}", case.id, sample.error, sample.warnings);
            assert!(reference.error.is_none() && reference.warnings.is_empty());
            assert_eq!((sample.width, sample.height), (128 * scale, 128 * scale));
            let mut max_delta = 0;
            let mut changed = 0;
            for (left, right) in sample.rgba.as_chunks::<4>().0.iter().zip(reference.rgba.as_chunks::<4>().0.iter()) {
                assert_eq!(left[3], 255);
                assert_eq!(right[3], 255);
                let delta = left.iter().zip(right).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
                max_delta = max_delta.max(delta);
                changed += usize::from(delta > 2);
            }
            for page in [&sample, &reference] {
                for y in 5 * scale..7 * scale {
                    for x in 5 * scale..7 * scale {
                        let at = (((128 * scale - 1 - y) * page.width + x) * 4) as usize;
                        assert_eq!(&page.rgba[at..at + 4], &[255, 0, 0, 255], "{}: following paint", case.id);
                    }
                }
            }
            for (index, probe) in fixtures::ink_probes(case).iter().enumerate() {
                let [x, y] = probe.point.map(|value| value * f64::from(scale));
                let mut mass = 0.0;
                for py in 0..reference.height {
                    for px in 0..reference.width {
                        let dx = f64::from(px) + 0.5 - x;
                        let dy = f64::from(reference.height) - f64::from(py) - 0.5 - y;
                        if dx.hypot(dy) <= 2.0 {
                            mass += (1.0 - f64::from(reference.rgba[((py * reference.width + px) * 4) as usize]) / 255.0) / probe.opacity;
                        }
                    }
                }
                assert!((probe.min_mass..=probe.max_mass).contains(&mass), "{}: independently authored ink control {index}, mass={mass}", case.id);
                println!("dash_reference_ink case={} index={index} scale={scale} mass={mass:.8}", case.id);
            }
            for rect in fixtures::empty_regions(case) {
                for y in rect[1] * scale..(rect[1] + rect[3]) * scale {
                    for x in rect[0] * scale..(rect[0] + rect[2]) * scale {
                        let at = (((128 * scale - 1 - y) * reference.width + x) * 4) as usize;
                        assert_eq!(&reference.rgba[at..at + 4], &[255, 255, 255, 255], "{}: authored empty region", case.id);
                    }
                }
            }
            if case.alpha_cross && scale == 1 {
                let at = ((127 - 64) * reference.width + 64) as usize * 4;
                assert!(reference.rgba[at].abs_diff(128) <= 2, "single-paint opacity control");
            }
            println!("dash_pair case={} scale={scale} changed_pixels={changed} max_delta={max_delta} tolerance=2", case.id);
            if changed > 0 {
                failures.push(format!("{}@{scale}: changed={changed}, max_delta={max_delta}", case.id));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("; "));
}

#[test]
fn zero_dash_entries_survive_both_content_and_graphics_state_parsing() {
    let all = fixtures::cases();
    let mut cases: Vec<_> = all.iter().filter(|case| matches!(case.id, "round-dots" | "square-dots" | "zero-gaps")).cloned().collect();
    let mut all_zero = all.iter().find(|case| case.id == "invalid-solid").unwrap().clone();
    all_zero.id = "all-zero-solid";
    all_zero.dashes = vec![0.0, 0.0];
    cases.push(all_zero);
    for case in cases {
        for scale in [1.0, 2.0, 4.0] {
            let render = |bytes| PageRenderer::new(Arc::new(bytes), RenderConfig::default()).render(RenderRequest { scale, ..Default::default() });
            let reference = render(fixtures::pdf(&case, true));
            assert!(reference.error.is_none() && reference.warnings.is_empty());
            assert_dark_ink(&reference);
            for ext_gstate in [false, true] {
                let sample = render(fixtures::pdf_with_dash_state(&case, false, ext_gstate));
                assert!(sample.error.is_none() && sample.warnings.is_empty());
                assert_eq!(sample.rgba.len(), reference.rgba.len());
                assert!(
                    sample.rgba.iter().zip(reference.rgba.iter()).all(|(a, b)| a.abs_diff(*b) <= 2),
                    "{}@{scale} ExtGState={ext_gstate}",
                    case.id
                );
            }
        }
    }
}

#[test]
fn zero_dash_caps_follow_transformed_straight_tangents() {
    let template = fixtures::cases().into_iter().find(|case| case.id == "square-dots").unwrap();
    let diagonal = std::f64::consts::FRAC_1_SQRT_2;
    for (matrix, direction, delta) in [
        ([0.0, 1.0, -0.01, 0.0, 64.5, 16.0], [1.0, 0.0], [0.0, 1.0 / 8192.0]),
        ([diagonal, diagonal, -0.01 * diagonal, 0.01 * diagonal, 24.0, 24.0], [1.0, 0.0], [1.0 / 16384.0, 1.0 / 16384.0]),
        ([1.0, 0.0, 0.0, 1.0, 24.0, 24.0], [0.6, 0.8], [3.0 / 32768.0, 4.0 / 32768.0]),
        ([1.0, 0.0, 0.0, 1.0, 24.0, 24.0], [diagonal, diagonal], [1.0 / 16384.0, 1.0 / 16384.0]),
    ] {
        let mut case = template.clone();
        case.matrix = matrix;
        // Length90 avoids ambiguous zero dots exactly at a serialized endpoint.
        case.path = vec![fixtures::Op::Move([0.0, 0.0]), fixtures::Op::Line(direction.map(|v| 90.0 * v))];
        let mut control = case.clone();
        control.matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        control.reference = [0.0, 24.0, 48.0, 72.0]
            .into_iter()
            .flat_map(|distance| {
                let [x, y] = direction.map(|value| value * distance);
                let [a, b, c, d, e, f] = matrix;
                let start = [a * x + c * y + e, b * x + d * y + f].map(|value| f64::from(value as f32));
                let end = [start[0] + delta[0], start[1] + delta[1]];
                // Binary-exact deltas preserve the intended tangent after PDF f32 parsing.
                for axis in 0..2 {
                    assert_eq!(f64::from(end[axis] as f32) - f64::from(start[axis] as f32), delta[axis]);
                }
                [fixtures::Op::Move(start), fixtures::Op::Line(end)]
            })
            .collect();
        for scale in [1.0, 2.0, 4.0] {
            let render = |is_control| {
                PageRenderer::new(Arc::new(fixtures::pdf(if is_control { &control } else { &case }, is_control)), RenderConfig::default())
                    .render(RenderRequest { scale, ..Default::default() })
            };
            let (sample, reference) = (render(false), render(true));
            assert!(sample.error.is_none() && reference.error.is_none());
            assert!(sample.warnings.is_empty() && reference.warnings.is_empty());
            assert_eq!(sample.rgba.len(), reference.rgba.len());
            assert_dark_ink(&reference);
            for distance in [0.0, 24.0, 48.0, 72.0] {
                let [local_x, local_y] = direction.map(|value| value * distance);
                let [a, b, c, d, e, f] = matrix;
                let px = (a * local_x + c * local_y + e) * f64::from(scale);
                let py = (b * local_x + d * local_y + f) * f64::from(scale);
                let mut mass = 0.0;
                for y in 0..reference.height {
                    for x in 0..reference.width {
                        let dx = f64::from(x) + 0.5 - px;
                        let dy = f64::from(reference.height - y) - 0.5 - py;
                        if dx.hypot(dy) <= 2.0 {
                            mass += 1.0 - f64::from(reference.rgba[((y * reference.width + x) * 4) as usize]) / 255.0;
                        }
                    }
                }
                assert!((0.5..=1.3).contains(&mass), "directional square-dot control mass={mass}");
            }
            assert!(
                sample.rgba.iter().zip(reference.rgba.iter()).all(|(a, b)| a.abs_diff(*b) <= 2),
                "matrix={matrix:?}, scale={scale}, direction={direction:?}"
            );
        }
    }
}

#[test]
fn compound_and_curved_zero_caps_retain_legacy_fallback() {
    let template = fixtures::cases().into_iter().find(|case| case.id == "round-dots").unwrap();
    for (path, matrix) in [
        (vec![fixtures::Op::Move([0.0, 0.0]), fixtures::Op::Line([48.0, 0.0]), fixtures::Op::Line([48.0, 24.0])], [1.0, 0.0, 0.0, 1.0, 24.0, 48.0]),
        (vec![fixtures::Op::Move([0.0, 0.0]), fixtures::Op::Curve([0.0, 24.0], [24.0, 24.0], [24.0, 0.0])], [1.0, 0.0, 0.0, 1.0, 24.0, 48.0]),
        (vec![fixtures::Op::Move([0.0, 0.0]), fixtures::Op::Line([96.0, 0.0])], [0.0, 0.0, 0.0, 1.0, 64.0, 48.0]),
    ] {
        let mut sample = template.clone();
        sample.matrix = matrix;
        sample.path = path;
        for scale in [1.0, 2.0, 4.0] {
            let mut legacy = sample.clone();
            legacy.width = 1.0 / scale;
            legacy.dashes = vec![0.01, 24.0];
            let render = |case| {
                PageRenderer::new(Arc::new(fixtures::pdf(case, false)), RenderConfig::default())
                    .render(RenderRequest { scale: scale as f32, ..Default::default() })
            };
            let (actual, expected) = (render(&sample), render(&legacy));
            assert!(actual.error.is_none() && expected.error.is_none());
            assert!(actual.warnings.is_empty() && expected.warnings.is_empty());
            assert_eq!(actual.rgba, expected.rgba, "fallback differs at scale {scale}");
        }
    }
}

fn assert_dark_ink(page: &kition_pdf_render::RenderedPage) {
    // The red continuation sentinel has red=255, so it contributes no dark mass.
    let mass: f64 = page.rgba.as_chunks::<4>().0.iter().map(|pixel| 1.0 - f64::from(pixel[0]) / 255.0).sum();
    assert!(mass >= 1.0, "expected visible black geometry, mass={mass}");
}

#[test]
fn mixed_exact_positive_end_phase_retains_legacy_fallback() {
    let mut sample = fixtures::cases().into_iter().find(|case| case.id == "round-dots").unwrap();
    sample.matrix = [1.0, 0.0, 0.0, 1.0, 24.0, 48.0];
    sample.path = vec![fixtures::Op::Move([0.0, 0.0]), fixtures::Op::Line([90.0, 0.0])];
    sample.dashes = vec![0.0, 12.0, 6.0, 12.0];
    sample.phase = 18.0;
    for scale in [1.0, 2.0, 4.0] {
        let mut legacy = sample.clone();
        legacy.width = 1.0 / scale;
        legacy.dashes = vec![0.01, 12.0, 6.0, 12.0];
        let render = |case| {
            PageRenderer::new(Arc::new(fixtures::pdf(case, false)), RenderConfig::default())
                .render(RenderRequest { scale: scale as f32, ..Default::default() })
        };
        let (actual, expected) = (render(&sample), render(&legacy));
        assert!(actual.error.is_none() && expected.error.is_none());
        assert!(actual.warnings.is_empty() && expected.warnings.is_empty());
        assert_eq!(actual.rgba, expected.rgba, "mixed-pattern fallback differs at scale {scale}");
    }
}

#[test]
fn mixed_excluded_sources_and_terminal_zero_caps_retain_legacy_pixels() {
    let template = fixtures::cases().into_iter().find(|case| case.id == "round-dots").unwrap();
    for (path, matrix, dashes) in [
        (
            vec![fixtures::Op::Move([0.0, 0.0]), fixtures::Op::Line([48.0, 0.0]), fixtures::Op::Line([48.0, 24.0])],
            [1.0, 0.0, 0.0, 1.0, 24.0, 48.0],
            vec![0.0, 12.0, 6.0, 12.0],
        ),
        (
            vec![fixtures::Op::Move([0.0, 0.0]), fixtures::Op::Curve([0.0, 24.0], [24.0, 24.0], [24.0, 0.0])],
            [1.0, 0.0, 0.0, 1.0, 24.0, 48.0],
            vec![0.0, 12.0, 6.0, 12.0],
        ),
        (vec![fixtures::Op::Move([0.0, 0.0]), fixtures::Op::Line([84.0, 0.0])], [0.0, 0.0, 0.0, 1.0, 64.0, 48.0], vec![0.0, 12.0, 6.0, 12.0]),
        (vec![fixtures::Op::Move([0.0, 0.0]), fixtures::Op::Line([30.0, 0.0])], [1.0, 0.0, 0.0, 1.0, 24.0, 48.0], vec![0.0, 12.0, 6.0, 12.0]),
        (vec![fixtures::Op::Move([0.0, 0.0]), fixtures::Op::Line([6.0, 0.0])], [1.0, 0.0, 0.0, 1.0, 24.0, 48.0], vec![6.0, 0.0, 0.0, 12.0]),
    ] {
        let mut sample = template.clone();
        sample.matrix = matrix;
        sample.path = path;
        sample.dashes = dashes;
        for cap in [1, 2] {
            sample.cap = cap;
            for scale in [1.0, 2.0, 4.0] {
                let mut legacy = sample.clone();
                legacy.width = 1.0 / scale;
                legacy.dashes.iter_mut().filter(|value| **value == 0.0).for_each(|value| *value = 0.01);
                let render = |case| {
                    PageRenderer::new(Arc::new(fixtures::pdf(case, false)), RenderConfig::default())
                        .render(RenderRequest { scale: scale as f32, ..Default::default() })
                };
                let (actual, expected) = (render(&sample), render(&legacy));
                assert!(actual.error.is_none() && expected.error.is_none());
                assert!(actual.warnings.is_empty() && expected.warnings.is_empty());
                assert_eq!(actual.rgba, expected.rgba, "excluded mixed fallback differs at scale {scale}, cap {cap}");
            }
        }
    }
}
