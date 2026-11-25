//! Two replicas of one document on one line of changes: an undo after the other replica's edit
//! takes back only this replica's own step.

mod common;

use common::{add_note, assert_converged, comments, edit, first, fixture, peer, sync};
use kdm_model::PdfSession;

fn pair(pages: usize) -> (PdfSession, PdfSession) {
    let a = first(&fixture(pages));
    let b = peer(&a);
    (a, b)
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
