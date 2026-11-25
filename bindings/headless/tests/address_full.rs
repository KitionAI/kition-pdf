//! C8-pdf-4 item 2. These necessity rules precede the first execution of this matrix.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod common;
mod matrix;

use common::*;
use kition_kit::KitDocument;
use matrix::*;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Address {
    PageNumber,
    PageRange,
    CommentNm,
    CommentIndex,
    FieldName,
    LinkIndex,
    BookmarkPath,
    BookmarkIndex,
    ContentIndex,
    ImageIndex,
    ParagraphIndex,
    FigureNumber,
    Point,
    Rectangle,
    Whole,
}

fn mine(bytes: &[u8], address: Address) -> Value {
    use Address::*;
    let mut doc = kition_pdf_kit::PdfDocument::open(FILE, bytes).unwrap();
    let cos = open(bytes);
    match address {
        PageNumber => step("page.rotate", json!({"pages":[2],"degrees":180})),
        PageRange => step("page.rotate", json!({"pages":[2,3],"degrees":180})),
        CommentNm | CommentIndex => {
            let notes = doc.execute("comment.list", json!({"page":2})).unwrap();
            let note = notes["comments"].as_array().unwrap().iter().find(|n| n["contents"] == "note 2").unwrap();
            if address == CommentNm {
                step("comment.edit", json!({"id":note["id"],"contents":"mine"}))
            } else {
                step("comment.edit", json!({"page":2,"index":note["index"],"contents":"mine"}))
            }
        }
        FieldName => step("form.fill", json!({"values":{"F2":"mine"}})),
        LinkIndex => {
            let links = doc.execute("link.list", json!({})).unwrap();
            let l = links["links"].as_array().unwrap().iter().find(|l| l["page"] == 2).unwrap();
            step("link.edit", json!({"page":2,"index":l["index"],"url":"https://example.com/mine"}))
        }
        BookmarkPath => step("bookmark.add", json!({"page":2,"parent":[2],"title":"Mine"})),
        BookmarkIndex => step("bookmark.add", json!({"page":2,"position":2,"title":"Mine"})),
        ContentIndex => step("content.set", json!({"page":2,"index":1,"text":"Mine"})),
        ImageIndex => step("image.edit", json!({"page":2,"image":1,"action":"rotate"})),
        ParagraphIndex => step("text.edit", json!({"page":2,"paragraph":1,"text":"Mine"})),
        FigureNumber => step("accessibility.setAlt", json!({"figure":figure(&cos,2).num,"alt":"Mine"})),
        Point => step("comment.add", json!({"page":2,"type":"note","at":[220,180],"contents":"mine"})),
        Rectangle => step("redaction.mark", json!({"page":2,"rect":[60,40,260,90]})),
        Whole => step("file.protect", json!({"open_password":"matrix-password"})),
    }
}

// A number, index, point or rectangle names its observed occupant. A named comment follows
// its stored identity. A field name must still resolve to that field. Unheld members merge.
// Content without stored identity holds the complete listed item; paragraph editing also
// removes overlapping text. Figure numbers are PDF object numbers, not ordinal list indexes.
fn want(a: Address, c: Change) -> Expected {
    use Address::*;
    use Kind::*;
    let n = c.page;
    if c.kind == Rewrite {
        return if matches!(a, ContentIndex | ImageIndex | ParagraphIndex | FigureNumber | Rectangle | Whole) { U } else { A };
    }
    if a == Whole {
        return C;
    }
    if c.kind == Property || c.kind == Incremental {
        return A;
    }
    let page_address = !matches!(a, CommentNm | FieldName | FigureNumber);
    let target = n == 2 || (a == PageRange && n == 3);
    if page_address {
        let displaced = match c.kind {
            InsertPage => n <= if a == PageRange { 3 } else { 2 },
            DeletePage => n < if a == PageRange { 3 } else { 2 },
            MovePage => n <= if a == PageRange { 3 } else { 2 } || n == 4,
            _ => false,
        };
        if c.kind == DeletePage && target {
            // The old page is gone and this slot has a different
            // occupant. Content on that replacement is not an in-place edit of
            // the deleted page's held content.
            return Expected { reason: "deleted+relocated" };
        }
        if displaced {
            return R;
        }
    }
    if a == CommentNm && c.kind == DeletePage && n == 2 {
        return D;
    }
    if a == FieldName && c.kind == DeletePage && n == 2 {
        // The fixture retains F2 and its widget in /Fields. redraw uses the
        // widget rectangle and /MK, not its optional page association.
        return A;
    }
    if matches!(a, Point | Rectangle | ImageIndex | ParagraphIndex) && target && matches!(c.kind, Rotate | Box) {
        return C;
    }
    if matches!(a, PageNumber | PageRange) && target && c.kind == Rotate {
        return C;
    }
    match a {
        CommentNm | CommentIndex if n == 2 => match c.kind {
            CommentText => C, // same-field rule: contents is replaced
            DeleteComment => D,
            MoveComment if a == CommentIndex => R,
            Slot { before: true, .. } if a == CommentIndex => R,
            _ => A,
        },
        FieldName if n == 2 && c.kind == FieldRename => R,
        FieldName if n == 2 && c.kind == FieldType => C,
        FieldName if n == 2 && c.kind == FieldValue => C, // same-field rule
        // F2 is plain text: choice options and the Required bit (2) are neither
        // replaced nor consumed by assigning its value.
        FieldName if n == 2 && matches!(c.kind, FieldOptions | FieldFlags) => A,
        FieldName if n == 2 && c.kind == DeleteField => D,
        LinkIndex if n == 2 => match c.kind {
            LinkTarget => C, // same-field rule: action is replaced
            DeleteLink => D,
            Slot { before: true, .. } | DeleteComment | MoveComment | DeleteField => R,
            _ => A,
        },
        BookmarkPath => match c.kind {
            BookmarkMove if n != 3 => R,
            _ => A,
        },
        BookmarkIndex => match c.kind {
            BookmarkMove if n != 3 => R,
            _ => A,
        },
        ContentIndex if n == 2 && matches!(c.kind, Text | DeleteText) => C,
        ImageIndex if n == 2 && c.kind == ImagePixels => C,
        ParagraphIndex if n == 2 && matches!(c.kind, Text | DeleteText) => C,
        Rectangle if n == 2 && matches!(c.kind, Text | DeleteText) => C,
        FigureNumber if n == 2 && c.kind == FigureAlt => C, // same-field rule
        // set_alt checks /S through RoleMap; page membership and geometry are
        // neither replaced nor consumed (unlike decorative).
        FigureNumber if n == 2 && matches!(c.kind, DeletePage | Rotate | Box) => A,
        _ => A,
    }
}

async fn family(addresses: &[Address]) {
    let base = matrix::fixture();
    let columns = changes();
    let mut bad = Vec::new();
    for &a in addresses {
        let mut totals = Totals::default();
        for writer in WRITERS {
            for &change in &columns {
                // Exercised below with the pinned SDK refusal; necessity D/R stays in want.
                if a == Address::FieldName && matches!(change.kind, Kind::DeleteField | Kind::FieldRename) && change.page == 2 {
                    continue;
                }
                let bytes = initial(&base, change);
                let theirs = mutate(&bytes, writer, change);
                cell(&bytes, &theirs, mine(&bytes, a), want(a, change), &format!("address/{a:?}/{writer:?}/{change:?}"), &mut totals).await;
            }
        }
        println!("address family {a:?}: {totals:?}");
        if !totals.clean() {
            bad.push(format!("{a:?}: {totals:?}"));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

macro_rules! address_test {
    ($name:ident,$address:ident) => {
        #[tokio::test(flavor = "multi_thread")]
        async fn $name() {
            family(&[Address::$address]).await;
        }
    };
}
address_test!(page_number, PageNumber);
address_test!(page_range, PageRange);
address_test!(comment_nm, CommentNm);
address_test!(comment_index, CommentIndex);
address_test!(field_name, FieldName);
address_test!(link_index, LinkIndex);
address_test!(bookmark_path, BookmarkPath);
address_test!(bookmark_index, BookmarkIndex);
address_test!(content_index, ContentIndex);
address_test!(image_index, ImageIndex);
address_test!(paragraph_index, ParagraphIndex);
address_test!(figure_number, FigureNumber);
address_test!(point, Point);
address_test!(rectangle, Rectangle);
address_test!(whole, Whole);

async fn regression(pairs: &[(Address, Kind)]) {
    let bytes = matrix::fixture();
    let mut totals = Totals::default();
    for writer in WRITERS {
        for &(address, kind) in pairs {
            let change = Change { kind, page: 2 };
            let theirs = mutate(&bytes, writer, change);
            cell(&bytes, &theirs, mine(&bytes, address), want(address, change), &format!("regression/{address:?}/{writer:?}/{kind:?}"), &mut totals)
                .await;
        }
    }
    assert!(totals.clean(), "{totals:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn regression_same_field() {
    regression(&[
        (Address::CommentNm, Kind::CommentText),
        (Address::CommentIndex, Kind::CommentText),
        (Address::FieldName, Kind::FieldValue),
        (Address::LinkIndex, Kind::LinkTarget),
        (Address::FigureNumber, Kind::FigureAlt),
    ])
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn regression_absolute_assignments() {
    regression(&[(Address::CommentNm, Kind::CommentText), (Address::CommentIndex, Kind::CommentText), (Address::LinkIndex, Kind::LinkTarget)]).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn regression_figure_assignment() {
    regression(&[
        (Address::FigureNumber, Kind::FigureAlt),
        (Address::FigureNumber, Kind::DeletePage),
        (Address::FigureNumber, Kind::Rotate),
        (Address::FigureNumber, Kind::Box),
    ])
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn regression_c8_fill_inputs() {
    regression(&[
        (Address::FieldName, Kind::FieldValue),
        (Address::FieldName, Kind::FieldOptions),
        (Address::FieldName, Kind::FieldFlags),
        (Address::FieldName, Kind::DeletePage),
        (Address::FieldName, Kind::FieldType),
    ])
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn regression_displaced_content_reasons() {
    let bytes = matrix::fixture();
    let mut totals = Totals::default();
    for writer in WRITERS {
        for change in changes()
            .into_iter()
            .filter(|c| matches!((c.kind, c.page), (Kind::MovePage, 1 | 2 | 4) | (Kind::DeletePage, 1 | 2) | (Kind::InsertPage, 1 | 2)))
        {
            let theirs = mutate(&bytes, writer, change);
            for address in [Address::ContentIndex, Address::ImageIndex, Address::ParagraphIndex, Address::Rectangle] {
                cell(
                    &bytes,
                    &theirs,
                    mine(&bytes, address),
                    want(address, change),
                    &format!("displaced/{address:?}/{writer:?}/{change:?}"),
                    &mut totals,
                )
                .await;
            }
        }
    }
    assert_eq!(totals.cells, 56);
    assert!(totals.clean(), "{totals:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn drawing_stream_edits_without_private_metadata() {
    let bytes = matrix::fixture();
    let mut totals = Totals::default();
    for writer in WRITERS {
        for page in 1..=4 {
            let theirs = mutate(&bytes, writer, Change { kind: Kind::DrawingText, page });
            for command in ["content.set", "content.delete"] {
                let params = if command == "content.set" { json!({"page":2,"index":1,"text":"Mine"}) } else { json!({"page":2,"index":1}) };
                cell(
                    &bytes,
                    &theirs,
                    step(command, params),
                    if page == 2 { C } else { A },
                    &format!("drawing-stream/{command}/{writer:?}/{page}"),
                    &mut totals,
                )
                .await;
            }
        }
    }
    println!("drawing stream supplement: {totals:?}");
    assert!(totals.clean(), "{totals:?}");
}

/// With only the current file available to `under`, a deleted name can no longer resolve to
/// its former stored id. The pinned SDK checks the name content fact (`changed` on pdf), not
/// deletion of that Field. Keep this limitation explicit until name-based targeting can hold
/// the object resolved at the observation. This test must change when that starts working.
#[tokio::test(flavor = "multi_thread")]
async fn waiting_for_sdk_deleted_named_field_identity() {
    let bytes = matrix::fixture();
    for writer in WRITERS {
        let f = Fixture::from_bytes(&bytes, Default::default());
        let (_, basis) = f.inspect().await;
        let theirs = mutate(&bytes, writer, Change { kind: Kind::DeleteField, page: 2 });
        std::fs::write(f.file(), &theirs).unwrap();
        let refused = f.refused("run", json!({"basis":basis,"steps":[mine(&bytes,Address::FieldName)]})).await;
        assert_eq!(reasons(&refused), "changed");
        assert!(refused["conflicts"].as_array().unwrap().iter().all(|c| c["id"] == "pdf"));
        assert_eq!(std::fs::read(f.file()).unwrap(), theirs);
        f.shutdown().await;
    }
    println!("waiting_for_sdk: 2 cells; necessity deleted, current name fact changed");
}

/// `under` sees only today's file, so the old name cannot resolve to the stored identity
/// that survives under Renamed2. content_at can report only changed/unknown, not relocated.
#[tokio::test(flavor = "multi_thread")]
async fn waiting_for_sdk_renamed_named_field_identity() {
    let bytes = matrix::fixture();
    let change = Change { kind: Kind::FieldRename, page: 2 };
    assert_eq!(want(Address::FieldName, change).reason, "relocated");
    for writer in WRITERS {
        let f = Fixture::from_bytes(&bytes, Default::default());
        let (_, basis) = f.inspect().await;
        let theirs = mutate(&bytes, writer, change);
        std::fs::write(f.file(), &theirs).unwrap();
        let refused = f.refused("run", json!({"basis":basis,"steps":[mine(&bytes,Address::FieldName)]})).await;
        assert_eq!(reasons(&refused), "changed");
        assert!(refused["conflicts"].as_array().unwrap().iter().all(|c| c["id"] == "pdf"));
        assert_eq!(std::fs::read(f.file()).unwrap(), theirs);
        f.shutdown().await;
    }
    println!("waiting_for_sdk: 2 cells; necessity relocated, current name fact changed");
}

#[tokio::test(flavor = "multi_thread")]
async fn unavailable_stored_id_addresses_are_rejected() {
    let bytes = matrix::fixture();
    let doc = kition_pdf_kit::PdfDocument::open(FILE, &bytes).unwrap();
    let objects = doc.objects();
    let p = &objects.pages[1];
    for (command, params, id) in [
        ("page.rotate", json!({"degrees":90}), format!("page:{}", p.id)),
        ("form.setProps", json!({"tooltip":"mine"}), format!("field:{}", objects.fields[1].id)),
        ("link.edit", json!({"url":"https://example.org/mine"}), p.links[0].id.clone()),
        ("bookmark.add", json!({"page":2,"title":"Mine"}), "bookmark:B2".into()),
    ] {
        let f = Fixture::from_bytes(&bytes, Default::default());
        let (_, basis) = f.inspect().await;
        let got = f.call("run", json!({"basis":basis,"steps":[{"command":command,"params":params,"on":[id]}]})).await;
        assert!(got.is_err(), "{command} unexpectedly accepts a stored-id target: {got:?}");
        assert_eq!(std::fs::read(f.file()).unwrap(), bytes);
        println!("unavailable address {command}: {got:?}");
        f.shutdown().await;
    }
}
