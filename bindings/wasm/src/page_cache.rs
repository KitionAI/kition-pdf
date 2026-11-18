//! Bounded 1:1 raster storage for one immutable display revision.

use std::collections::VecDeque;
use std::sync::Arc;

use kition_pdf_render::Pixels;

/// Retained pixel allocations only; renderer working memory and output frames are separate.
const PIXEL_BUDGET: usize = 64 * 1024 * 1024;
/// Bound metadata and linear recency lookup even for documents with tiny pages.
const MAX_ENTRIES: usize = 256;

pub(super) struct PageImage {
    pub pixels: Pixels,
    pub width: u32,
    pub height: u32,
    allocated_bytes: usize,
}

impl PageImage {
    pub fn new(pixels: Pixels, width: u32, height: u32) -> Self {
        // Taking and returning the words moves the allocation without a full-page copy.
        let words = pixels.into_words();
        let allocated_bytes = words.capacity().saturating_mul(std::mem::size_of::<u32>());
        Self { pixels: words.into(), width, height, allocated_bytes }
    }
}

pub(super) struct PageCache {
    budget: usize,
    bytes: usize,
    /// Least recently used first. Lookup is bounded by MAX_ENTRIES.
    entries: VecDeque<(usize, Arc<PageImage>)>,
}

impl Default for PageCache {
    fn default() -> Self {
        Self::new(PIXEL_BUDGET)
    }
}

impl PageCache {
    pub fn new(budget: usize) -> Self {
        #[cfg(feature = "cache-diagnostics")]
        super::diagnostics::cache_created(budget);
        Self { budget, bytes: 0, entries: VecDeque::new() }
    }

    pub fn get(&mut self, index: usize) -> Option<Arc<PageImage>> {
        let position = self.entries.iter().position(|(page, _)| *page == index);
        #[cfg(feature = "cache-diagnostics")]
        super::diagnostics::lookup(position.is_some());
        let entry = self.entries.remove(position?)?;
        let image = entry.1.clone();
        self.entries.push_back(entry);
        Some(image)
    }

    pub fn insert(&mut self, index: usize, image: Arc<PageImage>) {
        let bytes = image.allocated_bytes;
        // A large page still renders for this request, without displacing useful residents.
        if bytes == 0 || bytes > self.budget {
            #[cfg(feature = "cache-diagnostics")]
            super::diagnostics::retention_bypass();
            return;
        }
        #[cfg(feature = "cache-diagnostics")]
        let previous = (self.bytes, self.entries.len());
        if let Some(position) = self.entries.iter().position(|(page, _)| *page == index)
            && let Some((_, old)) = self.entries.remove(position)
        {
            self.bytes -= old.allocated_bytes;
            #[cfg(feature = "cache-diagnostics")]
            super::diagnostics::replacement();
        }
        while self.bytes > self.budget - bytes || self.entries.len() >= MAX_ENTRIES {
            let Some((_, oldest)) = self.entries.pop_front() else { break };
            self.bytes -= oldest.allocated_bytes;
            #[cfg(feature = "cache-diagnostics")]
            super::diagnostics::eviction();
        }
        self.bytes += bytes;
        self.entries.push_back((index, image));
        #[cfg(feature = "cache-diagnostics")]
        super::diagnostics::admitted(previous.0, previous.1, self.bytes, self.entries.len());
    }

    #[cfg(test)]
    pub fn contains_key(&self, index: &usize) -> bool {
        self.entries.iter().any(|(page, _)| page == index)
    }

    #[cfg(test)]
    pub fn resident_bytes(&self) -> usize {
        self.bytes
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(feature = "cache-diagnostics")]
impl Drop for PageCache {
    fn drop(&mut self) {
        super::diagnostics::cache_dropped(self.budget, self.bytes, self.entries.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(words: usize) -> Arc<PageImage> {
        Arc::new(PageImage::new(vec![0xff80_4020; words].into(), words as u32, 1))
    }

    #[test]
    fn touches_evict_the_oldest_pages_without_clearing_the_cache() {
        let mut cache = PageCache::new(48);
        for index in 0..3 {
            cache.insert(index, image(4));
        }
        let first = cache.get(0).unwrap();
        cache.insert(3, image(4));
        assert!(!cache.contains_key(&1));
        assert!(cache.contains_key(&2) && cache.contains_key(&3));
        assert!(Arc::ptr_eq(&first, &cache.get(0).unwrap()));
        assert_eq!((cache.len(), cache.resident_bytes()), (3, 48));
        for index in 4..104 {
            cache.insert(index, image(4));
            assert_eq!((cache.len(), cache.resident_bytes()), (3, 48));
        }
    }

    #[test]
    fn variable_sizes_evict_only_enough_space_and_replacements_count_once() {
        let mut cache = PageCache::new(80);
        cache.insert(0, image(8));
        cache.insert(1, image(8));
        cache.insert(2, image(4));
        cache.insert(3, image(16));
        assert!(!cache.contains_key(&0) && !cache.contains_key(&1));
        assert!(cache.contains_key(&2) && cache.contains_key(&3));
        assert_eq!(cache.resident_bytes(), 80);
        cache.insert(3, image(8));
        assert_eq!((cache.len(), cache.resident_bytes()), (2, 48));
    }

    #[test]
    fn oversize_and_spare_capacity_bypass_without_evicting_residents() {
        let mut cache = PageCache::new(16);
        let resident = image(4);
        cache.insert(0, resident.clone());
        cache.insert(1, image(5));
        let mut spare = Vec::with_capacity(8);
        spare.resize(1, 0xff80_4020);
        let spare = Arc::new(PageImage::new(spare.into(), 1, 1));
        cache.insert(2, spare);
        assert_eq!((cache.len(), cache.resident_bytes()), (1, 16));
        assert!(Arc::ptr_eq(&resident, &cache.get(0).unwrap()));
        let mut disabled = PageCache::new(0);
        disabled.insert(0, resident);
        assert_eq!((disabled.len(), disabled.resident_bytes()), (0, 0));
    }

    #[test]
    fn tiny_pages_also_have_bounded_metadata() {
        let mut cache = PageCache::default();
        for index in 0..MAX_ENTRIES + 20 {
            cache.insert(index, image(1));
        }
        assert_eq!((cache.len(), cache.resident_bytes()), (MAX_ENTRIES, MAX_ENTRIES * 4));
        assert!(!cache.contains_key(&19));
        assert!(cache.contains_key(&20));
    }
}
