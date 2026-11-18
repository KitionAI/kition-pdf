//! Contributor-original trailer extension refusal and physical-xref normalization.
use kition_pdf_cos::{CosError, Document, Object, SaveOptions, write_full, write_incremental};
use std::{fmt::Write, sync::Arc};

fn source(extension: &str, xref_stream: bool, repaired: bool) -> Vec<u8> {
    let bodies = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [] /Count 0 >>",
        "<< /Length 4 >>\nstream\ndata\nendstream",
        "<< /Payload 3 0 R >>",
        "<< /Next 6 0 R >>",
        "<< /Next 5 0 R >>",
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![0];
    for (index, body) in bodies.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", index + 1).as_bytes());
    }
    let xref_at = bytes.len();
    if xref_stream {
        assert!(!repaired);
        offsets.push(xref_at);
        let mut rows = Vec::new();
        for (num, offset) in offsets.iter().enumerate() {
            rows.push(u8::from(num != 0));
            rows.extend_from_slice(&u32::try_from(*offset).unwrap().to_be_bytes());
            rows.extend_from_slice(&(if num == 0 { 65535_u16 } else { 0 }).to_be_bytes());
        }
        bytes.extend_from_slice(
            format!("7 0 obj\n<< /Type /XRef /Size 8 /Root 1 0 R /W [1 4 2] /Index [0 8] /Length {} {extension} >>\nstream\n", rows.len()).as_bytes(),
        );
        bytes.extend_from_slice(&rows);
        bytes.extend_from_slice(b"\nendstream\nendobj\n");
    } else {
        let mut xref = String::from("xref\n0 7\n0000000000 65535 f \n");
        for offset in offsets.iter().skip(1) {
            writeln!(xref, "{offset:010} 00000 n ").unwrap();
        }
        write!(xref, "trailer\n<< /Size 7 /Root 1 0 R {extension} >>\n").unwrap();
        bytes.extend_from_slice(xref.as_bytes());
    }
    let startxref = if repaired { bytes.len() + 999 } else { xref_at };
    bytes.extend_from_slice(format!("startxref\n{startxref}\n%%EOF\n").as_bytes());
    bytes
}

fn assert_refused(doc: &Document, expected: CosError) {
    let before = doc.bytes().clone();
    let trailer = doc.trailer().clone();
    for object_streams in [false, true] {
        let options = SaveOptions { object_streams, mod_date: Some("D:20261023000000Z".into()), ..Default::default() };
        assert_eq!(write_full(doc, &options), Err(expected.clone()));
        assert_eq!(doc.trailer(), &trailer, "preflight must not stamp metadata");
        assert!(Arc::ptr_eq(doc.bytes(), &before));
    }
}

#[test]
fn standard_classic_and_stream_sources_rewrite_in_both_output_modes() {
    for input_stream in [false, true] {
        let doc = Document::open(Arc::new(source("", input_stream, false))).unwrap();
        assert!(doc.repair_log().is_empty());
        assert_eq!(doc.revisions().last().unwrap().is_stream, input_stream);
        assert!(doc.trailer().iter().all(|(k, _)| matches!(k.as_slice(), b"Size" | b"Root")));
        for output_stream in [false, true] {
            let bytes = write_full(&doc, &SaveOptions { object_streams: output_stream, ..Default::default() }).unwrap();
            let reopened = Document::open(Arc::new(bytes)).unwrap();
            assert!(reopened.repair_log().is_empty());
            assert_eq!(reopened.revisions().len(), 1);
            assert_eq!(reopened.revisions()[0].is_stream, output_stream);
            assert_eq!(reopened.get(reopened.root().unwrap()).as_dict().unwrap().name(b"Type"), Some(&b"Catalog"[..]));
            assert!(!reopened.trailer().contains(b"Prev") && !reopened.trailer().contains(b"XRefStm"));
        }
    }
}

#[test]
fn actual_xref_stream_decoded_length_is_physical_metadata() {
    // Eight uncompressed rows, each with widths [1, 4, 2], decode to 56 bytes.
    let bytes = source("/DL 56", true, false);
    assert!(bytes.windows(6).any(|part| part == b"/DL 56"));
    let doc = Document::open(Arc::new(bytes)).unwrap();
    assert!(doc.repair_log().is_empty());
    assert!(doc.revisions().last().unwrap().is_stream);
    assert!(!doc.trailer().contains(b"DL"));
    for output_stream in [false, true] {
        let bytes = write_full(&doc, &SaveOptions { object_streams: output_stream, ..Default::default() }).unwrap();
        let reopened = Document::open(Arc::new(bytes)).unwrap();
        assert!(reopened.repair_log().is_empty());
        assert_eq!(reopened.revisions().len(), 1);
        assert_eq!(reopened.revisions()[0].is_stream, output_stream);
        assert_eq!(reopened.get(reopened.root().unwrap()).as_dict().unwrap().name(b"Type"), Some(&b"Catalog"[..]));
        assert!(!reopened.trailer().contains(b"DL"));
    }
}

#[test]
fn unknown_values_refuse_without_traversing_their_reference_graph() {
    for value in ["1234", "(00001234)", "true", "null", "[1 false]", "<< /Inner 4 0 R >>", "4 0 R", "[4 0 R 4 0 R]", "5 0 R", "999999 0 R"] {
        for input_stream in [false, true] {
            let doc = Document::open(Arc::new(source(&format!("/SyntheticTrailer {value}"), input_stream, false))).unwrap();
            assert!(doc.repair_log().is_empty());
            assert!(doc.trailer().contains(b"SyntheticTrailer"));
            assert_refused(&doc, CosError::UnsupportedTrailerRewrite);
        }
    }
}

#[test]
fn classic_stream_like_custom_keys_survive_normalization_and_refuse_rewrite() {
    for key in ["Type", "W", "Index", "Filter", "DecodeParms", "Length", "F", "FFilter", "FDecodeParms", "DL"] {
        let bytes = source(&format!("/{key} (opaque original value)"), false, false);
        let mut doc = Document::open(Arc::new(bytes.clone())).unwrap();
        assert!(doc.repair_log().is_empty());
        let value = doc.trailer().get(key.as_bytes()).unwrap().clone();
        assert_refused(&doc, CosError::UnsupportedTrailerRewrite);
        let root = doc.root().unwrap();
        doc.update_dict(root, |d| d.set(b"SyntheticEdit".to_vec(), Object::Bool(true))).unwrap();
        let saved = write_incremental(&doc, &SaveOptions::default()).unwrap();
        assert!(saved.starts_with(&bytes));
        assert!(saved.len() > bytes.len());
        let reopened = Document::open(Arc::new(saved)).unwrap();
        assert_eq!(reopened.trailer().get(key.as_bytes()), Some(&value));
        assert_eq!(reopened.get(reopened.root().unwrap()).as_dict().unwrap().get(b"SyntheticEdit"), Some(&Object::Bool(true)));
    }
}

#[test]
fn incremental_save_retains_original_prefix_and_unknown_graph_in_both_xref_styles() {
    for input_stream in [false, true] {
        let bytes = source("/SyntheticTrailer [4 0 R 4 0 R 5 0 R]", input_stream, false);
        let mut doc = Document::open(Arc::new(bytes.clone())).unwrap();
        let root = doc.root().unwrap();
        doc.update_dict(root, |d| d.set(b"SyntheticEdit".to_vec(), Object::Bool(true))).unwrap();
        let saved = write_incremental(&doc, &SaveOptions::default()).unwrap();
        assert!(saved.starts_with(&bytes));
        let reopened = Document::open(Arc::new(saved)).unwrap();
        assert_eq!(reopened.trailer().get(b"SyntheticTrailer"), doc.trailer().get(b"SyntheticTrailer"));
        assert!(reopened.get(reopened.root().unwrap()).as_dict().unwrap().contains(b"SyntheticEdit"));
        for num in [3, 4, 5, 6] {
            let reference = kition_pdf_cos::ObjRef::new(num, 0);
            assert_eq!(reopened.get(reference), doc.get(reference));
        }
        doc.require_full_save();
        for object_streams in [false, true] {
            assert_eq!(write_incremental(&doc, &SaveOptions { object_streams, ..Default::default() }), Err(CosError::UnsupportedTrailerRewrite));
        }
    }
}

#[test]
fn repaired_classic_trailer_keeps_custom_key_and_cannot_bypass_refusal() {
    for extension in ["/SyntheticTrailer 4 0 R", "/Type (private classic type)", "/W null"] {
        let doc = Document::open(Arc::new(source(extension, false, true))).unwrap();
        assert!(!doc.repair_log().is_empty());
        assert!(doc.revisions().is_empty());
        assert_refused(&doc, CosError::UnsupportedTrailerRewrite);
        assert_eq!(write_incremental(&doc, &SaveOptions::default()), Err(CosError::UnsupportedTrailerRewrite));
    }
    let plain = Document::open(Arc::new(source("", false, true))).unwrap();
    for object_streams in [false, true] {
        let bytes = write_incremental(&plain, &SaveOptions { object_streams, ..Default::default() }).unwrap();
        assert!(Document::open(Arc::new(bytes)).unwrap().repair_log().is_empty());
    }
}

#[test]
fn checksum_and_read_only_errors_keep_precedence_and_constant_private_safe_text() {
    let bytes = Arc::new(source("/SyntheticTrailer 999999 0 R /DocChecksum /synthetic-checksum", false, false));
    let mut doc = Document::open(bytes.clone()).unwrap();
    assert_refused(&doc, CosError::UnsupportedChecksumRewrite);
    doc.require_full_save();
    assert_eq!(write_incremental(&doc, &SaveOptions::default()), Err(CosError::UnsupportedChecksumRewrite));
    let limited = Document::open_with_stream_limit(bytes, None, 1 << 20).unwrap();
    assert_refused(&limited, CosError::ReadOnlyLimit);
    assert_eq!(write_incremental(&limited, &SaveOptions::default()), Err(CosError::ReadOnlyLimit));
    let mut opaque = Document::new_empty();
    opaque.trailer_mut().set(vec![b'X'; 16384], Object::Null);
    assert_refused(&opaque, CosError::UnsupportedTrailerRewrite);
    let message = CosError::UnsupportedTrailerRewrite.to_string();
    assert!(message.len() < 256 && !message.contains("XXXX"));
    assert!(message.contains("incremental") && message.contains("when the operation permits"));
}

#[test]
fn newest_classic_section_retains_custom_keys_over_older_or_hybrid_stream() {
    for hybrid in [false, true] {
        let mut bytes = source("", true, false);
        let prior = String::from_utf8_lossy(&bytes).rsplit("startxref\n").next().unwrap().lines().next().unwrap().parse::<usize>().unwrap();
        let xref_at = bytes.len();
        let hybrid_key = if hybrid { format!("/XRefStm {prior}") } else { String::new() };
        bytes.extend_from_slice(format!("xref\n0 1\n0000000000 65535 f \ntrailer\n<< /Size 8 /Root 1 0 R /Prev {prior} {hybrid_key} /Type (classic extension) /W null >>\nstartxref\n{xref_at}\n%%EOF\n").as_bytes());
        let doc = Document::open(Arc::new(bytes)).unwrap();
        assert!(doc.repair_log().is_empty());
        assert!(doc.revisions().first().unwrap().is_stream);
        assert!(!doc.revisions().last().unwrap().is_stream);
        assert!(doc.trailer().contains(b"Type") && doc.trailer().contains(b"W"));
        assert!(!doc.trailer().contains(b"Prev") && !doc.trailer().contains(b"XRefStm"));
        assert_refused(&doc, CosError::UnsupportedTrailerRewrite);
    }
}
