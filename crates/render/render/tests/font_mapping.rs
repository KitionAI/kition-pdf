//! Synthetic embedded CID mapping observations, with strict format controls.
#[path = "support/font_mapping_fixtures.rs"]
mod fixtures;
use hayro::hayro_interpret::{
    self as interpret, BlendMode, ClipPath, Context, Device, GlyphDrawMode, Image, InterpreterCache, InterpreterSettings, Paint, PathDrawMode,
    SoftMask, font::Glyph,
};
use hayro_syntax::Pdf;
use kition_pdf_render::{PageRenderer, RenderConfig, RenderRequest};
use kurbo::{Affine, BezPath, Rect, Shape};
use skrifa::{
    FontRef, GlyphId, MetadataProvider,
    instance::{LocationRef, Size},
    outline::{DrawSettings, OutlinePen},
    raw::{
        TableProvider,
        ps::{cff::CffFontRef, string::Sid},
    },
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Default)]
struct Pen(BezPath);
impl OutlinePen for Pen {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.move_to((f64::from(x), f64::from(y)));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.line_to((f64::from(x), f64::from(y)));
    }
    fn quad_to(&mut self, x: f32, y: f32, x1: f32, y1: f32) {
        self.0.quad_to((f64::from(x), f64::from(y)), (f64::from(x1), f64::from(y1)));
    }
    fn curve_to(&mut self, x: f32, y: f32, x1: f32, y1: f32, x2: f32, y2: f32) {
        self.0.curve_to((f64::from(x), f64::from(y)), (f64::from(x1), f64::from(y1)), (f64::from(x2), f64::from(y2)));
    }
    fn close(&mut self) {
        self.0.close_path();
    }
}
fn check_shape(path: &BezPath, gid: u32) {
    let expected = match gid {
        1 => Rect::new(100.0, 0.0, 300.0, 700.0),
        2 => Rect::new(500.0, 0.0, 900.0, 700.0),
        _ => Rect::ZERO,
    };
    assert_eq!(path.bounding_box(), expected, "glyph {gid} authored bounds");
    assert_eq!(path.contains((200.0, 350.0).into()), gid == 1);
    assert_eq!(path.contains((600.0, 350.0).into()), gid == 2);
    assert_eq!(path.contains((800.0, 100.0).into()), gid == 2);
    assert!(!path.contains((800.0, 350.0).into()), "the L counter remains empty");
}

#[test]
fn original_font_programs_parse_with_complete_tables_known_charsets_and_geometry() {
    for cid in [false, true] {
        for swapped in [false, true] {
            let raw = fixtures::programs::cff(cid, swapped);
            let font = CffFontRef::new(&raw, 0, Some(1000)).unwrap();
            assert_eq!(font.is_cid(), cid);
            assert_eq!(font.num_glyphs(), 3);
            let charset = font.charset().unwrap();
            if cid {
                assert_eq!(charset.glyph_id(Sid::new(1)).unwrap().to_u32(), if swapped { 2 } else { 1 });
                assert_eq!(charset.glyph_id(Sid::new(2)).unwrap().to_u32(), if swapped { 1 } else { 2 });
                assert!(!charset.iter().any(|(_, sid)| sid.to_u16() == 3));
                // The upstream lookup may scan format-0 trailing data; bounded iteration
                // defines the actual authored charset independently of that lookup quirk.
            }
            let subfont = font.subfont(0, &[]).unwrap();
            for gid in 0..3 {
                let mut path = Pen::default();
                font.draw(&subfont, GlyphId::new(gid), &[], Some(1000.0), &mut path).unwrap();
                check_shape(&path.0, gid);
            }
            let sfnt = fixtures::programs::opentype(Some(raw.clone()));
            let parsed = FontRef::new(&sfnt).unwrap();
            assert_eq!(parsed.cff().unwrap().offset_data().as_ref(), font.data());
            check_sfnt(&parsed);
        }
    }
    let bytes = fixtures::programs::opentype(None);
    let parsed = FontRef::new(&bytes).unwrap();
    check_sfnt(&parsed);
    assert!(parsed.glyf().is_ok() && parsed.loca(None).is_ok());
    for gid in 1..3 {
        let mut path = Pen::default();
        parsed
            .outline_glyphs()
            .get(GlyphId::new(gid))
            .unwrap()
            .draw(DrawSettings::unhinted(Size::new(1000.0), LocationRef::default()), &mut path)
            .unwrap();
        check_shape(&path.0, gid);
    }
}
fn check_sfnt(font: &FontRef<'_>) {
    assert_eq!(font.head().unwrap().units_per_em(), 1000);
    assert_eq!(font.maxp().unwrap().num_glyphs(), 3);
    assert_eq!(font.hhea().unwrap().number_of_h_metrics(), 3);
    assert!(font.hmtx().is_ok() && font.post().is_ok() && font.cmap().is_ok() && font.name().is_ok() && font.os2().is_ok());
    assert_eq!(font.charmap().map('A').unwrap().to_u32(), 1);
    assert_eq!(font.charmap().map('B').unwrap().to_u32(), 2);
}

#[derive(Default)]
struct Trace(Vec<(u32, Vec<u8>, BezPath)>);
impl<'a> Device<'a> for Trace {
    fn set_soft_mask(&mut self, _: Option<SoftMask<'a>>) {}
    fn set_blend_mode(&mut self, _: BlendMode) {}
    fn draw_path(&mut self, _: &BezPath, _: Affine, _: &Paint<'a>, _: &PathDrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_glyph(&mut self, glyph: &Glyph<'a>, _: Affine, _: Affine, _: &Paint<'a>, _: &GlyphDrawMode) {
        let Glyph::Outline(glyph) = glyph else { panic!("original embedded font must produce an outline glyph") };
        self.0.push((glyph.glyph_id().to_u32(), glyph.font_data().unwrap().data.as_ref().as_ref().to_vec(), glyph.outline()));
    }
    fn draw_image(&mut self, _: Image<'a, '_>, _: Affine) {}
    fn pop_clip_path(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}

fn trace_pdf(bytes: Vec<u8>, id: &str) -> Trace {
    let pdf = Pdf::new(bytes).unwrap();
    let fallback = Arc::new(AtomicUsize::new(0));
    let calls = fallback.clone();
    let settings = InterpreterSettings {
        font_resolver: Arc::new(move |_| {
            calls.fetch_add(1, Ordering::Relaxed);
            None
        }),
        ..Default::default()
    };
    let cache = InterpreterCache::new();
    let mut context = Context::new(Affine::IDENTITY, Rect::new(0.0, 0.0, 40.0, 40.0), &cache, pdf.xref(), settings);
    let mut trace = Trace::default();
    interpret::interpret_page(&pdf.pages()[0], &mut context, &mut trace);
    assert_eq!(fallback.load(Ordering::Relaxed), 0, "{}: embedded program must load without substitution", id);
    trace
}

#[test]
fn embedded_mapping_controls_and_separately_labeled_compatibility_observations() {
    let cases = fixtures::cases();
    fixtures::export(&cases);
    let mut failures = Vec::new();
    for case in &cases {
        let bytes = fixtures::pdf(case);
        let trace = trace_pdf(bytes.clone(), case.id);
        assert_eq!(trace.0.len(), 1, "{}: one original character", case.id);
        let (gid, program, path) = &trace.0[0];
        assert_eq!(program, &case.font, "{}: selected embedded bytes", case.id);
        check_shape(path, if *gid < 3 { *gid } else { 0 });
        if case.code >= 3 {
            assert!(path.elements().is_empty(), "{}: an absent glyph must remain empty", case.id);
        }
        let matches = *gid == case.expected_gid;
        if case.assert_mapping && !matches {
            failures.push(format!("{}: gid={gid}, expected={}", case.id, case.expected_gid));
        }
        println!(
            "font_mapping case={} class={} actual_gid={gid} candidate_gid={} candidate_matches={matches} asserted={} fallback_calls=0 embedded_bytes_match=true",
            case.id, case.classification, case.expected_gid, case.assert_mapping
        );
        for scale in [1_u32, 2] {
            let page = PageRenderer::new(Arc::new(bytes.clone()), RenderConfig::default())
                .render(RenderRequest { scale: scale as f32, ..Default::default() });
            assert!(page.error.is_none() && page.warnings.is_empty(), "{}: {:?} {:?}", case.id, page.error, page.warnings);
            assert_eq!((page.width, page.height), (40 * scale, 40 * scale));
            for (rect, ink) in [([13_u32, 12, 2, 10], *gid == 1), ([21, 15, 2, 8], *gid == 2), ([25, 11, 2, 2], *gid == 2), ([25, 16, 2, 6], false)] {
                for py in rect[1] * scale..(rect[1] + rect[3]) * scale {
                    for px in rect[0] * scale..(rect[0] + rect[2]) * scale {
                        let at = (((40 * scale - 1 - py) * page.width + px) * 4) as usize;
                        let expected = if ink { 0 } else { 255 };
                        assert_eq!(&page.rgba[at..at + 4], &[expected, expected, expected, 255], "{} scale={scale} interior=({px},{py})", case.id);
                    }
                }
            }
            let sentinel = (((38 * scale) * page.width + scale) * 4) as usize;
            assert_eq!(&page.rgba[sentinel..sentinel + 4], &[255, 0, 0, 255], "{}: continuation", case.id);
            println!("font_raster case={} scale={scale} observed_gid={gid} authored_geometry_matches=true", case.id);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("; "));
}

#[test]
fn opentype_cid_charset_lookup_beyond_charstrings_selects_notdef() {
    let mut case = fixtures::cases().into_iter().find(|case| case.id == "opentype-cid-missing-cid").unwrap();
    // This CID is absent from the bounded charset, but the parser's reverse lookup
    // can find the value 3 in the following CharStrings INDEX header. It must not
    // become an out-of-range glyph ID in the new OpenType mapping path.
    case.code = 3;
    let trace = trace_pdf(fixtures::pdf(&case), case.id);
    assert_eq!(trace.0.len(), 1);
    assert_eq!(trace.0[0].0, 0);
    assert_eq!(trace.0[0].1, case.font);
    assert!(trace.0[0].2.elements().is_empty());
}

#[test]
fn unselected_file3_does_not_enable_opentype_cid_charset_mapping() {
    let mut case = fixtures::cases().into_iter().find(|case| case.id == "descriptor-file2-precedence").unwrap();
    // A deliberately malformed dual-program descriptor must use the provenance
    // of the selected FontFile2, even when a FontFile3/OpenType entry also exists.
    case.descendant = "CIDFontType0";
    case.font = fixtures::programs::opentype(Some(fixtures::programs::cff(true, true)));
    case.secondary = Some(fixtures::programs::opentype(None));
    let trace = trace_pdf(fixtures::pdf(&case), case.id);
    assert_eq!(trace.0.len(), 1);
    assert_eq!(trace.0[0].0, 1);
    assert_eq!(trace.0[0].1, case.font);
    check_shape(&trace.0[0].2, 1);
}

#[path = "support/cff_bounds.rs"]
mod cff_bounds;
