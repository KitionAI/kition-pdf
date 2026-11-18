//! Original ASCII Type3 geometry and independent page-local expected search boxes.

use serde_json::{Value, json};
use std::{fmt::Write, path::PathBuf, sync::OnceLock};

pub const FULL: [f32; 4] = [0.0, 0.0, 240.0, 200.0];
pub const CROP: [f32; 4] = [20.0, 30.0, 220.0, 190.0];

#[derive(Clone, Copy, Debug)]
pub struct Page {
    pub rotation: u32,
    pub crop: [f32; 4],
}

pub fn pages() -> Vec<Page> {
    [FULL, CROP].into_iter().flat_map(|crop| [0, 90, 180, 270].map(|rotation| Page { rotation, crop })).collect()
}

impl Page {
    pub fn size(self) -> [f32; 2] {
        let [left, bottom, right, top] = self.crop;
        if self.rotation.is_multiple_of(180) { [right - left, top - bottom] } else { [top - bottom, right - left] }
    }

    /// Transform authored PDF corners using elementary clockwise quarter turns.
    /// No renderer/inspector/extracted rectangle participates in this oracle.
    pub fn view_rect(self, [x0, y0, x1, y1]: [f32; 4]) -> [f32; 4] {
        let [left, bottom, right, top] = self.crop;
        let (width, height) = (right - left, top - bottom);
        let corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]].map(|[x, y]| {
            let (x, y) = (x - left, top - y);
            match self.rotation {
                0 => [x, y],
                90 => [height - y, x],
                180 => [width - x, height - y],
                270 => [y, width - x],
                _ => panic!("fixture rotation must be a quarter turn"),
            }
        });
        corners.into_iter().fold([f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY], |r, [x, y]| {
            [r[0].min(x), r[1].min(y), r[2].max(x), r[3].max(y)]
        })
    }

    pub fn rects(self, partial: bool) -> Vec<[f32; 4]> {
        // Font size 20, width 600, matrix 0.001: each painted glyph is 12 by 20 pt.
        // AB baseline (50,140); CD baseline (50,100); ink extends -4..16 pt in y.
        let boxes =
            if partial { [[62.0, 136.0, 74.0, 156.0], [50.0, 96.0, 62.0, 116.0]] } else { [[50.0, 136.0, 74.0, 156.0], [50.0, 96.0, 74.0, 116.0]] };
        boxes.into_iter().map(|rect| self.view_rect(rect)).collect()
    }
}

fn stream(content: &str) -> String {
    format!("<< /Length {} >>\nstream\n{content}endstream", content.len())
}

pub fn pdf() -> Vec<u8> {
    let cases = pages();
    let kids = (7..7 + cases.len()).map(|id| format!("{id} 0 R")).collect::<Vec<_>>().join(" ");
    let cmap = "/CIDInit /ProcSet findresource begin\n12 dict begin begincmap\n/CIDSystemInfo << /Registry (Kition) /Ordering (Original) /Supplement 0 >> def\n/CMapName /OriginalSearchGeometry def /CMapType 2 def\n1 begincodespacerange <00> <FF> endcodespacerange\n4 beginbfchar <01> <0041> <02> <0042> <03> <0043> <04> <0044> endbfchar\nendcmap CMapName currentdict /CMap defineresource pop end end\n";
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        format!("<< /Type /Pages /Count {} /Kids [{kids}] >>", cases.len()),
        "<< /Type /Font /Subtype /Type3 /FontBBox [0 -200 600 800] /FontMatrix [0.001 0 0 0.001 0 0] /FirstChar 1 /LastChar 4 /Widths [600 600 600 600] /Encoding << /Type /Encoding /Differences [1 /a /b /c /d] >> /CharProcs << /a 4 0 R /b 4 0 R /c 4 0 R /d 4 0 R >> /ToUnicode 5 0 R /Resources << >> >>".to_string(),
        stream("600 0 0 -200 600 800 d1\n0 -200 600 1000 re f\n"),
        stream(cmap),
        stream("0 g\nBT /F1 20 Tf 1 0 0 1 50 140 Tm <0102> Tj 1 0 0 1 50 100 Tm <0304> Tj ET\n"),
    ];
    for page in cases {
        let [left, bottom, right, top] = page.crop;
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 200] /CropBox [{left} {bottom} {right} {top}] /Rotate {} /Resources << /Font << /F1 3 0 R >> >> /Contents 6 0 R >>", page.rotation));
    }
    let mut out = String::from("%PDF-1.7\n");
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        writeln!(out, "{} 0 obj\n{object}\nendobj", index + 1).unwrap();
    }
    let xref = out.len();
    writeln!(out, "xref\n0 {}\n0000000000 65535 f ", objects.len() + 1).unwrap();
    for offset in offsets {
        writeln!(out, "{offset:010} 00000 n ").unwrap();
    }
    writeln!(out, "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF", objects.len() + 1).unwrap();
    assert!(out.is_ascii());
    out.into_bytes()
}

pub fn manifest() -> Value {
    json!({
        "schema": 1, "file": "search-geometry.pdf", "origin": "Contributor-original ASCII Type3 geometry, Apache-2.0",
        "coordinateSpace": "page-local-top-left-points", "rectangleFormat": "x0,y0,x1,y1", "tolerancePoints": 0.01,
        "glyphGeometry": { "fontMatrixScale": 0.001, "fontSize": 20, "advanceUnits": 600, "paintedBoxUnits": [0,-200,600,800] },
        "pages": pages().into_iter().enumerate().map(|(index,page)| {
            let [width,height] = page.size();
            json!({"page": index+1, "rotation": page.rotation, "mediaBox": FULL, "cropBox": page.crop,
                "width": width, "height": height, "text": "AB\nCD",
                "queries": [{"query":"AB CD","rects":page.rects(false)},{"query":"B C","rects":page.rects(true)}]})
        }).collect::<Vec<_>>()
    })
}

pub fn export() {
    static EXPORTED: OnceLock<()> = OnceLock::new();
    EXPORTED.get_or_init(|| {
        let Some(requested) = std::env::var_os("KITION_PDF_SEARCH_GEOMETRY_OUT") else { return };
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..").canonicalize().unwrap();
        let allowed = root.join("target/search-geometry-2026-10-22");
        assert_eq!(PathBuf::from(requested), allowed, "export only to the dedicated ignored search fixture directory");
        std::fs::create_dir_all(&allowed).unwrap();
        assert_eq!(allowed.canonicalize().unwrap(), allowed, "fixture directory must not redirect outside this checkout");
        for (name, bytes) in [("search-geometry.pdf", pdf()), ("manifest.json", serde_json::to_vec_pretty(&manifest()).unwrap())] {
            let target = allowed.join(name);
            if target.exists() {
                assert_eq!(std::fs::read(target).unwrap(), bytes, "refuse to replace different search fixture evidence");
            } else {
                std::fs::write(target, bytes).unwrap();
            }
        }
    });
}
