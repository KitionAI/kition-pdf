//! C8-pdf-4 item 3: necessity is declared independently before the first run.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)]
mod common;
mod matrix;

use common::*;
use kition_kit::KitDocument;
use kition_pdf_cos::{Object, Stream};
use matrix::*;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Derived {
    BlankSize,
    Labels,
    HeaderTotal,
    HeaderNumber,
    Calculation,
    Format,
    DefaultName,
    RadioGroup,
    TabMove,
    TabSet,
    Reply,
    Status,
    TextBox,
    Distance,
    Perimeter,
    Area,
    Redact,
    Urls,
    Structure,
    Ocr,
    TextOverlap,
    FlattenComments,
    FlattenFields,
}
const ROWS: [Derived; 23] = [
    Derived::BlankSize,
    Derived::Labels,
    Derived::HeaderTotal,
    Derived::HeaderNumber,
    Derived::Calculation,
    Derived::Format,
    Derived::DefaultName,
    Derived::RadioGroup,
    Derived::TabMove,
    Derived::TabSet,
    Derived::Reply,
    Derived::Status,
    Derived::TextBox,
    Derived::Distance,
    Derived::Perimeter,
    Derived::Area,
    Derived::Redact,
    Derived::Urls,
    Derived::Structure,
    Derived::Ocr,
    Derived::TextOverlap,
    Derived::FlattenComments,
    Derived::FlattenFields,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Input {
    NeighbourSize,
    Count,
    Labels,
    SourceValue,
    Calculation,
    Format,
    Name,
    Group,
    Tabs,
    ParentRect,
    ParentColor,
    BoxRect,
    BoxWidth,
    Scale,
    Redaction,
    Url,
    LinkRect,
    Heading,
    Structure,
    Pixels,
    Overlap,
    OtherComment,
    OtherTooltip,
    Property,
}
const INPUTS: [Input; 24] = [
    Input::NeighbourSize,
    Input::Count,
    Input::Labels,
    Input::SourceValue,
    Input::Calculation,
    Input::Format,
    Input::Name,
    Input::Group,
    Input::Tabs,
    Input::ParentRect,
    Input::ParentColor,
    Input::BoxRect,
    Input::BoxWidth,
    Input::Scale,
    Input::Redaction,
    Input::Url,
    Input::LinkRect,
    Input::Heading,
    Input::Structure,
    Input::Pixels,
    Input::Overlap,
    Input::OtherComment,
    Input::OtherTooltip,
    Input::Property,
];

fn fixture() -> Vec<u8> {
    let mut doc = kition_pdf_kit::PdfDocument::open(FILE, &matrix::fixture()).unwrap();
    for (command, params) in [
        ("form_fill", json!({"values":{"F2":"10","F3":"2"}})),
        ("form_set_props", json!({"field":"F4","calculate":{"op":"sum","fields":["F2","F3"]}})),
        ("form_set_props", json!({"field":"F2","format":{"type":"number","decimals":2}})),
        ("form_add_field", json!({"page":2,"type":"radio","name":"R","rect":[320,600,338,618]})),
        ("form_add_field", json!({"page":2,"type":"text","name":"S","rect":[350,600,400,618]})),
        ("comment_add", json!({"page":2,"type":"textbox","rect":[300,400,460,450],"contents":"box"})),
        ("measure_scale", json!({"page":2,"units_per_point":0.5,"unit":"m"})),
        ("redact_mark", json!({"page":1,"rect":[60,40,260,90]})),
        ("page_add_text", json!({"page":2,"at":[72,150],"text":"https://example.com/old"})),
        ("page_add_text", json!({"page":2,"at":[80,62],"text":"overlap"})),
    ] {
        doc.execute(command, params).unwrap_or_else(|e| panic!("fixture {command}: {e}"));
    }
    let mut cos = open(&doc.save(None).unwrap());
    let root = cos.root().unwrap();
    let tree = dictionary(&cos, root).reference(b"StructTreeRoot").unwrap();
    let heading = cos.add(dict(&[
        ("Type", Object::name("StructElem")),
        ("S", Object::name("H1")),
        ("P", Object::Ref(tree)),
        ("Pg", Object::Ref(page(&cos, 2))),
        ("K", 18.into()),
        ("ActualText", text("Heading")),
    ]));
    let mut k = list(&cos, tree, "K");
    k.push(Object::Ref(heading));
    set(&mut cos, tree, "K", Object::Array(k));
    kition_pdf_cos::write_incremental(&cos, &Default::default()).unwrap()
}

fn script(js: &str) -> Object {
    Object::Dict(dict(&[("S", Object::name("JavaScript")), ("JS", text(js))]))
}
fn mutate(bytes: &[u8], writer: Writer, input: Input) -> Vec<u8> {
    use Input::*;
    let generic = match input {
        Count => Some(Change { kind: Kind::InsertPage, page: 5 }),
        ParentRect => Some(Change { kind: Kind::CommentRect, page: 2 }),
        ParentColor => Some(Change { kind: Kind::CommentColor, page: 2 }),
        Heading => Some(Change { kind: Kind::Text, page: 2 }),
        Pixels => Some(Change { kind: Kind::ImagePixels, page: 2 }),
        OtherComment => Some(Change { kind: Kind::CommentText, page: 4 }),
        Property => Some(Change { kind: Kind::Property, page: 4 }),
        _ => None,
    };
    if let Some(change) = generic {
        let changed = matrix::mutate(bytes, writer, change);
        if matches!(input, ParentColor | OtherComment) {
            let mut doc = open(&changed);
            let r = annot(&doc, change.page, "Text");
            doc.update_dict(r, |d| {
                d.remove(b"AP");
            })
            .unwrap();
            return save_patch(&changed, &doc, writer, false);
        }
        return changed;
    }
    let mut doc = open(bytes);
    let p = page(&doc, 2);
    match input {
        NeighbourSize => {
            let p = page(&doc, 1);
            set(&mut doc, p, "MediaBox", array(&[0, 0, 500, 700]));
        }
        Labels => {
            let root = doc.root().unwrap();
            set(&mut doc, root, "PageLabels", dict(&[("Nums", Object::Array(vec![0.into(), Object::Dict(dict(&[("S", Object::name("r"))]))]))]));
        }
        SourceValue => {
            let f = field(&doc, 3);
            set(&mut doc, f, "V", text("7"));
        }
        Calculation | Format => {
            let f = field(&doc, if input == Format { 2 } else { 4 });
            let mut aa = dictionary(&doc, f).get(b"AA").and_then(|v| doc.dict(v)).unwrap_or_default();
            aa.set(
                if input == Format { b"F".to_vec() } else { b"C".to_vec() },
                script(if input == Format {
                    "AFNumber_Format(0,0,0,0,\"$\",true);"
                } else {
                    "AFSimple_Calculate(\"PRD\", new Array(\"F2\",\"F3\"));"
                }),
            );
            set(&mut doc, f, "AA", aa);
        }
        Name => {
            let f = field(&doc, 3);
            set(&mut doc, f, "T", text("Text1"));
        }
        Group => {
            let f = kition_pdf_forms::fields(&doc).iter().find(|f| f.name == "R").unwrap().widgets[0].obj;
            set(&mut doc, f, "Rect", array(&[360, 100, 380, 120]));
        }
        Tabs => {
            let mut a = list(&doc, p, "Annots");
            let fields = kition_pdf_forms::fields(&doc);
            let first = field(&doc, 2);
            let last = fields.iter().find(|f| f.name == "S").unwrap().obj;
            let i = a.iter().position(|o| o.as_ref() == Some(first)).unwrap();
            let j = a.iter().position(|o| o.as_ref() == Some(last)).unwrap();
            a.swap(i, j);
            set(&mut doc, p, "Annots", Object::Array(a));
            set(&mut doc, p, "Tabs", Object::name("C"));
        }
        BoxRect | BoxWidth => {
            let r = annot(&doc, 2, "FreeText");
            if input == BoxRect {
                set(&mut doc, r, "Rect", array(&[280, 300, 480, 370]));
            } else {
                set(&mut doc, r, "BS", dict(&[("W", 3.into())]));
            }
        }
        Scale => {
            let vp = list(&doc, p, "VP");
            let v = doc.dict(&vp[0]).unwrap();
            let measure = v.get(b"Measure").unwrap();
            let mut m = doc.dict(measure).unwrap();
            m.set(b"R".to_vec(), text("1 in = 10 m"));
            let mut x = doc.dict(&m.get(b"X").unwrap().as_array().unwrap()[0]).unwrap();
            x.set(b"C".to_vec(), Object::Real(2.0));
            m.set(b"X".to_vec(), Object::Array(vec![Object::Dict(x)]));
            let mut v = v;
            v.set(b"Measure".to_vec(), m);
            set(&mut doc, p, "VP", Object::Array(vec![Object::Dict(v)]));
        }
        Redaction => {
            let mark = doc.add(dict(&[
                ("Type", Object::name("Annot")),
                ("Subtype", Object::name("Redact")),
                ("Rect", array(&[60, 700, 260, 755])),
                ("NM", text("colleague-redaction")),
                ("IC", array(&[0, 0, 0])),
            ]));
            let p3 = page(&doc, 3);
            let mut a = list(&doc, p3, "Annots");
            a.push(Object::Ref(mark));
            set(&mut doc, p3, "Annots", Object::Array(a));
        }
        Url | Overlap => {
            let streams = list(&doc, p, "Contents");
            let old = if input == Url { "https://example.com/old" } else { "overlap" };
            let (r, s) = streams
                .iter()
                .filter_map(Object::as_ref)
                .find_map(|r| match &*doc.get(r) {
                    Object::Stream(s) if String::from_utf8_lossy(&s.decoded().unwrap()).contains(old) => Some((r, s.clone())),
                    _ => None,
                })
                .unwrap();
            let data = String::from_utf8(s.decoded().unwrap()).unwrap().replace(old, if input == Url { "https://example.org/new" } else { "nearby" });
            let mut d = s.dict.clone();
            d.remove(b"Filter");
            d.remove(b"DecodeParms");
            if let Some(Object::Dict(added)) = d.get_mut(b"PCAdded") {
                added.set(b"Text".to_vec(), text(if input == Url { "https://example.org/new" } else { "nearby" }));
            }
            doc.set(r, Stream::from_raw(d, data.into_bytes()));
        }
        LinkRect => {
            let r = annot(&doc, 2, "Link");
            set(&mut doc, r, "Rect", array(&[65, 620, 300, 650]));
        }
        Structure => {
            let tree = dictionary(&doc, doc.root().unwrap()).reference(b"StructTreeRoot").unwrap();
            let k = list(&doc, tree, "K");
            let r = k.last().unwrap().as_ref().unwrap();
            set(&mut doc, r, "ActualText", text("Renamed heading"));
        }
        OtherTooltip => {
            let f = field(&doc, 1);
            set(&mut doc, f, "TU", text("unrelated tooltip"));
        }
        _ => unreachable!(),
    }
    save_patch(bytes, &doc, writer, false)
}

fn mine(bytes: &[u8], d: Derived) -> Value {
    use Derived::*;
    let cos = open(bytes);
    let note = dictionary(&cos, annot(&cos, 2, "Text")).get(b"NM").unwrap().as_string().unwrap().to_text();
    let textbox = dictionary(&cos, annot(&cos, 2, "FreeText")).get(b"NM").unwrap().as_string().unwrap().to_text();
    match d {
        BlankSize => step("page_insert_blank", json!({"at":2})),
        Labels => step("page_number", json!({"from":2,"to":2,"style":"upper-roman"})),
        HeaderTotal => step("doc_header_footer", json!({"pages":[2],"footer_center":"<<1>> of <<n>>"})),
        HeaderNumber => step("doc_header_footer", json!({"pages":[2],"header_center":"<<1>>"})),
        Calculation => step("form_fill", json!({"values":{"F3":"15"}})),
        Format => step("form_fill", json!({"values":{"F2":"15"}})),
        DefaultName => step("form_add_field", json!({"page":2,"type":"text","rect":[20,300,100,320]})),
        RadioGroup => step("form_add_field", json!({"page":2,"type":"radio","group":"R","name":"R","export":"next","rect":[400,600,420,620]})),
        TabMove => step("form_tab_order", json!({"field":"F2","move":"later"})),
        TabSet => step("form_tab_order", json!({"pages":[2],"order":"row"})),
        Reply => step("comment_reply", json!({"id":note,"text":"mine"})),
        Status => step("comment_set_status", json!({"id":note,"status":"accepted"})),
        TextBox => step("comment_edit", json!({"id":textbox,"contents":"mine refits this text box"})),
        Distance | Perimeter | Area => step(
            match d {
                Distance => "measure_distance",
                Perimeter => "measure_perimeter",
                _ => "measure_area",
            },
            json!({"page":2,"points":if d==Distance {json!([[100,200],[200,200]])} else {json!([[100,200],[200,200],[200,300]])}}),
        ),
        Redact => step("redact_apply", json!({})),
        Urls => step("links_from_urls", json!({})),
        Structure => step("bookmark_from_structure", json!({})),
        Ocr => step("ocr_recognize", json!({"pages":[2]})),
        TextOverlap => step("text_edit", json!({"page":2,"line":1,"text":"Mine"})),
        FlattenComments => step("doc_flatten", json!({"comments":true,"fields":false})),
        FlattenFields => step("doc_flatten", json!({"comments":false,"fields":true})),
    }
}

// Each row holds the fields it replaces and the inputs its result reads. Other columns merge.
fn want(d: Derived, i: Input) -> Expected {
    use Derived::*;
    if d == Calculation && i == Input::Name {
        return R; // F3 still exists, now under Text1: the named address moved.
    }
    let held: &[Input] = match d {
        BlankSize => &[Input::NeighbourSize],
        Labels => &[Input::Labels],
        HeaderTotal => &[Input::Count],
        HeaderNumber => &[], // <<1>> is the physical page number, not its /PageLabels label.
        Calculation => &[Input::SourceValue, Input::Calculation, Input::Name], // same-field rule: F3 value
        Format => &[Input::SourceValue, Input::Calculation, Input::Format, Input::Name],
        DefaultName => &[Input::Name],
        RadioGroup => &[],         // appends a new widget; does not read an old widget's rect
        TabMove => &[Input::Tabs], // widget annotation order, not rectangle order
        TabSet => &[Input::Tabs],  // same-field rule: replaces /Tabs
        Reply | Status => &[Input::ParentRect, Input::ParentColor],
        TextBox => &[Input::BoxRect, Input::BoxWidth],
        Distance | Perimeter | Area => &[Input::Scale],
        Redact => &[Input::NeighbourSize, Input::Redaction],
        Structure => &[Input::Structure],
        // BoxWidth changes BS but retains the FreeText AP; it is not redrawn.
        // OtherComment changes a Text note's contents and regenerates the same
        // icon; note_icon never reads Contents.
        FlattenComments => &[Input::ParentRect, Input::ParentColor, Input::BoxRect, Input::Redaction],
        // The outside writer changes V alone, retaining the old AP/N. Flatten
        // draws that appearance and never regenerates a widget from its value.
        FlattenFields => &[Input::Group, Input::Tabs],
        Urls => &[Input::Url, Input::LinkRect],
        // The default skips this already-textual page. None
        // of these edits changes the nonempty-text predicate. Actual recognition
        // with skip_text_pages:false has different dependencies.
        Ocr => &[],
        TextOverlap => &[Input::Heading, Input::Overlap],
    };
    if held.contains(&i) { C } else { A }
}

async fn family(rows: &[Derived]) {
    let bytes = fixture();
    let mut bad = Vec::new();
    for &d in rows {
        let mut totals = Totals::default();
        for writer in WRITERS {
            for i in INPUTS {
                // Keep necessity R in want; the pinned API cannot resolve yesterday's name.
                if d == Derived::Calculation && i == Input::Name {
                    continue;
                }
                let theirs = mutate(&bytes, writer, i);
                cell(&bytes, &theirs, mine(&bytes, d), want(d, i), &format!("derived/{d:?}/{writer:?}/{i:?}"), &mut totals).await;
            }
        }
        println!("derived family {d:?}: {totals:?}");
        if !totals.clean() {
            bad.push(format!("{d:?}: {totals:?}"));
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}
#[tokio::test(flavor = "multi_thread")]
async fn pages_and_forms() {
    family(&ROWS[..10]).await;
}
#[tokio::test(flavor = "multi_thread")]
async fn comments_and_measurements() {
    family(&ROWS[10..16]).await;
}
#[tokio::test(flavor = "multi_thread")]
async fn computed_content() {
    family(&ROWS[16..]).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn waiting_for_sdk_renamed_calculation_target() {
    let bytes = fixture();
    assert_eq!(want(Derived::Calculation, Input::Name).reason, "relocated");
    for writer in WRITERS {
        let f = Fixture::from_bytes(&bytes, Default::default());
        let (_, basis) = f.inspect().await;
        let theirs = mutate(&bytes, writer, Input::Name);
        std::fs::write(f.file(), &theirs).unwrap();
        let refused = f.refused("run", json!({"basis":basis,"steps":[mine(&bytes,Derived::Calculation)]})).await;
        assert_eq!(reasons(&refused), "changed");
        assert!(refused["conflicts"].as_array().unwrap().iter().all(|c| c["id"] == "pdf"));
        assert_eq!(std::fs::read(f.file()).unwrap(), theirs);
        f.shutdown().await;
    }
    println!("waiting_for_sdk: 2 cells; necessity relocated, current calculation target fact changed");
}

async fn pairs(pairs: &[(Derived, Input)]) {
    let bytes = fixture();
    let mut totals = Totals::default();
    for writer in WRITERS {
        for &(d, i) in pairs {
            let theirs = mutate(&bytes, writer, i);
            cell(&bytes, &theirs, mine(&bytes, d), want(d, i), &format!("regression/{d:?}/{writer:?}/{i:?}"), &mut totals).await;
        }
    }
    assert!(totals.clean(), "{totals:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn regression_same_field() {
    pairs(&[(Derived::Calculation, Input::SourceValue), (Derived::TabSet, Input::Tabs)]).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn regression_forms() {
    pairs(&[
        (Derived::Calculation, Input::Calculation),
        (Derived::Format, Input::SourceValue),
        (Derived::TabSet, Input::Group),
        (Derived::Calculation, Input::OtherComment),
    ])
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn regression_content() {
    pairs(&[
        (Derived::Redact, Input::OtherTooltip),
        (Derived::Structure, Input::OtherComment),
        (Derived::FlattenComments, Input::Property),
        (Derived::FlattenFields, Input::OtherComment),
        (Derived::Ocr, Input::Pixels),
        (Derived::Ocr, Input::Tabs),
    ])
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn regression_c8_fill_inputs() {
    pairs(&[(Derived::Calculation, Input::SourceValue), (Derived::Calculation, Input::Calculation), (Derived::Format, Input::SourceValue)]).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn regression_c8_radio_tabs() {
    pairs(&[
        (Derived::RadioGroup, Input::Group),
        (Derived::TabMove, Input::Group),
        (Derived::TabSet, Input::Group),
        (Derived::TabSet, Input::Tabs),
        (Derived::TabMove, Input::Tabs),
    ])
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn regression_c8_flatten_inputs() {
    pairs(&[
        (Derived::FlattenComments, Input::BoxWidth),
        (Derived::FlattenComments, Input::OtherComment),
        (Derived::FlattenFields, Input::SourceValue),
        (Derived::FlattenComments, Input::BoxRect),
        (Derived::FlattenFields, Input::Group),
    ])
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn regression_c8_ocr_skip() {
    pairs(&[(Derived::Ocr, Input::Url), (Derived::Ocr, Input::Heading), (Derived::Ocr, Input::Pixels), (Derived::Ocr, Input::Overlap)]).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn regression_c8_appended_blank() {
    pairs(&[(Derived::Labels, Input::Count), (Derived::Urls, Input::Count), (Derived::Labels, Input::Labels), (Derived::Urls, Input::Url)]).await;
}

/// The original address representative was doc_flatten, later replaced by
/// doc_protect. Keep its combined comments+fields form exercised, including a
/// merge that the replacement's blanket refusal would conceal.
#[tokio::test(flavor = "multi_thread")]
async fn original_whole_flatten_has_scoped_dependencies() {
    let bytes = fixture();
    let mut totals = Totals::default();
    for writer in WRITERS {
        for (input, expected) in [(Input::Property, A), (Input::Pixels, A), (Input::BoxRect, C), (Input::Group, C)] {
            let theirs = mutate(&bytes, writer, input);
            cell(&bytes, &theirs, step("doc_flatten", json!({})), expected, &format!("original-flatten/{writer:?}/{input:?}"), &mut totals).await;
        }
    }
    println!("original flatten supplement: {totals:?}");
    assert!(totals.clean(), "{totals:?}");
}

/// Recognition really reads pixels when explicitly requested on a textual page.
/// This is distinct from the matrix's default, which skips that page.
#[tokio::test(flavor = "multi_thread")]
async fn recognition_without_skipping_holds_its_image_inputs() {
    let bytes = fixture();
    let mut totals = Totals::default();
    for writer in WRITERS {
        for input in [Input::Pixels, Input::Heading, Input::Overlap, Input::Url] {
            let theirs = mutate(&bytes, writer, input);
            cell(
                &bytes,
                &theirs,
                step("ocr_recognize", json!({"pages":[2],"skip_text_pages":false})),
                C,
                &format!("recognition/{writer:?}/{input:?}"),
                &mut totals,
            )
            .await;
        }
    }
    println!("recognition supplement: {totals:?}");
    assert!(totals.clean(), "{totals:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn regression_local_text() {
    pairs(&[(Derived::Urls, Input::Heading), (Derived::Urls, Input::Overlap), (Derived::TextOverlap, Input::Url)]).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn kit_defect_appended_blank_page_changes_neither_labels_nor_url_candidates() {
    pairs(&[(Derived::Labels, Input::Count), (Derived::Urls, Input::Count)]).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn saved_calculated_merge_renders() {
    let bytes = fixture();
    let theirs = mutate(&bytes, Writer::Lopdf, Input::OtherComment);
    let write = mine(&bytes, Derived::Format);
    let f = Fixture::from_bytes(&bytes, Default::default());
    let (_, basis) = f.inspect().await;
    std::fs::write(f.file(), &theirs).unwrap();
    f.ok("run", json!({"basis":basis,"steps":[write.clone()]})).await;
    let mut actual = on_disk(&f.file());
    let fields = actual.execute("form_fields", json!({})).unwrap();
    assert!(fields["fields"].as_array().unwrap().iter().any(|v| v["name"] == "F4" && v["value"] == "17"), "{fields}");
    assert_eq!(actual.execute("page_render", json!({"page":2,"dpi":72})).unwrap(), json!({"image":{"width":612,"height":792}}));
    let area = actual.inspect().unwrap()["pages"][1]["area"].clone();
    let render = |doc: &kition_pdf_kit::PdfDocument| doc.render(area["x"].as_f64().unwrap(), area["y"].as_f64().unwrap(), 1.0, 612, 792).unwrap();
    let pixels = render(&actual);
    assert_eq!(pixels.len(), 612 * 792 * 4);
    assert!(pixels.as_chunks::<4>().0.iter().filter(|p| p[0] < 240 || p[1] < 240 || p[2] < 240).count() > 1000);
    let mut expected = kition_pdf_kit::PdfDocument::open(FILE, &theirs).unwrap();
    expected.execute(write["command"].as_str().unwrap(), write["params"].clone()).unwrap();
    assert_eq!(pixels, render(&expected));
    if let Ok(path) = std::env::var("C8_PDF_4_RENDER") {
        std::fs::write(path, &pixels).unwrap();
    }
    f.shutdown().await;
}

#[test]
fn outside_inputs_really_change_the_mark_and_widget() {
    let bytes = fixture();
    for writer in WRITERS {
        let changed = mutate(&bytes, writer, Input::Redaction);
        let comments = kition_pdf_kit::PdfDocument::open(FILE, &changed).unwrap().execute("comment_list", json!({})).unwrap();
        assert!(
            comments["comments"].as_array().unwrap().iter().any(|c| c["id"] == "colleague-redaction" && c["page"] == 3),
            "{writer:?}: {comments}"
        );
        let changed = open(&mutate(&bytes, writer, Input::Group));
        let fields = kition_pdf_forms::fields(&changed);
        let widget = fields.iter().find(|f| f.name == "R").unwrap().widgets[0].obj;
        assert_eq!(dictionary(&changed, widget).get(b"Rect"), Some(&array(&[360, 100, 380, 120])));
        let changed = open(&mutate(&bytes, writer, Input::Count));
        assert_eq!(dictionary(&changed, page(&changed, 5)).name(b"Type"), Some(b"Page".as_slice()));
    }
}
