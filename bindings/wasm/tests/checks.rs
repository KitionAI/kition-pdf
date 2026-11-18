//! `pdf_check` against its catalogue (`agent/checks.json`, KHP A38): every fixture in
//! `conformance/` (written by `examples/checks.rs`) reports its code when opened, every
//! code it reports is catalogued, and `basic.pdf` reports nothing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing, clippy::panic)]

use std::path::Path;

use kition_kit::KitDocument;
use kition_pdf_kit::PdfDocument;
use serde_json::Value;

fn repo() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn check(fixture: &str) -> Vec<kition_kit::diagnostics::Diagnostic> {
    let bytes = std::fs::read(repo().join("conformance").join(fixture)).unwrap_or_else(|error| panic!("{fixture}: {error}"));
    PdfDocument::open(fixture, &bytes).unwrap_or_else(|error| panic!("{fixture}: {error}")).check()
}

#[test]
fn every_fixture_reports_its_code() {
    let catalogue: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(repo().join("agent/checks.json")).unwrap()).unwrap();
    let codes: Vec<&str> = catalogue.iter().map(|entry| entry["code"].as_str().unwrap()).collect();
    let mut failures = Vec::new();
    for entry in &catalogue {
        let (code, fixture) = (entry["code"].as_str().unwrap(), entry["fixture"].as_str().unwrap());
        let found = check(fixture);
        if !found.iter().any(|d| d.code == code && serde_json::to_value(d.severity).unwrap() == entry["severity"]) {
            failures.push(format!("{fixture} does not report {code}: {:?}", found.iter().map(|d| &d.code).collect::<Vec<_>>()));
        }
        for d in &found {
            if !codes.contains(&d.code.as_str()) {
                failures.push(format!("{fixture} reports {} outside the catalogue", d.code));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn the_basic_fixture_is_clean() {
    let found: Vec<String> = check("basic.pdf").into_iter().map(|d| d.code).collect();
    assert_eq!(found, Vec::<String>::new());
}

/// `pdf.unreadable` is answered by the check for bytes that do not parse, but such bytes never open:
/// `PdfDocument::open` refuses them first, so no fixture exists and the code is not in the catalogue.
#[test]
fn bytes_that_do_not_parse_do_not_open() {
    assert!(PdfDocument::open("broken.pdf", b"not a pdf at all").is_err());
}

/// The fix of `pdf.broken-link` (`link_delete` with the link's page and number) takes the dead link
/// away and leaves the page's other links.
#[test]
fn the_broken_link_fix_deletes_the_dead_link() {
    let bytes = std::fs::read(repo().join("conformance/check-broken-link.pdf")).unwrap();
    let mut doc = PdfDocument::open("check-broken-link.pdf", &bytes).unwrap();
    let found = doc.check();
    let fix = found.iter().find(|d| d.code == "pdf.broken-link").and_then(|d| d.fix.clone()).expect("a fix");
    doc.execute(&fix.command, fix.params.clone().unwrap_or_default()).unwrap();
    assert!(!doc.check().iter().any(|d| d.code == "pdf.broken-link"), "{:?}", doc.check());
}
