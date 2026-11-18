//! Contributor-original raw-CFF bounds controls; no frozen font matrix is changed.
use super::{check_shape, fixtures, trace_pdf};
use fixtures::{Case, programs};
use kition_pdf_render::{PageRenderer, RenderConfig, RenderRequest};
use skrifa::{
    GlyphId,
    raw::ps::{cff::CffFontRef, string::Sid},
};
use std::{path::PathBuf, sync::Arc};

struct Control {
    case: Case,
    format: &'static str,
    notdef_shape: u32,
    expected_shape: u32,
}

fn control(id: &'static str, charset: &[u8], format: &'static str, code: u16, gid: u32) -> Control {
    let mut case = fixtures::cases().into_iter().find(|case| case.id == "raw-cid-swapped-charset").unwrap();
    case.id = id;
    case.font = programs::cff_with_charset(true, charset, programs::RIGHT);
    case.code = code;
    case.expected_gid = gid;
    case.expectation_basis = "bounded CID charset selects authored GID or visible .notdef";
    Control { case, format, notdef_shape: 2, expected_shape: if gid == 0 { 2 } else { gid } }
}

fn controls() -> Vec<Control> {
    let mut all = Vec::new();
    for (valid, missing, format, charset, gid) in [
        ("raw-cff-f0-identity", "raw-cff-f0-identity-missing", "0", vec![0, 0, 1, 0, 2], 1),
        ("raw-cff-f0-swapped", "raw-cff-f0-swapped-missing", "0", vec![0, 0, 2, 0, 1], 2),
        ("raw-cff-f1-identity", "raw-cff-f1-identity-missing", "1", vec![1, 0, 1, 1], 1),
        ("raw-cff-f1-swapped", "raw-cff-f1-swapped-missing", "1", vec![1, 0, 2, 0, 0, 1, 0], 2),
        ("raw-cff-f2-identity", "raw-cff-f2-identity-missing", "2", vec![2, 0, 1, 0, 1], 1),
        ("raw-cff-f2-swapped", "raw-cff-f2-swapped-missing", "2", vec![2, 0, 2, 0, 0, 0, 1, 0, 0], 2),
    ] {
        all.push(control(valid, &charset, format, 1, gid));
        all.push(control(missing, &charset, format, 3, 0));
    }
    all.push(control("raw-cff-f0-missing-max", &[0, 0, 2, 0, 1], "0", u16::MAX, 0));
    // The absent CID first occurs at zero-based entry 65536 in original padding.
    // A reverse lookup truncating that index to u16 aliases it to valid GID 1.
    let mut wrapped = vec![0, 0, 2, 0, 1];
    wrapped.resize(1 + 65_536 * 2, 0);
    wrapped.extend_from_slice(&u16::MAX.to_be_bytes());
    all.push(control("raw-cff-f0-wrapped-missing", &wrapped, "0", u16::MAX, 0));
    let mut otf = control("opentype-cff-f0-wrapped-missing", &wrapped, "0", u16::MAX, 0);
    otf.case.font = programs::opentype(Some(otf.case.font));
    otf.case.font_kind = "opentype-cid-cff";
    otf.case.stream_subtype = "OpenType";
    all.push(otf);
    let mut empty = control("raw-cff-f0-empty-notdef", &[0, 0, 2, 0, 1], "0", 3, 0);
    empty.case.font = programs::cff(true, true);
    empty.notdef_shape = 0;
    empty.expected_shape = 0;
    all.push(empty);
    for (id, code, gid) in
        [("raw-cff-explicit-inverse-valid", 1, 2), ("raw-cff-explicit-inverse-missing", 3, 3), ("raw-cff-explicit-inverse-duplicate-zero", 0, 3)]
    {
        let mut probe = control(id, &[0, 0, 2, 0, 1], "0", code, gid);
        probe.case.map = "swapped";
        probe.case.classification = "malformed-explicit-map-compatibility";
        probe.case.expectation_basis = "preserve existing inverse map including missing and duplicate-zero behavior";
        probe.expected_shape = if gid < 3 { gid } else { 0 };
        all.push(probe);
    }
    let mut name = control("raw-cff-name-direct", &[0, 0, 34, 0, 35], "0", 1, 1);
    name.case.font = programs::cff_with_charset(false, &[0, 0, 34, 0, 35], programs::RIGHT);
    name.case.font_kind = "name-cff";
    all.push(name);
    all
}

fn export(controls: &[Control]) {
    let Some(requested) = std::env::var_os("KITION_PDF_CFF_OUT") else { return };
    let target = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../target").canonicalize().unwrap();
    let out = PathBuf::from(requested);
    assert!(out.is_absolute() && out.starts_with(&target));
    assert!(!out.components().any(|part| part == std::path::Component::ParentDir));
    std::fs::create_dir_all(&out).unwrap();
    let out = out.canonicalize().unwrap();
    assert!(out.starts_with(target));
    let write = |name: &str, bytes: &[u8]| {
        let path = out.join(name);
        if path.exists() {
            assert_eq!(std::fs::read(path).unwrap(), bytes, "refuse different CFF fixture bytes");
        } else {
            std::fs::write(path, bytes).unwrap();
        }
    };
    let mut rows = Vec::new();
    for item in controls {
        let case = &item.case;
        write(&format!("{}.pdf", case.id), &fixtures::pdf(case));
        write(&format!("{}.font", case.id), &case.font);
        let notdef = match item.notdef_shape {
            1 => "left-bar",
            2 => "right-l",
            _ => "empty",
        };
        rows.push(format!(r#"{{"id":"{}","file":"{}.pdf","fontFile":"{}.font","classification":"{}","expectationBasis":"{}","fontKind":"{}","descendant":"{}","descriptorEntry":"{}","streamSubtype":"{}","cidToGidMap":"{}","code":{},"candidateGlyphId":{},"assertMapping":true,"toUnicodeSwapped":false,"hasSecondaryProgram":false,"charsetFormat":"{}","notdefShape":"{}"}}"#,
            case.id, case.id, case.id, case.classification, case.expectation_basis, case.font_kind, case.descendant, case.font_file, case.stream_subtype, case.map, case.code, case.expected_gid, item.format, notdef));
    }
    write("manifest.raw.json", format!(r#"{{"schema":1,"origin":"Contributor-original geometric font programs, Apache-2.0","pageWidth":40,"pageHeight":40,"scales":[1,2],"coordinateSpace":"pdf-user-space-y-up","cases":[{}]}}"#, rows.join(",")).as_bytes());
}

#[test]
fn raw_cff_charsets_trace_glyph_identity_and_original_notdef_pixels() {
    let controls = controls();
    assert_eq!(controls.len(), 20);
    export(&controls);
    let mut failures = Vec::new();
    for item in &controls {
        let case = &item.case;
        let bytes = fixtures::pdf(case);
        let trace = trace_pdf(bytes.clone(), case.id);
        assert_eq!(trace.0.len(), 1, "{}: one glyph", case.id);
        let (gid, program, path) = &trace.0[0];
        assert_eq!(program, &case.font, "{}: original embedded bytes", case.id);
        let observed_shape = match *gid {
            0 => item.notdef_shape,
            1 | 2 => *gid,
            _ => 0,
        };
        check_shape(path, observed_shape);
        if *gid != case.expected_gid {
            failures.push(format!("{}: actual_gid={gid} expected_gid={}", case.id, case.expected_gid));
        }
        println!(
            "cff_trace case={} code={} actual_gid={gid} expected_gid={} observed_shape={observed_shape} expected_shape={} embedded_bytes_match=true fallback_calls=0",
            case.id, case.code, case.expected_gid, item.expected_shape
        );
        for scale in [1_u32, 2] {
            let page = PageRenderer::new(Arc::new(bytes.clone()), RenderConfig::default())
                .render(RenderRequest { scale: scale as f32, ..Default::default() });
            assert!(page.error.is_none() && page.warnings.is_empty(), "{}: {:?} {:?}", case.id, page.error, page.warnings);
            let mut wrong = 0;
            for (rect, ink) in [
                ([13_u32, 12, 2, 10], item.expected_shape == 1),
                ([21, 15, 2, 8], item.expected_shape == 2),
                ([25, 11, 2, 2], item.expected_shape == 2),
                ([25, 16, 2, 6], false),
            ] {
                for py in rect[1] * scale..(rect[1] + rect[3]) * scale {
                    for px in rect[0] * scale..(rect[0] + rect[2]) * scale {
                        let at = (((40 * scale - 1 - py) * page.width + px) * 4) as usize;
                        let value = if ink { 0 } else { 255 };
                        wrong += usize::from(page.rgba[at..at + 4] != [value, value, value, 255]);
                    }
                }
            }
            let sentinel = (((38 * scale) * page.width + scale) * 4) as usize;
            assert_eq!(&page.rgba[sentinel..sentinel + 4], &[255, 0, 0, 255], "{}: continuation", case.id);
            println!("cff_raster case={} scale={scale} wrong_interior_pixels={wrong}", case.id);
            if wrong != 0 {
                failures.push(format!("{}: scale={scale} wrong_interior_pixels={wrong}", case.id));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("; "));
}

#[test]
fn original_cff_charset_programs_have_bounded_entries_and_visible_notdef() {
    for item in controls().into_iter().filter(|item| item.case.font_kind != "opentype-cid-cff") {
        let case = &item.case;
        let font = CffFontRef::new(&case.font, 0, Some(1000)).unwrap();
        assert_eq!(font.num_glyphs(), 3);
        let charset = font.charset().unwrap();
        let entries: Vec<_> = charset.iter().map(|(gid, sid)| (gid.to_u32(), sid.to_u16())).collect();
        assert_eq!(entries.len(), 3, "{}: charset has only the real glyphs", case.id);
        let reverse = charset.glyph_id(Sid::new(case.code)).ok().map(GlyphId::to_u32);
        println!("cff_parser case={} entries={entries:?} reverse_lookup={reverse:?}", case.id);
        if case.id == "raw-cff-f0-wrapped-missing" {
            assert_eq!(reverse, Some(1), "retain evidence that an unbounded trailing match wraps to a real GID");
            assert!(!entries.iter().any(|(_, cid)| *cid == u16::MAX));
        }
        let subfont = font.subfont(0, &[]).unwrap();
        for gid in 0..3 {
            let mut path = super::Pen::default();
            font.draw(&subfont, GlyphId::new(gid), &[], Some(1000.0), &mut path).unwrap();
            check_shape(&path.0, if gid == 0 { item.notdef_shape } else { gid });
        }
    }
}

#[test]
fn raw_cff_identity_maps_predefined_tables_and_malformed_ranges_keep_boundaries() {
    let mut failures = Vec::new();
    let mut observe = |case: Case, shape| {
        let trace = trace_pdf(fixtures::pdf(&case), case.id);
        assert_eq!(trace.0.len(), 1);
        assert_eq!(trace.0[0].1, case.font);
        println!("cff_native case={} map={} code={} actual_gid={} expected_gid={}", case.id, case.map, case.code, trace.0[0].0, case.expected_gid);
        if trace.0[0].0 == case.expected_gid {
            check_shape(&trace.0[0].2, shape);
        } else {
            failures.push(format!("{}: actual_gid={} expected_gid={}", case.id, trace.0[0].0, case.expected_gid));
        }
    };
    for item in controls().into_iter().filter(|item| item.case.map == "absent" && item.case.font_kind == "cid-cff") {
        let mut case = item.case;
        case.map = "Identity";
        observe(case, item.expected_shape);
    }
    for (id, charset, code, gid) in [
        ("f0-duplicate-first", vec![0, 0, 1, 0, 1], 1, 1),
        ("f1-duplicate-first", vec![1, 0, 1, 0, 0, 1, 0], 1, 1),
        ("f2-duplicate-first", vec![2, 0, 1, 0, 0, 0, 1, 0, 0], 1, 1),
        ("f0-duplicate-zero", vec![0, 0, 0, 0, 1], 0, 0),
        ("f1-max-valid", vec![1, 255, 254, 1], 65535, 2),
        ("f2-max-valid", vec![2, 255, 254, 0, 1], 65535, 2),
        ("f1-overlong-valid", vec![1, 0, 1, 255], 2, 2),
        ("f1-overlong-absent", vec![1, 0, 1, 255], 3, 0),
        ("f2-overlong-valid", vec![2, 0, 1, 255, 255], 2, 2),
        ("f2-overlong-absent", vec![2, 0, 1, 255, 255], 65535, 0),
        ("predefined-valid", vec![], 1, 1),
        ("predefined-absent", vec![], 3, 0),
        ("predefined-notdef", vec![], 0, 0),
    ] {
        let item = control(id, &charset, "native-only", code, gid);
        observe(item.case, item.expected_shape);
    }
    // Overflow in a malformed range does not hide a later representable CID.
    // Original reverse lookup finds CID 1 at GID 3; preserve that in-range match.
    for (id, charset) in
        [("f1-overflow-later-valid", vec![1, 255, 255, 1, 0, 1, 0]), ("f2-overflow-later-valid", vec![2, 255, 255, 0, 1, 0, 1, 0, 0])]
    {
        let mut item = control(id, &charset, "native-only", 1, 3);
        item.case.font = programs::cff_with_glyphs(true, &charset, &[programs::RIGHT, programs::LEFT, programs::RIGHT, programs::LEFT]);
        observe(item.case, 1);
    }
    assert!(failures.is_empty(), "{}", failures.join("; "));
}
