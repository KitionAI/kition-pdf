//! Original cycle, resource-rebinding, repeated-glyph, and warning reset regressions.
#[path = "support/type3_fixtures.rs"]
mod fixtures;
use kition_pdf_render::{PageRenderer, RenderConfig, RenderRequest, RenderWarning};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

#[test]
fn type3_cycles_warn_and_finite_resource_rebinding_siblings_and_transforms_render() {
    let cases = fixtures::cases();
    fixtures::export(&cases);
    let mut rasters = HashMap::new();
    for case in &cases {
        for scale in [1_u32, 2] {
            let started = Instant::now();
            let data = case.data.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let mut renderer = PageRenderer::new(Arc::new(data), RenderConfig::default());
                let page = renderer.render(RenderRequest { scale: scale as f32, ..Default::default() });
                let next = renderer.render(RenderRequest { page: 1, scale: scale as f32, ..Default::default() });
                let repeated = renderer.render(RenderRequest { scale: scale as f32, ..Default::default() });
                let _ = tx.send((page, next, repeated));
            });
            let (page, next, repeated) =
                rx.recv_timeout(Duration::from_secs(20)).expect("three Type3 renders must finish within the unchanged 20-second bound");
            assert!(page.error.is_none(), "{}: {:?}", case.id, page.error);
            assert_eq!((page.width, page.height), (40 * scale, 40 * scale));
            let warnings = page.warnings.iter().filter(|w| **w == RenderWarning::ContentTruncated).count();
            assert_eq!(warnings, usize::from(case.expected_truncation), "{}: {:?}", case.id, page.warnings);
            assert_eq!(page.warnings.len(), warnings, "{}: unexpected warning", case.id);
            assert!(next.error.is_none() && next.warnings.is_empty(), "{}: the next page resets warning state", case.id);
            assert!(repeated.error.is_none(), "{}: repeated page error", case.id);
            for (x, y, expected) in [(11, 11, [0, 255, 0, 255]), (1, 1, [255, 0, 0, 255]), (32, 32, [255; 4])] {
                let offset = (((40 - y) * scale - 1) * next.width + x * scale) as usize * 4;
                assert_eq!(&next.rgba[offset..offset + 4], &expected, "{}: reset-page content", case.id);
            }
            assert_eq!(page.rgba, repeated.rgba, "{}: guards do not survive a completed render", case.id);
            assert_eq!(page.warnings, repeated.warnings);
            let mut checked = 0;
            for region in &case.regions {
                let [left, bottom, width, height] = region.rect;
                let before = checked;
                for y in 0..page.height {
                    for x in 0..page.width {
                        let px = (f64::from(x) + 0.5) / f64::from(scale);
                        let py = 40.0 - (f64::from(y) + 0.5) / f64::from(scale);
                        if px >= left && px < left + width && py >= bottom && py < bottom + height {
                            checked += 1;
                            let pixel = &page.rgba[((y * page.width + x) * 4) as usize..][..4];
                            assert_eq!(pixel[3], 255);
                            assert!(
                                pixel[..3].iter().zip(region.rgb).all(|(a, b)| a.abs_diff(b) <= 8),
                                "{} scale={scale} pixel=({x},{y}) actual={pixel:?} expected={:?}",
                                case.id,
                                region.rgb
                            );
                        }
                    }
                }
                assert!(checked > before, "{}: region contains no sampled pixels", case.id);
            }
            println!(
                "type3 case={} scale={scale} warning_count={warnings} pixels={checked} elapsed_ms={:.3}",
                case.id,
                started.elapsed().as_secs_f64() * 1000.0
            );
            rasters.insert((case.id, scale), page.rgba);
        }
    }
    for case in &cases {
        if let Some(control) = case.expected_match {
            for scale in [1, 2] {
                assert_eq!(rasters[&(case.id, scale)], rasters[&(control, scale)], "{} vs {control} scale={scale}", case.id);
            }
        }
    }
}
