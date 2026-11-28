//! Stage 3 (issue-hand-e10-thumbnails-blank): an edit repaints only the pages it changed. Page
//! stamps follow what each page draws, and zoomed tiles are kept by stamp, so scrolling back and
//! repainting after an edit of another page take pixels from the cache.

use super::*;

/// Three 200 × 150 pages with distinct backgrounds, reopened from their saved bytes.
fn document() -> PdfDocument {
    let mut doc = PdfDocument::create("Paint.pdf", 200, 150).unwrap();
    doc.execute("page.insertBlank", json!({ "at": 2 })).unwrap();
    doc.execute("page.insertBlank", json!({ "at": 3 })).unwrap();
    for (index, color) in ["#d02040", "#2040d0", "#e0b030"].iter().enumerate() {
        doc.execute("file.setBackground", json!({ "pages": [index + 1], "color": color })).unwrap();
    }
    PdfDocument::open("Paint.pdf", &doc.save(None).unwrap()).unwrap()
}

fn rasterizations(doc: &PdfDocument) -> u64 {
    doc.with_renderer(|renderer| renderer.pages.stats().rasterizations).unwrap()
}

fn stamps(doc: &PdfDocument) -> Vec<String> {
    let answer: Value = serde_json::from_str(&doc.page_stamps()).unwrap();
    assert_eq!(answer["revision"], json!(doc.revision()));
    answer["stamps"].as_array().unwrap().iter().map(|stamp| stamp.as_str().unwrap().to_owned()).collect()
}

/// The whole canvas at device pixel ratio 2, as the editor's frame at 200 % asks for it.
fn view(doc: &PdfDocument) -> Vec<u8> {
    doc.render(0.0, 0.0, 2.0, doc.width() * 2, doc.height() * 2).unwrap()
}

#[test]
fn a_zoomed_view_drawn_again_takes_every_tile_from_the_cache() {
    let doc = document();
    let first = view(&doc);
    let drawn = rasterizations(&doc);
    assert!(drawn >= 3, "one tile or more per page");
    assert_eq!(view(&doc), first);
    assert_eq!(rasterizations(&doc), drawn, "a view seen before is not drawn again");
    // A frame that starts mid-page and mid-tile, as a scroll gives, is cut from the same tiles.
    let area = doc.areas()[1];
    let part = doc.render(area.x + 13.25, area.y + 7.5, 2.0, 211, 97).unwrap();
    assert_eq!(rasterizations(&doc), drawn);
    let uncached = document();
    uncached.paints.tiles.replace(crate::tile_cache::TileCache::new(0));
    assert_eq!(uncached.render(area.x + 13.25, area.y + 7.5, 2.0, 211, 97).unwrap(), part, "cached tiles draw what rendering anew does");
    // Another scale is other tiles.
    doc.render(area.x, area.y, 3.0, 64, 64).unwrap();
    assert!(rasterizations(&doc) > drawn);
}

#[test]
fn an_edit_of_one_page_redraws_that_page_only_and_undo_brings_its_old_tiles_back() {
    let mut doc = document();
    let before = view(&doc);
    let stamps_before = stamps(&doc);
    assert_eq!(stamps_before.len(), 3);
    assert!(stamps_before[0] != stamps_before[1] && stamps_before[1] != stamps_before[2]);

    doc.execute("comment.add", json!({ "page": 2, "type": "rectangle", "rect": [20, 20, 120, 100] })).unwrap();
    let stamps_after = stamps(&doc);
    assert_eq!((&stamps_after[0], &stamps_after[2]), (&stamps_before[0], &stamps_before[2]), "the other pages draw as before");
    assert_ne!(stamps_after[1], stamps_before[1]);
    let after = view(&doc);
    assert_ne!(after, before, "the comment shows");
    let page_two = doc.with_renderer(|renderer| renderer.pages.stats().rasterizations).unwrap();
    assert!(page_two >= 1);
    // Only page 2 was drawn: a fresh document draws all three pages for the same view.
    let fresh = PdfDocument::open("Edited.pdf", &doc.save(None).unwrap()).unwrap();
    assert_eq!(view(&fresh), after, "kept tiles of the other pages draw what the edited file does");
    assert!(rasterizations(&fresh) > page_two, "the edit redrew fewer tiles than the whole view has");

    doc.execute("edit.undo", json!({})).unwrap();
    assert_eq!(stamps(&doc), stamps_before, "undo brings the stamps back");
    assert_eq!(view(&doc), before);
    assert_eq!(rasterizations(&doc), 0, "undo draws nothing anew: every page's tiles were kept");
}

#[test]
fn moving_and_turning_pages_keep_the_stamps_of_what_did_not_change() {
    let mut doc = document();
    let before = stamps(&doc);
    doc.execute("page.move", json!({ "pages": [3], "to": 1 })).unwrap();
    assert_eq!(stamps(&doc), vec![before[2].clone(), before[0].clone(), before[1].clone()], "moved pages keep their stamps");
    view(&doc);
    assert_eq!(rasterizations(&doc), 3, "after a move each page draws once");
    let moved = stamps(&doc);
    doc.execute("page.rotate", json!({ "pages": [2], "degrees": 90 })).unwrap();
    let turned = stamps(&doc);
    assert_eq!((&turned[0], &turned[2]), (&moved[0], &moved[2]));
    assert_ne!(turned[1], moved[1]);
    // A bookmark draws on no page.
    doc.execute("bookmark.add", json!({ "page": 1, "title": "Start" })).unwrap();
    assert_eq!(stamps(&doc), turned);
}

#[test]
fn a_save_starts_new_stamps_that_stay_stable() {
    let mut doc = document();
    doc.execute("comment.add", json!({ "page": 1, "type": "note", "contents": "Saved", "at": [40, 40] })).unwrap();
    let edited = stamps(&doc);
    doc.save(None).unwrap();
    doc.mark_saved();
    let saved = stamps(&doc);
    assert_eq!(saved.len(), 3);
    assert!(saved.iter().zip(&edited).all(|(a, b)| a != b), "stamps of another source never match");
    assert_eq!(stamps(&doc), saved, "and stay stable");
}

#[test]
fn a_page_drawn_whole_and_cut_into_tiles_matches_the_renderer_drawing_that_area() {
    let mut doc = document();
    doc.execute("comment.add", json!({ "page": 1, "type": "oval", "rect": [13.3, 17.7, 151.1, 121.9] })).unwrap();
    let area = doc.areas()[0];
    let view = doc.render(area.x + 10.0, area.y + 20.0, 2.0, 300, 200).unwrap();
    let direct = doc
        .with_renderer(|renderer| {
            let tile = renderer.pages.render(RenderRequest {
                page: 0,
                kind: RequestKind::Pixels,
                tile: Some(Tile { x: 20, y: 40, w: 300, h: 200 }),
                scale: 2.0,
                tag: 0,
            });
            let mut pixels = tile.rgba.to_vec();
            straight(&mut pixels);
            pixels
        })
        .unwrap();
    // The rasterizer's antialiasing moves by a unit in a rare edge pixel with where a call starts
    // (as frames at other scroll positions always did); a seam or a shift would differ widely.
    let differ: Vec<u8> = view.iter().zip(&direct).filter(|(a, b)| a != b).map(|(a, b)| a.abs_diff(*b)).collect();
    assert!(
        differ.len() <= 8 && differ.iter().all(|step| *step <= 2),
        "no seam or shift where the tiles meet: {} channels differ, by up to {:?}",
        differ.len(),
        differ.iter().max()
    );
    let colors: std::collections::HashSet<[u8; 4]> = view.as_chunks::<4>().0.iter().copied().collect();
    assert!(colors.len() > 8, "the oval's antialiased edge is in the area");
}
