//! Read-only selection through the public automation table, with independent authored geometry.

#[path = "support/search_geometry_fixture.rs"]
mod fixture;

use kition_pdf_automation::{Automation, Content, ToolError, tools};
use serde_json::{Value, json};
use std::sync::Arc;

fn open() -> (Automation, u64) {
    fixture::export();
    let mut automation = Automation::new();
    let id = automation.session_mut().open("SelectionGeometry.pdf", None, Arc::new(fixture::pdf()), None).unwrap();
    (automation, id.0)
}

fn call(automation: &mut Automation, name: &str, args: Value) -> Value {
    let Content::Json(value) = automation.call(name, &args).unwrap().remove(0) else { panic!("expected JSON") };
    value
}

fn center(rect: [f32; 4]) -> [f32; 2] {
    [(rect[0] + rect[2]) / 2., (rect[1] + rect[3]) / 2.]
}

fn glyph(page: fixture::Page, index: usize) -> [f32; 4] {
    page.view_rect(match index {
        0 => [50., 136., 62., 156.],
        1 => [62., 136., 74., 156.],
        2 => [50., 96., 62., 116.],
        3 => [62., 96., 74., 116.],
        _ => panic!("fixture has four glyphs"),
    })
}

fn rects_equal(value: &Value, expected: &[[f32; 4]]) {
    let actual: Vec<[f32; 4]> = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(actual.len(), expected.len());
    for (a, e) in actual.iter().flatten().zip(expected.iter().flatten()) {
        assert!(a.is_finite() && (a - e).abs() <= 0.01, "{a} != {e}");
    }
}

#[test]
fn inclusive_full_partial_reverse_and_single_glyph_ranges_follow_rotations_and_crops() {
    let (mut automation, id) = open();
    for (index, page) in fixture::pages().into_iter().enumerate() {
        for (start, end, expected, partial) in [(0, 3, "AB\nCD", false), (1, 2, "B\nC", true)] {
            let args = json!({"doc":id,"page":index + 1,"anchor":center(glyph(page,start)),"focus":center(glyph(page,end))});
            let result = call(&mut automation, "text.select", args.clone());
            assert_eq!(result["text"], expected);
            assert_eq!(result["pages"][0]["text"], expected);
            rects_equal(&result["pages"][0]["rects"], &page.rects(partial));
            let reversed = json!({"anchor":args["focus"],"focus":args["anchor"],"doc":id,"page":index + 1});
            assert_eq!(call(&mut automation, "text.select", reversed), result);
        }
        let result =
            call(&mut automation, "text.select", json!({"doc":id,"page":index + 1,"anchor":center(glyph(page,1)),"focus":center(glyph(page,1))}));
        assert_eq!(result["text"], "B");
        rects_equal(&result["pages"][0]["rects"], &[glyph(page, 1)]);
    }
}

#[test]
fn cross_page_range_normalizes_direction_and_includes_whole_intermediate_pages() {
    let (mut automation, id) = open();
    let pages = fixture::pages();
    let args = json!({"doc":id,"page":2,"endPage":5,"anchor":center(glyph(pages[1],1)),"focus":center(glyph(pages[4],2))});
    let result = call(&mut automation, "text.select", args.clone());
    assert_eq!(result["text"], "B\nCD\nAB\nCD\nAB\nCD\nAB\nC");
    assert_eq!(result["pages"].as_array().unwrap().iter().map(|p| p["page"].as_u64().unwrap()).collect::<Vec<_>>(), [2, 3, 4, 5]);
    rects_equal(&result["pages"][0]["rects"], &[glyph(pages[1], 1), pages[1].rects(false)[1]]);
    rects_equal(&result["pages"][1]["rects"], &pages[2].rects(false));
    rects_equal(&result["pages"][2]["rects"], &pages[3].rects(false));
    rects_equal(&result["pages"][3]["rects"], &[pages[4].rects(false)[0], glyph(pages[4], 2)]);
    assert_eq!(call(&mut automation, "text.select", json!({"doc":id,"page":5,"endPage":2,"anchor":args["focus"],"focus":args["anchor"]})), result);
}

#[test]
fn column_selection_uses_glyph_centers_and_retains_only_selected_line_runs() {
    let (mut automation, id) = open();
    for (index, page) in fixture::pages().into_iter().enumerate() {
        let rect = page.view_rect([50., 96., 62., 156.]);
        let result = call(
            &mut automation,
            "text.select",
            json!({"doc":id,"page":index + 1,"mode":"column","anchor":[rect[0],rect[1]],"focus":[rect[2],rect[3]]}),
        );
        assert_eq!(result["text"], if page.rotation == 180 { "C\nA" } else { "A\nC" });
        rects_equal(&result["pages"][0]["rects"], &[glyph(page, 0), glyph(page, 2)]);
        assert_eq!(
            call(
                &mut automation,
                "text.select",
                json!({"doc":id,"page":index + 1,"mode":"column","anchor":[rect[2],rect[3]],"focus":[rect[0],rect[1]]})
            ),
            result
        );
    }
    let empty = call(&mut automation, "text.select", json!({"doc":id,"page":1,"mode":"column","anchor":[0,0],"focus":[1,1]}));
    assert_eq!(empty, json!({"text":"","pages":[{"page":1,"text":"","rects":[]}]}));
}

#[test]
fn blank_pages_and_maximum_page_span_are_safe_without_partial_results() {
    let mut automation = Automation::new();
    let bytes = automation.session().create_blank(240., 200., 65).unwrap();
    let id = automation.session_mut().open("BlankSelection.pdf", None, bytes, None).unwrap().0;
    let result = call(&mut automation, "text.select", json!({"doc":id,"page":1,"anchor":[1,1],"focus":[2,2]}));
    assert_eq!(result, json!({"text":"","pages":[{"page":1,"text":"","rects":[]}]}));
    let args = json!({"doc":id,"page":1,"endPage":64,"anchor":[1,1],"focus":[2,2]});
    let result = call(&mut automation, "text.select", args.clone());
    assert_eq!(result["pages"].as_array().unwrap().len(), 64);
    assert_eq!(result["text"], "\n".repeat(63));
    let mut too_many = args;
    too_many["endPage"] = json!(65);
    assert!(automation.call("text.select", &too_many).unwrap_err().to_string().contains("64 pages"));
}

#[test]
fn invalid_arguments_and_cross_page_columns_are_actionable_errors() {
    let (mut automation, id) = open();
    let base = json!({"doc":id,"page":1,"anchor":[1,1],"focus":[2,2]});
    for (key, value) in [
        ("page", json!(0)),
        ("page", json!(9)),
        ("page", json!(1.5)),
        ("page", json!(4_294_967_297i64)),
        ("endPage", json!(-1)),
        ("endPage", json!(9)),
        ("endPage", json!(i64::MAX)),
        ("anchor", json!([1])),
        ("anchor", json!([1, 2, 3])),
        ("anchor", json!([null, 0])),
        ("anchor", json!([1e100, 0])),
        ("focus", json!([0, -1e100])),
        ("mode", json!("invalid")),
        ("unexpected", json!(true)),
    ] {
        let mut args = base.clone();
        args[key] = value;
        assert!(matches!(automation.call("text.select", &args), Err(ToolError::InvalidArgs(_))), "{args}");
    }
    for key in ["anchor", "focus", "page", "doc"] {
        let mut args = base.clone();
        args.as_object_mut().unwrap().remove(key);
        assert!(automation.call("text.select", &args).is_err(), "missing {key}");
    }
    let mut args = base;
    args["endPage"] = json!(2);
    args["mode"] = json!("column");
    assert!(automation.call("text.select", &args).unwrap_err().to_string().contains("same"));
}

#[test]
fn selection_is_read_only_in_tool_metadata_bytes_history_and_document_summary() {
    let (mut automation, id) = open();
    let tool = tools().into_iter().find(|tool| tool.name == "text.select").unwrap();
    assert!(tool.read_only && !tool.destructive);
    let before = call(&mut automation, "file.getInfo", json!({"doc":id}));
    let bytes = automation.session().docs()[0].bytes.clone();
    call(&mut automation, "text.select", json!({"doc":id,"page":1,"anchor":[56,54],"focus":[68,94]}));
    assert_eq!(call(&mut automation, "file.getInfo", json!({"doc":id})), before);
    assert!(Arc::ptr_eq(&automation.session().docs()[0].bytes, &bytes));
}
