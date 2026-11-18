//! Poppler is an external oracle only; no oracle code or private PDFs enter the product.
use super::*;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("kition-fidelity-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Redirect output to files so a full pipe cannot deadlock the timeout poller.
fn run(tool: &str, args: &[&std::ffi::OsStr], work: &Path, timeout: Duration) -> Result<Vec<u8>, String> {
    let stdout = work.join("stdout");
    let stderr = work.join("stderr");
    let mut child = Command::new(tool)
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&stdout).map_err(|e| e.to_string())?)
        .stderr(std::fs::File::create(&stderr).map_err(|e| e.to_string())?)
        .spawn()
        .map_err(|e| format!("{tool}: {e}"))?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            if !status.success() {
                return Err(format!("{tool}: {status}: {}", String::from_utf8_lossy(&std::fs::read(&stderr).unwrap_or_default())));
            }
            return std::fs::read(stdout).map_err(|e| e.to_string());
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{tool}: timeout after {} seconds", timeout.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn page_count(path: &Path, work: &Path, timeout: Duration) -> Result<usize, String> {
    let info = run("pdfinfo", &[path.as_os_str()], work, timeout)?;
    String::from_utf8_lossy(&info)
        .lines()
        .find_map(|line| line.strip_prefix("Pages:").and_then(|v| v.trim().parse().ok()))
        .filter(|n| *n > 0)
        .ok_or_else(|| "pdfinfo: missing or zero page count".into())
}

fn text(path: &Path, work: &Path, timeout: Duration) -> Result<Vec<u8>, String> {
    run("pdftotext", &["-layout".as_ref(), "-enc".as_ref(), "UTF-8".as_ref(), path.as_os_str(), "-".as_ref()], work, timeout)
}

fn pixels(path: &Path, page: usize, work: &Path, timeout: Duration) -> Result<Vec<u8>, String> {
    let page = page.to_string();
    let prefix = work.join("render");
    run(
        "pdftoppm",
        &[
            "-f".as_ref(),
            page.as_ref(),
            "-l".as_ref(),
            page.as_ref(),
            "-r".as_ref(),
            "72".as_ref(),
            "-singlefile".as_ref(),
            path.as_os_str(),
            prefix.as_os_str(),
        ],
        work,
        timeout,
    )?;
    std::fs::read(prefix.with_extension("ppm")).map_err(|e| e.to_string())
}

fn compare_saved(original: &Path, saved: &Path, work: &Path, timeout: Duration, all_pages: bool) -> Result<Value, String> {
    let pages = page_count(original, work, timeout).map_err(|e| format!("reference: {e}"))?;
    let saved_pages = page_count(saved, work, timeout).map_err(|e| format!("saved: {e}"))?;
    if pages != saved_pages {
        return Err(format!("page count changed: {pages} -> {saved_pages}"));
    }
    let original_text = text(original, work, timeout).map_err(|e| format!("reference: {e}"))?;
    let saved_text = text(saved, work, timeout).map_err(|e| format!("saved: {e}"))?;
    if original_text != saved_text {
        return Err(format!("all-page text changed: {} -> {} bytes", original_text.len(), saved_text.len()));
    }
    let mut selected: Vec<_> = if all_pages { (1..=pages).collect() } else { vec![1, pages.div_ceil(2), pages] };
    selected.sort_unstable();
    selected.dedup();
    for page in &selected {
        let before = pixels(original, *page, work, timeout).map_err(|e| format!("reference page {page}: {e}"))?;
        let after = pixels(saved, *page, work, timeout).map_err(|e| format!("saved page {page}: {e}"))?;
        if before != after {
            std::fs::write(work.join("before.ppm"), before).map_err(|e| e.to_string())?;
            std::fs::write(work.join("after.ppm"), after).map_err(|e| e.to_string())?;
            return Err(format!("pixels changed on page {page} at 72 dpi"));
        }
    }
    Ok(
        json!({ "pages": pages, "textPagesCompared": pages, "textBytes": original_text.len(), "visualPagesCompared": selected, "dpi": 72, "pageBox": "MediaBox" }),
    )
}

#[test]
fn corpus_save_oracle() {
    if std::env::var("KITION_FIDELITY_ORACLE").as_deref() != Ok("1") {
        eprintln!("SKIP: KITION_FIDELITY_ORACLE is not 1 (external save/rewrite comparison)");
        return;
    }
    let (root, files) = corpus_files().expect("KITION_PDF_CORPUS is required for the oracle");
    let timeout =
        Duration::from_secs(std::env::var("KITION_FIDELITY_TIMEOUT_SECS").map(|s| s.parse::<u64>().expect("positive timeout seconds")).unwrap_or(60));
    assert!(!timeout.is_zero(), "timeout must be positive");
    let all_pages = std::env::var("KITION_FIDELITY_VISUAL_ALL").as_deref() == Ok("1");
    let scratch = Scratch::new();
    let mut versions = serde_json::Map::new();
    for tool in ["pdfinfo", "pdftotext", "pdftoppm"] {
        run(tool, &["-v".as_ref()], &scratch.0, timeout).unwrap_or_else(|e| panic!("oracle unavailable: {e}"));
        versions.insert(tool.into(), json!(std::fs::read_to_string(scratch.0.join("stderr")).unwrap().lines().next().unwrap_or("unknown")));
    }
    let out = std::env::var_os("KITION_FIDELITY_OUT").map(PathBuf::from);
    if let Some(out) = &out {
        std::fs::create_dir_all(out).unwrap();
    }
    let mut progress = out.as_ref().map(|out| std::fs::File::create(out.join("pdf-save-oracle.jsonl")).unwrap());
    let mut results = Vec::new();
    let mut failures = 0;
    for (index, path) in files.iter().enumerate() {
        let bytes = std::fs::read(path).unwrap();
        for leg in [Leg::Save, Leg::Rewrite] {
            let start = Instant::now();
            for name in ["saved.pdf", "before.ppm", "after.ppm"] {
                let _ = std::fs::remove_file(scratch.0.join(name));
            }
            let result = (|| {
                let doc = match prepare(leg, &bytes)? {
                    Prepared::Ready(doc) => doc,
                    Prepared::Skip(reason) => return Ok(json!({ "status": "skipped", "reason": reason })),
                };
                let written = export(leg, &doc)?;
                if leg.incremental() && !written.starts_with(&bytes) {
                    return Err("original prefix was lost".into());
                }
                import(&written).map_err(|e| format!("engine reopen: {e}"))?;
                let saved = scratch.0.join("saved.pdf");
                std::fs::write(&saved, written).map_err(|e| e.to_string())?;
                let mut measured = compare_saved(path, &saved, &scratch.0, timeout, all_pages)?;
                measured["status"] = json!("passed");
                Ok::<_, String>(measured)
            })();
            let mut record = match result {
                Ok(record) => record,
                Err(error) => {
                    failures += 1;
                    let mut record = json!({ "status": "failed", "reason": error });
                    if let Some(out) = &out {
                        let artifacts = out.join("oracle-failures").join(format!("{index:04}-{}", leg.name()));
                        std::fs::create_dir_all(&artifacts).unwrap();
                        for name in ["saved.pdf", "before.ppm", "after.ppm"] {
                            let source = scratch.0.join(name);
                            if source.is_file() {
                                std::fs::copy(source, artifacts.join(name)).unwrap();
                            }
                        }
                        record["artifacts"] = json!(artifacts);
                    }
                    record
                }
            };
            record["file"] = json!(path.strip_prefix(&root).unwrap_or(path).to_string_lossy());
            record["leg"] = json!(leg.name());
            record["elapsedMs"] = json!(start.elapsed().as_millis());
            eprintln!("{record}");
            if let Some(progress) = &mut progress {
                use std::io::Write;
                writeln!(progress, "{record}").unwrap();
                progress.flush().unwrap();
            }
            results.push(record);
        }
    }
    let summary = json!({ "corpusFiles": files.len(), "attempted": results.len(), "failed": failures, "oracleVersions": versions,
        "commandTimeoutSeconds": timeout.as_secs(), "pageBox": "MediaBox", "visualScope": if all_pages { "all" } else { "first-middle-last" }, "results": results });
    if let Some(out) = out {
        std::fs::write(out.join("pdf-save-oracle.json"), serde_json::to_vec_pretty(&summary).unwrap()).unwrap();
    }
    assert_eq!(failures, 0, "save/rewrite external fidelity failures; see per-file records");
}

#[test]
fn corpus_discovery_rejects_empty_and_missing_and_finds_nested_pdfs() {
    let scratch = Scratch::new();
    assert!(corpus_files_at(&scratch.0).unwrap_err().contains("no PDFs"));
    assert!(corpus_files_at(&scratch.0.join("missing")).is_err());
    let nested = scratch.0.join("nested");
    std::fs::create_dir(&nested).unwrap();
    std::fs::write(nested.join("sample.PDF"), synthetic()).unwrap();
    std::fs::write(nested.join("._sample.pdf"), b"resource fork").unwrap();
    let metadata = scratch.0.join("__MACOSX");
    std::fs::create_dir(&metadata).unwrap();
    std::fs::write(metadata.join("ignored.pdf"), b"resource fork").unwrap();
    assert_eq!(corpus_files_at(&scratch.0).unwrap(), vec![nested.join("sample.PDF")]);
}

#[test]
fn oracle_detects_text_and_visual_corruption() {
    if std::env::var("KITION_FIDELITY_ORACLE").as_deref() != Ok("1") {
        return;
    }
    let scratch = Scratch::new();
    let original = scratch.0.join("original.pdf");
    let saved = scratch.0.join("saved.pdf");
    let bytes = synthetic();
    std::fs::write(&original, &bytes).unwrap();
    let mut doc = import(&bytes).unwrap();
    std::fs::write(&saved, export(Leg::Rewrite, &doc).unwrap()).unwrap();
    let timeout = Duration::from_secs(60);
    assert!(compare_saved(&original, &saved, &scratch.0, timeout, true).is_ok());
    let page = kition_pdf_model::pages(&doc)[0].obj;
    let content = doc.get(page).as_dict().unwrap().reference(b"Contents").unwrap();
    for (content_text, expected_error) in
        [("BT /F1 12 Tf 72 720 Td (Changed) Tj ET", "text changed"), ("BT /F1 12 Tf 72 720 Td (First) Tj ET 0 0 20 20 re f", "pixels changed")]
    {
        doc.set(content, Object::Stream(kition_pdf_cos::Stream::from_raw(Dict::new(), content_text.as_bytes().to_vec())));
        std::fs::write(&saved, export(Leg::Rewrite, &doc).unwrap()).unwrap();
        assert!(compare_saved(&original, &saved, &scratch.0, timeout, true).unwrap_err().contains(expected_error));
    }
}
