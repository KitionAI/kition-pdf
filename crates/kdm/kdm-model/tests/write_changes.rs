//! Minimal writes: an edit writes the overlay entries it changed and nothing else.

mod common;

use std::collections::BTreeSet;

use common::{add_note, edit, fill, first, fixture, replace, rewritten};
use kdm_model::root_id;

/// The object numbers `after` edits that `before` does not (or edits differently).
fn changed(before: &kition_pdf_cos::Document, after: &kition_pdf_cos::Document) -> BTreeSet<u32> {
    let was: std::collections::HashMap<u32, _> = before.overlay_entries().map(|(n, g, o)| (n, (g, o))).collect();
    after.overlay_entries().filter(|(n, g, o)| was.get(n) != Some(&(*g, o.clone()))).map(|(n, _, _)| n).collect()
}

#[test]
fn adding_a_comment_writes_only_its_new_overlay_entries() {
    let mut a = first(&fixture(3));
    let before = a.document().clone();
    let entries_before = a.replica().map_entries(&root_id(), "overlay").unwrap_or_default().len();
    assert_eq!(edit(&mut a, |doc| add_note(doc, 1, "hello")), 1, "one undo step");
    let after = a.document().clone();
    let expected: BTreeSet<String> = changed(&before, &after).into_iter().map(|n| a.model().ids().known_key(n).unwrap_or_default()).collect();
    let written: BTreeSet<String> = a.last_written().keys.iter().cloned().collect();
    assert!(!a.last_written().rebased);
    assert_eq!(written, expected, "exactly the objects the comment changed");
    // The note, its appearance (or popup) and the page whose /Annots it joined: a handful, not the
    // document.
    assert!((2..=8).contains(&written.len()), "{written:?}");
    let base_max = a.model().ids().base_max();
    let page = kition_pdf_annot::page_refs(&after).unwrap_or_else(|e| panic!("pages: {e}"))[1];
    assert!(written.contains(&page.num.to_string()), "the page is written under its own number");
    assert!(written.iter().filter(|k| kdm_model::base_number(k).is_none()).count() >= 1, "the added note has a key of its own");
    assert!(written.iter().all(|k| kdm_model::base_number(k).is_none_or(|n| n <= base_max)));
    let entries_after = a.replica().map_entries(&root_id(), "overlay").unwrap_or_default().len();
    assert_eq!(entries_after, entries_before + written.len());
}

#[test]
fn a_second_edit_writes_only_what_it_changed() {
    let mut a = first(&fixture(2));
    edit(&mut a, |doc| add_note(doc, 0, "one"));
    let before = a.document().clone();
    edit(&mut a, |doc| fill(doc, "Grace"));
    let expected: BTreeSet<String> = changed(&before, a.document()).into_iter().map(|n| a.model().ids().known_key(n).unwrap_or_default()).collect();
    let written: BTreeSet<String> = a.last_written().keys.iter().cloned().collect();
    assert_eq!(written, expected, "the field and its appearance, not the note again");
}

#[test]
fn an_edit_that_changes_nothing_writes_nothing() {
    let mut a = first(&fixture(1));
    let version = a.version();
    assert_eq!(edit(&mut a, |_| {}), 0);
    assert_eq!(a.version(), version);
    assert!(a.last_written().keys.is_empty());
}

#[test]
fn a_full_rewrite_is_a_rebase_with_a_new_base_and_an_empty_overlay() {
    let mut a = first(&fixture(2));
    edit(&mut a, |doc| add_note(doc, 0, "folded in"));
    assert_eq!(replace(&mut a, rewritten), 1, "a rebase is one undo step");
    assert!(a.last_written().rebased);
    assert!(a.replica().map_entries(&root_id(), "overlay").unwrap_or_default().is_empty(), "the overlay is cleared");
    assert!(!a.document().is_modified());
    assert_eq!(common::comments(a.document()), [(0, "folded in".to_string())], "the edit is in the new base");
    // Undoing the rebase brings back the old base and its overlay.
    assert!(a.undo().unwrap_or_else(|e| panic!("undo: {e}")).0);
    assert!(a.document().is_modified());
    assert_eq!(common::comments(a.document()), [(0, "folded in".to_string())]);
}
