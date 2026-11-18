//! Original pure-zero disconnected dashes versus independent per-subpath dots.
#[path = "support/pure_zero_separated_fixtures.rs"]
mod fixtures;
use kition_pdf_render::{PageRenderer, RenderConfig, RenderRequest};
use std::sync::Arc;

#[test]
fn pure_zero_separated_hairlines_restart_phase_and_keep_one_device_paint() {
    let cases = fixtures::cases();
    fixtures::export(&cases);
    let mut failures = Vec::new();
    for case in &cases {
        for scale in [1_u32, 2, 4] {
            let render = |control| {
                PageRenderer::new(Arc::new(fixtures::pdf(case, control)), RenderConfig::default())
                    .render(RenderRequest { scale: scale as f32, ..Default::default() })
            };
            let (sample, reference) = (render(false), render(true));
            if let Some(out) = std::env::var_os("KITION_PDF_PURE_ZERO_SEPARATED_OUT") {
                for (control, page) in [(false, &sample), (true, &reference)] {
                    let suffix = if control { "-control" } else { "" };
                    let path = std::path::PathBuf::from(&out).join(format!("{}{suffix}@{scale}.rgba", case.id));
                    if path.exists() {
                        assert_eq!(std::fs::read(path).unwrap().as_slice(), &page.rgba[..], "refuse changed pure-zero native raster");
                    } else {
                        std::fs::write(path, &page.rgba).unwrap();
                    }
                }
            }
            for page in [&sample, &reference] {
                assert!(page.error.is_none() && page.warnings.is_empty(), "{}: {:?} {:?}", case.id, page.error, page.warnings);
                assert_eq!((page.width, page.height), (128 * scale, 128 * scale));
                assert_eq!(page.rgba.len(), (page.width * page.height * 4) as usize);
                for y in 5 * scale..7 * scale {
                    for x in 5 * scale..7 * scale {
                        let at = (((128 * scale - 1 - y) * page.width + x) * 4) as usize;
                        assert_eq!(&page.rgba[at..at + 4], &[255, 0, 0, 255]);
                    }
                }
            }
            for (i, probe) in fixtures::probes(case).iter().enumerate() {
                let [x, y] = probe.point.map(|v| v * f64::from(scale));
                let mut mass = 0.0;
                for py in 0..reference.height {
                    for px in 0..reference.width {
                        let dx = f64::from(px) + 0.5 - x;
                        let dy = f64::from(reference.height - py) - 0.5 - y;
                        if dx.hypot(dy) <= 2.0 {
                            mass += (1.0 - f64::from(reference.rgba[((py * reference.width + px) * 4) as usize]) / 255.0) / probe.opacity;
                        }
                    }
                }
                println!("pure_zero_separated_reference_ink case={} scale={scale} probe={i} mass={mass:.8}", case.id);
                assert!((probe.min..=probe.max).contains(&mass), "{} independent probe{i} scale{scale} mass{mass}", case.id);
            }
            for rect in &case.gaps {
                for y in rect[1] * scale..(rect[1] + rect[3]) * scale {
                    for x in rect[0] * scale..(rect[0] + rect[2]) * scale {
                        let at = (((reference.height - 1 - y) * reference.width + x) * 4) as usize;
                        assert_eq!(&reference.rgba[at..at + 4], &[255, 255, 255, 255], "{} empty region", case.id);
                    }
                }
            }
            if let Some([x, y, _, _]) = case.alpha_pixel.filter(|_| scale == 1) {
                let at = ((127 - y) * reference.width + x) as usize * 4;
                assert!(reference.rgba[at].abs_diff(128) <= 2);
            }
            let (mut changed, mut delta) = (0, 0);
            for (a, b) in sample.rgba.as_chunks::<4>().0.iter().zip(reference.rgba.as_chunks::<4>().0.iter()) {
                assert_eq!((a[3], b[3]), (255, 255));
                let d = a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
                delta = delta.max(d);
                changed += usize::from(d > 2);
            }
            println!("pure_zero_separated_pair case={} scale={scale} changed_pixels={changed} max_delta={delta} tolerance=2", case.id);
            if changed > 0 {
                failures.push(format!("{}@{scale}: changed={changed}, max_delta={delta}", case.id));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("; "));
}
