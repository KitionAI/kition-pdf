//! Original mesh visibility guards and equivalent patch-stream controls.
#[path = "support/mesh_fixtures.rs"]
mod fixtures;
use kition_pdf_render::{PageRenderer, RenderConfig, RenderRequest};
use std::{collections::HashMap, sync::Arc, time::Instant};

#[test]
fn original_patch_meshes_keep_visible_interiors_curves_clips_and_continuations() {
    let cases = fixtures::cases();
    fixtures::export(&cases);
    let mut rasters = HashMap::new();
    let mut failures = Vec::new();
    for case in &cases {
        for scale in [1_u32, 2] {
            let started = Instant::now();
            let bytes = fixtures::pdf(case);
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let page =
                    PageRenderer::new(Arc::new(bytes), RenderConfig::default()).render(RenderRequest { scale: scale as f32, ..Default::default() });
                let _ = tx.send(page);
            });
            let page = rx.recv_timeout(std::time::Duration::from_secs(60)).expect("mesh fixture must finish within the unchanged 60-second bound");
            assert!(page.error.is_none() && page.warnings.is_empty(), "{}:{:?} {:?}", case.id, page.error, page.warnings);
            assert_eq!((page.width, page.height), (fixtures::SIZE * scale, fixtures::SIZE * scale));
            let mut checked = 0;
            let mut mismatched = 0;
            for region in &case.regions {
                let [left, bottom, width, height] = region.rect;
                for y in 0..page.height {
                    for x in 0..page.width {
                        let px = (x as f64 + 0.5) / scale as f64;
                        let py = fixtures::SIZE as f64 - (y as f64 + 0.5) / scale as f64;
                        if px >= left && px < left + width && py >= bottom && py < bottom + height {
                            checked += 1;
                            let pixel = &page.rgba[((y * page.width + x) * 4) as usize..][..4];
                            if pixel[3] != 255 || pixel[..3].iter().zip(region.rgb).any(|(a, b)| a.abs_diff(b) > 8) {
                                mismatched += 1;
                            }
                        }
                    }
                }
            }
            assert!(checked > 0);
            println!(
                "mesh case={} scale={scale} seconds={:.6} region_pixels={checked} mismatched={mismatched}",
                case.id,
                started.elapsed().as_secs_f64()
            );
            if mismatched != 0 {
                failures.push(format!("{} scale{scale}: {mismatched}/{checked} region pixels", case.id));
            }
            rasters.insert((case.id, scale), page.rgba.to_vec());
        }
    }
    for case in &cases {
        if let Some(reference) = case.expected_match {
            for scale in [1_u32, 2] {
                let a = &rasters[&(case.id, scale)];
                let b = &rasters[&(reference, scale)];
                let mismatched = a.iter().zip(b).filter(|(a, b)| a != b).count();
                println!("mesh pair={} reference={reference} scale={scale} mismatched_channels={mismatched}", case.id);
                if mismatched != 0 {
                    failures.push(format!("{} vs{reference} scale{scale}: {mismatched} channels", case.id));
                }
            }
        }
    }
    assert!(failures.is_empty(), "mesh fixture guards failed:\n{}", failures.join("\n"));
}

#[test]
fn repeated_empty_meshes_at_editor_fit_scale_keep_background_clip_and_sentinel() {
    for background in [false, true] {
        let mut case = fixtures::cases().into_iter().find(|case| case.id == "coons-repeated-offpage").unwrap();
        case.background = background;
        case.clip = Some([5.0, 5.0, 30.0, 30.0]);
        let mut document = lopdf::Document::load_mem(&fixtures::pdf(&case)).unwrap();
        let content = document.get_object_mut((4, 0)).unwrap().as_stream_mut().unwrap();
        let mut with_underlay = b"q 0.25 0.5 0.75 rg 0 0 40 40 re f Q\n".to_vec();
        with_underlay.extend_from_slice(&content.content);
        content.set_content(with_underlay);
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).unwrap();
        let started = Instant::now();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let page = PageRenderer::new(Arc::new(bytes), RenderConfig::default()).render(RenderRequest { scale: 20.0, ..Default::default() });
            let _ = tx.send(page);
        });
        let page = rx.recv_timeout(std::time::Duration::from_secs(60)).expect("repeated mesh at editor fit scale must finish within 60 seconds");
        assert!(page.error.is_none() && page.warnings.is_empty(), "{:?} {:?}", page.error, page.warnings);
        assert_eq!((page.width, page.height), (800, 800));
        let mut regions = case.regions;
        regions.last_mut().unwrap().rgb = if background { [0, 255, 0] } else { [64, 128, 191] };
        regions.push(fixtures::Region { rect: [1.0, 8.0, 2.0, 24.0], rgb: [64, 128, 191] });
        let mut checked = 0;
        for region in regions {
            let [left, bottom, width, height] = region.rect;
            for y in 0..page.height {
                for x in 0..page.width {
                    let px = (x as f64 + 0.5) / 20.0;
                    let py = fixtures::SIZE as f64 - (y as f64 + 0.5) / 20.0;
                    if px >= left && px < left + width && py >= bottom && py < bottom + height {
                        let pixel = &page.rgba[((y * page.width + x) * 4) as usize..][..4];
                        assert_eq!(pixel[3], 255);
                        assert!(
                            pixel[..3].iter().zip(region.rgb).all(|(a, b)| a.abs_diff(b) <= 8),
                            "background={background} at ({x},{y}): {pixel:?}"
                        );
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 0);
        println!("mesh editor-fit background={background} pixels={checked} seconds={:.6}", started.elapsed().as_secs_f64());
    }
}

#[path = "support/mesh_contexts.rs"]
mod contexts;

#[test]
fn empty_mesh_contexts_preserve_the_opaque_underlay_and_later_paints() {
    let mut case = fixtures::cases().into_iter().find(|case| case.id == "coons-offpage-background").unwrap();
    case.background = false;
    let source = fixtures::pdf(&case);
    for context in ["root", "multiply", "mask", "group", "type3-fill", "type3-stroke", "pattern"] {
        for scale in [1.0, 2.0] {
            let render = |mesh| {
                let bytes = contexts::pdf(&source, context, mesh);
                let page = PageRenderer::new(Arc::new(bytes), RenderConfig::default()).render(RenderRequest { scale, ..Default::default() });
                assert!(page.error.is_none() && page.warnings.is_empty(), "{context}: {:?} {:?}", page.error, page.warnings);
                page.rgba.to_vec()
            };
            let actual = render(true);
            let expected = render(false);
            let mismatched = actual.iter().zip(&expected).filter(|(a, b)| a != b).count();
            assert_eq!(mismatched, 0, "{context} scale={scale}: transparent mesh changed existing pixels");
            println!("mesh paint-context={context} scale={scale} full_rgba_equal=true");
        }
    }
}
