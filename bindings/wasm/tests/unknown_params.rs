//! A command with a parameter schema refuses a key the schema does not name, and says which
//! (platform rule E7a, kit requirement AX15): an agent that misspells a parameter is told instead
//! of the command running without it.
#![allow(clippy::unwrap_used, clippy::panic)]

use std::path::Path;

use kition_kit::KitDocument;
use kition_pdf_kit::PdfDocument;

fn basic() -> PdfDocument {
    let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../conformance/basic.pdf"));
    PdfDocument::open("basic.pdf", &std::fs::read(path).unwrap()).unwrap()
}

#[test]
fn every_command_refuses_an_unknown_parameter() {
    if let Err(problems) = kition_kit::testing::check_unknown_params(basic, &[]) {
        panic!("{} problems:\n{}", problems.len(), problems.join("\n"));
    }
}

/// `doc` is not a parameter of any published command: a command runs on the document it is called
/// on. A caller that passes `doc` (believing it picks another document) is refused and told so,
/// like any other unknown key, instead of the command running on this document.
#[test]
fn every_command_refuses_a_doc_parameter() {
    let mut problems = Vec::new();
    for command in basic().commands().into_iter().filter(|command| command.schema.is_some()) {
        let mut doc = basic();
        let before = doc.revision();
        match doc.execute(&command.id, serde_json::json!({ "doc": 2 })) {
            Ok(_) => problems.push(format!("{}: accepted `doc`", command.id)),
            Err(message) if !message.contains("\"doc\"") => {
                problems.push(format!("{}: refused, but the error does not name `doc`: {message}", command.id));
            }
            Err(_) => {}
        }
        if doc.revision() != before {
            problems.push(format!("{}: changed the document", command.id));
        }
    }
    assert!(problems.is_empty(), "{} problems:\n{}", problems.len(), problems.join("\n"));
}

/// The audit's call: a rotation that names another document must not rotate this one.
#[test]
fn a_doc_parameter_does_not_rotate_the_open_document() {
    let mut doc = basic();
    let before = doc.revision();
    let error = doc.execute("page_rotate", serde_json::json!({ "degrees": 90, "doc": "something-else" })).unwrap_err();
    assert!(error.contains("\"doc\""), "the error names `doc`: {error}");
    assert!(error.contains("document"), "the error says how the document is chosen: {error}");
    assert_eq!(doc.revision(), before, "the open document is unchanged");
}
