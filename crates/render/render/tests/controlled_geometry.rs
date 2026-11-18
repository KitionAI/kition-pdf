//! Geometry guards deliberately exclude antialiased edges; coverage is reported separately.

#[path = "support/geometry_fixtures.rs"]
mod fixtures;

use fixtures::{GLYPHS, HEIGHT, RECTS, SEGMENTS, WIDTH};
use kition_pdf_render::{PageRenderer, RenderConfig, RenderRequest, RenderedPage};
use std::sync::Arc;

const EDGE_PX: f64 = 1.5;

fn render(bytes: Vec<u8>, scale: u32) -> RenderedPage {
    let page = PageRenderer::new(Arc::new(bytes), RenderConfig::default()).render(RenderRequest { scale: scale as f32, ..Default::default() });
    assert!(page.error.is_none(), "{:?}", page.error);
    assert!(page.warnings.is_empty(), "{:?}", page.warnings);
    assert_eq!((page.width, page.height), (WIDTH * scale, HEIGHT * scale));
    assert_eq!(page.rgba.len(), (page.width * page.height * 4) as usize);
    for pixel in page.rgba.as_chunks::<4>().0.iter() {
        assert_eq!(pixel[3], 255, "white background must be opaque");
        assert_eq!(pixel[0], pixel[1], "original geometry is monochrome");
        assert_eq!(pixel[1], pixel[2]);
    }
    page
}

fn ink(page: &RenderedPage, x: u32, y: u32) -> f64 {
    1.0 - f64::from(page.rgba[((y * page.width + x) * 4) as usize]) / 255.0
}

fn segment_distance(x: f64, y: f64, points: [f64; 4]) -> f64 {
    let [x0, y0, x1, y1] = points;
    let (dx, dy) = (x1 - x0, y1 - y0);
    let t = (((x - x0) * dx + (y - y0) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    (x - x0 - t * dx).hypot(y - y0 - t * dy)
}

#[test]
fn original_hairlines_and_subpixel_strokes_keep_continuity_and_placement_at_three_dpis() {
    fixtures::export();
    for scale in [1, 2, 4] {
        let page = render(fixtures::strokes(), scale);
        let scale = f64::from(scale);
        for segment in SEGMENTS {
            let [x0, y0, x1, y1] = segment.points;
            let length = (x1 - x0).hypot(y1 - y0);
            let (tx, ty) = ((x1 - x0) / length, (y1 - y0) / length);
            let half = (segment.width * scale / 2.0).max(if segment.width == 0.0 { 0.5 } else { 0.0 });
            let mut bins = vec![0.0_f64; (length * scale).floor() as usize];
            let (mut mass, mut moment) = (0.0, 0.0);
            for y in 0..page.height {
                for x in 0..page.width {
                    let px = (f64::from(x) + 0.5) / scale;
                    let py = f64::from(HEIGHT) - (f64::from(y) + 0.5) / scale;
                    let along = ((px - x0) * tx + (py - y0) * ty) * scale;
                    let across = (-(px - x0) * ty + (py - y0) * tx) * scale;
                    if along >= 2.0 && along < length * scale - 2.0 && across.abs() <= half + EDGE_PX {
                        let value = ink(&page, x, y);
                        bins[along.floor() as usize] += value;
                        mass += value;
                        moment += value * across;
                    }
                }
            }
            assert!(bins[2..bins.len() - 2].iter().all(|coverage| *coverage > 0.01), "{} at {} dpi: lost stroke interval", segment.id, scale * 72.0);
            assert!((moment / mass).abs() <= EDGE_PX, "{}: displaced stroke centroid", segment.id);
            let equivalent_width = mass / (length * scale - 4.0);
            assert!(equivalent_width > 0.01 && equivalent_width <= segment.width * scale + 2.0, "{}: empty or grossly thickened stroke", segment.id);
            println!(
                "geometry stroke={} dpi={} coverage_mass={mass:.6} equivalent_width={equivalent_width:.6} centroid_offset_px={:.6}",
                segment.id,
                scale * 72.0,
                moment / mass
            );
        }
        for y in 0..page.height {
            for x in 0..page.width {
                let px = (f64::from(x) + 0.5) / scale;
                let py = f64::from(HEIGHT) - (f64::from(y) + 0.5) / scale;
                let outside = SEGMENTS.iter().all(|s| {
                    let half = (s.width * scale / 2.0).max(if s.width == 0.0 { 0.5 } else { 0.0 });
                    segment_distance(px, py, s.points) * scale > half + EDGE_PX
                });
                if outside {
                    assert!(ink(&page, x, y) <= 5.0 / 255.0, "unexpected stroke ink at ({x},{y}), scale {scale}");
                }
            }
        }
    }
}

// Positive inside a rectangle, negative outside. Max over the rectangles forms the L.
fn rectangle_distance(x: f64, y: f64, [left, bottom, right, top]: [f64; 4]) -> f64 {
    if x >= left && x <= right && y >= bottom && y <= top {
        (x - left).min(right - x).min(y - bottom).min(top - y)
    } else {
        -((left - x).max(0.0).max(x - right)).hypot((bottom - y).max(0.0).max(y - top))
    }
}

fn glyph_distance(x: f64, y: f64, [a, b, c, d, e, f]: [f64; 6]) -> f64 {
    let determinant = a * d - b * c;
    let local_x = (d * (x - e) - c * (y - f)) / determinant;
    let local_y = (-b * (x - e) + a * (y - f)) / determinant;
    // Conservative inverse-matrix norm converts local distance to page-space distance.
    let inverse_norm = (d.abs() + c.abs()).max(b.abs() + a.abs()) / determinant.abs();
    RECTS.into_iter().map(|rect| rectangle_distance(local_x, local_y, rect)).fold(f64::NEG_INFINITY, f64::max) / inverse_norm
}

#[test]
fn original_type3_glyphs_keep_shape_counters_transforms_and_advance_at_three_dpis() {
    fixtures::export();
    for scale in [1, 2, 4] {
        let page = render(fixtures::glyphs(), scale);
        let scale = f64::from(scale);
        let mut interiors = [0_usize; GLYPHS.len()];
        let (mut edge_mass, mut edge_pixels) = (0.0, 0);
        for y in 0..page.height {
            for x in 0..page.width {
                let px = (f64::from(x) + 0.5) / scale;
                let py = f64::from(HEIGHT) - (f64::from(y) + 0.5) / scale;
                let distances = GLYPHS.map(|(_, matrix)| glyph_distance(px, py, matrix) * scale);
                let value = ink(&page, x, y);
                if let Some(index) = distances.iter().position(|distance| *distance > EDGE_PX) {
                    assert!(value >= 1.0 - 32.0 / 255.0, "{} loses interior at ({x},{y}), scale {scale}", GLYPHS[index].0);
                    interiors[index] += 1;
                } else if distances.iter().all(|distance| *distance < -EDGE_PX) {
                    assert!(value <= 5.0 / 255.0, "unexpected glyph ink or filled open space at ({x},{y}), scale {scale}");
                } else {
                    edge_mass += value;
                    edge_pixels += 1;
                }
            }
        }
        for (index, count) in interiors.into_iter().enumerate() {
            assert!(count > 30, "{} has insufficient tested interior", GLYPHS[index].0);
        }
        println!(
            "geometry type3 dpi={} deep_interior_pixels={interiors:?} edge_pixels={edge_pixels} edge_coverage_mass={edge_mass:.6}",
            scale * 72.0
        );
    }
}
