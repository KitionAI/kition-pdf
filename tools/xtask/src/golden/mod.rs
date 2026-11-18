//! Private, local-only golden corpus measurement. Never bundles corpus assets.
mod inventory;
mod runner;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const SCHEMA: u32 = 2;
const MAX_PDF: u64 = 512 * 1024 * 1024;
const MAX_TOTAL: u64 = 8 * 1024 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Manifest {
    schema_version: u32,
    kind: String,
    environment: Value,
    files: Vec<Entry>,
}
#[derive(Serialize, Deserialize)]
struct Entry {
    source: String,
    group: String,
    path: PathBuf,
    archive: Option<PathBuf>,
    archive_entry_raw_hex: Option<String>,
    sha256: String,
    bytes: u64,
    metadata: Value,
}

pub fn run(args: &[String]) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("inventory") => inventory::run(&args[1..]),
        Some("run") => runner::run(&args[1..]),
        Some("worker-inspect") => inventory::worker(&args[1..]),
        Some("worker-render") => runner::worker(&args[1..]),
        _ => bail!(
            "usage: cargo xtask golden inventory --manifest FILE --cache DIR --sources DIR_OR_ZIP ... [--timeout 120]\n       cargo xtask golden run --manifest FILE --out DIR --suite smoke|full [--dpi 72] [--timeout 60] [--resume] [--group NAME] [--limit-files N]"
        ),
    }
}

fn arg(args: &[String], name: &str) -> Result<String> {
    option(args, name).with_context(|| format!("missing {name}"))
}
fn option(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).filter(|s| !s.starts_with("--")).cloned()
}
fn number(args: &[String], name: &str, default: u64) -> Result<u64> {
    let n = option(args, name).map(|v| v.parse()).transpose()?.unwrap_or(default);
    ensure!(n > 0, "{name} must be positive");
    Ok(n)
}
fn json_write(path: &Path, value: &impl Serialize) -> Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&temporary, serde_json::to_vec_pretty(value)?)?;
    fs::rename(temporary, path)?;
    Ok(())
}
fn sha(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    ensure!(file.metadata()?.len() <= MAX_PDF, "PDF exceeds {MAX_PDF} bytes: {}", path.display());
    sha_stream(&mut file)
}
fn sha_stream(file: &mut fs::File) -> Result<String> {
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(buffer.get(..read).context("invalid read size")?);
    }
    Ok(hex(&digest.finalize()))
}
fn timed(command: &mut Command, seconds: u64, log: &Path) -> Result<Value> {
    let stderr = fs::File::create(log)?;
    let start = Instant::now();
    let mut child = command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(stderr).spawn()?;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(
                json!({"status": if status.success() {"ok"} else {"error"}, "exit_code": status.code(), "elapsed_ms": start.elapsed().as_millis()}),
            );
        }
        if start.elapsed() >= Duration::from_secs(seconds) {
            child.kill().context("terminate timed-out worker")?;
            child.wait()?;
            return Ok(json!({"status":"timeout", "elapsed_ms": start.elapsed().as_millis()}));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn environment() -> Result<Value> {
    let revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|v| v.status.success())
        .map(|v| String::from_utf8_lossy(&v.stdout).trim().to_string());
    let poppler = Command::new("pdftoppm").arg("-v").output().ok().map(|v| String::from_utf8_lossy(&v.stderr).trim().to_string());
    let runner_hash = sha_stream(&mut fs::File::open(std::env::current_exe()?)?)?;
    Ok(
        json!({"engine_revision":revision, "runner_sha256": runner_hash, "os":std::env::consts::OS,"arch":std::env::consts::ARCH,"poppler":poppler,"font_configuration":"native embedded defaults; no injected font; Poppler may use host fallback", "kition_fonts_dir_build_input":option_env!("KITION_FONTS_DIR"),"build_profile":if cfg!(debug_assertions) {"debug"} else {"release"}}),
    )
}

fn validate_args(args: &[String], values: &[&str], flags: &[&str], multiple: Option<&str>) -> Result<()> {
    let mut seen = std::collections::BTreeSet::new();
    let mut i = 0;
    while let Some(key) = args.get(i) {
        ensure!(seen.insert(key), "duplicate option {key}");
        if flags.contains(&key.as_str()) {
            i += 1;
            continue;
        }
        ensure!(values.contains(&key.as_str()) || multiple == Some(key.as_str()), "unknown option {key}");
        i += 1;
        ensure!(args.get(i).is_some_and(|value| !value.starts_with("--")), "missing value for {key}");
        if multiple == Some(key.as_str()) {
            while args.get(i).is_some_and(|value| !value.starts_with("--")) {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reject_unknown_missing_and_duplicate_options() {
        for input in
            [vec!["--limt-files", "1"], vec!["--limit-files"], vec!["--limit-files", "--resume"], vec!["--limit-files", "1", "--limit-files", "2"]]
        {
            let args = input.into_iter().map(str::to_string).collect::<Vec<_>>();
            assert!(validate_args(&args, &["--limit-files"], &["--resume"], None).is_err());
        }
        assert!(validate_args(&["--limit-files".into(), "1".into(), "--resume".into()], &["--limit-files"], &["--resume"], None).is_ok());
    }
    #[test]
    #[cfg(unix)]
    fn timeout_reaps_the_child() {
        let log = std::env::temp_dir().join(format!("kition-golden-timeout-{}.log", std::process::id()));
        let mut command = Command::new("sleep");
        command.arg("30");
        let result = timed(&mut command, 1, &log).unwrap();
        assert_eq!(result["status"], "timeout");
        assert!(result["elapsed_ms"].as_u64().unwrap() < 5000);
        fs::remove_file(log).unwrap();
    }
}
