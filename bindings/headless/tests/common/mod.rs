//! What the guard tests share (task card C8-land-14-pdf): PDFs built with this kit's engine, the
//! `kition-pdf` executable driven through the kits' MCP server's tools (`kition_kits::tools`, the
//! library `kition-kits-mcp` serves) as an agent drives it, and a colleague's program that changes
//! the file between the agent's observation and its write: this kit's engine, an instance of its
//! own that shares nothing with the server under test, opening the file, running commands and
//! saving it back.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing, dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use kition_kit::KitDocument;
use kition_kits::tools::{self, CallOptions};
use kition_kits::{KitDefinition, KitManager};
use kition_pdf_kit::PdfDocument;
use serde_json::{Value, json};

pub const ME: &str = "agent-me";
pub const FILE: &str = "Report.pdf";

pub fn kit() -> &'static KitDefinition {
    KitDefinition::from_manifest_text(include_str!("../../../../kit.json")).unwrap()
}

/// A PDF of `pages` Letter pages, page `n` (from 1) with the text `Page n` at the top, one note
/// comment `note n` at (100, 100 + n), a text field `F<n>` and a bookmark `B<n>` to it.
pub fn fixture_doc(pages: usize) -> PdfDocument {
    let mut doc = PdfDocument::create(FILE, 612, 792).unwrap();
    for at in 2..=pages {
        doc.execute("page.insertBlank", json!({ "at": at })).unwrap();
    }
    for n in 1..=pages {
        doc.execute("page.addText", json!({ "page": n, "at": [72, 60], "text": format!("Page {n}") })).unwrap();
        doc.execute("comment.add", json!({ "page": n, "type": "note", "at": [100, 100 + n], "contents": format!("note {n}") })).unwrap();
        doc.execute("form.addField", json!({ "page": n, "type": "text", "rect": [72, 600, 300, 624], "name": format!("F{n}") })).unwrap();
        doc.execute("bookmark.add", json!({ "page": n, "title": format!("B{n}") })).unwrap();
    }
    doc
}

pub fn fixture(pages: usize) -> Vec<u8> {
    fixture_doc(pages).save(None).unwrap()
}

/// Sets the file's modification time an hour before its own, so that only its content tells the
/// server it changed.
fn backdate(file: &Path, before: std::time::SystemTime) {
    std::fs::File::options().write(true).open(file).unwrap().set_modified(before - Duration::from_secs(3600)).unwrap();
}

/// A colleague's program: this kit's engine, its own instance, opening the file from disk,
/// running `steps` and saving the file back (incrementally, as the kit saves).
pub fn colleague(file: &Path, steps: &[(&str, Value)]) -> Vec<Value> {
    let at = std::fs::metadata(file).unwrap().modified().unwrap();
    let mut doc = PdfDocument::open(FILE, &std::fs::read(file).unwrap()).unwrap();
    let answers = steps
        .iter()
        .map(|(command, params)| doc.execute(command, params.clone()).unwrap_or_else(|error| panic!("colleague's {command}: {error}")))
        .collect();
    std::fs::write(file, doc.save(Some(FILE)).unwrap()).unwrap();
    backdate(file, at);
    answers
}

/// The file as a new engine reads it.
pub fn on_disk(file: &Path) -> PdfDocument {
    PdfDocument::open(FILE, &std::fs::read(file).unwrap()).unwrap()
}

/// `command` run on the file as a new engine reads it (a read: nothing is written).
pub fn read_on_disk(file: &Path, command: &str, params: Value) -> Value {
    on_disk(file).execute(command, params).unwrap()
}

static WORKSPACES: AtomicUsize = AtomicUsize::new(0);

/// A fresh, empty directory of this test process.
fn workspace() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kition-pdf-agent-{}-{}", std::process::id(), WORKSPACES.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(dir).unwrap()
}

pub struct Fixture {
    pub kits: KitManager,
    pub options: CallOptions,
    pub workspace: PathBuf,
}

impl Fixture {
    pub fn new(pages: usize) -> Self {
        Self::from_bytes(&fixture(pages), CallOptions::default())
    }

    pub fn from_bytes(bytes: &[u8], options: CallOptions) -> Self {
        let workspace = workspace();
        std::fs::write(workspace.join(FILE), bytes).unwrap();
        let kits = KitManager::new(Some(workspace.clone()), vec![(kit(), Some(PathBuf::from(env!("CARGO_BIN_EXE_kition-pdf"))))]);
        Self { kits, options, workspace }
    }

    pub fn file(&self) -> PathBuf {
        self.workspace.join(FILE)
    }

    pub fn colleague(&self, steps: &[(&str, Value)]) -> Vec<Value> {
        colleague(&self.file(), steps)
    }

    pub async fn call(&self, action: &str, mut input: Value) -> Result<Value, Value> {
        input["path"] = json!(FILE);
        match tools::call_as(&self.kits, &format!("pdf_{action}"), &input, self.options, ME).await {
            Some(Ok(output)) => Ok(output.output),
            Some(Err(error)) => Err(error.structured()),
            None => panic!("pdf_{action} is no tool"),
        }
    }

    /// A call by another agent of the same server (`other`), as a second MCP connection makes it.
    pub async fn call_by(&self, other: &str, action: &str, mut input: Value) -> Value {
        input["path"] = json!(FILE);
        match tools::call_as(&self.kits, &format!("pdf_{action}"), &input, self.options, other).await {
            Some(Ok(output)) => output.output,
            Some(Err(error)) => panic!("{other}'s pdf_{action}: {}", error.structured()),
            None => panic!("pdf_{action} is no tool"),
        }
    }

    pub async fn ok(&self, action: &str, input: Value) -> Value {
        self.call(action, input).await.unwrap_or_else(|error| panic!("pdf_{action}: {error}"))
    }

    pub async fn refused(&self, action: &str, input: Value) -> Value {
        match self.call(action, input).await {
            Err(error) => error,
            Ok(answer) => panic!("pdf_{action} was expected to be refused: {answer}"),
        }
    }

    /// The document as the agent inspects it (`details`), and the answer's `revision`.
    pub async fn inspect(&self) -> (Value, String) {
        let seen = self.ok("inspect", json!({})).await;
        (seen["details"].clone(), revision(&seen))
    }

    pub async fn shutdown(self) {
        self.kits.shutdown().await;
        let _ = std::fs::remove_dir_all(&self.workspace);
    }
}

pub fn revision(answer: &Value) -> String {
    answer["revision"].as_str().unwrap_or_else(|| panic!("a revision: {answer}")).to_owned()
}

/// The conflicts' reasons, joined with `+`.
pub fn reasons(refused: &Value) -> String {
    refused["conflicts"].as_array().map(|list| list.iter().filter_map(|c| c["reason"].as_str()).collect::<Vec<_>>().join("+")).unwrap_or_default()
}

pub fn step(command: &str, params: Value) -> Value {
    json!({ "command": command, "params": params })
}

/// What one case wants of the agent's write: applied, or refused with this reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Want {
    Applies,
    Refused(&'static str),
}

/// One case: the agent observes, the colleague runs `theirs` on the file, the agent writes `mine`
/// with the basis of its observation. Checks the outcome, that a refusal wrote nothing, and
/// returns the answer (or the refusal).
pub async fn case(f: &Fixture, theirs: &[(&str, Value)], mine: Value, want: Want) -> Value {
    let (_, basis) = f.inspect().await;
    if !theirs.is_empty() {
        f.colleague(theirs);
    }
    let before = std::fs::read(f.file()).unwrap();
    let answer = f.call("run", json!({ "basis": basis, "steps": [mine.clone()] })).await;
    match (want, answer) {
        (Want::Applies, Ok(ran)) => ran,
        (Want::Refused(reason), Err(refused)) => {
            assert_eq!(reasons(&refused), reason, "{mine} after {theirs:?}: {refused}");
            assert_eq!(std::fs::read(f.file()).unwrap(), before, "a refused write wrote nothing");
            refused
        }
        (want, got) => panic!("{mine} after {theirs:?}: wanted {want:?}, got {got:?}"),
    }
}
