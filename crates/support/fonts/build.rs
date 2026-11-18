//! Embeds the Japanese, Arabic and Telugu faces of the optional kition-fonts build input
//! (`KITION_FONTS_DIR`, a checkout's `fonts/manifest.txt`) as `KITION_FONTS`, and records whether
//! the variable was set as `KITION_FONTS_DIR_SET`; unset, `KITION_FONTS` is empty. It only reads
//! the local checkout (no network).
//!
//! Only those scripts (`src/built_in.rs`): a kition-fonts checkout also holds Chinese, Latin and
//! emoji faces, which are run-time fonts a host adds, as Kition's web editor does. Built in, they
//! would make a build with the checkout write different PDFs from one without it.
//!
//! On wasm32 it embeds only the UI faces (BIZ UDPGothic Regular and any `Arab` or `Telu` face), to
//! keep the web build within hosting limits (Cloudflare Pages: 25 MiB per file).

use std::fmt::Write as _;
use std::path::PathBuf;

#[path = "src/built_in.rs"]
mod built_in;

fn main() {
    println!("cargo::rerun-if-env-changed=KITION_FONTS_DIR");
    println!("cargo::rerun-if-env-changed=KITION_FONTS_REQUIRED");
    let dir = std::env::var_os("KITION_FONTS_DIR").map(PathBuf::from);
    let mut src = format!("pub const KITION_FONTS_DIR_SET: bool = {};\n", dir.is_some());
    src.push_str("pub static KITION_FONTS: &[KitionFont] = &[\n");
    if let Some(dir) = dir {
        match kition_fonts(&dir) {
            Ok(entries) if entries.is_empty() => println!(
                "cargo::warning=KITION_FONTS_DIR={}: no face to build in (Japanese, Arabic or Telugu), \
                 so the tests that need one fail and name it",
                dir.display()
            ),
            Ok(entries) => src.push_str(&entries),
            Err(e) if std::env::var_os("KITION_FONTS_REQUIRED").is_some() => {
                println!("cargo::error=KITION_FONTS_DIR={}: {e}", dir.display());
            }
            Err(e) => println!("cargo::warning=building without kition-fonts: KITION_FONTS_DIR={}: {e}", dir.display()),
        }
    }
    src.push_str("];\n");
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap_or_default()).join("kition_fonts.rs");
    if let Err(e) = std::fs::write(&out, src) {
        println!("cargo::error=writing {}: {e}", out.display());
    }
}

/// Family-name prefixes of the faces AGENTS.md §1.1 forbids: Adobe's Source families and Noto
/// CJK, which is Source Han under another name. The test in `src/kition_fonts.rs` has the same list.
const BARRED_FAMILIES: [&str; 5] = ["Source Han", "Source Serif", "Source Sans", "Noto Sans CJK", "Noto Serif CJK"];

/// One `KitionFont { .. }` initialiser per manifest line of a face that is built in.
fn kition_fonts(dir: &std::path::Path) -> Result<String, String> {
    let manifest = dir.join("fonts/manifest.txt");
    println!("cargo::rerun-if-changed={}", manifest.display());
    let text = std::fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
    let wasm = std::env::var("CARGO_CFG_TARGET_ARCH").is_ok_and(|a| a == "wasm32");
    let mut out = String::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let f: Vec<&str> = line.split(" | ").map(str::trim).collect();
        let [family, style, file, scripts, ..] = f.as_slice() else {
            return Err(format!("malformed manifest line: {line}"));
        };
        // AGENTS.md §1.1 bars Adobe's type designs whatever their licence, so a checkout that
        // carries one (Noto Sans CJK JP, which is Source Han) never embeds it.
        if BARRED_FAMILIES.iter().any(|barred| family.starts_with(barred)) {
            println!("cargo::warning=kition-fonts: not embedding {family} {style} (AGENTS.md §1.1)");
            continue;
        }
        let script_list: Vec<&str> = scripts.split(',').map(str::trim).collect();
        if !built_in::built_in(&script_list) {
            continue;
        }
        // Arabic interface faces are small, so the web build keeps them too.
        let arabic = script_list.contains(&"Arab");
        // Telugu faces too, for the Telugu interface.
        let telugu = script_list.contains(&"Telu");
        let web_face = (*family == "BIZ UDPGothic" && *style == "Regular") || arabic || telugu;
        if wasm && !web_face {
            continue;
        }
        let path = dir.join(file).canonicalize().map_err(|e| format!("{file}: {e}"))?;
        println!("cargo::rerun-if-changed={}", path.display());
        let scripts: Vec<String> = script_list.iter().map(|s| format!("{s:?}")).collect();
        let _ = writeln!(
            out,
            "    KitionFont {{ family: {family:?}, style: {style:?}, scripts: &[{}], bytes: include_bytes!({:?}) }},",
            scripts.join(", "),
            path.display().to_string(),
        );
    }
    Ok(out)
}
