//! A comment's `/NM` is random unless a test build asks for counted ids. `SOURCE_DATE_EPOCH`,
//! which hosts pass to every kit process for timestamps, fixes the dates a session writes but
//! never its ids: two kit processes started with the same epoch name their comments differently.
//! Counted ids need both the dev-only Cargo feature `reproducible-ids` (on here through the
//! crate's dev-dependencies) and `KITION_REPRODUCIBLE_IDS`. The variables are read from the
//! environment, so each session runs in a process of its own: this test starts itself.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::process::Command;

use kition_pdf_engine::{Edit, NewAnnotation, NoteIcon, Session, Shape, Style};

const OUT: &str = "KITION_PDF_IDS_OUT";
const EPOCH: &str = "1794182400";

/// A note on a blank page in a session with no injected clock; the note's `/NM`.
fn name_a_comment() -> String {
    let mut s = Session::new();
    let blank = s.create_blank(612.0, 792.0, 1).unwrap();
    let id = s.open("ids.pdf", None, blank, None).unwrap();
    let note = NewAnnotation {
        page: 0,
        shape: Shape::Note { at: [100.0, 100.0], icon: NoteIcon::Comment },
        style: Style::default(),
        contents: "ids".into(),
        author: "Test".into(),
    };
    s.apply(id, Edit::AddAnnotation(note)).unwrap();
    let annotations = &s.get(id).unwrap().info.annotations;
    assert_eq!(annotations.len(), 1);
    let name = annotations[0].name.clone().expect("the note has an /NM");
    assert_eq!(name.len(), 36, "a UUID: {name}");
    name
}

/// The child's half: writes the name into the file `OUT` names (nothing without it).
#[test]
fn child_names_a_comment() {
    if let Ok(path) = std::env::var(OUT) {
        std::fs::write(path, name_a_comment()).unwrap();
    }
}

/// The name a fresh process gives its note, with `SOURCE_DATE_EPOCH` set and
/// `KITION_REPRODUCIBLE_IDS` set to `seed` (or unset).
fn name_in_a_process(path: &Path, seed: Option<&str>) -> String {
    let mut child = Command::new(std::env::current_exe().unwrap());
    child.args(["--exact", "child_names_a_comment", "--test-threads=1", "--quiet"]).env("SOURCE_DATE_EPOCH", EPOCH).env(OUT, path);
    match seed {
        Some(seed) => child.env("KITION_REPRODUCIBLE_IDS", seed),
        None => child.env_remove("KITION_REPRODUCIBLE_IDS"),
    };
    assert!(child.status().unwrap().success(), "the child failed");
    std::fs::read_to_string(path).unwrap()
}

fn scratch(what: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("kition-pdf-ids-{what}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn source_date_epoch_alone_leaves_ids_random() {
    let dir = scratch("epoch");
    let first = name_in_a_process(&dir.join("0"), None);
    let second = name_in_a_process(&dir.join("1"), None);
    std::fs::remove_dir_all(&dir).ok();
    assert_ne!(first, second, "two processes with the same SOURCE_DATE_EPOCH minted the same /NM");
}

#[test]
fn a_test_build_with_kition_reproducible_ids_counts_ids() {
    let dir = scratch("counted");
    let first = name_in_a_process(&dir.join("0"), Some("7"));
    let second = name_in_a_process(&dir.join("1"), Some("7"));
    let other = name_in_a_process(&dir.join("2"), Some("8"));
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(first, second, "the same seed counts the same /NM");
    assert_ne!(first, other, "another seed counts another /NM");
}

#[test]
fn two_sessions_in_one_process_mint_different_ids() {
    assert_ne!(name_a_comment(), name_a_comment());
}
