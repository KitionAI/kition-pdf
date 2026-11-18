//! Search highlights must be page-local, rotation/crop aware, and split by actual line.

#[path = "support/search_geometry_fixture.rs"]
mod fixture;

use kition_pdf_automation::{Automation, Content};
use kition_pdf_render::{PageRenderer, RenderConfig, RenderRequest, RequestKind};
use serde_json::{Value, json};
use std::sync::Arc;

fn rectangles_equal(actual: &[[f32; 4]], expected: &[[f32; 4]], context: &str) {
    assert_eq!(actual.len(), expected.len(), "{context}: one rectangle per matched line");
    for (actual, expected) in actual.iter().zip(expected) {
        for (actual, expected) in actual.iter().zip(expected) {
            assert!(actual.is_finite() && (actual - expected).abs() <= 0.01, "{context}: {actual} != {expected}");
        }
    }
}

#[test]
fn page_text_and_original_ink_agree_with_authored_rotated_and_cropped_geometry() {
    fixture::export();
    let mut renderer = PageRenderer::new(Arc::new(fixture::pdf()), RenderConfig::default());
    assert_eq!(renderer.page_count(), 8);
    for (index, page) in fixture::pages().into_iter().enumerate() {
        let extracted = renderer.render(RenderRequest { page: index, kind: RequestKind::Text, scale: 1.0, ..Default::default() });
        assert!(extracted.error.is_none() && extracted.warnings.is_empty(), "page {}: {:?}", index + 1, extracted.error);
        let text = extracted.text.unwrap();
        assert_eq!(text.plain_text(), "AB\nCD", "page {} reading order", index + 1);
        assert_eq!(text.glyphs.len(), 4);
        for (query, partial, expected_range) in [("AB CD", false, 0..4), ("B C", true, 1..3)] {
            let matches = text.find(query);
            assert_eq!(matches, [expected_range], "page {} query {query}", index + 1);
            rectangles_equal(&text.line_rects(matches[0].clone()), &page.rects(partial), &format!("page {} query {query}", index + 1));
        }
        let raster = renderer.render(RenderRequest { page: index, scale: 1.0, ..Default::default() });
        assert!(raster.error.is_none() && raster.warnings.is_empty());
        assert_eq!([raster.width as f32, raster.height as f32], page.size());
        let expected = page.rects(false);
        let mut deep_ink = 0;
        for y in 0..raster.height {
            for x in 0..raster.width {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let inside = expected.iter().any(|r| px > r[0] + 0.75 && px < r[2] - 0.75 && py > r[1] + 0.75 && py < r[3] - 0.75);
                let outside = expected.iter().all(|r| px < r[0] - 0.75 || px > r[2] + 0.75 || py < r[1] - 0.75 || py > r[3] + 0.75);
                let pixel = &raster.rgba[((y * raster.width + x) * 4) as usize..][..4];
                assert_eq!(pixel[3], 255);
                if inside {
                    assert!(pixel[..3].iter().all(|channel| *channel <= 32), "page {} lost authored ink at {x},{y}", index + 1);
                    deep_ink += 1;
                } else if outside {
                    assert!(pixel[..3].iter().all(|channel| *channel >= 250), "page {} unexpected ink at {x},{y}", index + 1);
                }
            }
        }
        assert!(deep_ink >= 700, "each authored line supplies a substantial tested interior");
        println!("search geometry page={} rotation={} crop={:?} expected={expected:?} deep_ink={deep_ink}", index + 1, page.rotation, page.crop);
    }
}

fn call(automation: &mut Automation, name: &str, args: Value) -> Value {
    let result = automation.call(name, &args).unwrap();
    let Content::Json(value) = result.into_iter().next().unwrap() else { panic!("expected JSON") };
    value
}

#[test]
fn text_find_returns_independent_multiline_and_partial_rects_for_every_page_transform() {
    fixture::export();
    let mut automation = Automation::new();
    let id = automation.session_mut().open("SearchGeometry.pdf", None, Arc::new(fixture::pdf()), None).unwrap();
    for (query, partial, matched_text) in [("AB CD", false, "AB\nCD"), ("ab \t cd", false, "AB\nCD"), ("B C", true, "B\nC")] {
        let args = json!({"doc":id.0,"query":query});
        let result = call(&mut automation, "text_find", args.clone());
        assert_eq!(result["count"], 8, "query {query}");
        assert_eq!(call(&mut automation, "text_find", args), result, "cached lookup preserves geometry");
        for (index, (found, page)) in result["matches"].as_array().unwrap().iter().zip(fixture::pages()).enumerate() {
            assert_eq!(found["page"], index + 1);
            assert_eq!(found["text"], matched_text);
            let rects: Vec<[f32; 4]> = serde_json::from_value(found["rects"].clone()).unwrap();
            rectangles_equal(&rects, &page.rects(partial), &format!("tool page {} query {query}", index + 1));
        }
        println!("search tool query={query:?} pages=8 repeated_cache_result=identical");
    }
    assert_eq!(call(&mut automation, "text_find", json!({"doc":id.0,"query":"AB EF"}))["count"], 0);
}
