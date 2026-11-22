//! What each family of writes rests on (task card C8-land-14-pdf, item 4), end to end: the agent
//! observes the file, a colleague's program changes it, the agent writes with the `basis` of its
//! observation. A change of what the write rests on refuses it (`changed`: the kit tells the guard
//! facts of the document, `premises.rs` in `kition-pdf-kit`), a change elsewhere does not.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use common::*;
use serde_json::json;

const CHANGED: Want = Want::Refused("changed");

/// A comment addressed by page and index rests on which comment that is and the members it sets.
#[tokio::test(flavor = "multi_thread")]
async fn comments_by_index_hold_the_comment_there_and_the_members_set() {
    let f = Fixture::new(3);
    let mine = |text: &str| step("comment_edit", json!({ "page": 2, "index": 1, "contents": text }));
    // Another comment's change, on another page: applies.
    case(&f, &[("comment_edit", json!({ "page": 3, "index": 1, "contents": "theirs" }))], mine("a"), Want::Applies).await;
    // The same comment's colour: a member the write does not set.
    case(&f, &[("comment_edit", json!({ "page": 2, "index": 1, "color": "blue" }))], mine("b"), Want::Applies).await;
    // The same comment's text: refused.
    case(&f, &[("comment_edit", json!({ "page": 2, "index": 1, "contents": "theirs" }))], mine("c"), CHANGED).await;
    // A page put before: page 2 is another page now. Two pages are listed: the one pushed out of
    // the place the write names and the one that moved into it.
    case(&f, &[("page_insert_blank", json!({ "at": 1 }))], mine("d"), Want::Refused("relocated+relocated")).await;
    // A page put after: applies.
    case(&f, &[("page_insert_blank", json!({ "at": 5 }))], step("comment_edit", json!({ "page": 3, "index": 1, "contents": "e" })), Want::Applies)
        .await;
    f.shutdown().await;
}

/// A comment addressed by its id (`/NM`) follows it to wherever its page is.
#[tokio::test(flavor = "multi_thread")]
async fn comments_by_id_follow_the_comment() {
    let f = Fixture::new(3);
    let id = read_on_disk(&f.file(), "comment_list", json!({ "page": 2 }))["comments"][0]["id"].clone();
    let mine = |text: &str| step("comment_edit", json!({ "id": id, "contents": text }));
    case(&f, &[("page_insert_blank", json!({ "at": 1 }))], mine("a"), Want::Applies).await;
    case(&f, &[("comment_edit", json!({ "id": id, "contents": "theirs" }))], mine("b"), CHANGED).await;
    case(&f, &[("comment_delete", json!({ "id": id }))], mine("c"), Want::Refused("deleted")).await;
    f.shutdown().await;
}

/// A fill rests on the value of the fields it fills.
#[tokio::test(flavor = "multi_thread")]
async fn form_fills_hold_the_values_they_set() {
    let f = Fixture::new(3);
    let mine = |value: &str| step("form_fill", json!({ "values": { "F2": value } }));
    case(&f, &[("form_fill", json!({ "values": { "F3": "theirs" } }))], mine("a"), Want::Applies).await;
    case(&f, &[("form_set_props", json!({ "field": "F2", "tooltip": "theirs" }))], mine("b"), Want::Applies).await;
    case(&f, &[("form_fill", json!({ "values": { "F2": "theirs" } }))], mine("c"), CHANGED).await;
    case(&f, &[("form_set_props", json!({ "field": "F2", "name": "G2" }))], step("form_fill", json!({ "values": { "F2": "d" } })), CHANGED).await;
    f.shutdown().await;
}

/// A page write by index rests on which page the index names (and its geometry for a rotation).
#[tokio::test(flavor = "multi_thread")]
async fn page_writes_hold_the_page_the_index_names() {
    let f = Fixture::new(4);
    let rotate = |page: u64| step("page_rotate", json!({ "pages": [page], "degrees": 90 }));
    case(&f, &[("page_rotate", json!({ "pages": [3], "degrees": 90 }))], rotate(2), Want::Applies).await;
    case(&f, &[("page_insert_blank", json!({ "at": 5 }))], rotate(2), Want::Applies).await;
    case(&f, &[("page_rotate", json!({ "pages": [2], "degrees": 180 }))], rotate(2), CHANGED).await;
    // Page 1 deleted: the page that was second moved out of the place the write names and the
    // page that was third moved into it; both are listed.
    case(&f, &[("page_delete", json!({ "pages": [1] }))], rotate(2), Want::Refused("relocated+relocated")).await;
    // Page 4 moved to the front: the page that was second moved into the place the write names
    // and the page that was third was pushed out of it; both are listed.
    case(&f, &[("page_move", json!({ "pages": [4], "to": 1 }))], step("page_delete", json!({ "pages": [3] })), Want::Refused("relocated+relocated"))
        .await;
    case(
        &f,
        &[("comment_add", json!({ "page": 1, "type": "note", "at": [300, 300], "contents": "theirs" }))],
        step("page_delete", json!({ "pages": [2] })),
        Want::Applies,
    )
    .await;
    f.shutdown().await;
}

/// A write at a point of a page rests on the page and on what decides where the point lands.
#[tokio::test(flavor = "multi_thread")]
async fn writes_at_a_point_hold_the_page_and_its_geometry() {
    let f = Fixture::new(3);
    let mine = || step("comment_add", json!({ "page": 2, "type": "note", "at": [200, 200], "contents": "mine" }));
    case(&f, &[("comment_add", json!({ "page": 2, "type": "note", "at": [300, 300], "contents": "theirs" }))], mine(), Want::Applies).await;
    case(&f, &[("page_set_box", json!({ "pages": [2], "box": "crop", "margins": [36, 36, 36, 36] }))], mine(), CHANGED).await;
    case(&f, &[("page_rotate", json!({ "pages": [2], "degrees": 90 }))], mine(), CHANGED).await;
    // Page 2 moved away and page 3 moved into its place: both are listed.
    case(&f, &[("page_move", json!({ "pages": [2], "to": 4 }))], mine(), Want::Refused("relocated+relocated")).await;
    case(&f, &[("page_rotate", json!({ "pages": [1], "degrees": 90 }))], mine(), Want::Applies).await;
    f.shutdown().await;
}

/// Inserting a page rests on the pages either side of where it goes.
#[tokio::test(flavor = "multi_thread")]
async fn page_insertion_holds_its_neighbours() {
    let f = Fixture::new(3);
    let mine = || step("page_insert_blank", json!({ "at": 2 }));
    case(&f, &[("comment_add", json!({ "page": 3, "type": "note", "at": [300, 300], "contents": "theirs" }))], mine(), Want::Applies).await;
    case(&f, &[("page_delete", json!({ "pages": [4] }))], mine(), Want::Applies).await;
    // The two pages either side: the first deleted, the second moved to where the first was.
    case(&f, &[("page_delete", json!({ "pages": [1] }))], mine(), Want::Refused("relocated+relocated+deleted")).await;
    f.shutdown().await;
}

/// An item of a page's content by index rests on the item there (it has no id).
#[tokio::test(flavor = "multi_thread")]
async fn content_items_hold_the_item_at_the_index() {
    let f = Fixture::new(3);
    let mine = |text: &str| step("content_update", json!({ "page": 1, "index": 1, "text": text }));
    case(&f, &[("content_update", json!({ "page": 2, "index": 1, "text": "theirs" }))], mine("a"), Want::Applies).await;
    case(&f, &[("content_update", json!({ "page": 1, "index": 1, "text": "theirs" }))], mine("b"), CHANGED).await;
    f.shutdown().await;
}

/// A bookmark put among the top level rests on the bookmarks either side of where it goes (C8-pdf-3:
/// objects in a window, no longer the parent's whole list as a fact): one put before them moves
/// them (`relocated`), one put after them does not. The refusal lists three bookmarks: the new one
/// and `B1`, which it moves, and the agent's earlier `Mine`, pushed out of the window.
#[tokio::test(flavor = "multi_thread")]
async fn bookmarks_hold_the_parents_children() {
    let f = Fixture::new(3);
    let mine = || step("bookmark_add", json!({ "page": 2, "title": "Mine", "position": 2 }));
    case(&f, &[("comment_add", json!({ "page": 2, "type": "note", "at": [300, 300], "contents": "theirs" }))], mine(), Want::Applies).await;
    case(&f, &[("bookmark_add", json!({ "page": 3, "title": "Theirs", "position": 1 }))], mine(), Want::Refused("relocated+relocated+relocated"))
        .await;
    case(&f, &[("bookmark_add", json!({ "page": 3, "title": "After" }))], mine(), Want::Applies).await;
    f.shutdown().await;
}

/// A link by page and index rests on the link there; one on another page applies.
#[tokio::test(flavor = "multi_thread")]
async fn links_hold_the_link_at_the_index() {
    let mut doc = fixture_doc(2);
    use kition_kit::KitDocument;
    doc.execute("link_add", json!({ "page": 1, "rect": [100, 300, 200, 320], "url": "https://example.com/a" })).unwrap();
    doc.execute("link_add", json!({ "page": 2, "rect": [100, 300, 200, 320], "url": "https://example.com/b" })).unwrap();
    let f = Fixture::from_bytes(&doc.save(None).unwrap(), Default::default());
    let index = |page: u64| {
        read_on_disk(&f.file(), "link_list", json!({}))["links"].as_array().unwrap().iter().find(|l| l["page"] == page).unwrap()["index"].clone()
    };
    let (one, two) = (index(1), index(2));
    let mine = |url: &str| step("link_edit", json!({ "page": 1, "index": one, "url": url }));
    case(
        &f,
        &[("link_edit", json!({ "page": 2, "index": two, "url": "https://example.com/theirs" }))],
        mine("https://example.com/m1"),
        Want::Applies,
    )
    .await;
    case(&f, &[("link_edit", json!({ "page": 1, "index": one, "url": "https://example.com/theirs" }))], mine("https://example.com/m2"), CHANGED)
        .await;
    f.shutdown().await;
}

/// A setting of the document rests on that setting (C8-pdf-3 item 4: how the document opens, no
/// longer all of it); a write that rewrites the whole file still rests on all of it.
#[tokio::test(flavor = "multi_thread")]
async fn document_settings_hold_the_whole_document() {
    let f = Fixture::new(2);
    let mine = || step("doc_initial_view", json!({ "layout": "single" }));
    case(&f, &[("comment_add", json!({ "page": 2, "type": "note", "at": [300, 300], "contents": "theirs" }))], mine(), Want::Applies).await;
    case(&f, &[("doc_initial_view", json!({ "layout": "two_up" }))], mine(), CHANGED).await;
    case(&f, &[], mine(), Want::Applies).await;
    let protect = || step("doc_protect", json!({ "open_password": "secret" }));
    case(&f, &[("comment_add", json!({ "page": 1, "type": "note", "at": [300, 300], "contents": "theirs" }))], protect(), CHANGED).await;
    f.shutdown().await;
}

/// The document's scripts: a script by name rests on the scripts (it rewrites their list), not on
/// the pages.
#[tokio::test(flavor = "multi_thread")]
async fn document_scripts_hold_the_scripts() {
    let f = Fixture::new(2);
    let mine = || step("js_set_document_script", json!({ "name": "init", "script": "var a = 1;" }));
    case(&f, &[("page_rotate", json!({ "pages": [2], "degrees": 90 }))], mine(), Want::Applies).await;
    case(&f, &[("js_set_document_script", json!({ "name": "other", "script": "var b = 2;" }))], mine(), CHANGED).await;
    f.shutdown().await;
}

// ---- C8-pdf-3 item 1: what the code reads that the declarations did not hold ----

/// A parameter read otherwise than its name says: `page_move`'s `to` is a place in the list
/// without the moved pages (organize `move_pages`), so pages [1] to 2 go between pages 2 and 3. A
/// page put there is in the gap (refused, listing the new page, which moved into the window, and
/// the page it pushed out of it); one put at the end is not.
#[tokio::test(flavor = "multi_thread")]
async fn page_move_holds_the_pages_either_side_of_where_its_pages_go() {
    let f = Fixture::new(4);
    let mine = || step("page_move", json!({ "pages": [1], "to": 2 }));
    case(&f, &[("page_insert_blank", json!({ "at": 3 }))], mine(), Want::Refused("relocated+relocated")).await;
    case(&f, &[("page_insert_blank", json!({ "at": 5 }))], mine(), Want::Applies).await;
    f.shutdown().await;
}

/// A value naming another object: an action that goes to page 2 names the page there. Moving page
/// 2 away is refused, listing it and the page that moved into its place.
#[tokio::test(flavor = "multi_thread")]
async fn field_actions_hold_the_page_they_go_to() {
    let f = Fixture::new(3);
    let mine = || step("form_set_actions", json!({ "field": "F1", "actions": [{ "trigger": "mouse_up", "page": 2 }] }));
    case(&f, &[("page_rotate", json!({ "pages": [3], "degrees": 90 }))], mine(), Want::Applies).await;
    case(&f, &[("page_move", json!({ "pages": [2], "to": 3 }))], mine(), Want::Refused("relocated+relocated")).await;
    f.shutdown().await;
}

/// A default taken from the neighbours: a field added without a name is named free of every name
/// there is (forms `free_name`); another field's value is no matter. (The first write makes
/// `Text1`; the colleague's `Text2` is the name the second would have taken.)
#[tokio::test(flavor = "multi_thread")]
async fn a_field_without_a_name_holds_every_name() {
    let f = Fixture::new(2);
    let mine = || step("form_add_field", json!({ "page": 1, "type": "text", "rect": [72, 400, 300, 424] }));
    case(&f, &[("form_fill", json!({ "values": { "F2": "theirs" } }))], mine(), Want::Applies).await;
    case(&f, &[("form_add_field", json!({ "page": 2, "type": "text", "rect": [72, 400, 300, 424], "name": "Text2" }))], mine(), CHANGED).await;
    f.shutdown().await;
}

/// A default taken from the document: `<<n>>` in a footer is its number of pages.
#[tokio::test(flavor = "multi_thread")]
async fn a_footer_with_the_number_of_pages_holds_every_page() {
    let f = Fixture::new(3);
    let mine = || step("doc_header_footer", json!({ "pages": [1], "footer_center": "<<1>> of <<n>>" }));
    case(&f, &[("comment_edit", json!({ "page": 3, "index": 1, "contents": "theirs" }))], mine(), Want::Applies).await;
    case(&f, &[("page_insert_blank", json!({ "at": 4 }))], mine(), CHANGED).await;
    f.shutdown().await;
}

/// Two code paths for one fact: an offset in displayed points turns into page space through the
/// page's rotation (engine `view_vector`), which the declaration did not hold.
#[tokio::test(flavor = "multi_thread")]
async fn a_move_by_an_offset_holds_the_pages_geometry() {
    let f = Fixture::new(2);
    let mine = || step("object_move", json!({ "page": 1, "objects": [{ "index": 1, "kind": "added" }], "offset": [10, 0] }));
    case(&f, &[("page_rotate", json!({ "pages": [2], "degrees": 90 }))], mine(), Want::Applies).await;
    // The page's rotation, and the item as `object_list` reads it (its place on the page turned).
    case(&f, &[("page_rotate", json!({ "pages": [1], "degrees": 90 }))], mine(), Want::Refused("changed+changed")).await;
    f.shutdown().await;
}

/// A measurement reads the scale of the page's viewport at its first point.
#[tokio::test(flavor = "multi_thread")]
async fn a_measurement_holds_the_pages_scale() {
    let f = Fixture::new(2);
    let mine = || step("measure_distance", json!({ "page": 1, "points": [[100, 100], [300, 100]] }));
    let scale = |page: u64| ("measure_scale", json!({ "page": page, "points": [[0, 0], [72, 0]], "distance": 2.54, "unit": "cm" }));
    case(&f, &[scale(2)], mine(), Want::Applies).await;
    case(&f, &[scale(1)], mine(), CHANGED).await;
    f.shutdown().await;
}

/// A reply copies the comment's rectangle and colour (annot `reply`).
#[tokio::test(flavor = "multi_thread")]
async fn a_reply_holds_what_it_copies() {
    let f = Fixture::new(2);
    let mine = || step("comment_reply", json!({ "page": 1, "index": 1, "text": "mine" }));
    case(&f, &[("comment_edit", json!({ "page": 1, "index": 1, "contents": "theirs" }))], mine(), Want::Applies).await;
    case(&f, &[("comment_edit", json!({ "page": 1, "index": 1, "color": "blue" }))], mine(), CHANGED).await;
    f.shutdown().await;
}

// ---- C8-pdf-3 item 5: writes by a rectangle over the page's content ----

/// A redaction mark by a rectangle lands on what the agent saw there: a change of the text under
/// it refuses; text added elsewhere on the page does not.
#[tokio::test(flavor = "multi_thread")]
async fn a_redaction_by_a_rectangle_holds_the_content_it_covers() {
    let f = Fixture::new(2);
    let mine = || step("redact_mark", json!({ "page": 1, "rect": [60, 50, 200, 90] }));
    case(&f, &[("page_add_text", json!({ "page": 1, "at": [72, 500], "text": "elsewhere" }))], mine(), Want::Applies).await;
    case(&f, &[("content_update", json!({ "page": 1, "index": 1, "text": "Other text" }))], mine(), CHANGED).await;
    f.shutdown().await;
}

/// A free comment at a point holds the page and its geometry, not the content under it.
#[tokio::test(flavor = "multi_thread")]
async fn a_note_at_a_point_holds_only_the_page() {
    let f = Fixture::new(2);
    let mine = || step("comment_add", json!({ "page": 1, "type": "note", "at": [80, 62], "contents": "mine" }));
    case(&f, &[("content_update", json!({ "page": 1, "index": 1, "text": "Other text" }))], mine(), Want::Applies).await;
    case(&f, &[("page_rotate", json!({ "pages": [1], "degrees": 90 }))], mine(), CHANGED).await;
    f.shutdown().await;
}
