//! Regression: a callout's leader stays outside its text box and lands
//! square on the side facing it (never at a corner), with or without a knee; measurements read
//! areas in m² (not m^2) and draw their label with the reading.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use kition_kit::KitDocument;
use kition_pdf_kit::PdfDocument;
use serde_json::{Value, json};

fn run(doc: &mut PdfDocument, command: &str, params: Value) -> Value {
    doc.execute(command, params).unwrap_or_else(|error| panic!("{command}: {error}"))
}

/// The numbers of `/<key> [...]` in each annotation dictionary of the saved file, in order.
fn arrays(bytes: &[u8], key: &str) -> Vec<Vec<f64>> {
    let text = String::from_utf8_lossy(bytes);
    let tag = format!("/{key} [");
    text.match_indices(&tag)
        .map(|(at, _)| {
            let rest = &text[at + tag.len()..];
            rest[..rest.find(']').unwrap()].split_whitespace().map(|n| n.parse::<f64>().unwrap()).collect()
        })
        .collect()
}

/// Whether the segment a–b passes through the inside of `rect` (shrunk by a hair, so touching
/// an edge is not crossing).
fn crosses(rect: [f64; 4], a: [f64; 2], b: [f64; 2]) -> bool {
    let r = [rect[0] + 0.5, rect[1] + 0.5, rect[2] - 0.5, rect[3] - 0.5];
    (0..=200).any(|i| {
        let t = f64::from(i) / 200.0;
        let (x, y) = (a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t);
        x > r[0] && x < r[2] && y > r[1] && y < r[3]
    })
}

/// A callout's text box, the point it points at and its knee, if given.
type Callout = ([f64; 4], [f64; 2], Option<[f64; 2]>);

#[test]
fn callout_leaders_stay_outside_the_box_and_land_square() {
    let mut doc = PdfDocument::create("callouts.pdf", 612, 792).unwrap();
    // (text box, the point, the knee), top-left-origin points.
    let callouts: [Callout; 6] = [
        // No knee, the point straight below the box: the leader used to run up through the text.
        ([60.0, 60.0, 200.0, 104.0], [130.0, 200.0], None),
        // No knee, the point below and to the left.
        ([300.0, 300.0, 440.0, 344.0], [250.0, 420.0], None),
        // A knee up and to the right, past the corner.
        ([300.0, 60.0, 440.0, 104.0], [520.0, 200.0], Some([480.0, 30.0])),
        // A knee just right of the box, level with its upper half.
        ([60.0, 300.0, 200.0, 344.0], [260.0, 420.0], Some([230.0, 312.0])),
        // A knee far above the box and a little to its right.
        ([60.0, 560.0, 200.0, 604.0], [230.0, 460.0], Some([210.0, 470.0])),
        // A knee inside the box.
        ([300.0, 560.0, 440.0, 604.0], [500.0, 700.0], Some([370.0, 580.0])),
    ];
    for (rect, to, knee) in callouts {
        let mut params = json!({ "page": 1, "type": "callout", "rect": rect, "to": to, "contents": "42% up on Q2: split the recall from routine work.", "font_size": 9.5 });
        if let Some(knee) = knee {
            params["knee"] = json!(knee);
        }
        run(&mut doc, "comment.add", params);
    }
    let bytes = doc.save(Some("callouts.pdf")).unwrap();
    let (leaders, outers, insets) = (arrays(&bytes, "CL"), arrays(&bytes, "Rect"), arrays(&bytes, "RD"));
    assert_eq!(leaders.len(), callouts.len());
    for (i, ((cl, outer), rd)) in leaders.iter().zip(&outers).zip(&insets).enumerate() {
        // The text box in user space, from /Rect and /RD.
        let b = [outer[0] + rd[0], outer[1] + rd[1], outer[2] - rd[2], outer[3] - rd[3]];
        let (point, knee, attach) = ([cl[0], cl[1]], [cl[2], cl[3]], [cl[4], cl[5]]);
        assert!(!crosses(b, knee, attach) && !crosses(b, point, knee), "callout {i}: the leader {cl:?} crosses the text box {b:?}");
        // It lands on a side, square to it, and well away from the corners.
        let (w, h) = (b[2] - b[0], b[3] - b[1]);
        let on_side = if (attach[0] - b[0]).abs() < 0.01 || (attach[0] - b[2]).abs() < 0.01 {
            assert!(attach[1] >= b[1] + h / 4.0 - 0.01 && attach[1] <= b[3] - h / 4.0 + 0.01, "callout {i}: lands near a corner {attach:?} of {b:?}");
            (knee[1] - attach[1]).abs() < 0.01 || knee[1] > b[3] || knee[1] < b[1]
        } else if (attach[1] - b[1]).abs() < 0.01 || (attach[1] - b[3]).abs() < 0.01 {
            assert!(attach[0] >= b[0] + w / 4.0 - 0.01 && attach[0] <= b[2] - w / 4.0 + 0.01, "callout {i}: lands near a corner {attach:?} of {b:?}");
            (knee[0] - attach[0]).abs() < 0.01 || knee[0] > b[2] || knee[0] < b[0]
        } else {
            false
        };
        assert!(on_side, "callout {i}: the leader {cl:?} meets the box {b:?} square on a side");
    }
    // A knee level with the box gives a straight landing.
    let level = &leaders[3];
    assert!((level[3] - level[5]).abs() < 0.01, "{level:?}");
    // A knee far above the box lands on its top, not its right side's corner.
    let above = &leaders[4];
    let b = {
        let (o, rd) = (&outers[4], &insets[4]);
        [o[0] + rd[0], o[1] + rd[1], o[2] - rd[2], o[3] - rd[3]]
    };
    assert!((above[5] - b[3]).abs() < 0.01, "{above:?} lands on the top of {b:?}");
}

#[test]
fn measurements_read_square_units_and_draw_their_labels() {
    let mut doc = PdfDocument::create("plot.pdf", 612, 792).unwrap();
    run(&mut doc, "measure.setScale", json!({ "page": 1, "rect": [86, 140, 526, 500], "units_per_point": 0.05, "unit": "m", "name": "Site plan" }));
    run(&mut doc, "measure.distance", json!({ "page": 1, "points": [[106, 494], [506, 494]], "label": "Frontage" }));
    run(&mut doc, "measure.area", json!({ "page": 1, "points": [[126, 180], [256, 180], [256, 280], [126, 280]], "label": "Brassica bed" }));
    let list = run(&mut doc, "measure.list", json!({ "page": 1 }));
    let area = &list["measurements"][1]["reading"];
    assert_eq!(area["unit"], json!("m²"), "{area}");
    assert_eq!(area["label"], json!("32.50 m²"), "{area}");
    assert_eq!(list["measurements"][0]["reading"]["label"], json!("20.00 m"));
    // The captions show the label with the reading: flattened into the page, they are its text.
    run(&mut doc, "file.flatten", json!({ "comments": true }));
    let text = run(&mut doc, "text.extract", json!({}))["pages"][0]["text"].as_str().unwrap_or_default().to_owned();
    for caption in ["Frontage: 20.00 m", "Brassica bed: 32.50 m²"] {
        assert!(text.contains(caption), "{caption:?} in {text:?}");
    }
}
