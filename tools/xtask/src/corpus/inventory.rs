use super::*;
use std::{collections::BTreeSet, io::Write, path::Component, sync::Arc};

pub(super) fn run(args: &[String]) -> Result<()> {
    validate_args(args, &["--manifest", "--cache", "--timeout"], &[], Some("--sources"))?;
    let manifest = PathBuf::from(arg(args, "--manifest")?);
    let cache = PathBuf::from(arg(args, "--cache")?);
    fs::create_dir_all(&cache)?;
    let cache = fs::canonicalize(cache)?;
    let timeout = number(args, "--timeout", 120)?;
    let start = args.iter().position(|s| s == "--sources").context("missing --sources")? + 1;
    let sources: Vec<_> = args.iter().skip(start).take_while(|s| !s.starts_with("--")).collect();
    ensure!(!sources.is_empty(), "--sources requires directories, PDFs or ZIPs");
    let mut entries = Vec::new();
    let mut total = 0u64;
    for source in sources {
        let source = fs::canonicalize(source).with_context(|| format!("source does not exist: {source}"))?;
        let group = source.file_stem().context("source has no name")?.to_string_lossy().into_owned();
        if source.is_dir() {
            walk(&source, &source, &group, &mut entries, &mut total)?;
        } else if extension(&source, "zip") {
            extract(&source, &group, &cache, &mut entries, &mut total)?;
        } else if extension(&source, "pdf") {
            entries.push(entry(
                source.clone(),
                group.clone(),
                format!("{group}/{}", source.file_name().context("missing name")?.to_string_lossy()),
                None,
                None,
                &mut total,
            )?);
        } else {
            bail!("unsupported source {}", source.display());
        }
    }
    ensure!(!entries.is_empty(), "no PDFs found; corpus must not silently skip an empty corpus");
    entries.sort_by(|a, b| a.source.cmp(&b.source));
    let mut names = BTreeSet::new();
    for e in &entries {
        ensure!(names.insert(e.source.clone()), "duplicate normalized source {}", e.source);
    }
    for (i, entry) in entries.iter_mut().enumerate() {
        let metadata_path = cache.join(format!("{}.metadata.json", entry.sha256));
        let log = cache.join(format!("{}.inspect.log", entry.sha256));
        // Never reuse stale metadata after changing the engine.
        let mut command = Command::new(std::env::current_exe()?);
        command.args(["corpus", "worker-inspect"]).arg(&entry.path).arg(&metadata_path);
        let status = timed(&mut command, timeout, &log)?;
        entry.metadata = if status["status"] == "ok" {
            serde_json::from_slice(&fs::read(metadata_path)?)?
        } else {
            json!({"status":status["status"],"process":status,"log":log})
        };
        eprintln!("inventory {}/{}: {} ({})", i + 1, names.len(), entry.source, entry.metadata["status"]);
    }
    let report = Manifest { schema_version: SCHEMA, kind: "native_corpus_inventory".into(), environment: environment()?, files: entries };
    json_write(&manifest, &report)?;
    eprintln!("wrote {} files to {}", report.files.len(), manifest.display());
    let failed = report.files.iter().filter(|entry| entry.metadata["status"] != "ok").count();
    ensure!(failed == 0, "{failed} inventory inspections failed; manifest contains all failures");
    Ok(())
}
fn extension(path: &Path, ext: &str) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext))
}
fn ignored(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s == "__MACOSX" || s.starts_with("._")
    })
}
fn walk(root: &Path, dir: &Path, group: &str, entries: &mut Vec<Entry>, total: &mut u64) -> Result<()> {
    // Iterative traversal avoids input-controlled recursion; do not follow symlinks.
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for item in fs::read_dir(&dir)? {
            let item = item?;
            let path = item.path();
            if ignored(path.strip_prefix(root)?) {
                continue;
            }
            let kind = item.file_type()?;
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() && extension(&path, "pdf") {
                let name = format!("{group}/{}", path.strip_prefix(root)?.to_string_lossy());
                entries.push(entry(path, group.into(), name, None, None, total)?);
            }
        }
    }
    Ok(())
}
fn entry(path: PathBuf, group: String, source: String, archive: Option<PathBuf>, raw: Option<String>, total: &mut u64) -> Result<Entry> {
    let bytes = fs::metadata(&path)?.len();
    *total = total.checked_add(bytes).context("corpus size overflow")?;
    ensure!(*total <= MAX_TOTAL, "corpus exceeds {MAX_TOTAL} bytes");
    Ok(Entry { source, group, sha256: sha(&path)?, path, archive, archive_entry_raw_hex: raw, bytes, metadata: Value::Null })
}
fn safe_name(raw: &[u8], fallback: &str) -> Result<PathBuf> {
    // Some exporters store UTF-8 bytes without the ZIP UTF-8 bit. Preserve those bytes.
    let name = std::str::from_utf8(raw).unwrap_or(fallback);
    let drive_prefix = name.as_bytes().first().is_some_and(u8::is_ascii_alphabetic) && name.as_bytes().get(1) == Some(&b':');
    ensure!(!name.contains(['\\', '\0']) && !drive_prefix, "unsafe ZIP entry {name:?}");
    let path = PathBuf::from(name);
    ensure!(!path.as_os_str().is_empty() && path.components().all(|c| matches!(c, Component::Normal(_))), "unsafe ZIP entry {name:?}");
    Ok(path)
}
fn extract(source: &Path, group: &str, cache: &Path, entries: &mut Vec<Entry>, total: &mut u64) -> Result<()> {
    let mut archive = zip::ZipArchive::new(fs::File::open(source)?)?;
    ensure!(archive.len() <= 100_000, "too many ZIP entries");
    let mut names = BTreeSet::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        // The decoded name is only the fallback for raw names that are not UTF-8.
        let decoded = file.name().map(|name| name.into_owned()).unwrap_or_default();
        let name = safe_name(file.name_raw(), &decoded)?;
        if ignored(&name) || file.is_dir() || !extension(&name, "pdf") {
            continue;
        }
        ensure!(file.unix_mode().is_none_or(|mode| mode & 0o170000 != 0o120000), "ZIP symlink forbidden");
        ensure!(names.insert(name.clone()), "duplicate ZIP entry {}", name.display());
        ensure!(file.size() <= MAX_PDF && total.saturating_add(file.size()) <= MAX_TOTAL, "ZIP uncompressed size limit exceeded");
        let raw = file.name_raw().iter().map(|b| format!("{b:02x}")).collect::<String>();
        // Flat cache names never expose archive paths to filesystem traversal or symlinks.
        let temporary = cache.join(format!("extract-{}-{index}.pdf", std::process::id()));
        let mut output = fs::OpenOptions::new().write(true).create_new(true).open(&temporary)?;
        let copied = std::io::copy(&mut (&mut file).take(MAX_PDF + 1), &mut output)?;
        output.flush()?;
        ensure!(copied == file.size() && copied <= MAX_PDF, "ZIP entry size mismatch");
        drop(output);
        let hash = sha(&temporary)?;
        let path = cache.join(format!("{hash}.pdf"));
        if path.exists() {
            ensure!(sha(&path)? == hash, "cache hash mismatch");
            fs::remove_file(&temporary)?;
        } else {
            fs::rename(&temporary, &path)?;
        }
        entries.push(entry(
            path,
            group.into(),
            if name.starts_with(group) { name.to_string_lossy().into_owned() } else { format!("{group}/{}", name.to_string_lossy()) },
            Some(source.into()),
            Some(raw),
            total,
        )?);
    }
    Ok(())
}

pub(super) fn worker(args: &[String]) -> Result<()> {
    let path = args.first().context("missing PDF path")?;
    let out = args.get(1).context("missing metadata output")?;
    let start = Instant::now();
    ensure!(fs::metadata(path)?.len() <= MAX_PDF, "PDF too large");
    let info = kition_pdf_render::inspect(Arc::new(fs::read(path)?), None)?;
    let pages: Vec<_> = info
        .pages
        .iter()
        .enumerate()
        .map(|(i, p)| json!({"page":i+1,"width_pt":p.width,"height_pt":p.height,"crop_box":p.crop,"rotation":p.rotation,"label":p.label}))
        .collect();
    let fonts: Vec<_> =
        info.fonts.iter().map(|f| json!({"name":f.name,"kind":f.kind,"embedded":f.embedded,"subset":f.subset,"encoding":f.encoding})).collect();
    json_write(
        Path::new(out),
        &json!({"status":"ok","elapsed_ms":start.elapsed().as_millis(),"page_count":pages.len(),"pages":pages,"fonts":fonts,"annotations":info.annotations.len(),"links":info.links.len(),"fields":info.fields.len(),"tagged":info.tagged,"encrypted":info.encrypted,"pdf_version":info.pdf_version,"warnings":info.warnings,"unmeasured_features":["ToUnicode","image counts","transparency/masks","raw MediaBox/UserUnit"]}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zip_paths_and_unflagged_utf8() {
        for name in ["../bad.pdf", "/bad.pdf", "a/../../bad.pdf", "C:/bad.pdf", "a\\bad.pdf"] {
            assert!(safe_name(name.as_bytes(), name).is_err());
        }
        assert_eq!(safe_name("字体.pdf".as_bytes(), "mojibake.pdf").unwrap(), PathBuf::from("字体.pdf"));
        assert!(ignored(Path::new("__MACOSX/._x.pdf")));
        assert!(safe_name("golden_word/保存:另存为:下载/样本.pdf".as_bytes(), "unused").is_ok());
    }
}
