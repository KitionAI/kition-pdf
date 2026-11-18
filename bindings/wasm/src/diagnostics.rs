//! Opt-in, bounded counters for a single WASM instance's execution thread.
//!
//! Totals aggregate all PDF documents/page caches on that thread and survive renderer
//! replacement. Browser measurements must require one live cache before interpreting
//! them as one document. No filenames, page keys, pixel handles, or event log are retained.

use std::cell::RefCell;

#[cfg(any(test, target_arch = "wasm32"))]
use serde_json::{Value, json};

const COUNTER_LIMIT: u64 = (1 << 53) - 1;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub hits: u64,
    pub misses: u64,
    pub admissions: u64,
    pub evictions: u64,
    pub replacements: u64,
    pub retention_bypasses: u64,
    pub live_caches: u64,
    pub resident_bytes: u64,
    pub resident_entries: u64,
    pub capacity_bytes: u64,
    pub renderer_creations: u64,
    pub display_invalidations: u64,
    pub last_renderer_generation: u64,
    pub last_display_revision: u64,
    pub full_render_requests: u64,
    pub uncached_tile_requests: u64,
    pub large_page_bypasses: u64,
    pub saturated: bool,
}

thread_local! {
    static STATE: RefCell<Snapshot> = const { RefCell::new(Snapshot {
        hits: 0, misses: 0, admissions: 0, evictions: 0, replacements: 0,
        retention_bypasses: 0, live_caches: 0, resident_bytes: 0, resident_entries: 0,
        capacity_bytes: 0, renderer_creations: 0, display_invalidations: 0,
        last_renderer_generation: 0, last_display_revision: 0, full_render_requests: 0,
        uncached_tile_requests: 0, large_page_bypasses: 0, saturated: false,
    }) };
}

fn update(f: impl FnOnce(&mut Snapshot)) {
    // No callbacks/engine work run while the short counter borrow is held. During
    // thread-local teardown diagnostics must never turn a cache drop into a panic.
    let _ = STATE.try_with(|state| {
        if let Ok(mut state) = state.try_borrow_mut() {
            f(&mut state);
        }
    });
}

fn add(value: &mut u64, amount: u64, saturated: &mut bool) {
    let next = value.saturating_add(amount);
    *saturated |= next > COUNTER_LIMIT;
    *value = next.min(COUNTER_LIMIT);
}

#[cfg(any(test, target_arch = "wasm32"))]
pub(super) fn snapshot() -> Result<Snapshot, String> {
    STATE
        .try_with(|state| state.try_borrow().map(|state| *state).map_err(|_| "PDF cache diagnostics are busy".to_owned()))
        .map_err(|_| "PDF cache diagnostics are unavailable on this thread".to_owned())?
}

#[cfg(any(test, target_arch = "wasm32"))]
pub(super) fn value() -> Result<Value, String> {
    let state = snapshot()?;
    Ok(json!({
        "schema": 1,
        "scope": "wasm-instance-thread-aggregate",
        "counterLimit": COUNTER_LIMIT,
        "saturated": state.saturated,
        "hits": state.hits,
        "misses": state.misses,
        "admissions": state.admissions,
        "evictions": state.evictions,
        "replacements": state.replacements,
        "retentionBypasses": state.retention_bypasses,
        "liveCaches": state.live_caches,
        "residentBytes": state.resident_bytes,
        "residentEntries": state.resident_entries,
        "capacityBytes": state.capacity_bytes,
        "rendererCreations": state.renderer_creations,
        "displayInvalidations": state.display_invalidations,
        "lastRendererGeneration": state.last_renderer_generation,
        "lastDisplayRevision": state.last_display_revision,
        "fullRenderRequests": state.full_render_requests,
        "uncachedTileRequests": state.uncached_tile_requests,
        "largePageBypasses": state.large_page_bypasses,
    }))
}

pub(super) fn cache_created(capacity: usize) {
    update(|state| {
        add(&mut state.live_caches, 1, &mut state.saturated);
        add(&mut state.capacity_bytes, capacity as u64, &mut state.saturated);
    });
}

pub(super) fn cache_dropped(capacity: usize, bytes: usize, entries: usize) {
    update(|state| {
        state.live_caches = state.live_caches.saturating_sub(1);
        state.capacity_bytes = state.capacity_bytes.saturating_sub(capacity as u64);
        state.resident_bytes = state.resident_bytes.saturating_sub(bytes as u64);
        state.resident_entries = state.resident_entries.saturating_sub(entries as u64);
    });
}

pub(super) fn lookup(hit: bool) {
    update(|state| add(if hit { &mut state.hits } else { &mut state.misses }, 1, &mut state.saturated));
}

/// Every successful insert is an admission, including replacement. Evictions count
/// only LRU removals, not replacements, invalidation, or dropping a document.
pub(super) fn admitted(previous_bytes: usize, previous_entries: usize, bytes: usize, entries: usize) {
    update(|state| {
        add(&mut state.admissions, 1, &mut state.saturated);
        state.resident_bytes = state.resident_bytes.saturating_sub(previous_bytes as u64);
        state.resident_entries = state.resident_entries.saturating_sub(previous_entries as u64);
        add(&mut state.resident_bytes, bytes as u64, &mut state.saturated);
        add(&mut state.resident_entries, entries as u64, &mut state.saturated);
    });
}

pub(super) fn eviction() {
    update(|state| add(&mut state.evictions, 1, &mut state.saturated));
}

pub(super) fn replacement() {
    update(|state| add(&mut state.replacements, 1, &mut state.saturated));
}

pub(super) fn retention_bypass() {
    update(|state| add(&mut state.retention_bypasses, 1, &mut state.saturated));
}

pub(super) fn renderer_created(revision: u64, invalidated: bool) {
    update(|state| {
        add(&mut state.renderer_creations, 1, &mut state.saturated);
        state.last_renderer_generation = state.renderer_creations;
        state.last_display_revision = revision.min(COUNTER_LIMIT);
        state.saturated |= revision > COUNTER_LIMIT;
        if invalidated {
            add(&mut state.display_invalidations, 1, &mut state.saturated);
        }
    });
}

pub(super) fn full_render_request() {
    update(|state| add(&mut state.full_render_requests, 1, &mut state.saturated));
}

pub(super) fn tile_request() {
    update(|state| add(&mut state.uncached_tile_requests, 1, &mut state.saturated));
}

/// One full-view page intersection skipped the full cache due to page dimensions.
/// Reduction may visit one page in multiple source chunks; this is not a view count.
pub(super) fn large_page_bypass() {
    update(|state| add(&mut state.large_page_bypasses, 1, &mut state.saturated));
}

#[cfg(target_arch = "wasm32")]
#[kition_kit::wasm_bindgen::prelude::wasm_bindgen(js_name = pdfCacheDiagnostics, wasm_bindgen = kition_kit::wasm_bindgen)]
pub fn pdf_cache_diagnostics() -> Result<String, kition_kit::wasm_bindgen::JsValue> {
    value().map(|value| value.to_string()).map_err(|error| kition_kit::wasm_bindgen::JsValue::from_str(&error))
}

#[cfg(test)]
pub(super) fn reset() -> Result<(), String> {
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        if state.live_caches != 0 {
            return Err("Cannot reset diagnostics while a cache is live".into());
        }
        *state = Snapshot::default();
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_saturate_before_json_numbers_lose_precision() {
        reset().unwrap();
        update(|state| state.hits = COUNTER_LIMIT - 1);
        lookup(true);
        assert!(!snapshot().unwrap().saturated);
        lookup(true);
        let state = snapshot().unwrap();
        assert_eq!(state.hits, COUNTER_LIMIT);
        assert!(state.saturated);
        assert_eq!(value().unwrap()["hits"].as_u64(), Some(COUNTER_LIMIT));
        reset().unwrap();
    }
}
