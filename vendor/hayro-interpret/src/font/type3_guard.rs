//! Active Type3 reentry detection, preserving inherited resource lookup order.

use hayro_syntax::page::Resources;
use hayro_syntax::xref::XRef;
use skrifa::GlyphId;
use std::cell::RefCell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DictionaryIdentity {
    address: usize,
    length: usize,
}

#[derive(Debug, PartialEq, Eq)]
struct ActiveGlyph {
    glyph: GlyphId,
    xref: usize,
    resources: [Vec<DictionaryIdentity>; 7],
}

#[derive(Debug, Default)]
pub(super) struct ActiveGlyphs(RefCell<Vec<ActiveGlyph>>);

impl ActiveGlyphs {
    pub(super) fn enter(
        &self,
        glyph: GlyphId,
        xref: &XRef,
        resources: &Resources<'_>,
    ) -> Option<ActiveGlyphGuard<'_>> {
        let mut chains: [Vec<DictionaryIdentity>; 7] = Default::default();
        let mut current = Some(resources);
        while let Some(resources) = current {
            for (chain, dict) in chains.iter_mut().zip([
                &resources.ext_g_states,
                &resources.fonts,
                &resources.properties,
                &resources.color_spaces,
                &resources.x_objects,
                &resources.patterns,
                &resources.shadings,
            ]) {
                let data = dict.data();
                // Missing category dictionaries have no bytes. Keep explicit empty
                // dictionaries as conservative identities rather than indexing a resource
                // map merely to check emptiness; unused maps can be very large.
                if !data.is_empty() {
                    let identity = DictionaryIdentity {
                        address: data.as_ptr() as usize,
                        length: data.len(),
                    };
                    // Lookup falls back per name (and expected object type). A repeated
                    // identical dictionary cannot succeed after its earlier occurrence failed.
                    // Use underlying slice identity, not hashes or equal dictionary bytes:
                    // different reader contexts and stream payloads must remain distinguishable.
                    // Equal content at distinct addresses can conservatively miss a cycle;
                    // the existing nesting and paint limits remain the final bounds.
                    if !chain.contains(&identity) {
                        chain.push(identity);
                    }
                }
            }
            current = resources.parent();
        }
        self.enter_key(ActiveGlyph {
            glyph,
            xref: std::ptr::from_ref(xref) as usize,
            resources: chains,
        })
    }

    fn enter_key(&self, key: ActiveGlyph) -> Option<ActiveGlyphGuard<'_>> {
        let mut active = self.0.borrow_mut();
        if active.contains(&key) {
            return None;
        }
        active.push(key);
        Some(ActiveGlyphGuard(self))
    }
}

pub(super) struct ActiveGlyphGuard<'a>(&'a ActiveGlyphs);

impl Drop for ActiveGlyphGuard<'_> {
    fn drop(&mut self) {
        // No RefCell borrow spans glyph interpretation. Synchronous nested guards unwind
        // in stack order, including decoder early returns and device panics.
        self.0.0.borrow_mut().pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(glyph: u32, xref: usize) -> ActiveGlyph {
        ActiveGlyph {
            glyph: GlyphId::new(glyph),
            xref,
            resources: Default::default(),
        }
    }

    #[test]
    fn active_guard_preserves_siblings_other_glyphs_documents_and_resource_bindings() {
        let active = ActiveGlyphs::default();
        let outer = active.enter_key(key(1, 1)).unwrap();
        assert!(active.enter_key(key(1, 1)).is_none());
        assert!(active.enter_key(key(2, 1)).is_some());
        assert!(active.enter_key(key(1, 2)).is_some());
        let mut rebound = key(1, 1);
        rebound.resources[1].push(DictionaryIdentity {
            address: 5,
            length: 10,
        });
        assert!(active.enter_key(rebound).is_some());
        drop(outer);
        assert!(active.enter_key(key(1, 1)).is_some());
        assert!(active.0.borrow().is_empty());
    }

    #[test]
    fn active_guard_cleans_up_during_unwind() {
        let active = ActiveGlyphs::default();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _outer = active.enter_key(key(1, 1)).unwrap();
            let _inner = active.enter_key(key(2, 1)).unwrap();
            panic!("original synthetic device failure");
        }));
        assert!(result.is_err());
        assert!(active.0.borrow().is_empty());
        assert!(active.enter_key(key(1, 1)).is_some());
    }
}
