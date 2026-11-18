//! Quality gates: layering, wasm and the CI bundle.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, anyhow, bail};

use crate::layers;

/// Workspace root (two levels above the xtask crate, which lives in `tools/xtask`).
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2).expect("xtask lives in tools/xtask").to_path_buf()
}

fn cargo() -> Command {
    let mut c = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    c.current_dir(root());
    c
}

fn run(mut cmd: Command, what: &str) -> anyhow::Result<()> {
    eprintln!("$ {what}");
    let status = cmd.status().with_context(|| format!("{what}: failed to spawn"))?;
    if !status.success() {
        bail!("{what}: exited with {status}");
    }
    Ok(())
}

fn metadata() -> anyhow::Result<serde_json::Value> {
    let out = cargo().args(["metadata", "--format-version", "1", "--no-deps"]).output().context("cargo metadata")?;
    if !out.status.success() {
        bail!("cargo metadata failed:\n{}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(serde_json::from_slice(&out.stdout)?)
}

pub fn layers(_: &[String]) -> anyhow::Result<()> {
    let crates = layers::from_metadata(&metadata()?).map_err(|e| anyhow!(e))?;
    println!("Dependency layering (plan/architecture.md §3)\n");
    println!("{:<24} {:<14} workspace deps", "crate", "layer");
    for c in &crates {
        let ws: Vec<String> = c
            .deps
            .iter()
            .filter(|d| d.workspace)
            .map(|d| format!("{}{}", layers::short_name(&d.name), if d.kind == layers::DepKind::Dev { " (dev)" } else { "" }))
            .collect();
        println!("{:<24} {:<14} {}", c.name, layers::describe(layers::classify(&c.name)), ws.join(", "));
    }
    let violations = layers::check(&crates);
    println!();
    if violations.is_empty() {
        println!("OK: {} crates, no layering violations.", crates.len());
        return Ok(());
    }
    for v in &violations {
        println!("  - {v}");
    }
    bail!("{} layering violation(s)", violations.len())
}

pub fn wasm(_: &[String]) -> anyhow::Result<()> {
    let crates = layers::from_metadata(&metadata()?).map_err(|e| anyhow!(e))?;
    let set: Vec<String> = crates
        .into_iter()
        .filter(|c| matches!(layers::classify(&c.name), Some(layers::Class::Layer(_) | layers::Class::Standalone(_))))
        .map(|c| c.name)
        .collect();
    let mut failed = Vec::new();
    for pkg in &set {
        let mut c = cargo();
        c.args(["check", "--target", "wasm32-unknown-unknown", "-p", pkg]);
        if run(c, &format!("cargo check --target wasm32-unknown-unknown -p {pkg}")).is_err() {
            failed.push(pkg.clone());
        }
    }
    if failed.is_empty() {
        println!("wasm32: {} crates ok", set.len());
        Ok(())
    } else {
        bail!("wasm32 check failed for: {}", failed.join(", "))
    }
}

pub fn ci(_: &[String]) -> anyhow::Result<()> {
    type Step = (&'static str, Box<dyn Fn() -> anyhow::Result<()>>);
    let steps: Vec<Step> = vec![
        ("fmt", Box::new(|| run_args(&["fmt", "--all", "--", "--check"]))),
        ("clippy", Box::new(|| run_args(&["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]))),
        ("test", Box::new(|| run_args(&["test", "--workspace"]))),
        ("layers", Box::new(|| layers(&[]))),
        ("wasm", Box::new(|| wasm(&[]))),
        ("assets", Box::new(|| crate::assets::run(&[]))),
        ("deny", Box::new(|| deny(&[]))),
        ("parity", Box::new(|| crate::parity::run(&[]))),
    ];
    for (i, (name, f)) in steps.iter().enumerate() {
        eprintln!("\n=== ci: {name} ===");
        if let Err(e) = f() {
            println!("\nCI: {} passed, FAILED at `{name}`: {e:#}", i);
            bail!("ci failed at `{name}`");
        }
    }
    println!("\nCI: all {} steps passed", steps.len());
    Ok(())
}

/// Dependency licences, bans, sources and advisories (`deny.toml`). Skipped with a hint when
/// cargo-deny is not installed, except under CI (`CI` set), where it is required.
pub fn deny(_: &[String]) -> anyhow::Result<()> {
    let installed = Command::new("cargo-deny").arg("--version").output().is_ok_and(|o| o.status.success());
    if !installed {
        if std::env::var_os("CI").is_some() {
            bail!("cargo-deny is required in CI: cargo install cargo-deny --locked");
        }
        eprintln!("deny: SKIPPED, cargo-deny is not installed (cargo install cargo-deny --locked)");
        return Ok(());
    }
    run_args(&["deny", "--log-level", "error", "check"])
}

fn run_args(args: &[&str]) -> anyhow::Result<()> {
    let mut c = cargo();
    c.args(args);
    run(c, &format!("cargo {}", args.join(" ")))
}
