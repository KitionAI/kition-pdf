//! Cache behavior through the real SDK renderer and editing session.

use super::*;

const WIDTH: u32 = 64;
const HEIGHT: u32 = 48;
const PAGE_BYTES: usize = WIDTH as usize * HEIGHT as usize * 4;

fn document() -> PdfDocument {
    let mut doc = PdfDocument::create("Cache.pdf", WIDTH, HEIGHT).unwrap();
    doc.execute("page.insertBlank", json!({ "at": 2 })).unwrap();
    doc.execute("page.insertBlank", json!({ "at": 3 })).unwrap();
    for (index, color) in ["#d02040", "#2040d0", "#e0b030"].iter().enumerate() {
        doc.execute("file.setBackground", json!({ "pages": [index + 1], "color": color })).unwrap();
    }
    PdfDocument::open("Cache.pdf", &doc.save(None).unwrap()).unwrap()
}

fn budget(doc: &PdfDocument, bytes: usize) {
    doc.with_renderer(|renderer| renderer.full = PageCache::new(bytes)).unwrap();
}

fn page(doc: &PdfDocument, index: usize) -> Vec<u8> {
    let area = doc.areas()[index];
    doc.render(area.x, area.y, 1.0, WIDTH, HEIGHT).unwrap()
}

fn rasterizations(doc: &PdfDocument) -> u64 {
    doc.with_renderer(|renderer| renderer.pages.stats().rasterizations).unwrap()
}

#[test]
fn real_page_hits_eviction_and_repeated_navigation_keep_pixels_and_bound_bytes() {
    let cached = document();
    let uncached = document();
    budget(&cached, PAGE_BYTES * 2);
    budget(&uncached, 0);
    let expected: Vec<_> = (0..3).map(|index| page(&uncached, index)).collect();
    assert_ne!(expected[0], expected[1], "the fixture has visibly distinct pages");
    for index in [0, 1, 0, 2] {
        assert_eq!(page(&cached, index), expected[index]);
    }
    assert_eq!(rasterizations(&cached), 3, "one repeat hit avoided rasterization");
    cached.with_renderer(|renderer| {
        assert!(renderer.full.contains_key(&0) && renderer.full.contains_key(&2));
        assert!(!renderer.full.contains_key(&1), "evict the least recently used page only");
    });
    assert_eq!(page(&cached, 0), expected[0]);
    assert_eq!(rasterizations(&cached), 3, "the most recently used page survives admission");
    assert_eq!(page(&cached, 1), expected[1]);
    assert_eq!(rasterizations(&cached), 4, "an evicted page is reconstructed correctly");
    for _ in 0..100 {
        for index in [0, 2] {
            assert_eq!(page(&cached, index), expected[index]);
            cached.with_renderer(|renderer| {
                assert_eq!(renderer.full.len(), 2);
                assert!(renderer.full.resident_bytes() <= PAGE_BYTES * 2);
            });
        }
    }
    assert_eq!(rasterizations(&cached), 5, "warm navigation keeps both pages resident");
    let area = cached.areas()[0];
    assert_eq!(
        cached.render(area.x, area.y, 0.5, WIDTH / 2, HEIGHT / 2).unwrap(),
        uncached.render(area.x, area.y, 0.5, WIDTH / 2, HEIGHT / 2).unwrap(),
        "reduced views preserve the same pixels with the cache disabled"
    );
}

#[test]
fn real_oversize_pages_render_without_becoming_resident() {
    let cached = document();
    let uncached = document();
    budget(&cached, PAGE_BYTES - 1);
    budget(&uncached, 0);
    let expected = page(&uncached, 0);
    for _ in 0..2 {
        assert_eq!(page(&cached, 0), expected);
        cached.with_renderer(|renderer| assert_eq!((renderer.full.len(), renderer.full.resident_bytes()), (0, 0)));
    }
    assert_eq!(rasterizations(&cached), 2, "oversize requests bypass retention");
}

#[test]
fn edits_undo_and_redo_replace_the_display_cache_and_never_reuse_old_pixels() {
    let mut doc = document();
    let before = page(&doc, 0);
    page(&doc, 1);
    let previous_display = doc.with_renderer(|renderer| renderer.display.clone()).unwrap();
    doc.execute("file.setBackground", json!({ "pages": [1], "color": "#20c040", "replace": true })).unwrap();
    let changed = page(&doc, 0);
    assert_ne!(changed, before, "the edit must change visible page pixels");
    doc.with_renderer(|renderer| {
        assert!(!Arc::ptr_eq(&previous_display, &renderer.display));
        assert_eq!((renderer.full.len(), renderer.full.resident_bytes()), (1, PAGE_BYTES));
        assert!(!renderer.full.contains_key(&1), "all pages from the old display revision are invalidated");
    });
    let uncached = PdfDocument::open("Edited.pdf", &doc.save(None).unwrap()).unwrap();
    budget(&uncached, 0);
    assert_eq!(changed, page(&uncached, 0), "cached edits match a reopened uncached document");
    doc.execute("edit.undo", json!({})).unwrap();
    assert_eq!(page(&doc, 0), before);
    doc.execute("edit.redo", json!({})).unwrap();
    assert_eq!(page(&doc, 0), changed);
}

#[test]
fn real_oversized_page_views_use_colored_tiles_without_retaining_a_full_raster() {
    // Use a real, valid PDF through the ordinary SDK/session path. No renderer replacement.
    let mut fixture = PdfDocument::create("RendererLimit.pdf", 9000, 300).unwrap();
    fixture.execute("page.insertBlank", json!({ "at": 2, "width": 300, "height": 300 })).unwrap();
    fixture.execute("file.setBackground", json!({ "color": "#20c040" })).unwrap();
    let mut doc = PdfDocument::open("RendererLimit.pdf", &fixture.save(None).unwrap()).unwrap();
    doc.execute("comment.add", json!({ "page": 2, "type": "note", "contents": "Retained edit", "at": [80, 80] })).unwrap();
    let before = doc.save(None).unwrap();
    // Observe the immutable display and prepared-save state rather than saving again.
    let display_before = doc.doc().unwrap().display.clone();
    let prepared_before = doc.saved.borrow().as_ref().unwrap().clone();
    let revision = doc.revision();
    let area = doc.areas()[0];
    assert_eq!((area.w, area.h), (9000.0, 300.0));
    let (x, y) = (area.x + 20.0, area.y + 20.0);
    let green = |pixels: &[u8]| {
        assert_eq!(pixels.len(), 16 * 16 * 4);
        assert!(
            pixels.as_chunks::<4>().0.iter().all(|pixel| pixel[0] < 60 && pixel[1] > 150 && pixel[2] < 100 && pixel[3] == 255),
            "painted green content"
        );
    };
    for scale in [1.0, 0.5, 2.0] {
        green(&doc.render(x, y, scale, 16, 16).unwrap());
        doc.with_renderer(|renderer| assert_eq!((renderer.full.len(), renderer.full.resident_bytes()), (0, 0)));
    }
    green(&doc.render_area(x as i32, y as i32, x as i32 + 32, y as i32 + 32, 16, 16).unwrap());
    doc.with_renderer(|renderer| assert_eq!(renderer.full.len(), 0, "bounded tiles are not full-page cache entries"));
    let healthy = doc.areas()[1];
    green(&doc.render(healthy.x + 20.0, healthy.y + 20.0, 1.0, 16, 16).unwrap());
    doc.with_renderer(|renderer| {
        assert!(!renderer.full.contains_key(&0));
        assert!(renderer.full.contains_key(&1));
    });
    assert!(doc.dirty(), "tiled views must not discard pending edits");
    assert_eq!(doc.revision(), revision);
    assert!(Arc::ptr_eq(&doc.doc().unwrap().display, &display_before), "rendering must retain the immutable display");
    assert!(doc.saved.borrow().as_ref().is_some_and(|bytes| Arc::ptr_eq(bytes, &prepared_before)), "rendering must retain the prepared save");
    assert_eq!(doc.saved.borrow().as_deref().map(Vec::as_slice), Some(before.as_slice()), "prepared save bytes remain unchanged");
}
