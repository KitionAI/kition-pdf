//! Type 3 fonts drawn from a document face's glyph outlines: how text the standard fonts can't
//! draw (Chinese, say) is written into a PDF. The font carries the outline of each character it
//! shows, so the page draws the same everywhere without embedding the face's font program, and a
//! ToUnicode map keeps the text copyable and searchable.

use kition_pdf_cos::{Dict, Document, ObjRef, Object, PdfString, Stream};

use crate::{DocumentFace, GlyphOutline};

/// The most characters one Type 3 font holds (its codes are one byte, from 1).
pub const TYPE3_GLYPHS: usize = 240;

fn n(v: f64) -> String {
    let s = format!("{:.3}", if v.abs() < 5e-4 { 0.0 } else { v });
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

/// A Type 3 font with `chars` as codes 1…, each drawn from the outline `glyph` gives for it (a
/// character without one is left out), named after `face`. At most [`TYPE3_GLYPHS`] characters
/// are meant to go in one font; codes past 255 are not written.
pub fn type3_font<'a>(doc: &mut Document, face: &DocumentFace, chars: &[char], glyph: impl Fn(char) -> Option<&'a GlyphOutline>) -> ObjRef {
    let mut charprocs = Dict::new();
    let mut differences = vec![Object::Int(1)];
    let mut widths = Vec::with_capacity(chars.len());
    let mut bbox = [0.0f64; 4];
    let mut to_unicode = Vec::new();
    let count = chars.len().min(255);
    for (k, c) in chars.iter().take(count).enumerate() {
        let code = k + 1;
        let Some(g) = glyph(*c) else { continue };
        let mut paths = Vec::new();
        for contour in &g.contours {
            for (j, p) in contour.iter().enumerate() {
                paths.extend(format!("{} {} {}\n", n(p[0] * 1000.0), n(p[1] * 1000.0), if j == 0 { "m" } else { "l" }).bytes());
            }
            paths.extend_from_slice(b"h\n");
        }
        let [x0, y0, x1, y1] = if g.contours.is_empty() {
            [0.0; 4]
        } else {
            [(g.bbox[0] * 1000.0).floor(), (g.bbox[1] * 1000.0).floor(), (g.bbox[2] * 1000.0).ceil(), (g.bbox[3] * 1000.0).ceil()]
        };
        bbox = [bbox[0].min(x0), bbox[1].min(y0), bbox[2].max(x1), bbox[3].max(y1)];
        let mut proc = format!("{} 0 {} {} {} {} d1\n", n(g.width * 1000.0), n(x0), n(y0), n(x1), n(y1)).into_bytes();
        if !paths.is_empty() {
            proc.extend(paths);
            proc.extend_from_slice(b"f\n");
        }
        let name = format!("g{code:02X}");
        charprocs.set(name.clone().into_bytes(), Object::Ref(doc.add(Object::Stream(Stream::flate(Dict::new(), &proc)))));
        differences.push(Object::name(&name));
        widths.push(Object::Real((g.width * 1000.0).round()));
        let hex: String = c.encode_utf16(&mut [0u16; 2]).iter().map(|u| format!("{u:04X}")).collect();
        to_unicode.push(format!("<{code:02X}> <{hex}>\n"));
    }
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CMapType 2 def\n1 begincodespacerange\n<01> <FF>\nendcodespacerange\n",
    );
    for block in to_unicode.chunks(100) {
        cmap.push_str(&format!("{} beginbfchar\n{}endbfchar\n", block.len(), block.concat()));
    }
    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    let cmap = doc.add(Object::Stream(Stream::flate(Dict::new(), cmap.as_bytes())));
    let family = face.face.family;
    let mut descriptor = Dict::new();
    descriptor.set(b"Type".to_vec(), Object::name("FontDescriptor"));
    descriptor.set(b"FontName".to_vec(), Object::name(&face.font_name()));
    descriptor.set(b"FontFamily".to_vec(), Object::String(PdfString::literal(family.as_bytes().to_vec())));
    descriptor.set(b"Flags".to_vec(), Object::Int(4));
    descriptor.set(b"ItalicAngle".to_vec(), Object::Int(0));
    let mut encoding = Dict::new();
    encoding.set(b"Type".to_vec(), Object::name("Encoding"));
    encoding.set(b"Differences".to_vec(), Object::Array(differences));
    let mut font = Dict::new();
    font.set(b"Type".to_vec(), Object::name("Font"));
    font.set(b"Subtype".to_vec(), Object::name("Type3"));
    font.set(b"FontDescriptor".to_vec(), Object::Ref(doc.add(Object::Dict(descriptor))));
    font.set(b"FontBBox".to_vec(), Object::Array(bbox.iter().map(|v| Object::Real(*v)).collect()));
    font.set(
        b"FontMatrix".to_vec(),
        Object::Array(vec![Object::Real(0.001), Object::Int(0), Object::Int(0), Object::Real(0.001), Object::Int(0), Object::Int(0)]),
    );
    font.set(b"FirstChar".to_vec(), Object::Int(1));
    font.set(b"LastChar".to_vec(), Object::Int(count as i64));
    font.set(b"Widths".to_vec(), Object::Array(widths));
    font.set(b"Encoding".to_vec(), Object::Dict(encoding));
    font.set(b"CharProcs".to_vec(), Object::Dict(charprocs));
    font.set(b"ToUnicode".to_vec(), Object::Ref(cmap));
    doc.add(Object::Dict(font))
}
