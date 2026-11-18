//! The premise guard and receipts when the PDF itself changes (task card C8-land-14-pdf, item 6):
//! a colleague's program (this kit's engine, its own instance, sharing nothing with the server
//! under test) opens the file, edits it and saves it back between the agent's observation and its
//! write; the server sees only a changed file. The agent works through the kits' MCP server's
//! tools on the `kition-pdf` executable.
//!
//! The kit's model is one `Pdf` object, so `others` names `pdf` (its `base`, `epoch` and `~state`)
//! for any change of the file, and every refusal is `changed` or `unknown`: the kit's facts tell
//! the guard whether the change touched what the write rests on (`premises.rs`), not where an
//! object went.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

mod common;

use std::collections::BTreeMap;

use common::*;
use kition_kit::KitDocument;
use kition_kits::tools::{CallOptions, Guard};
use serde_json::{Value, json};

const CHANGED: Want = Want::Refused("changed");
const RELOCATED: Want = Want::Refused("relocated");

fn comment_id(f: &Fixture, page: u64) -> Value {
    read_on_disk(&f.file(), "comment_list", json!({ "page": page }))["comments"][0]["id"].clone()
}

fn contents_on_disk(f: &Fixture, page: u64) -> Value {
    read_on_disk(&f.file(), "comment_list", json!({ "page": page }))["comments"][0]["contents"].clone()
}

// ---- Scenarios ----

/// 1. An unrelated change applies; `others` names the `Pdf` object, with `~state`.
#[tokio::test(flavor = "multi_thread")]
async fn s1_an_unrelated_change_applies_and_others_names_the_document() {
    let f = Fixture::new(3);
    let ran = case(
        &f,
        &[("comment_edit", json!({ "page": 3, "index": 1, "contents": "theirs" }))],
        step("comment_edit", json!({ "page": 1, "index": 1, "contents": "mine" })),
        Want::Applies,
    )
    .await;
    let others = &ran["others"];
    assert_eq!(others["by"], json!(["outside"]), "{ran}");
    // The comment the colleague changed, by its id, and the document object (its base and epoch,
    // and `~state`: see `waiting_for_sdk_a_private_content_fact_holds_after_an_outside_reread`).
    let changed: Vec<&str> = others["changed"].as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap()).collect();
    let theirs = read_on_disk(&f.file(), "comment_list", json!({ "page": 3 }))["comments"][0]["id"].clone();
    assert_eq!(changed, [theirs.as_str().unwrap(), "pdf"], "{others}");
    // Its text (and its modification date, when the clock moved on a second).
    assert!(others["changed"][0]["fields"].as_array().unwrap().contains(&json!("contents")), "{others}");
    println!("s1: others for one unrelated comment edit: {} bytes", others.to_string().len());
    assert_eq!(contents_on_disk(&f, 1), json!("mine"));
    assert_eq!(contents_on_disk(&f, 3), json!("theirs"));
    f.shutdown().await;
}

/// 2. The same member of the target: refused `changed`, nothing written; the retry applies.
#[tokio::test(flavor = "multi_thread")]
async fn s2_the_same_member_is_refused_and_the_retry_applies() {
    let f = Fixture::new(3);
    let mine = step("comment_edit", json!({ "page": 2, "index": 1, "contents": "mine" }));
    let refused = case(&f, &[("comment_edit", json!({ "page": 2, "index": 1, "contents": "theirs" }))], mine.clone(), CHANGED).await;
    println!("s2: refused: {} bytes", refused.to_string().len());
    assert_eq!(contents_on_disk(&f, 2), json!("theirs"));
    case(&f, &[], mine, Want::Applies).await;
    assert_eq!(contents_on_disk(&f, 2), json!("mine"));
    f.shutdown().await;
}

/// 3. The target deleted: a write by id is refused (`changed`: the kit cannot say `deleted`).
#[tokio::test(flavor = "multi_thread")]
async fn s3_a_deleted_target_refuses_the_write_by_id() {
    let f = Fixture::new(3);
    let id = comment_id(&f, 2);
    case(&f, &[("comment_delete", json!({ "id": id }))], step("comment_edit", json!({ "id": id, "contents": "mine" })), Want::Refused("deleted"))
        .await;
    f.shutdown().await;
}

/// 4. A page inserted before the index a write names: refused; 5. after it: applies.
#[tokio::test(flavor = "multi_thread")]
async fn s4_s5_a_page_inserted_before_refuses_and_after_applies() {
    let f = Fixture::new(3);
    let mine = step("comment_edit", json!({ "page": 2, "index": 1, "contents": "mine" }));
    case(&f, &[("page_insert_blank", json!({ "at": 2 }))], mine.clone(), RELOCATED).await;
    case(&f, &[("page_insert_blank", json!({ "at": 4 }))], step("comment_edit", json!({ "page": 3, "index": 1, "contents": "mine" })), Want::Applies)
        .await;
    f.shutdown().await;
}

/// 6. The target comment moved: a write by index that sets its text applies (it does not rest on
/// where the comment is); a write at a point of a page that moved is refused.
#[tokio::test(flavor = "multi_thread")]
async fn s6_a_moved_comment_and_a_write_by_point() {
    let f = Fixture::new(3);
    case(
        &f,
        &[("comment_edit", json!({ "page": 2, "index": 1, "move": [400, 400] }))],
        step("comment_edit", json!({ "page": 2, "index": 1, "contents": "mine" })),
        Want::Applies,
    )
    .await;
    let at = step("comment_add", json!({ "page": 2, "type": "note", "at": [200, 200], "contents": "at a point" }));
    case(&f, &[("comment_edit", json!({ "page": 2, "index": 1, "move": [200, 200] }))], at.clone(), Want::Applies).await;
    case(&f, &[("page_move", json!({ "pages": [2], "to": 3 }))], at, RELOCATED).await;
    f.shutdown().await;
}

/// 7. The page holding the target moved ("parent moved"): by id the write follows the comment;
/// by page and index it is refused.
#[tokio::test(flavor = "multi_thread")]
async fn s7_the_page_holding_the_target_moved() {
    let f = Fixture::new(3);
    let id = comment_id(&f, 2);
    case(&f, &[("page_move", json!({ "pages": [2], "to": 3 }))], step("comment_edit", json!({ "id": id, "contents": "by id" })), Want::Applies).await;
    let id3 = comment_id(&f, 3);
    assert_eq!(id3, id, "the comment is on page 3 now");
    case(
        &f,
        &[("page_move", json!({ "pages": [3], "to": 1 }))],
        step("comment_edit", json!({ "page": 2, "index": 1, "contents": "by index" })),
        RELOCATED,
    )
    .await;
    f.shutdown().await;
}

/// 8. Two fields swap names: a fill by name is refused.
#[tokio::test(flavor = "multi_thread")]
async fn s8_a_name_swap_refuses_a_fill_by_name() {
    let f = Fixture::new(3);
    let swap = [
        ("form_set_props", json!({ "field": "F1", "name": "Tmp" })),
        ("form_set_props", json!({ "field": "F2", "name": "F1" })),
        ("form_set_props", json!({ "field": "Tmp", "name": "F2" })),
    ];
    // The fields keep their ids (stored in them, C8-pdf-3): `F1` names the other field now, whose
    // name changed.
    case(&f, &swap, step("form_fill", json!({ "values": { "F1": "mine" } })), CHANGED).await;
    f.shutdown().await;
}

/// 9. The name renamed away: refused; another field's rename applies.
#[tokio::test(flavor = "multi_thread")]
async fn s9_a_rename_away_refuses_a_write_by_that_name() {
    let f = Fixture::new(3);
    case(&f, &[("form_set_props", json!({ "field": "F3", "name": "G3" }))], step("form_fill", json!({ "values": { "F2": "mine" } })), Want::Applies)
        .await;
    case(&f, &[("form_set_props", json!({ "field": "F2", "name": "G2" }))], step("form_fill", json!({ "values": { "F2": "mine" } })), CHANGED).await;
    f.shutdown().await;
}

/// 10. `epoch` refuses an unrelated change, `premise` applies it, `off` applies even a related one.
#[tokio::test(flavor = "multi_thread")]
async fn s10_epoch_premise_and_off() {
    let unrelated = [("comment_edit", json!({ "page": 3, "index": 1, "contents": "theirs" }))];
    let related = [("comment_edit", json!({ "page": 1, "index": 1, "contents": "theirs" }))];
    let mine = step("comment_edit", json!({ "page": 1, "index": 1, "contents": "mine" }));
    let epoch = Fixture::from_bytes(&fixture(3), CallOptions { guard: Guard::Epoch, ..CallOptions::default() });
    let refused = case(&epoch, &unrelated, mine.clone(), CHANGED).await;
    println!("s10: epoch refuses: {}", refused["error"]["message"]);
    epoch.shutdown().await;
    let premise = Fixture::new(3);
    case(&premise, &unrelated, mine.clone(), Want::Applies).await;
    premise.shutdown().await;
    let off = Fixture::from_bytes(&fixture(3), CallOptions { guard: Guard::Off, ..CallOptions::default() });
    case(&off, &related, mine, Want::Applies).await;
    assert_eq!(contents_on_disk(&off, 1), json!("mine"), "off: the write applied over the colleague's");
    off.shutdown().await;
}

/// 11. A program that writes the file anew and numbers its objects again. Page 1 of the fixture
/// is the blank page the file was made with: no stored id, named by its object number, which
/// the rewrite changed, so a write naming it is refused `unknown`, never a write to another page.
/// Page 2 is a page the kit made (an id in its private data) and its comment has a `/NM`: both
/// survive the rewrite, so a write by page and index applies, to the same comment.
#[tokio::test(flavor = "multi_thread")]
async fn s11_a_renumbering_rewrite_refuses_unknown() {
    let f = Fixture::new(3);
    let id = comment_id(&f, 2);
    let (_, basis) = f.inspect().await;
    renumber(&f.file());
    let before = std::fs::read(f.file()).unwrap();
    let refused =
        f.refused("run", json!({ "basis": basis, "steps": [step("comment_edit", json!({ "page": 1, "index": 1, "contents": "mine" }))] })).await;
    assert_eq!(reasons(&refused), "unknown", "{refused}");
    assert_eq!(std::fs::read(f.file()).unwrap(), before);
    f.ok("run", json!({ "basis": basis, "steps": [step("comment_edit", json!({ "page": 2, "index": 1, "contents": "mine" }))] })).await;
    assert_eq!(comment_id(&f, 2), id, "the same comment");
    assert_eq!(contents_on_disk(&f, 2), json!("mine"));
    let (_, basis) = f.inspect().await;
    f.ok("run", json!({ "basis": basis, "steps": [step("comment_edit", json!({ "page": 1, "index": 1, "contents": "mine" }))] })).await;
    f.shutdown().await;
}

/// Writes the file anew, as a program that saves in full does: every object numbered again in
/// the order a traversal from the catalogue reaches it; some numbers coincide with the old ones
/// (the catalogue's), others name other objects now.
fn renumber(file: &std::path::Path) {
    let at = std::fs::metadata(file).unwrap().modified().unwrap();
    let doc = kition_pdf_cos::Document::open(std::sync::Arc::new(std::fs::read(file).unwrap())).unwrap();
    let bytes = kition_pdf_cos::write_full(&doc, &kition_pdf_cos::SaveOptions::default()).unwrap();
    let written = kition_pdf_cos::Document::open(std::sync::Arc::new(bytes.clone())).unwrap();
    assert_ne!(kition_pdf_organize::pages(&written).unwrap(), kition_pdf_organize::pages(&doc).unwrap(), "the pages have other numbers");
    std::fs::write(file, bytes).unwrap();
    std::fs::File::options().write(true).open(file).unwrap().set_modified(at - std::time::Duration::from_secs(3600)).unwrap();
}

// ---- Tables ----

/// 50 rounds of an unrelated change: `premise` must refuse none, `epoch` all.
#[tokio::test(flavor = "multi_thread")]
async fn table_fifty_unrelated_rounds() {
    let mut refused = BTreeMap::new();
    for (name, guard) in [("premise", Guard::Premise), ("epoch", Guard::Epoch)] {
        let f = Fixture::from_bytes(&fixture(4), CallOptions { guard, ..CallOptions::default() });
        let mut count = 0;
        for round in 0..50u64 {
            let (_, basis) = f.inspect().await;
            let theirs = match round % 4 {
                0 => ("comment_edit", json!({ "page": 3, "index": 1, "contents": format!("theirs {round}") })),
                1 => ("form_fill", json!({ "values": { "F4": format!("theirs {round}") } })),
                2 => ("page_rotate", json!({ "pages": [4], "degrees": 90 })),
                _ => ("comment_add", json!({ "page": 2, "type": "note", "at": [300, 300], "contents": format!("theirs {round}") })),
            };
            f.colleague(&[theirs]);
            let answer = f
                .call(
                    "run",
                    json!({ "basis": basis, "steps": [step("comment_edit", json!({ "page": 1, "index": 1, "contents": format!("mine {round}") }))] }),
                )
                .await;
            if answer.is_err() {
                count += 1;
            }
        }
        refused.insert(name, count);
        f.shutdown().await;
    }
    println!("50 unrelated rounds, refused: premise {}/50, epoch {}/50", refused["premise"], refused["epoch"]);
    assert_eq!(refused["premise"], 0);
    assert_eq!(refused["epoch"], 50);
}

/// The colleague's change kinds of the mixed table (on a 4-page fixture; the agent's writes are
/// about page 2, its comment and field `F2`).
const KINDS: &[&str] = &[
    "C1 target comment's text",
    "C2 another page's comment text",
    "C3 F2's value",
    "C4 F3's value",
    "C5 page inserted before page 2",
    "C6 page inserted at the end",
    "C7 page 2 rotated",
    "C8 page 4 rotated",
    "C9 target comment deleted",
    "C10 page 2 moved to the end",
    "C11 F2 renamed G2",
    "C12 target comment's colour",
];

fn kind_steps(kind: usize, id: &Value) -> Vec<(&'static str, Value)> {
    match kind {
        0 => vec![("comment_edit", json!({ "id": id, "contents": "theirs" }))],
        1 => vec![("comment_edit", json!({ "page": 3, "index": 1, "contents": "theirs" }))],
        2 => vec![("form_fill", json!({ "values": { "F2": "theirs" } }))],
        3 => vec![("form_fill", json!({ "values": { "F3": "theirs" } }))],
        4 => vec![("page_insert_blank", json!({ "at": 1 }))],
        5 => vec![("page_insert_blank", json!({ "at": 5 }))],
        6 => vec![("page_rotate", json!({ "pages": [2], "degrees": 90 }))],
        7 => vec![("page_rotate", json!({ "pages": [4], "degrees": 90 }))],
        8 => vec![("comment_delete", json!({ "id": id }))],
        9 => vec![("page_move", json!({ "pages": [2], "to": 4 }))],
        10 => vec![("form_set_props", json!({ "field": "F2", "name": "G2" }))],
        _ => vec![("comment_edit", json!({ "id": id, "color": "blue" }))],
    }
}

/// The write forms of the mixed table, and the rule that says which change kinds make a refusal
/// necessary, written down before the table first ran:
///
/// - W1 by id, `comment_edit {id, contents}`: the comment's text changed (C1) or it is gone (C9).
///   Not its page's moves, other pages, fields, its colour.
/// - W2 by name, `form_fill {F2}`: F2's value changed (C3) or no field is named F2 now (C11).
/// - W3 by index, `comment_edit {page: 2, index: 1, contents}`: page 2 / comment 1 names another
///   comment or none now (C5, C9, C10) or the comment's text changed (C1).
/// - W4 by point, `comment_add {page: 2, at}`: page 2 names another page (C5, C10) or its geometry
///   changed (C7). Not the comments on it.
/// - W5 structural, `page_delete {pages: [2]}`: page 2 names another page (C5, C10). Not what is
///   on the page.
const FORMS: &[(&str, &[usize])] =
    &[("W1 by id", &[0, 8]), ("W2 by name", &[2, 10]), ("W3 by index", &[0, 4, 8, 9]), ("W4 by point", &[4, 6, 9]), ("W5 structural", &[4, 9])];

fn form_step(form: usize, id: &Value) -> Value {
    match form {
        0 => step("comment_edit", json!({ "id": id, "contents": "mine" })),
        1 => step("form_fill", json!({ "values": { "F2": "mine" } })),
        2 => step("comment_edit", json!({ "page": 2, "index": 1, "contents": "mine" })),
        3 => step("comment_add", json!({ "page": 2, "type": "note", "at": [200, 200], "contents": "mine" })),
        _ => step("page_delete", json!({ "pages": [2] })),
    }
}

fn next(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

#[tokio::test(flavor = "multi_thread")]
async fn table_mixed_seeded() {
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    let mut cells: BTreeMap<(usize, usize), Vec<&str>> = BTreeMap::new();
    let mut totals: BTreeMap<&str, usize> = BTreeMap::new();
    let bytes = fixture(4);
    let mut lines = Vec::new();
    for round in 0..50 {
        let kind = usize::try_from(next(&mut seed) % KINDS.len() as u64).unwrap();
        let form = usize::try_from(next(&mut seed) % FORMS.len() as u64).unwrap();
        let f = Fixture::from_bytes(&bytes, CallOptions::default());
        let id = comment_id(&f, 2);
        let (_, basis) = f.inspect().await;
        f.colleague(&kind_steps(kind, &id));
        let before = std::fs::read(f.file()).unwrap();
        let answer = f.call("run", json!({ "basis": basis, "steps": [form_step(form, &id)] })).await;
        let necessary = FORMS[form].1.contains(&kind);
        let class = match answer {
            Ok(_) if necessary => "applied wrongly",
            Ok(_) => "applied",
            Err(error) if error["error"]["code"] == "conflict" => {
                assert_eq!(std::fs::read(f.file()).unwrap(), before, "a refusal wrote nothing");
                if necessary { "refused necessary" } else { "refused false" }
            }
            Err(_) => "failed at kit",
        };
        lines.push(format!("round {round:2}: {} x {}: {class}", FORMS[form].0, KINDS[kind]));
        cells.entry((form, kind)).or_default().push(class);
        *totals.entry(class).or_default() += 1;
        f.shutdown().await;
    }
    for line in &lines {
        println!("{line}");
    }
    println!("mixed table totals: {totals:?}");
    for ((form, kind), classes) in &cells {
        println!("cell {} x {}: {classes:?}", FORMS[*form].0, KINDS[*kind]);
    }
    assert_eq!(totals.get("applied wrongly"), None, "{totals:?}");
    assert_eq!(totals.get("refused false"), None, "{totals:?}");
}

/// What each cell of the full table must answer, written down before its first run (C8-pdf-2):
/// `A` applies, else the refusal's reasons. Rows are the forms W1-W5, columns the kinds C1-C12.
///
/// - W1 by id (`comment_edit {id, contents}`): its text (C1) `changed`, gone (C9) `deleted`.
/// - W2 by name (`form_fill {F2}`): its value (C3) `changed`; renamed (C11) `changed` (the name's
///   fact: see `a_rename_by_another_program_refuses_by_tool_id`).
/// - W3 by index (`comment_edit {page: 2, index: 1}`): text (C1) `changed`; a page put before (C5)
///   or page 2 moved (C10) `relocated`; the comment gone (C9) `deleted`.
/// - W4 by point (`comment_add {page: 2, at}`): a page put before (C5) or page 2 moved (C10)
///   `relocated`; page 2 rotated (C7) `changed`.
/// - W5 structural (`page_delete {pages: [2]}`): a page put before (C5) or page 2 moved (C10)
///   `relocated`.
const FULL: [[&str; 12]; 5] = [
    ["changed", "A", "A", "A", "A", "A", "A", "A", "deleted", "A", "A", "A"],
    ["A", "A", "changed", "A", "A", "A", "A", "A", "A", "A", "changed", "A"],
    ["changed", "A", "A", "A", "relocated", "A", "A", "A", "deleted", "relocated", "A", "A"],
    ["A", "A", "A", "A", "relocated", "A", "changed", "A", "A", "relocated", "A", "A"],
    ["A", "A", "A", "A", "relocated", "A", "A", "A", "A", "relocated", "A", "A"],
];

/// Every form x kind of the mixed table once (60 cells), deterministic: the seeded table never
/// drew W1 x C1, W1 x C9 or C2.
#[tokio::test(flavor = "multi_thread")]
async fn table_every_form_and_kind_once() {
    let bytes = fixture(4);
    let mut lines = Vec::new();
    let mut totals: BTreeMap<&str, usize> = BTreeMap::new();
    for (form, row) in FULL.iter().enumerate() {
        for (kind, want) in row.iter().enumerate() {
            let f = Fixture::from_bytes(&bytes, CallOptions::default());
            let id = comment_id(&f, 2);
            let (_, basis) = f.inspect().await;
            f.colleague(&kind_steps(kind, &id));
            let before = std::fs::read(f.file()).unwrap();
            let answer = f.call("run", json!({ "basis": basis, "steps": [form_step(form, &id)] })).await;
            let got = match &answer {
                Ok(_) => "A".to_owned(),
                Err(error) if error["error"]["code"] == "conflict" => {
                    assert_eq!(std::fs::read(f.file()).unwrap(), before, "a refusal wrote nothing");
                    reasons(error)
                }
                Err(error) => format!("failed: {}", error["error"]["message"]),
            };
            let necessary = FORMS[form].1.contains(&kind);
            let class = match (got == "A", necessary) {
                (true, true) => "applied wrongly",
                (true, false) => "applied",
                (false, true) if got.starts_with("failed") => "failed at kit",
                (false, true) => "refused necessary",
                (false, false) => "refused false",
            };
            *totals.entry(class).or_default() += 1;
            // Reasons compare as a set: a page moved away refuses `relocated` for it and for the
            // page now at its number (`relocated+relocated`).
            let set = |reasons: &str| reasons.split('+').map(str::to_owned).collect::<std::collections::BTreeSet<String>>();
            let mark = if set(&got) == set(want) { "as written" } else { "DIFFERS" };
            lines.push(format!("{} x {}: {got} ({class}; written {want}: {mark})", FORMS[form].0, KINDS[kind]));
            f.shutdown().await;
        }
    }
    for line in &lines {
        println!("full: {line}");
    }
    println!("full table totals: {totals:?}");
    let differs: Vec<&String> = lines.iter().filter(|line| line.ends_with("DIFFERS)")).collect();
    assert!(differs.is_empty(), "{differs:#?}");
    assert_eq!(totals.get("applied wrongly"), None, "{totals:?}");
    assert_eq!(totals.get("refused false"), None, "{totals:?}");
}

/// The kinds of the extended table (C8-pdf-3 item 7): the twelve above and four more.
const KINDS2: &[&str] = &[
    "C1 target comment's text",
    "C2 another page's comment text",
    "C3 F2's value",
    "C4 F3's value",
    "C5 page inserted before page 2",
    "C6 page inserted at the end",
    "C7 page 2 rotated",
    "C8 page 4 rotated",
    "C9 target comment deleted",
    "C10 page 2 moved to the end",
    "C11 F2 renamed G2",
    "C12 target comment's colour",
    "C13 page 2's link retargeted",
    "C14 bookmark B2 moved to the end",
    "C15 the text under the rectangle edited",
    "C16 a document property set",
];

/// The extended table's forms: W1-W5 above and five that address by place what has no id of its
/// own, or rest on less than the whole document now.
///
/// - W6 bookmark by index, `bookmark_add {page: 2, parent: [2]}`: page 2 names another page (C5,
///   C10) or `[2]` another bookmark (C14).
/// - W7 link by index, `link_edit {page: 2, index}` setting its address: page 2 names another page
///   (C5, C10), the index another annotation (C9: the comment and its pop-up before it went), or
///   the link's action changed (C13).
/// - W8 content item by index, `content_update {page: 2, index: 1, text}`: page 2 names another
///   page (C5, C10) or the item changed (C15).
/// - W9 rectangle over content, `redact_mark {page: 2, rect}` over `Page 2`: page 2 names another
///   page (C5, C10), its geometry changed (C7) or the content under the rectangle did (C15).
/// - W10 a document setting, `doc_initial_view {layout}`: the setting changed (C16).
const FORMS2: &[(&str, &[usize])] = &[
    ("W1 by id", &[0, 8]),
    ("W2 by name", &[2, 10]),
    ("W3 by index", &[0, 4, 8, 9]),
    ("W4 by point", &[4, 6, 9]),
    ("W5 structural", &[4, 9]),
    ("W6 bookmark by index", &[4, 9, 13]),
    ("W7 link by index", &[4, 8, 9, 12]),
    ("W8 content item by index", &[4, 9, 14]),
    ("W9 rectangle over content", &[4, 6, 9, 14]),
    ("W10 document setting", &[15]),
];

/// The 4-page fixture with a link on every page.
fn fixture_with_links() -> Vec<u8> {
    let mut doc = fixture_doc(4);
    for n in 1..=4 {
        doc.execute("link_add", json!({ "page": n, "rect": [100, 300, 200, 320], "url": format!("https://example.com/{n}") })).unwrap();
    }
    doc.save(None).unwrap()
}

/// Page `page`'s link's place in `/Annots`, as `link_list` numbers it.
fn link_index(f: &Fixture, page: u64) -> Value {
    read_on_disk(&f.file(), "link_list", json!({}))["links"].as_array().unwrap().iter().find(|l| l["page"] == page).unwrap()["index"].clone()
}

/// Kind `kind` of [`KINDS2`] done by the colleague.
fn colleague2(f: &Fixture, kind: usize, id: &Value, link: &Value) {
    match kind {
        12 => {
            f.colleague(&[("link_edit", json!({ "page": 2, "index": link, "to_page": 3 }))]);
        }
        13 => {
            // The kit has no command that moves a bookmark: another program moves it (the
            // engine's outline edit on the file, saved incrementally).
            let at = std::fs::metadata(f.file()).unwrap().modified().unwrap();
            let mut doc = kition_pdf_cos::Document::open(std::sync::Arc::new(std::fs::read(f.file()).unwrap())).unwrap();
            kition_pdf_organize::move_bookmark(&mut doc, &[1], &[], usize::MAX).unwrap();
            std::fs::write(f.file(), kition_pdf_cos::write_incremental(&doc, &kition_pdf_cos::SaveOptions::default()).unwrap()).unwrap();
            std::fs::File::options().write(true).open(f.file()).unwrap().set_modified(at - std::time::Duration::from_secs(3600)).unwrap();
        }
        14 => {
            f.colleague(&[("content_update", json!({ "page": 2, "index": 1, "text": "Theirs 2" }))]);
        }
        15 => {
            f.colleague(&[("doc_initial_view", json!({ "layout": "two_up" }))]);
        }
        _ => {
            f.colleague(&kind_steps(kind, id));
        }
    }
}

fn form_step2(form: usize, id: &Value, link: &Value) -> Value {
    match form {
        5 => step("bookmark_add", json!({ "page": 2, "title": "Mine", "parent": [2] })),
        6 => step("link_edit", json!({ "page": 2, "index": link, "url": "https://example.com/mine" })),
        7 => step("content_update", json!({ "page": 2, "index": 1, "text": "Mine 2" })),
        8 => step("redact_mark", json!({ "page": 2, "rect": [60, 50, 200, 90] })),
        9 => step("doc_initial_view", json!({ "layout": "single" })),
        _ => form_step(form, id),
    }
}

/// What each cell of the extended table must answer, written down before its first run (C8-pdf-3
/// item 7): `A` applies, else the refusal's reasons (as a set). Rows W1-W10, columns C1-C16.
/// W8/W9 x C5/C10 require only relocated; replacement-page
/// content after displacement is not an in-place change to the observed content.
const FULL2: [[&str; 16]; 10] = [
    ["changed", "A", "A", "A", "A", "A", "A", "A", "deleted", "A", "A", "A", "A", "A", "A", "A"],
    ["A", "A", "changed", "A", "A", "A", "A", "A", "A", "A", "changed", "A", "A", "A", "A", "A"],
    ["changed", "A", "A", "A", "relocated", "A", "A", "A", "deleted", "relocated", "A", "A", "A", "A", "A", "A"],
    ["A", "A", "A", "A", "relocated", "A", "changed", "A", "A", "relocated", "A", "A", "A", "A", "A", "A"],
    ["A", "A", "A", "A", "relocated", "A", "A", "A", "A", "relocated", "A", "A", "A", "A", "A", "A"],
    ["A", "A", "A", "A", "relocated", "A", "A", "A", "A", "relocated", "A", "A", "A", "relocated", "A", "A"],
    ["A", "A", "A", "A", "relocated", "A", "A", "A", "relocated", "relocated", "A", "A", "changed", "A", "A", "A"],
    ["A", "A", "A", "A", "relocated", "A", "A", "A", "A", "relocated", "A", "A", "A", "A", "changed", "A"],
    ["A", "A", "A", "A", "relocated", "A", "changed", "A", "A", "relocated", "A", "A", "A", "A", "changed", "A"],
    ["A", "A", "A", "A", "A", "A", "A", "A", "A", "A", "A", "A", "A", "A", "A", "changed"],
];

/// Every form x kind of the extended table once (160 cells), deterministic.
#[tokio::test(flavor = "multi_thread")]
async fn table_every_form_and_kind_extended() {
    let bytes = fixture_with_links();
    let mut lines = Vec::new();
    let mut totals: BTreeMap<&str, usize> = BTreeMap::new();
    for (form, row) in FULL2.iter().enumerate() {
        for (kind, want) in row.iter().enumerate() {
            let f = Fixture::from_bytes(&bytes, CallOptions::default());
            let id = comment_id(&f, 2);
            let link = link_index(&f, 2);
            let (_, basis) = f.inspect().await;
            colleague2(&f, kind, &id, &link);
            let before = std::fs::read(f.file()).unwrap();
            let answer = f.call("run", json!({ "basis": basis, "steps": [form_step2(form, &id, &link)] })).await;
            let got = match &answer {
                Ok(_) => "A".to_owned(),
                Err(error) if error["error"]["code"] == "conflict" => {
                    assert_eq!(std::fs::read(f.file()).unwrap(), before, "a refusal wrote nothing");
                    reasons(error)
                }
                Err(error) => format!("failed: {}", error["error"]["message"]),
            };
            let necessary = FORMS2[form].1.contains(&kind);
            let class = match (got == "A", necessary) {
                (true, true) => "applied wrongly",
                (true, false) => "applied",
                (false, true) if got.starts_with("failed") => "failed at kit",
                (false, true) => "refused necessary",
                (false, false) if got.starts_with("failed") => "failed at kit",
                (false, false) => "refused false",
            };
            *totals.entry(class).or_default() += 1;
            let set = |reasons: &str| reasons.split('+').map(str::to_owned).collect::<std::collections::BTreeSet<String>>();
            let mark = if set(&got) == set(want) { "as written" } else { "DIFFERS" };
            lines.push(format!("{} x {}: {got} ({class}; written {want}: {mark})", FORMS2[form].0, KINDS2[kind]));
            f.shutdown().await;
        }
    }
    for line in &lines {
        println!("full2: {line}");
    }
    println!("extended table totals: {totals:?}");
    let differs: Vec<&String> = lines.iter().filter(|line| line.ends_with("DIFFERS)")).collect();
    assert!(differs.is_empty(), "{differs:#?}");
    assert_eq!(totals.get("applied wrongly"), None, "{totals:?}");
    assert_eq!(totals.get("refused false"), None, "{totals:?}");
}

// ---- Receipt bytes ----

#[tokio::test(flavor = "multi_thread")]
async fn receipt_bytes() {
    let size = |ran: &Value| ran["changes"].to_string().len();
    let f = Fixture::new(2);
    let ran = case(&f, &[], step("comment_edit", json!({ "page": 1, "index": 1, "contents": "one property" })), Want::Applies).await;
    println!("receipt: one property (comment text): {} bytes", size(&ran));
    let ran = case(&f, &[], step("comment_edit", json!({ "page": 1, "index": 1, "move": [300, 300] })), Want::Applies).await;
    println!("receipt: one move (comment): {} bytes", size(&ran));
    f.shutdown().await;
    for children in [5usize, 200] {
        let mut doc = fixture_doc(3);
        for n in 0..children {
            doc.execute(
                "comment_add",
                json!({ "page": 2, "type": "note", "at": [50 + (n % 20) * 25, 150 + (n / 20) * 25], "contents": format!("c{n}") }),
            )
            .unwrap();
        }
        let f = Fixture::from_bytes(&doc.save(None).unwrap(), CallOptions::default());
        let ran = case(&f, &[], step("page_move", json!({ "pages": [2], "to": 3 })), Want::Applies).await;
        println!("receipt: a page with {children} comments moved: {} bytes", size(&ran));
        f.shutdown().await;
    }
    let mut doc = fixture_doc(1);
    for n in 0..200 {
        doc.execute("page_add_text", json!({ "page": 1, "at": [50 + (n % 10) * 50, 100 + (n / 10) * 30], "text": format!("t{n}") })).unwrap();
    }
    let f = Fixture::from_bytes(&doc.save(None).unwrap(), CallOptions::default());
    let objects: Vec<Value> = (1..=200).map(|index| json!({ "index": index, "kind": "added" })).collect();
    let ran = f.call("run", json!({ "steps": [step("object_move", json!({ "page": 1, "objects": objects, "offset": [5, 5] }))] })).await;
    match ran {
        Ok(ran) => println!("receipt: the same offset on 200 objects: {} bytes", size(&ran)),
        Err(error) => println!("receipt: the same offset on 200 objects: failed: {error}"),
    }
    f.shutdown().await;
}

// ---- Waiting for the SDK ----

/// A file whose form fields have no stored id (as another program makes them): each is named by
/// its name.
fn without_field_ids(bytes: &[u8]) -> Vec<u8> {
    let mut doc = kition_pdf_cos::Document::open(std::sync::Arc::new(bytes.to_vec())).unwrap();
    for field in kition_pdf_forms::fields(&doc) {
        doc.update_dict(field.obj, |dict| {
            dict.remove(kition_pdf_engine::projection::FIELD_ID_KEY);
        })
        .unwrap();
    }
    kition_pdf_cos::write_incremental(&doc, &kition_pdf_cos::SaveOptions::default()).unwrap()
}

/// On fields named by their names (no stored id): another program renames field `F2` to `G2`.
/// The reread pairs the field object by its place and tells the change under its new tool id
/// (`field:G2`, `name` F2 -> G2). Since kition-sdk 2ce294a1 the guard ties the premise on
/// `field:F2` (fields `value`, `name`) to that renamed object: the write is refused `relocated` on
/// `field:F2`, with `now` naming `field:G2`. The kit's content facts on the name (the field by
/// name and the fill's inputs, `premises::region`) changed too, so `pdf` is `changed` as well.
#[tokio::test(flavor = "multi_thread")]
async fn a_rename_by_another_program_refuses_by_tool_id() {
    let f = Fixture::from_bytes(&without_field_ids(&fixture(3)), CallOptions::default());
    let refused = case(
        &f,
        &[("form_set_props", json!({ "field": "F2", "name": "G2" }))],
        step("form_fill", json!({ "values": { "F2": "mine" } })),
        Want::Refused("relocated+changed"),
    )
    .await;
    let conflicts = refused["conflicts"].as_array().unwrap();
    let ids: Vec<&str> = conflicts.iter().filter_map(|c| c["id"].as_str()).collect();
    assert_eq!(ids, ["field:F2", "pdf"], "{refused}");
    assert_eq!(conflicts[0]["reason"], "relocated", "{refused}");
    assert_eq!(conflicts[0]["now"]["id"], "field:G2", "{refused}");
    let renamed = refused["others"]["changed"].as_array().unwrap().iter().find(|c| c["type"] == "Field").unwrap().clone();
    assert_eq!(renamed["id"], "field:G2", "{renamed}");
    f.shutdown().await;
}

/// C8-pdf-3 item 3: a field the kit made has an id stored in its dictionary (`KitionPDF_Id`), which
/// another program that writes the file anew keeps (lopdf, renumbering every object): its rename
/// is told as the same field (`field:<id>`) whose `name` changed. A write by the old name is
/// refused: `unknown`, as the name's fact cannot be read across a rewrite (a write names a field
/// by its name, which the kit resolves in the file as it is now: `F2` names no field there, and
/// only the fact holds it). By the new name, after inspecting again, it applies to the same field.
#[tokio::test(flavor = "multi_thread")]
async fn independent_writer_rename_keeps_the_fields_id() {
    let f = Fixture::new(3);
    let id_of =
        |name: &str| on_disk(&f.file()).objects().fields.iter().find(|field| field.name == name).map(|field| (field.id.clone(), field.stored));
    let (id, stored) = id_of("F2").unwrap();
    assert!(stored, "the kit stores an id in the fields it makes");
    let (_, basis) = f.inspect().await;
    let at = std::fs::metadata(f.file()).unwrap().modified().unwrap();
    let mut doc = lopdf::Document::load_mem(&std::fs::read(f.file()).unwrap()).unwrap();
    let field = doc
        .objects
        .iter()
        .find(|(_, object)| object.as_dict().is_ok_and(|dict| dict.get(b"T").ok().and_then(|t| t.as_str().ok()) == Some(&b"F2"[..])))
        .map(|(id, _)| *id)
        .unwrap();
    doc.get_dictionary_mut(field).unwrap().set("T", lopdf::Object::String(b"G2".to_vec(), lopdf::StringFormat::Literal));
    doc.renumber_objects_with(100);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    std::fs::write(f.file(), &bytes).unwrap();
    std::fs::File::options().write(true).open(f.file()).unwrap().set_modified(at - std::time::Duration::from_secs(3600)).unwrap();
    assert_eq!(id_of("G2"), Some((id.clone(), true)), "lopdf kept the field's id");
    let refused = f.refused("run", json!({ "basis": basis, "steps": [step("form_fill", json!({ "values": { "F2": "mine" } }))] })).await;
    assert_eq!(reasons(&refused), "unknown", "{refused}");
    let renamed = refused["others"]["changed"].as_array().unwrap().iter().find(|c| c["type"] == "Field").unwrap().clone();
    assert_eq!(renamed["id"], json!(format!("field:{id}")), "{renamed}");
    let (_, basis) = f.inspect().await;
    f.ok("run", json!({ "basis": basis, "steps": [step("form_fill", json!({ "values": { "G2": "mine" } }))] })).await;
    let filled = on_disk(&f.file()).objects().fields.iter().find(|field| field.id == id).unwrap().fields["value"].clone();
    assert_eq!(filled, json!(["mine"]));
    f.shutdown().await;
}

/// C8-pdf-3 item 6: another program appends an incremental update that gives page 1's object
/// number to another page (the old page 1 gone, a new page in its place with the same number and
/// generation, other content). Page 1 has no stored id (it is named by that number), so a write
/// naming it is refused `unknown`, never applied to the other page; page 2 (a stored id) applies.
#[tokio::test(flavor = "multi_thread")]
async fn independent_writer_reuses_a_pages_number() {
    use lopdf::{Dictionary, Object, Stream};
    let f = Fixture::new(3);
    let (_, basis) = f.inspect().await;
    let at = std::fs::metadata(f.file()).unwrap().modified().unwrap();
    let before = std::fs::read(f.file()).unwrap();
    let prev = lopdf::Document::load_mem(&before).unwrap();
    let page = *prev.get_pages().get(&1).unwrap();
    let parent = prev.get_dictionary(page).unwrap().get(b"Parent").unwrap().clone();
    let mut update = lopdf::IncrementalDocument::create_from(before.clone(), prev);
    let content = update.new_document.add_object(Stream::new(Dictionary::new(), b"BT /F1 12 Tf 72 700 Td (another page) Tj ET".to_vec()));
    let mut other = Dictionary::new();
    other.set("Type", Object::Name(b"Page".to_vec()));
    other.set("Parent", parent);
    other.set("MediaBox", Object::Array(vec![0.into(), 0.into(), 595.into(), 842.into()]));
    other.set("Contents", Object::Reference(content));
    update.new_document.objects.insert(page, Object::Dictionary(other));
    let mut bytes = Vec::new();
    update.save_to(&mut bytes).unwrap();
    assert!(bytes.starts_with(&before), "an incremental update");
    std::fs::write(f.file(), &bytes).unwrap();
    std::fs::File::options().write(true).open(f.file()).unwrap().set_modified(at - std::time::Duration::from_secs(3600)).unwrap();
    let pages = read_on_disk(&f.file(), "doc_inspect", json!({}))["pages"].clone();
    assert_eq!(pages[0]["width"], json!(595.0), "page 1 is another page now: {pages}");
    let refused = f.refused("run", json!({ "basis": basis, "steps": [step("page_rotate", json!({ "pages": [1], "degrees": 90 }))] })).await;
    assert_eq!(reasons(&refused), "unknown", "{refused}");
    assert_eq!(std::fs::read(f.file()).unwrap(), bytes, "a refused write wrote nothing");
    f.ok("run", json!({ "basis": basis, "steps": [step("page_rotate", json!({ "pages": [2], "degrees": 90 }))] })).await;
    f.shutdown().await;
}

// ---- A writer that is not this kit's engine ----

/// Another program (lopdf, an independent PDF library) writes the file anew with every object
/// numbered again from 100. Page 1 (no stored id: named by its number) is unproven, so a write
/// naming it is refused `unknown`; page 2 (an id in its private data, which lopdf keeps) and its
/// comment (`/NM`) are proven, so a write by page and index applies, to the same comment.
#[tokio::test(flavor = "multi_thread")]
async fn independent_writer_full_rewrite_renumbering() {
    let f = Fixture::new(3);
    let id = comment_id(&f, 2);
    let (_, basis) = f.inspect().await;
    let at = std::fs::metadata(f.file()).unwrap().modified().unwrap();
    let mut doc = lopdf::Document::load_mem(&std::fs::read(f.file()).unwrap()).unwrap();
    doc.renumber_objects_with(100);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    std::fs::write(f.file(), &bytes).unwrap();
    std::fs::File::options().write(true).open(f.file()).unwrap().set_modified(at - std::time::Duration::from_secs(3600)).unwrap();
    let refused =
        f.refused("run", json!({ "basis": basis, "steps": [step("comment_edit", json!({ "page": 1, "index": 1, "contents": "mine" }))] })).await;
    assert_eq!(reasons(&refused), "unknown", "{refused}");
    assert_eq!(std::fs::read(f.file()).unwrap(), bytes, "a refused write wrote nothing");
    let ran = f.ok("run", json!({ "basis": basis, "steps": [step("comment_edit", json!({ "page": 2, "index": 1, "contents": "mine" }))] })).await;
    println!("independent rewrite: others {}", ran["others"]);
    assert_eq!(comment_id(&f, 2), id, "the same comment");
    assert_eq!(contents_on_disk(&f, 2), json!("mine"));
    f.shutdown().await;
}

/// Another program (lopdf) appends an incremental update that adds a sticky note without `/NM`
/// to page 3. An unrelated write applies, and `others` names the new comment (by its object
/// number, `a<number>`) and the document object, nothing else.
#[tokio::test(flavor = "multi_thread")]
async fn independent_writer_incremental_comment() {
    use lopdf::{Dictionary, Object, StringFormat};
    let f = Fixture::new(3);
    let (_, basis) = f.inspect().await;
    let at = std::fs::metadata(f.file()).unwrap().modified().unwrap();
    let before = std::fs::read(f.file()).unwrap();
    let prev = lopdf::Document::load_mem(&before).unwrap();
    let page = *prev.get_pages().get(&3).unwrap();
    let mut update = lopdf::IncrementalDocument::create_from(before.clone(), prev);
    update.opt_clone_object_to_new_document(page).unwrap();
    let mut note = Dictionary::new();
    note.set("Type", Object::Name(b"Annot".to_vec()));
    note.set("Subtype", Object::Name(b"Text".to_vec()));
    note.set("Rect", Object::Array(vec![300.into(), 300.into(), 320.into(), 320.into()]));
    note.set("Contents", Object::String(b"from lopdf".to_vec(), StringFormat::Literal));
    let note = update.new_document.add_object(Object::Dictionary(note));
    let page_dict = update.new_document.get_dictionary_mut(page).unwrap();
    match page_dict.get_mut(b"Annots") {
        Ok(Object::Array(annots)) => annots.push(Object::Reference(note)),
        _ => panic!("page 3's /Annots is not a direct array"),
    }
    let mut bytes = Vec::new();
    update.save_to(&mut bytes).unwrap();
    assert!(bytes.starts_with(&before), "an incremental update");
    std::fs::write(f.file(), &bytes).unwrap();
    std::fs::File::options().write(true).open(f.file()).unwrap().set_modified(at - std::time::Duration::from_secs(3600)).unwrap();
    let ran = f.ok("run", json!({ "basis": basis, "steps": [step("comment_edit", json!({ "page": 1, "index": 1, "contents": "mine" }))] })).await;
    let others = &ran["others"];
    let created: Vec<&str> = others["created"].as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(created, [format!("a{}", note.0)], "{others}");
    let changed: Vec<&str> = others["changed"].as_array().unwrap().iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(changed, ["pdf"], "{others}");
    assert!(others["deleted"].as_array().unwrap().is_empty(), "{others}");
    let notes = read_on_disk(&f.file(), "comment_list", json!({ "page": 3 }));
    assert!(notes["comments"].as_array().unwrap().iter().any(|c| c["contents"] == "from lopdf" && c["id"] == format!("a{}", note.0)), "{notes}");
    f.shutdown().await;
}
