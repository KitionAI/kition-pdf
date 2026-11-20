//! `kition-pdf` reads and writes only inside its workspace, and only through its host (KHP A28,
//! A29, A37, A42, A45, section 10.6): whatever an agent runs, and whatever a PDF asks for, nothing
//! appears in the process's current, home or temporary folder, nothing outside the workspace is
//! opened, read, changed or fetched, no program is started, no answer tells whether a file outside
//! exists or where the kit is installed, and no connection is made. A workspace-escape audit
//! found by reading code that
//! `content_update`'s `image` (a parameter the kit's file check did not know) read any file by
//! path into the page, and that `ocr_status` answered folders of the machine (beside the
//! executable, the source tree).
//!
//! The test runs itself again as a child process whose current folder, `HOME` and `TMPDIR` are
//! empty scratch folders and whose `PATH` starts with fake programs (`open`, `lp`, `lpstat`...)
//! that only record that they ran, beside a planted `outside/` folder of sentinel files and named
//! pipes (opening a pipe to read is seen even when no answer shows it). The agent names them
//! directly and in `model.batch` (no origin, the agent's, a person's the caller claims), and opens
//! a PDF whose actions and streams name them: a launch action, a link into another PDF, form data
//! to import, a stream whose data is in another file, a file specification, JavaScript that asks
//! to open and submit to a localhost server, and a link to that server. Every command the kit
//! offers then runs with empty parameters. The check reports each reference to another file
//! (`pdf.external-reference`); none is followed.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use kition_headless::{Headless, HostFolder, KitInfo};
use kition_kit_server::protocol::{Origin, ScreenshotScope};
use kition_kit_server::{KitBackend, OpenRequest, ScreenshotRequest};
use kition_pdf_kit::PdfDocument;
use serde_json::{Map, Value, json};

/// Set in the child: the folder holding `workspace/`, `outside/`, and the scratch folders.
const CHILD: &str = "KITION_PDF_TEST_SANDBOX";
/// Set in the child: the port of the server no document may reach.
const PORT: &str = "KITION_PDF_TEST_PORT";
/// Text only files outside the workspace hold: it must reach no answer.
const SECRET: &str = "SENTINEL-5e0b";
/// A file outside the workspace that no call names: only a listing could find it.
const LISTED_ONLY: &str = "listed-only-5e0b.txt";
/// Named pipes outside the workspace that probes and documents name instead of a file.
const PIPES: [&str; 4] = ["pipe.bmp", "pipe.pdf", "pipe.bin", "pipe.fdf"];
/// Programs a PDF or a tool might start, faked at the front of the child's `PATH`.
const PROGRAMS: [&str; 9] = ["open", "xdg-open", "lp", "lpr", "lpstat", "lpoptions", "cmd.exe", "fake-app", "sh-launch"];

/// Every file under `dir`, relative to it, with its bytes and modification time (folders and
/// pipes: no bytes).
fn tree(dir: &Path) -> BTreeMap<String, (Vec<u8>, Option<std::time::SystemTime>)> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in std::fs::read_dir(&next).into_iter().flatten().flatten() {
            let path = entry.path();
            let meta = std::fs::symlink_metadata(&path).unwrap();
            if meta.is_dir() {
                stack.push(path.clone());
            }
            let bytes = if meta.is_file() { std::fs::read(&path).unwrap_or_default() } else { Vec::new() };
            out.insert(path.strip_prefix(dir).unwrap_or(&path).to_string_lossy().into_owned(), (bytes, meta.modified().ok()));
        }
    }
    out
}

fn at(dir: &Path, name: &str) -> String {
    dir.join(name).to_string_lossy().into_owned()
}

fn params(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

/// A protocol error as the agent reads it: its message and its data (the kit's detail).
fn error(error: &kition_kit_server::RpcError) -> String {
    format!("{error} {}", error.data.as_ref().map(Value::to_string).unwrap_or_default())
}

/// A `w` x `h` 24-bit BMP of one colour.
fn bmp(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
    let row = (w * 3).div_ceil(4) * 4;
    let size = 54 + row * h;
    let mut out = Vec::new();
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    for _ in 0..h {
        for _ in 0..w {
            out.extend_from_slice(&[rgb[2], rgb[1], rgb[0]]);
        }
        out.resize(out.len() + (row - w * 3) as usize, 0);
    }
    out
}

/// A PDF of `objects` (object 1 is the catalog), with an xref table.
fn pdf(objects: &[String]) -> Vec<u8> {
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    out
}

/// A one-page PDF whose actions, streams and file specifications name files outside the
/// workspace, programs, and the server at `port`.
fn referring(outside: &Path, port: u16) -> Vec<u8> {
    let file = |name: &str| at(outside, name);
    let script = format!(
        "app.launchURL('http://127.0.0.1:{port}/js'); this.submitForm('http://127.0.0.1:{port}/submit'); this.importDataObject('x', '{}');",
        file("secret.txt")
    );
    let content = "q 100 0 0 100 50 50 cm /X Do Q";
    pdf(&[
        "<< /Type /Catalog /Pages 2 0 R /OpenAction 6 0 R /Names << /EmbeddedFiles << /Names [(note.txt) 9 0 R] >> >> >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 612 792] >>".into(),
        "<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /XObject << /X 7 0 R >> >> /Annots [8 0 R 10 0 R 11 0 R] >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()),
        "<< /Producer (workspace_only) >>".into(),
        format!("<< /S /JavaScript /JS ({script}) /Next 12 0 R >>"),
        format!(
            "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length 0 /F ({}) >>\nstream\n\nendstream",
            file("pipe.bin"),
        ),
        format!("<< /Type /Annot /Subtype /Link /Rect [0 0 50 50] /A << /S /Launch /F ({}) /Win << /F (cmd.exe) >> >> >>", file("fake-app")),
        format!("<< /Type /Filespec /F ({0}) /UF ({0}) >>", file("secret.txt")),
        "<< /Type /Annot /Subtype /Link /Rect [60 0 110 50] /A << /S /GoToR /F (../outside/pipe.pdf) /D [0 /Fit] >> >>".into(),
        format!("<< /Type /Annot /Subtype /Link /Rect [120 0 170 50] /A << /S /URI /URI (http://127.0.0.1:{port}/uri) >> >>"),
        format!("<< /S /ImportData /F ({}) >>", file("pipe.fdf")),
    ])
}

/// A one-page PDF with a picture added the way `page_add_image` adds one (so `content_update`
/// edits it), made by the engine outside the kit.
fn with_picture() -> Vec<u8> {
    let mut session = kition_pdf_engine::Session::new();
    let blank = session.create_blank(612.0, 792.0, 1).unwrap();
    let id = session.open("picture.pdf", None, blank, None).unwrap();
    let picture = Arc::new(bmp(8, 8, [200, 30, 30]));
    session.apply(id, kition_pdf_engine::Edit::AddImage { page: 0, rect: None, name: "inside.bmp".into(), bytes: picture }).unwrap();
    session.save_bytes(id).unwrap().to_vec()
}

/// Watches the [`PIPES`]: a reader that opens one is let through (the pipe is opened to write and
/// closed, so the reader reads an empty file and goes on) and recorded.
struct PipeWatcher {
    stop: Arc<std::sync::atomic::AtomicBool>,
    thread: std::thread::JoinHandle<Vec<String>>,
}

impl PipeWatcher {
    fn start(outside: &Path) -> Self {
        use std::os::unix::fs::OpenOptionsExt;
        #[cfg(target_os = "macos")]
        const O_NONBLOCK: i32 = 0x0004;
        #[cfg(not(target_os = "macos"))]
        const O_NONBLOCK: i32 = 0o4000;
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let pipes: Vec<PathBuf> = PIPES.iter().map(|pipe| outside.join(pipe)).collect();
        let stopped = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            let mut opened = Vec::new();
            while !stopped.load(std::sync::atomic::Ordering::SeqCst) {
                for pipe in &pipes {
                    // Opening a pipe to write without waiting succeeds only while a reader has it open.
                    if std::fs::OpenOptions::new().write(true).custom_flags(O_NONBLOCK).open(pipe).is_ok() {
                        opened.push(pipe.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default());
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            opened
        });
        Self { stop, thread }
    }

    /// The pipes a reader opened, once each time.
    fn stop(self) -> Vec<String> {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        self.thread.join().unwrap()
    }
}

/// The agent's session: the kit as kition-headless serves it, on a host folder (the workspace).
struct Agent {
    kit: Headless<PdfDocument>,
    folder: HostFolder,
    /// Every answer, as text.
    seen: Vec<String>,
    problems: Vec<String>,
}

impl Agent {
    fn new(workspace: &Path) -> Self {
        // What the headless kit's `main` does before it serves.
        kition_pdf_kit::hosting::set_files_only();
        let mut kit = Headless::<PdfDocument>::with_replica_port(
            KitInfo::from_manifest(include_str!("../../../kit.json"), &[], kition_pdf_kit::EDITOR_STATE).unwrap(),
        );
        let folder = HostFolder::new(workspace).unwrap();
        kit.set_host(folder.host());
        Self { kit, folder, seen: Vec::new(), problems: Vec::new() }
    }

    fn open(&mut self, name: &str) -> String {
        let file = self.folder.file(name).unwrap();
        self.kit.open(OpenRequest { file: &file, read_only: false, create: None }).unwrap().id
    }

    fn answer(&mut self, answer: Result<Value, String>) -> String {
        let text = match answer {
            Ok(value) => value.to_string(),
            Err(message) => format!("error: {message}"),
        };
        self.seen.push(text.clone());
        text
    }

    /// `command` as `commands.execute` runs it for the agent.
    fn run(&mut self, id: &str, command: &str, value: Value) -> String {
        let answer = self.kit.execute(id, command, params(value), Origin::Agent);
        let answer = answer.map(|done| done.result.unwrap_or(Value::Null)).map_err(|e| error(&e));
        self.answer(answer)
    }

    /// `command` as a `command` operation of `model.batch`, with the batch's `origin`.
    fn batch(&mut self, id: &str, command: &str, value: Value, origin: &Value) -> String {
        let mut request = json!({ "documentId": id, "ops": [{ "op": "command", "id": command, "params": value }] });
        if !origin.is_null() {
            request["origin"] = origin.clone();
        }
        let answer = self.kit.model("model.batch", request).map_err(|e| error(&e));
        self.answer(answer)
    }

    /// Every way the agent can run `command` with `value`: directly, and in `model.batch` with no
    /// origin, the agent's, and a person's the caller claims. The answers, in that order.
    fn every_way(&mut self, id: &str, command: &str, value: &Value) -> Vec<String> {
        let mut answers = vec![self.run(id, command, value.clone())];
        for origin in [Value::Null, json!({ "kind": "agent", "principal": "agent:test" }), json!({ "kind": "person", "principal": "person:local" })] {
            answers.push(self.batch(id, command, value.clone(), &origin));
        }
        answers
    }

    /// Everything a reader of the document gets: a picture, the check, the text, inspect.
    fn read_all(&mut self, id: &str) -> Value {
        let shot = self
            .kit
            .screenshot(id, &ScreenshotRequest { scope: ScreenshotScope::Canvas, max_size: 512, scale: 1.0, target: None })
            .map(|shot| shot.png.len());
        self.seen.push(format!("{shot:?}"));
        let checked = self.kit.check(id).unwrap_or_default();
        self.seen.push(checked.to_string());
        let text = self.kit.text(id, 100_000).unwrap_or_default();
        self.seen.push(text.to_string());
        let described = self.kit.describe(id).map(|described| Value::Object(described.state)).unwrap_or_default();
        self.seen.push(described.to_string());
        checked
    }

    /// Every command the kit offers, with no parameters.
    fn sweep(&mut self, id: &str) {
        let offered: Vec<String> = self.kit.commands(Some(id)).unwrap().into_iter().map(|command| command.id).collect();
        for command in offered {
            self.run(id, &command, json!({}));
        }
    }

    /// Nothing outside the workspace reached an answer: its files' text, a listing, a folder of
    /// this process or of the kit's installation.
    fn assert_nothing_leaked(&mut self, root: &Path) {
        let repository = std::fs::canonicalize(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).unwrap().to_string_lossy().into_owned();
        let leaks = [SECRET.to_owned(), LISTED_ONLY.to_owned(), at(root, "home"), at(root, "tmp"), repository];
        for seen in &self.seen {
            for leak in &leaks {
                if seen.contains(leak.as_str()) {
                    let excerpt: String = seen.chars().take(300).collect();
                    self.problems.push(format!("an answer holds `{leak}`, from outside the workspace: {excerpt}"));
                }
            }
        }
        assert!(self.problems.is_empty(), "{:#?}", self.problems);
    }
}

/// The answers to `existing` and `missing`, the same probes naming a file outside that is there
/// and one that is not, must be the same once the names are taken out.
fn no_existence_oracle(agent: &mut Agent, what: &str, existing: &[String], missing: &[String]) {
    let a: Vec<String> = existing.iter().map(|answer| answer.replace("secret", "<file>")).collect();
    let b: Vec<String> = missing.iter().map(|answer| answer.replace("absent", "<file>")).collect();
    if a != b {
        agent.problems.push(format!("{what}: the answers tell whether a file outside the workspace exists:\n  there:   {a:?}\n  missing: {b:?}"));
    }
}

/// What the agent does in the child.
fn act(root: &Path) {
    let workspace = root.join("workspace");
    let outside = root.join("outside");
    let port: u16 = std::env::var(PORT).unwrap().parse().unwrap();
    let mut agent = Agent::new(&workspace);

    // A picture added to the page: content_update's `image` replaces it by a file's bytes.
    let id = agent.open("picture.pdf");
    let listed = agent.run(&id, "content_list", json!({}));
    if !listed.contains("index") {
        agent.problems.push(format!("picture.pdf should list its added picture: {listed}"));
    }
    let probe = |name: &str| json!({ "page": 1, "index": 1, "image": at(&outside, name) });
    let there = agent.every_way(&id, "content_update", &probe("secret.bmp"));
    let absent = agent.every_way(&id, "content_update", &probe("absent.bmp"));
    no_existence_oracle(&mut agent, "content_update image", &there, &absent);
    agent.every_way(&id, "content_update", &probe("pipe.bmp"));
    for (command, value) in [
        ("page_add_image", json!({ "page": 1, "path": at(&outside, "pipe.bmp") })),
        ("comment_add", json!({ "page": 1, "type": "attachment", "at": [10, 10], "path": at(&outside, "secret.txt") })),
        ("doc_import_data", json!({ "path": at(&outside, "pipe.fdf") })),
        ("ocr_status", json!({})),
        ("printers", json!({})),
        ("doc_print", json!({})),
        ("sign_document", json!({ "id": at(&outside, "pipe.bin"), "out": at(&outside, "signed.pdf") })),
        ("action_run", json!({ "action": "x", "paths": [at(&outside, "pipe.pdf")], "folder": at(&outside, "out") })),
    ] {
        agent.every_way(&id, command, &value);
    }
    agent.sweep(&id);
    let saved = agent.kit.save(&id, None, None).map(|_| Value::Null).map_err(|e| error(&e));
    agent.answer(saved);

    // A PDF that refers to other files, programs and a server: read, checked, swept.
    let refers = agent.open("refers.pdf");
    let checked = agent.read_all(&refers);
    let reported = checked.to_string().matches("pdf.external-reference").count();
    if reported < 4 {
        let wanted = "the launch action, the link into another PDF, the form data to import and the stream in another file";
        agent.problems.push(format!("the check should report {wanted} as pdf.external-reference: {checked}"));
    }
    for (command, value) in [
        ("js_document_scripts", json!({})),
        ("js_run", json!({ "script": format!("app.launchURL('http://127.0.0.1:{port}/run'); this.submitForm('http://127.0.0.1:{port}/run');") })),
        ("link_list", json!({})),
        ("attachments", json!({})),
        ("form_fields", json!({})),
        ("page_render", json!({ "page": 1 })),
    ] {
        agent.run(&refers, command, value);
    }
    agent.sweep(&refers);
    agent.read_all(&refers);
    agent.assert_nothing_leaked(root);
}

#[test]
fn the_agent_and_its_documents_reach_nothing_outside_the_workspace() {
    if let Some(root) = std::env::var_os(CHILD) {
        act(&PathBuf::from(root));
        return;
    }
    let root = std::env::temp_dir().join(format!("kition-pdf-workspace-only-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for dir in ["workspace", "cwd", "home", "tmp", "outside/sub", "bin", "marks"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    let root = std::fs::canonicalize(root).unwrap();
    let (workspace, outside) = (root.join("workspace"), root.join("outside"));
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    server.set_nonblocking(true).unwrap();
    let port = server.local_addr().unwrap().port();

    // Sentinels outside, pipes, and fake programs that record that they ran.
    std::fs::write(outside.join("secret.bmp"), bmp(8, 8, [23, 211, 97])).unwrap();
    std::fs::write(outside.join("secret.txt"), SECRET).unwrap();
    std::fs::write(outside.join(LISTED_ONLY), SECRET).unwrap();
    std::fs::write(outside.join("sub/notes.txt"), SECRET).unwrap();
    for pipe in PIPES {
        assert!(std::process::Command::new("mkfifo").arg(outside.join(pipe)).status().unwrap().success(), "mkfifo {pipe}");
    }
    for program in PROGRAMS {
        use std::os::unix::fs::PermissionsExt;
        let fake = root.join("bin").join(program);
        std::fs::write(&fake, format!("#!/bin/sh\necho \"{program} $*\" >> '{}'\n", at(&root, "marks/started"))).unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::write(workspace.join("picture.pdf"), with_picture()).unwrap();
    std::fs::write(workspace.join("refers.pdf"), referring(&outside, port)).unwrap();

    let planted = tree(&outside);
    let watcher = PipeWatcher::start(&outside);
    let path = format!("{}:{}", at(&root, "bin"), std::env::var("PATH").unwrap_or_default());
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "the_agent_and_its_documents_reach_nothing_outside_the_workspace", "--nocapture", "--test-threads=1"])
        .current_dir(root.join("cwd"))
        .env(CHILD, &root)
        .env(PORT, port.to_string())
        .env("PATH", path)
        .env("HOME", root.join("home"))
        .env("TMPDIR", root.join("tmp"))
        .env("XDG_CONFIG_HOME", root.join("home/.config"))
        .env("XDG_DATA_HOME", root.join("home/.local/share"))
        .env("XDG_CACHE_HOME", root.join("home/.cache"))
        .env_remove("KITION_PDF_MODELS")
        .status()
        .unwrap();
    let mut wrong = Vec::new();
    let opened = watcher.stop();
    if !opened.is_empty() {
        wrong.push(format!("the kit opened files outside the workspace to read them: {opened:?}"));
    }
    if !status.success() {
        wrong.push(format!("the agent's session reached outside the workspace (see its output above): {status}"));
    }
    for dir in ["cwd", "home", "tmp", "marks"] {
        let found: Vec<String> = tree(&root.join(dir)).into_keys().collect();
        if !found.is_empty() {
            wrong.push(format!("the kit wrote into its {dir} folder or started a program: {found:?}"));
        }
    }
    if tree(&outside) != planted {
        wrong.push(format!("the folder outside the workspace changed: {:?}", tree(&outside).into_keys().collect::<Vec<_>>()));
    }
    let found: Vec<String> = tree(&workspace).into_keys().collect();
    if found != ["picture.pdf", "refers.pdf"] {
        wrong.push(format!("the workspace holds {found:?}, not only what the host saved and the test wrote"));
    }
    let asked = server.accept();
    if !matches!(&asked, Err(error) if error.kind() == std::io::ErrorKind::WouldBlock) {
        wrong.push(format!("the kit connected to a server a document named (KHP A37): {asked:?}"));
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    let _ = std::fs::remove_dir_all(&root);
}
