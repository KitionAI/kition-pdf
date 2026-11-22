//! Real PDF regressions for bounded large-page rendering and exact integer reduction.

use super::*;
use kition_kit::raster::{self, Frame};

fn fixture() -> PdfDocument {
    let sizes = [(9000, 300), (300, 9000), (300, 300)];
    let mut objects = vec!["<< /Type /Catalog /Pages 2 0 R >>".to_owned(), "<< /Type /Pages /Kids [3 0 R 5 0 R 7 0 R] /Count 3 >>".to_owned()];
    for (index, (w, h)) in sizes.into_iter().enumerate() {
        let stream = format!(
            "0.125 0.75 0.25 rg 0 0 {w} {h} re f\n0.125 0.25 0.875 rg {} 0 200 {h} re f\n0.875 0.125 0.25 rg 0 0 {w} 100 re f\nq /Half gs 1 1 0 rg 100 100 500 70 re f Q\n0 0 0 RG 1 w 1023.5 0 m 1023.5 {h} l S 0 1023.5 m {w} 1023.5 l S",
            w - 200
        );
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Resources << /ExtGState << /Half << /Type /ExtGState /ca 0.5 >> >> >> /Contents {} 0 R >>", index * 2 + 4));
        objects.push(format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()));
    }
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![0];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let start = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in offsets.iter().skip(1) {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n", offsets.len()).as_bytes());
    PdfDocument::open("Oversized.pdf", &bytes).unwrap()
}

#[test]
fn far_right_bottom_and_composed_seams_keep_real_page_content() {
    let doc = fixture();
    let areas = doc.areas();
    let wide = areas[0];
    let tall = areas[1];
    let blue = doc.render(wide.x + 8900.0, wide.y + 10.0, 1.0, 8, 8).unwrap();
    assert!(blue.as_chunks::<4>().0.iter().all(|p| p[0] < 60 && p[2] > 180 && p[3] == 255));
    let red = doc.render(tall.x + 10.0, tall.y + 8900.0, 1.0, 8, 8).unwrap();
    assert!(red.as_chunks::<4>().0.iter().all(|p| p[0] > 180 && p[1] < 60 && p[3] == 255));
    for (x, y, width, height, horizontal) in [(wide.x, wide.y + 20.0, 7000, 4, true), (tall.x + 20.0, tall.y, 4, 7000, false)] {
        for scale in [1.0, 1.5] {
            let all = doc.render(x, y, scale, width, height).unwrap();
            let mut stitched = vec![0; all.len()];
            let span = if horizontal { width } else { height };
            for offset in (0..span).step_by(511) {
                let count = (span - offset).min(511);
                let (w, h) = if horizontal { (count, height) } else { (width, count) };
                let part = doc
                    .render(
                        x + if horizontal { f64::from(offset) / scale } else { 0.0 },
                        y + if horizontal { 0.0 } else { f64::from(offset) / scale },
                        scale,
                        w,
                        h,
                    )
                    .unwrap();
                for row in 0..h {
                    let to = if horizontal { (row * width + offset) * 4 } else { ((offset + row) * width) * 4 } as usize;
                    stitched[to..to + w as usize * 4].copy_from_slice(&part[row as usize * w as usize * 4..(row + 1) as usize * w as usize * 4]);
                }
            }
            assert_eq!(all, stitched, "seams at scale {scale}, horizontal {horizontal}");
        }
    }
    doc.with_renderer(|renderer| assert_eq!(renderer.full.len(), 0));
}

#[test]
fn reduced_large_spans_keep_far_edges_and_match_pinned_integer_bins() {
    let doc = fixture();
    let areas = doc.areas();
    for (x, y, w, h, ow, oh) in [
        (areas[0].x as i32 - 7, areas[0].y as i32 - 3, 9020, 36, 4510, 18),
        (areas[0].x as i32 - 7, areas[0].y as i32 - 3, 9020, 36, 173, 13),
        (areas[1].x as i32 - 3, areas[1].y as i32 - 7, 35, 9030, 11, 319),
        (areas[0].x as i32 + 20, areas[0].y as i32 + 270, 16, 100, 31, 17),
    ] {
        let full = doc.full(i64::from(x), i64::from(y), w, h).unwrap();
        let expected = raster::render_area(Frame { rgba: &full, width: w, height: h }, 0, 0, w as i32, h as i32, ow, oh);
        assert_eq!(doc.render_area(x, y, x + w as i32, y + h as i32, ow, oh).unwrap(), expected, "source {w}x{h}, target {ow}x{oh}");
    }
    let wide = areas[0];
    let output = doc.render_area(wide.x as i32, wide.y as i32, wide.x as i32 + 9000, wide.y as i32 + 10, 90, 1).unwrap();
    assert!(output[89 * 4 + 2] > 180 && output[89 * 4] < 60, "far-right blue survives full-width reduction");
    let tall = areas[1];
    let output = doc.render_area(tall.x as i32, tall.y as i32, tall.x as i32 + 10, tall.y as i32 + 9000, 1, 90).unwrap();
    assert!(output[89 * 4] > 180 && output[89 * 4 + 1] < 60, "far-bottom red survives full-height reduction");
    // render(scale<1) uses the same ceil span and integer bins as the pinned helper.
    let (w, h, scale) = (173, 3, 0.019);
    let (sw, sh) = ((f64::from(w) / scale).ceil() as u32, (f64::from(h) / scale).ceil() as u32);
    let full = doc.full(wide.x as i64, wide.y as i64, sw, sh).unwrap();
    assert_eq!(doc.render(wide.x, wide.y, scale, w, h).unwrap(), raster::render(Frame { rgba: &full, width: sw, height: sh }, 0.0, 0.0, scale, w, h));
}

#[test]
fn area_reduction_preserves_alpha_pasteboard_and_cross_page_bins() {
    let mut doc = PdfDocument::create("Alpha.pdf", 64, 48).unwrap();
    doc.execute("page_insert_blank", json!({ "at": 2, "width": 64, "height": 48 })).unwrap();
    doc.execute("doc_background", json!({ "pages": [1], "color": "#20c040" })).unwrap();
    doc.execute("doc_background", json!({ "pages": [2], "color": "#2040c0" })).unwrap();
    // The canvas stores premultiplied colors; reduction averages after straight conversion.
    doc.pasteboard = [40, 30, 20, 80];
    let full = doc.full(-10, -10, 170, 210).unwrap();
    let mut pixel = [40, 30, 20, 80];
    straight(&mut pixel);
    assert_eq!(&full[..4], &pixel);
    for (w, h) in [(85, 105), (31, 29), (180, 41), (0, 0)] {
        let expected = raster::render_area(Frame { rgba: &full, width: 170, height: 210 }, 0, 0, 170, 210, w, h);
        assert_eq!(doc.render_area(-10, -10, 160, 200, w, h).unwrap(), expected);
    }
    assert_eq!(doc.render_area(10, 10, -100, -100, 3, 7).unwrap(), pixel.repeat(21), "reversed bounds have one source pixel per axis");
}

#[test]
fn bounded_failures_and_successful_tiles_preserve_cache_revision_and_save() {
    let mut doc = fixture();
    doc.execute("comment_add", json!({ "page": 3, "type": "note", "contents": "Keep edit", "at": [40, 40] })).unwrap();
    let areas = doc.areas();
    let normal = areas[2];
    let old = doc.render(normal.x + 10.0, normal.y + 10.0, 1.0, 16, 16).unwrap();
    let saved = doc.save(None).unwrap();
    // Observe the immutable display and prepared-save state rather than saving again.
    let display_before = doc.doc().unwrap().display.clone();
    let prepared_before = doc.saved.borrow().as_ref().unwrap().clone();
    let revision = doc.revision();
    let count = doc.with_renderer(|renderer| renderer.pages.stats().rasterizations).unwrap();
    for (x0, y0, x1, y1, w, h) in
        [(0, 0, 9000, 9000, 16, 16), (i32::MIN, i32::MIN, i32::MAX, i32::MAX, 16, 16), (0, 0, 1, 1, 8192, 8192), (-64_000_000, -1, 0, 0, 8192, 1)]
    {
        let error = doc.render_area(x0, y0, x1, y1, w, h).unwrap_err();
        assert!(error.contains("rendering work budget") && error.contains("smaller area") && error.contains("Retry"), "{error}");
    }
    assert_eq!(doc.with_renderer(|renderer| renderer.pages.stats().rasterizations).unwrap(), count, "excessive samples rejected before any render");
    assert_eq!(doc.render_area(i32::MAX - 2, i32::MAX - 2, i32::MAX, i32::MAX, 2, 2).unwrap(), PASTEBOARD.repeat(4));
    doc.render(areas[0].x + 8800.0, areas[0].y, 1.0, 16, 16).unwrap();
    doc.with_renderer(|renderer| assert!(renderer.full.contains_key(&2) && !renderer.full.contains_key(&0)));
    assert_eq!(doc.render(normal.x + 10.0, normal.y + 10.0, 1.0, 16, 16).unwrap(), old);
    assert!(doc.dirty());
    assert_eq!(doc.revision(), revision);
    assert!(Arc::ptr_eq(&doc.doc().unwrap().display, &display_before), "rendering must retain the immutable display");
    assert!(doc.saved.borrow().as_ref().is_some_and(|bytes| Arc::ptr_eq(bytes, &prepared_before)), "rendering must retain the prepared save");
    assert_eq!(doc.saved.borrow().as_deref().map(Vec::as_slice), Some(saved.as_slice()), "prepared save bytes remain unchanged");
    doc.execute("page_rotate", json!({ "pages": [1], "degrees": 180 })).unwrap();
    let edited = doc.render(areas[0].x + 20.0, areas[0].y + 20.0, 1.0, 16, 16).unwrap();
    let reopened = PdfDocument::open("Edited.pdf", &doc.save(None).unwrap()).unwrap();
    assert_eq!(edited, reopened.render(areas[0].x + 20.0, areas[0].y + 20.0, 1.0, 16, 16).unwrap());
    doc.execute("edit.undo", json!({})).unwrap();
    assert_ne!(edited, doc.render(areas[0].x + 20.0, areas[0].y + 20.0, 1.0, 16, 16).unwrap());
    doc.execute("edit.redo", json!({})).unwrap();
    assert_eq!(edited, doc.render(areas[0].x + 20.0, areas[0].y + 20.0, 1.0, 16, 16).unwrap());
}
