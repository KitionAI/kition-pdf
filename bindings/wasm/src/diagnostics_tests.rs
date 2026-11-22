//! Diagnostic totals measure real operations without changing pixels, state, or LRU order.

use super::*;
use diagnostics::{reset, snapshot};

fn image(words: usize) -> Arc<PageImage> {
    Arc::new(PageImage::new(vec![0xff80_4020; words].into(), words as u32, 1))
}

#[test]
fn counts_hits_misses_lru_evictions_replacement_and_retention_bypass() {
    reset().unwrap();
    let mut cache = PageCache::new(48);
    assert!(reset().is_err(), "a live cache must not lose its accounting");
    assert!(cache.get(0).is_none());
    for index in 0..3 {
        cache.insert(index, image(4));
    }
    assert!(cache.get(0).is_some());
    let before = snapshot().unwrap();
    assert_eq!(diagnostics::value().unwrap()["scope"], "wasm-instance-thread-aggregate");
    assert_eq!(snapshot().unwrap(), before, "observation must not increment counters");
    cache.insert(3, image(4));
    assert!(!cache.contains_key(&1), "snapshot must not disturb which page is least recent");
    assert!(cache.get(1).is_none());
    assert!(cache.get(0).is_some());
    cache.insert(2, image(8));
    cache.insert(4, image(16));
    let state = snapshot().unwrap();
    assert_eq!((state.hits, state.misses, state.admissions), (2, 2, 5));
    assert_eq!((state.evictions, state.replacements, state.retention_bypasses), (2, 1, 1));
    assert_eq!((state.live_caches, state.resident_bytes, state.resident_entries, state.capacity_bytes), (1, 48, 2, 48));
    assert!(cache.contains_key(&0) && cache.contains_key(&2));
    drop(cache);
    let dropped = snapshot().unwrap();
    assert_eq!((dropped.live_caches, dropped.resident_bytes, dropped.resident_entries, dropped.capacity_bytes), (0, 0, 0, 0));
    assert_eq!((dropped.hits, dropped.misses, dropped.evictions), (state.hits, state.misses, state.evictions));
}

#[test]
fn disabled_and_spare_capacity_entries_bypass_without_losing_live_gauges() {
    reset().unwrap();
    let mut cache = PageCache::new(16);
    cache.insert(0, image(4));
    let mut spare = Vec::with_capacity(8);
    spare.resize(1, 0xff80_4020);
    cache.insert(1, Arc::new(PageImage::new(spare.into(), 1, 1)));
    let mut disabled = PageCache::new(0);
    disabled.insert(0, image(1));
    let state = snapshot().unwrap();
    assert_eq!((state.live_caches, state.resident_bytes, state.resident_entries, state.capacity_bytes), (2, 16, 1, 16));
    assert_eq!((state.admissions, state.retention_bypasses, state.evictions), (1, 2, 0));
    drop(disabled);
    drop(cache);
    assert_eq!(snapshot().unwrap().live_caches, 0);
}

#[test]
fn observing_real_document_does_not_make_renderer_and_invalidations_keep_lifetime_totals() {
    reset().unwrap();
    let mut doc = PdfDocument::create("Diagnostics.pdf", 32, 24).unwrap();
    let unopened = snapshot().unwrap();
    assert_eq!(unopened, diagnostics::Snapshot::default());
    assert!(doc.renderer.borrow().is_none());
    let area = doc.areas()[0];
    let first = doc.render(area.x, area.y, 1.0, 8, 8).unwrap();
    let cold = snapshot().unwrap();
    assert_eq!((cold.hits, cold.misses, cold.admissions, cold.full_render_requests), (0, 1, 1, 1));
    assert_eq!((cold.renderer_creations, cold.last_renderer_generation, cold.display_invalidations), (1, 1, 0));
    assert_eq!(doc.render(area.x, area.y, 1.0, 8, 8).unwrap(), first);
    assert_eq!((snapshot().unwrap().hits, snapshot().unwrap().full_render_requests), (1, 1));

    doc.execute("doc_background", json!({ "color": "#20c040" })).unwrap();
    let before_render = snapshot().unwrap();
    assert_eq!(before_render.display_invalidations, 0, "snapshot observes existing state; only rendering replaces the old display cache");
    let saved = doc.save(None).unwrap();
    // Observe the immutable display and prepared-save state rather than saving again.
    let display_before = doc.doc().unwrap().display.clone();
    let prepared_before = doc.saved.borrow().as_ref().unwrap().clone();
    let revision = doc.revision();
    assert_eq!(snapshot().unwrap(), before_render);
    let edited = doc.render(area.x, area.y, 1.0, 8, 8).unwrap();
    assert_ne!(edited, first);
    let after = snapshot().unwrap();
    assert_eq!((after.hits, after.misses, after.admissions, after.full_render_requests), (1, 2, 2, 2));
    assert_eq!(
        (after.renderer_creations, after.last_renderer_generation, after.display_invalidations, after.last_display_revision),
        (2, 2, 1, revision)
    );
    assert_eq!((after.live_caches, after.resident_entries, after.resident_bytes, after.capacity_bytes), (1, 1, 32 * 24 * 4, 64 * 1024 * 1024));
    assert!(Arc::ptr_eq(&doc.doc().unwrap().display, &display_before), "rendering must retain the immutable display");
    assert!(doc.saved.borrow().as_ref().is_some_and(|bytes| Arc::ptr_eq(bytes, &prepared_before)), "rendering must retain the prepared save");
    assert_eq!(doc.saved.borrow().as_deref().map(Vec::as_slice), Some(saved.as_slice()), "prepared save bytes remain unchanged");
    assert_eq!(doc.revision(), revision);
    assert!(doc.dirty());

    doc.execute("edit.undo", json!({})).unwrap();
    assert_eq!(doc.render(area.x, area.y, 1.0, 8, 8).unwrap(), first);
    doc.execute("edit.redo", json!({})).unwrap();
    assert_eq!(doc.render(area.x, area.y, 1.0, 8, 8).unwrap(), edited);
    let final_state = snapshot().unwrap();
    assert_eq!((final_state.display_invalidations, final_state.renderer_creations, final_state.misses, final_state.admissions), (3, 4, 4, 4));
    assert_eq!(final_state.evictions, 0, "invalidating a display is not an LRU eviction");
    drop(doc);
    let closed = snapshot().unwrap();
    assert_eq!((closed.live_caches, closed.resident_entries, closed.resident_bytes, closed.capacity_bytes), (0, 0, 0, 0));
    assert_eq!(closed.renderer_creations, 4);
}

#[test]
fn real_large_and_vector_tiles_never_masquerade_as_full_cache_hits() {
    reset().unwrap();
    let doc = PdfDocument::create("LargeDiagnostics.pdf", 9000, 24).unwrap();
    let area = doc.areas()[0];
    assert_eq!(doc.render(area.x + 8800.0, area.y, 1.0, 8, 8).unwrap(), [255; 8 * 8 * 4]);
    let first = snapshot().unwrap();
    assert_eq!((first.large_page_bypasses, first.uncached_tile_requests, first.full_render_requests), (1, 1, 0));
    assert_eq!((first.hits, first.misses, first.admissions, first.retention_bypasses), (0, 0, 0, 0));
    assert_eq!((first.resident_bytes, first.resident_entries, first.live_caches), (0, 0, 1));
    assert_eq!(doc.render(area.x + 8800.0, area.y, 2.0, 8, 8).unwrap(), [255; 8 * 8 * 4]);
    let second = snapshot().unwrap();
    assert_eq!((second.large_page_bypasses, second.uncached_tile_requests, second.hits, second.misses), (1, 2, 0, 0));
}
