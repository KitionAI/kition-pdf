//! Bounded storage of zoomed page tiles (scales above 1:1, which a device pixel ratio of 2 makes
//! the usual case), so scrolling back to what was seen, or repainting after an edit of another
//! page, blits pixels instead of interpreting the page again.
//!
//! A tile is a fixed square of one page's pixels at one scale, keyed by the page's stamp
//! (`kition_pdf_engine::stamps`), not its index: an edit that leaves a page's stamp alone leaves
//! its tiles usable, a page moved elsewhere keeps them, and a changed page is never served old
//! pixels. Least recently used tiles go first.

use std::collections::VecDeque;
use std::sync::Arc;

use super::PageImage;

/// Retained tile pixels.
const TILE_BUDGET: usize = 128 * 1024 * 1024;
/// Bound metadata and the linear lookup even for tiny tiles.
const MAX_TILES: usize = 512;

/// One tile: the page's stamp, the renderer scale's bits, and the tile's column and row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TileKey {
    pub stamp: u64,
    pub scale: u32,
    pub column: u32,
    pub row: u32,
}

pub(super) struct TileCache {
    budget: usize,
    bytes: usize,
    /// The fonts added when these tiles were drawn ([`super::font_generation`]): a face added
    /// later can change how a page draws, so the tiles go.
    fonts: usize,
    /// Least recently used first.
    entries: VecDeque<(TileKey, Arc<PageImage>)>,
}

impl Default for TileCache {
    fn default() -> Self {
        Self::new(TILE_BUDGET)
    }
}

impl TileCache {
    pub fn new(budget: usize) -> Self {
        Self { budget, bytes: 0, fonts: super::font_generation(), entries: VecDeque::new() }
    }

    /// Drops everything when a font was added since the tiles were drawn.
    pub fn follow_fonts(&mut self) {
        let fonts = super::font_generation();
        if fonts != self.fonts {
            self.entries.clear();
            self.bytes = 0;
            self.fonts = fonts;
        }
    }

    pub fn get(&mut self, key: TileKey) -> Option<Arc<PageImage>> {
        let position = self.entries.iter().position(|(entry, _)| *entry == key)?;
        let entry = self.entries.remove(position)?;
        let image = entry.1.clone();
        self.entries.push_back(entry);
        Some(image)
    }

    /// Whether `key` is kept (it does not count as a use).
    pub fn contains(&self, key: TileKey) -> bool {
        self.entries.iter().any(|(entry, _)| *entry == key)
    }

    pub fn insert(&mut self, key: TileKey, image: Arc<PageImage>) {
        let bytes = image.allocated_bytes();
        if bytes == 0 || bytes > self.budget {
            return;
        }
        if let Some(position) = self.entries.iter().position(|(entry, _)| *entry == key)
            && let Some((_, old)) = self.entries.remove(position)
        {
            self.bytes -= old.allocated_bytes();
        }
        while self.bytes > self.budget - bytes || self.entries.len() >= MAX_TILES {
            let Some((_, oldest)) = self.entries.pop_front() else { break };
            self.bytes -= oldest.allocated_bytes();
        }
        self.bytes += bytes;
        self.entries.push_back((key, image));
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub fn resident_bytes(&self) -> usize {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(words: usize) -> Arc<PageImage> {
        Arc::new(PageImage::new(vec![0xff00_00ff; words].into(), words as u32, 1))
    }

    fn key(stamp: u64, column: u32) -> TileKey {
        TileKey { stamp, scale: 2.0f32.to_bits(), column, row: 0 }
    }

    #[test]
    fn least_recently_used_tiles_go_first_within_the_budget() {
        let mut cache = TileCache::new(48);
        for column in 0..3 {
            cache.insert(key(1, column), image(4));
        }
        assert!(cache.get(key(1, 0)).is_some());
        cache.insert(key(2, 0), image(4));
        assert!(cache.get(key(1, 1)).is_none(), "the least recently used tile went");
        assert!(cache.get(key(1, 0)).is_some() && cache.get(key(1, 2)).is_some() && cache.get(key(2, 0)).is_some());
        assert_eq!((cache.len(), cache.resident_bytes()), (3, 48));
        assert!(cache.get(TileKey { scale: 3.0f32.to_bits(), ..key(1, 0) }).is_none(), "another scale is another tile");
        cache.insert(key(2, 0), image(2));
        assert_eq!((cache.len(), cache.resident_bytes()), (3, 40), "a replaced tile counts once");
        cache.insert(key(3, 0), image(13));
        assert!(cache.get(key(3, 0)).is_none(), "a tile larger than the budget is not kept");
        assert_eq!((cache.len(), cache.resident_bytes()), (3, 40), "and pushes nothing out");
    }

    #[test]
    fn metadata_stays_bounded_for_tiny_tiles() {
        let mut cache = TileCache::default();
        for column in 0..(MAX_TILES as u32 + 10) {
            cache.insert(key(7, column), image(1));
        }
        assert_eq!(cache.len(), MAX_TILES);
        assert!(cache.get(key(7, 9)).is_none() && cache.get(key(7, 10)).is_some());
    }
}
