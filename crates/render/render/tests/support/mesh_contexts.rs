//! Contributor-original paint contexts for transparent off-page mesh controls.
use lopdf::{Object, Stream, dictionary};

pub fn pdf(source: &[u8], context: &str, mesh: bool) -> Vec<u8> {
    let mut doc = lopdf::Document::load_mem(source).unwrap();
    let shading = if mesh { "/Sh0 sh" } else { "" };
    let resources = dictionary! { "Shading" => dictionary! { "Sh0" => (5, 0) } };
    let mut page_resources = resources.clone();
    let paint = match context {
        "root" => shading.to_owned(),
        "multiply" => {
            page_resources.set("ExtGState", dictionary! { "GS" => dictionary! { "BM" => "Multiply" } });
            format!("q /GS gs {shading} Q")
        }
        "mask" => {
            let mask = doc.add_object(Stream::new(
                dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 40.into(), 40.into()],
                "Group" => dictionary! { "S" => "Transparency", "CS" => "DeviceGray" } },
                b"0.5 g 0 0 40 40 re f".to_vec(),
            ));
            page_resources.set("ExtGState", dictionary! { "GS" => dictionary! { "SMask" => dictionary! { "S" => "Alpha", "G" => mask } } });
            format!("q /GS gs {shading} Q")
        }
        "group" => {
            let group = doc.add_object(Stream::new(
                dictionary! { "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 40.into(), 40.into()],
                "Resources" => resources, "Group" => dictionary! { "S" => "Transparency", "I" => true, "K" => true } },
                shading.as_bytes().to_vec(),
            ));
            page_resources.set("XObject", dictionary! { "Group" => group });
            "/Group Do".to_owned()
        }
        "type3-fill" | "type3-stroke" => {
            let glyph = doc.add_object(Stream::new(dictionary! {}, format!("40 0 d0 {shading}").into_bytes()));
            let font = doc.add_object(dictionary! {
                "Type" => "Font", "Subtype" => "Type3", "FontBBox" => vec![0.into(), 0.into(), 40.into(), 40.into()],
                "FontMatrix" => vec![1.into(), 0.into(), 0.into(), 1.into(), 0.into(), 0.into()],
                "CharProcs" => dictionary! { "A" => glyph }, "Encoding" => dictionary! { "Type" => "Encoding", "Differences" => vec![65.into(), Object::Name(b"A".to_vec())] },
                "FirstChar" => 65, "LastChar" => 65, "Widths" => vec![40.into()], "Resources" => resources,
            });
            page_resources.set("Font", dictionary! { "F0" => font });
            format!("BT /F0 1 Tf {} Tr (A) Tj ET", if context == "type3-stroke" { 1 } else { 0 })
        }
        "pattern" => {
            let pattern = doc.add_object(Stream::new(
                dictionary! { "Type" => "Pattern", "PatternType" => 1, "PaintType" => 1, "TilingType" => 1,
                "BBox" => vec![0.into(), 0.into(), 40.into(), 40.into()], "XStep" => 40, "YStep" => 40, "Resources" => resources },
                shading.as_bytes().to_vec(),
            ));
            page_resources.set("Pattern", dictionary! { "P0" => pattern });
            "q /Pattern cs /P0 scn 0 0 40 40 re f Q".to_owned()
        }
        _ => panic!("unknown authored context"),
    };
    doc.get_object_mut((3, 0)).unwrap().as_dict_mut().unwrap().set("Resources", page_resources);
    doc.get_object_mut((4, 0))
        .unwrap()
        .as_stream_mut()
        .unwrap()
        .set_content(format!("q 0.25 0.5 0.75 rg 0 0 40 40 re f Q q 5 5 30 30 re W n {paint} Q 1 0 0 rg 0 0 4 4 re f").into_bytes());
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}
