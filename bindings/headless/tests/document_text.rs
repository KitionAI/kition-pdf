//! `document.text` (kition-sdk `docs/platform/sets/text.md`, `KitText 1.0`): the words of every
//! page in reading order, one part per page, so a host indexes a PDF whose words are not in its
//! object model.

use std::path::PathBuf;

use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit::KitDocument;
use kition_kit::testing::check_text;
use kition_kit::text::{DEFAULT_LIMIT, TextPart};
use kition_kit_server::{ErrorReason, KitBackend, OpenRequest};
use kition_pdf_kit::PdfDocument;
use serde_json::{Value, json};

/// Three pages headed Overview, Budget for 2026 and Appendix (written by
/// bindings/wasm/examples/ai_inputs.rs for the case pdf.agent.observe/budget-page).
const REPORT: &[u8] = include_bytes!("../../../cases/pdf.agent.observe/budget-page/input/report.pdf");

fn kit() -> Headless<PdfDocument> {
    Headless::with_replica_port(KitInfo::from_manifest(include_str!("../../../kit.json"), &[], kition_pdf_kit::EDITOR_STATE).unwrap())
}

fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kition-pdf-text-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(dir).unwrap()
}

/// A scanned page: one picture over the whole page and no text layer.
fn scanned() -> Vec<u8> {
    let content = b"q 612 0 0 792 0 0 cm /Scan Do Q";
    let objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /Scan 5 0 R >> >> /Contents 4 0 R >>".to_vec(),
        [format!("<< /Length {} >>\nstream\n", content.len()).as_bytes(), content, b"\nendstream"].concat(),
        [
            &b"<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 4 >>\nstream\n"[..],
            &[0, 255, 255, 0],
            b"\nendstream",
        ]
        .concat(),
    ];
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (n, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", n + 1).as_bytes());
        pdf.extend_from_slice(object);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    pdf
}

fn part(label: &str, start: usize, end: usize) -> TextPart {
    TextPart { label: label.into(), start, end }
}

#[test]
fn answers_the_pages_of_a_report_in_reading_order() {
    let doc = PdfDocument::open("report.pdf", REPORT).unwrap();
    let text = check_text(&doc).unwrap();
    assert_eq!(
        text.text,
        "Overview\nSales rose in every region.\nBudget for 2026\nSpending stays within the plan.\nAppendix\nFigures are in thousands."
    );
    assert!(!text.truncated);
    assert_eq!(text.parts, [part("Page 1", 0, 36), part("Page 2", 37, 84), part("Page 3", 85, 119)]);
}

#[test]
fn cuts_at_the_limit_and_says_so() {
    let doc = PdfDocument::open("report.pdf", REPORT).unwrap();
    let cut = doc.text(45).unwrap();
    assert_eq!(cut.text, "Overview\nSales rose in every region.\nBudget f");
    assert!(cut.truncated);
    assert_eq!(cut.parts, [part("Page 1", 0, 36), part("Page 2", 37, 45)]);
    // Exactly the first page: the second is what was left out.
    let first = doc.text(36).unwrap();
    assert_eq!((first.text.as_str(), first.truncated, first.parts.len()), ("Overview\nSales rose in every region.", true, 1));
}

#[test]
fn pages_without_words_have_no_part_and_edits_are_read() {
    let mut doc = PdfDocument::create("notes.pdf", 612, 792).unwrap();
    doc.execute("page.insertBlank", json!({ "at": 2 })).unwrap();
    doc.execute("page.insertBlank", json!({ "at": 3 })).unwrap();
    doc.execute("page.addText", json!({ "page": 1, "text": "First", "at": [72, 100] })).unwrap();
    doc.execute("page.addText", json!({ "page": 3, "text": "Third", "at": [72, 100] })).unwrap();
    let text = check_text(&doc).unwrap();
    assert_eq!(text.text, "First\nThird");
    assert_eq!(text.parts, [part("Page 1", 0, 5), part("Page 3", 6, 11)]);
}

#[test]
fn a_scanned_pdf_has_no_words() {
    let doc = PdfDocument::open("scan.pdf", &scanned()).unwrap();
    assert_eq!(doc.inspect().unwrap()["pages"].as_array().map(Vec::len), Some(1), "the scan opens as one page");
    let text = check_text(&doc).unwrap();
    assert_eq!((text.text.as_str(), text.truncated, text.parts.len()), ("", false, 0));
}

#[test]
fn the_headless_kit_lists_the_capability_and_serves_document_text() {
    let root = workspace("serve");
    std::fs::write(root.join("report.pdf"), REPORT).unwrap();
    let mut kit = kit();
    assert_eq!(kit.capabilities()["capabilities"], json!(["document-text"]));
    let folder = HostFolder::new(&root).unwrap();
    kit.set_host(folder.host());
    let id = kit.open(OpenRequest { file: &folder.read_only("report.pdf").unwrap(), read_only: true, create: None }).unwrap().id;
    let before = kit.describe(&id).unwrap();
    let whole = kit.text(&id, DEFAULT_LIMIT as u64).unwrap();
    assert_eq!(whole["parts"].as_array().map(Vec::len), Some(3));
    assert_eq!(whole["truncated"], false);
    assert!(whole["text"].as_str().is_some_and(|text| text.contains("Budget for 2026")));
    let cut = kit.text(&id, 8).unwrap();
    assert_eq!(cut, json!({ "text": "Overview", "truncated": true, "parts": [{ "label": "Page 1", "start": 0, "end": 8 }] }));
    let after = kit.describe(&id).unwrap();
    assert_eq!((after.summary, Value::Object(after.state)), (before.summary, Value::Object(before.state)), "reading the text changes nothing");
    assert!(!kit.document(&id).unwrap().dirty);
    assert_eq!(kit.text("no-such-document", 10).unwrap_err().code, ErrorReason::DocumentNotFound.code());
    let _ = std::fs::remove_dir_all(root);
}
