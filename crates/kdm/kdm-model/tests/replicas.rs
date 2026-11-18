//! Two replicas: concurrent edits of different objects both survive, comments added to one page,
//! pages inserted and form fields added at once are all kept (and deletions beside them hold),
//! added objects never collide, and a rebase against a concurrent edit drops the edit, counted,
//! on both sides alike.

mod common;

use common::{add_note, assert_converged, comments, edit, field_value, fill, first, fixture, peer, replace, rewritten, sync};
use kdm_model::{PdfSession, base_number};

fn pair(pages: usize) -> (PdfSession, PdfSession) {
    let a = first(&fixture(pages));
    let b = peer(&a);
    (a, b)
}

#[test]
fn comments_on_different_pages_are_both_kept() {
    let (mut a, mut b) = pair(3);
    edit(&mut a, |doc| add_note(doc, 0, "from a"));
    edit(&mut b, |doc| add_note(doc, 2, "from b"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(comments(a.document()), [(0, "from a".to_string()), (2, "from b".to_string())]);
}

/// The comments of the view, sorted (the order two concurrent additions take is the merge's).
fn sorted_comments(s: &PdfSession) -> Vec<(usize, String)> {
    let mut list = comments(s.document());
    list.sort();
    list
}

fn delete_first_comment(doc: &mut kition_pdf_cos::Document, page: usize) {
    let index = kition_pdf_annot::summaries(doc).iter().find(|c| c.page == page && c.subtype != "Widget").map(|c| c.index);
    let index = index.unwrap_or_else(|| panic!("no comment on page {page}"));
    kition_pdf_annot::delete_annotation(doc, page, index).unwrap_or_else(|e| panic!("delete: {e}"));
}

#[test]
fn comments_added_to_one_page_at_once_are_both_kept() {
    let (mut a, mut b) = pair(2);
    edit(&mut a, |doc| add_note(doc, 1, "from a"));
    edit(&mut b, |doc| add_note(doc, 1, "from b"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let both = [(1, "from a".to_string()), (1, "from b".to_string())];
    assert_eq!(sorted_comments(&a), both, "the page lists both comments, not only the last writer's");
    // The repaired page is the one both edit on from here.
    edit(&mut b, |doc| add_note(doc, 1, "then b"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(sorted_comments(&a).len(), 3);
    // A file saved from either replica keeps them.
    for s in [&a, &b] {
        let saved = kition_pdf_cos::write_incremental(s.document(), &kition_pdf_cos::SaveOptions::default()).unwrap_or_else(|e| panic!("save: {e}"));
        let reopened = kition_pdf_cos::Document::open(std::sync::Arc::new(saved)).unwrap_or_else(|e| panic!("reopen: {e}"));
        assert_eq!(comments(&reopened).len(), 3);
    }
}

#[test]
fn a_deleted_comment_stays_deleted_beside_a_concurrent_comment_on_its_page() {
    let (mut a, mut b) = pair(2);
    edit(&mut a, |doc| add_note(doc, 1, "old"));
    sync(&mut a, &mut b);
    edit(&mut a, |doc| delete_first_comment(doc, 1));
    edit(&mut b, |doc| add_note(doc, 1, "new"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(sorted_comments(&a), [(1, "new".to_string())], "the deletion holds and the new comment is kept");
    // Undoing the deletion brings the comment back, beside the other.
    assert!(a.undo().unwrap_or_else(|e| panic!("undo: {e}")).0);
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(sorted_comments(&a), [(1, "new".to_string()), (1, "old".to_string())]);
    // A deletion with nothing concurrent is still a deletion on both sides.
    edit(&mut b, |doc| delete_first_comment(doc, 1));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(sorted_comments(&a).len(), 1);
}

/// Every page the page tree lists is a page, and `/Count` says how many.
fn assert_page_tree_whole(doc: &kition_pdf_cos::Document) {
    let root = doc.root().unwrap_or_else(|| panic!("no catalog"));
    let pages = doc.get(root).as_dict().and_then(|d| d.reference(b"Pages")).unwrap_or_else(|| panic!("no page tree"));
    let tree = doc.get(pages);
    let tree = tree.as_dict().unwrap_or_else(|| panic!("the page tree is not a dictionary"));
    let kids = tree.get(b"Kids").map(|k| doc.resolve(k).as_array().cloned().unwrap_or_default()).unwrap_or_default();
    for kid in &kids {
        let page = doc.resolve(kid);
        assert_eq!(page.as_dict().and_then(|d| d.name(b"Type")), Some(&b"Page"[..]), "{kid:?} is a page");
    }
    assert_eq!(tree.int(b"Count"), Some(kids.len() as i64));
}

#[test]
fn a_page_taken_back_while_a_peer_moves_pages_leaves_no_dangling_page() {
    let (mut a, mut b) = pair(2);
    // The page tree is edited already, so taking the insertion back writes it (not deletes it).
    edit(&mut a, |doc| kition_pdf_organize::move_pages(doc, &[1], 0).unwrap_or_else(|e| panic!("move: {e}")));
    edit(&mut a, |doc| {
        kition_pdf_organize::insert_blank_page(doc, 2, 612.0, 792.0).unwrap_or_else(|e| panic!("insert: {e}"));
    });
    sync(&mut a, &mut b);
    // A takes the page back while B moves it (writing the page tree that lists it).
    assert!(a.undo().unwrap_or_else(|e| panic!("undo: {e}")).0);
    edit(&mut b, |doc| {
        kition_pdf_organize::move_pages(doc, &[2], 0).unwrap_or_else(|e| panic!("move: {e}"));
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    for s in [&a, &b] {
        assert_page_tree_whole(s.document());
        assert_eq!(common::page_count(s.document()), 2, "the page A took back is gone on both sides");
    }
}

/// The widths of the view's pages, in page order (the tests tell inserted pages apart by width).
fn page_widths(s: &PdfSession) -> Vec<i64> {
    let doc = s.document();
    let root = doc.root().unwrap_or_else(|| panic!("no catalog"));
    let pages = doc.get(root).as_dict().and_then(|d| d.reference(b"Pages")).unwrap_or_else(|| panic!("no page tree"));
    let tree = doc.get(pages);
    let kids = tree.as_dict().and_then(|d| d.get(b"Kids")).map(|k| doc.resolve(k).as_array().cloned().unwrap_or_default()).unwrap_or_default();
    kids.iter()
        .map(|kid| {
            let page = doc.resolve(kid);
            let mediabox = page.as_dict().and_then(|d| d.get(b"MediaBox")).map(|b| doc.resolve(b));
            let width = mediabox.as_deref().and_then(|b| b.as_array()).and_then(|b| b.get(2)).map(|w| doc.resolve(w));
            match width.as_deref() {
                Some(kition_pdf_cos::Object::Int(w)) => *w,
                Some(kition_pdf_cos::Object::Real(w)) => *w as i64,
                _ => 0,
            }
        })
        .collect()
}

fn sorted_widths(s: &PdfSession) -> Vec<i64> {
    let mut widths = page_widths(s);
    widths.sort_unstable();
    widths
}

fn insert(s: &mut PdfSession, at: usize, width: f64) {
    edit(s, |doc| {
        kition_pdf_organize::insert_blank_page(doc, at, width, 792.0).unwrap_or_else(|e| panic!("insert: {e}"));
    });
}

fn delete_page_of_width(s: &mut PdfSession, width: i64) {
    let index = page_widths(s).iter().position(|w| *w == width).unwrap_or_else(|| panic!("no page {width} wide"));
    edit(s, |doc| kition_pdf_organize::delete_pages(doc, &[index]).unwrap_or_else(|e| panic!("delete: {e}")));
}

#[test]
fn pages_inserted_at_once_are_both_kept() {
    let (mut a, mut b) = pair(2);
    insert(&mut a, 1, 300.0);
    insert(&mut b, 2, 400.0);
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    for s in [&a, &b] {
        assert_page_tree_whole(s.document());
        assert_eq!(sorted_widths(s), [300, 400, 612, 612], "both inserted pages are in the page tree");
    }
    assert_eq!(page_widths(&a), page_widths(&b), "in the same order on both sides");
    // Editing goes on from the repaired tree.
    edit(&mut b, |doc| kition_pdf_organize::move_pages(doc, &[3], 0).unwrap_or_else(|e| panic!("move: {e}")));
    insert(&mut a, 0, 500.0);
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    for s in [&a, &b] {
        assert_page_tree_whole(s.document());
        assert_eq!(sorted_widths(s), [300, 400, 500, 612, 612]);
    }
}

#[test]
fn a_page_deleted_beside_a_concurrent_insert_stays_deleted_and_undo_restores_it() {
    let (mut a, mut b) = pair(2);
    insert(&mut a, 2, 500.0);
    sync(&mut a, &mut b);
    delete_page_of_width(&mut a, 500);
    insert(&mut b, 0, 400.0);
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    for s in [&a, &b] {
        assert_page_tree_whole(s.document());
        assert_eq!(sorted_widths(s), [400, 612, 612], "the deletion holds and the other insert is kept");
    }
    assert!(a.undo().unwrap_or_else(|e| panic!("undo: {e}")).0);
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    for s in [&a, &b] {
        assert_page_tree_whole(s.document());
        assert_eq!(sorted_widths(s), [400, 500, 612, 612], "the undo brings the deleted page back");
    }
}

fn field_names(s: &PdfSession) -> Vec<String> {
    let mut names: Vec<String> = kition_pdf_forms::fields(s.document()).into_iter().map(|f| f.name).collect();
    names.sort();
    names
}

fn add_text_field(s: &mut PdfSession, name: &str, y: f64) {
    edit(s, |doc| {
        let kind = kition_pdf_forms::NewField::Text { multiline: false };
        kition_pdf_forms::add_field(doc, 1, [72.0, y, 300.0, y + 24.0], &kind, Some(name)).unwrap_or_else(|e| panic!("field: {e}"));
    });
}

/// The form's fields are all on their pages too: each has a widget its page lists.
fn assert_fields_on_pages(s: &PdfSession) {
    let doc = s.document();
    let root = doc.root().unwrap_or_else(|| panic!("no catalog"));
    let pages = doc.get(root).as_dict().and_then(|d| d.reference(b"Pages")).unwrap_or_else(|| panic!("no page tree"));
    let tree = doc.get(pages);
    let kids = tree.as_dict().and_then(|d| d.get(b"Kids")).map(|k| doc.resolve(k).as_array().cloned().unwrap_or_default()).unwrap_or_default();
    let widgets: usize = kids
        .iter()
        .map(|kid| {
            let page = doc.resolve(kid);
            let annots = page.as_dict().and_then(|d| d.get(b"Annots")).map(|a| doc.resolve(a).as_array().cloned().unwrap_or_default());
            annots.unwrap_or_default().iter().filter(|a| doc.resolve(a).as_dict().and_then(|d| d.name(b"Subtype")) == Some(&b"Widget"[..])).count()
        })
        .sum();
    assert_eq!(widgets, kition_pdf_forms::fields(s.document()).len(), "every field's widget is on its page");
}

#[test]
fn form_fields_added_at_once_are_both_kept() {
    let (mut a, mut b) = pair(2);
    add_text_field(&mut a, "FromA", 600.0);
    add_text_field(&mut b, "FromB", 500.0);
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    for s in [&a, &b] {
        assert_eq!(field_names(s), ["FromA", "FromB", "Name"], "the form lists both new fields");
        assert_fields_on_pages(s);
    }
    edit(&mut a, |doc| {
        kition_pdf_forms::set_value(doc, "FromB", &kition_pdf_forms::FieldValue::Text("typed".into())).unwrap_or_else(|e| panic!("fill: {e}"));
    });
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let value = kition_pdf_forms::fields(b.document()).into_iter().find(|f| f.name == "FromB").map(|f| f.value);
    assert_eq!(value, Some(vec!["typed".to_string()]));
}

#[test]
fn a_field_deleted_beside_a_concurrent_field_stays_deleted_and_undo_restores_it() {
    let (mut a, mut b) = pair(2);
    add_text_field(&mut a, "Old", 600.0);
    sync(&mut a, &mut b);
    edit(&mut a, |doc| kition_pdf_forms::delete_field(doc, "Old").unwrap_or_else(|e| panic!("delete: {e}")));
    add_text_field(&mut b, "New", 500.0);
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    for s in [&a, &b] {
        assert_eq!(field_names(s), ["Name", "New"], "the deletion holds and the other field is kept");
        assert_fields_on_pages(s);
    }
    assert!(a.undo().unwrap_or_else(|e| panic!("undo: {e}")).0);
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    for s in [&a, &b] {
        assert_eq!(field_names(s), ["Name", "New", "Old"], "the undo brings the field back");
        assert_fields_on_pages(s);
    }
}

#[test]
fn a_comment_and_a_form_fill_made_at_once_are_both_kept() {
    let (mut a, mut b) = pair(2);
    edit(&mut a, |doc| add_note(doc, 1, "note"));
    edit(&mut b, |doc| fill(doc, "Lin"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(comments(b.document()), [(1, "note".to_string())]);
    assert_eq!(field_value(a.document()), ["Lin"]);
}

#[test]
fn two_peers_adding_objects_get_different_keys_and_keep_both() {
    let (mut a, mut b) = pair(2);
    let added = |s: &PdfSession| -> Vec<String> {
        s.last_written().keys.iter().filter(|k| base_number(k).is_none() && *k != kdm_model::TRAILER).cloned().collect()
    };
    edit(&mut a, |doc| add_note(doc, 0, "a"));
    let keys_a = added(&a);
    edit(&mut b, |doc| add_note(doc, 1, "b"));
    let keys_b = added(&b);
    assert!(!keys_a.is_empty() && !keys_b.is_empty());
    assert!(keys_a.iter().all(|k| !keys_b.contains(k)), "{keys_a:?} and {keys_b:?} never collide");
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    // Each replica numbers every added object, all distinct, above the base.
    for s in [&a, &b] {
        let max = s.model().ids().base_max();
        let mut numbers: Vec<u32> =
            keys_a.iter().chain(&keys_b).map(|k| s.model().ids().added().find(|(key, _)| key == k).map(|(_, n)| n).unwrap_or(0)).collect();
        assert!(numbers.iter().all(|n| *n > max), "{numbers:?} above {max}");
        numbers.sort_unstable();
        numbers.dedup();
        assert_eq!(numbers.len(), keys_a.len() + keys_b.len(), "distinct numbers");
    }
    assert_eq!(comments(a.document()).len(), 2);
}

#[test]
fn a_rebase_drops_a_concurrent_comment_and_counts_it() {
    let (mut a, mut b) = pair(2);
    edit(&mut a, |doc| add_note(doc, 0, "before the rewrite"));
    sync(&mut a, &mut b);
    // A rewrites the file in full (redaction, security…) while B comments.
    replace(&mut a, rewritten);
    edit(&mut b, |doc| add_note(doc, 1, "lost"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(comments(a.document()), [(0, "before the rewrite".to_string())], "the comment against the old base is dropped");
    // The replica whose edit was dropped counts it (the other merges the drop with the edit, and
    // counts it only if it saw the edit first).
    assert!(b.dropped() > 0, "B counts what it lost");
    // Nothing is lost silently: each dropped entry is a conflict object both replicas hold alike.
    let conflicts = a.conflicts();
    assert!(!conflicts.is_empty());
    assert_eq!(conflicts, b.conflicts());
    assert!(conflicts.iter().all(|conflict| conflict.kind == kdm_model::conflict::DROPPED_EDIT && conflict.id.starts_with("conflict-")));
    assert!(a.document().overlay_entries().next().is_none());
    // Editing on goes on in the new epoch, on both sides.
    edit(&mut b, |doc| add_note(doc, 1, "after"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(comments(a.document()).len(), 2);
}

#[test]
fn concurrent_edits_of_one_object_keep_one_on_both_sides() {
    let (mut a, mut b) = pair(1);
    edit(&mut a, |doc| fill(doc, "from a"));
    edit(&mut b, |doc| fill(doc, "from b"));
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    let value = field_value(a.document());
    assert!(value == ["from a"] || value == ["from b"], "{value:?}");
}

#[test]
fn an_undo_after_a_peers_edit_keeps_the_peers_edit() {
    let (mut a, mut b) = pair(2);
    edit(&mut a, |doc| add_note(doc, 0, "mine"));
    sync(&mut a, &mut b);
    edit(&mut b, |doc| add_note(doc, 1, "theirs"));
    sync(&mut a, &mut b);
    assert!(a.undo().unwrap_or_else(|e| panic!("undo: {e}")).0);
    sync(&mut a, &mut b);
    assert_converged(&a, &b);
    assert_eq!(comments(a.document()), [(1, "theirs".to_string())]);
}
