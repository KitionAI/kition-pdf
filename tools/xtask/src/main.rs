//! Project automation for Kition PDF, run via `cargo xtask <command>`.

use std::process::ExitCode;

mod assets;
mod demo_pdf;
mod gates;
mod golden;
mod layers;
mod parity;
mod version;

type Command = fn(&[String]) -> anyhow::Result<()>;

/// Every subcommand: name, one-line summary, entry point.
const COMMANDS: &[(&str, &str, Command)] = &[
    ("golden", "Inventory private directory/ZIP PDF corpora and compare native renders with external Poppler", golden::run),
    ("version", "Print the workspace version, or `version set X.Y.Z[-pre]` to change it and refresh Cargo.lock", version_cmd),
    ("layers", "Enforce the crate dependency layering (plan/architecture.md §3)", gates::layers),
    ("wasm", "cargo check --target wasm32-unknown-unknown for every crate below L8", gates::wasm),
    ("deny", "Dependency licences, bans, sources and advisories (deny.toml; needs cargo-deny)", gates::deny),
    ("ci", "fmt --check, clippy -D warnings, test, layers, wasm, assets, deny, parity (stops at first failure)", gates::ci),
    ("assets", "Enforce the asset policy (AGENTS.md §1) against docs/attribution.toml; --write regenerates docs/attribution.md", assets::run),
    ("parity", "Validate tools/parity/acrobat-features.toml against the registry, tools and tests; report progress (--json, --partial)", parity::run),
    ("models", "Fetch the OCR models (docs/attribution.toml kind = \"model\") into assets/models/, verified by SHA-256", assets::models),
    ("demo-pdf", "Build dist/demo/kition-pdf-showcase.pdf (needs Google Chrome or Chromium)", demo_pdf::run),
    ("eval", "The layer: eval cases with real models (kition-sdk tools/eval-cases.mjs); only a maintainer starts it (F-QA-16)", eval),
];

/// `cargo xtask eval`: the `layer: eval` cases through kition-sdk's runner (the checkout beside this one,
/// or `KITION_SDK_DIR`). Only a maintainer starts it: never a hook, CI or an agent on its own.
fn eval(args: &[String]) -> anyhow::Result<()> {
    let root = gates::root();
    let sdk = std::env::var_os("KITION_SDK_DIR").map(std::path::PathBuf::from).unwrap_or_else(|| root.join("..").join("kition-sdk"));
    let status = std::process::Command::new("node").arg(sdk.join("tools").join("eval-cases.mjs")).arg("--repo").arg(&root).args(args).status()?;
    anyhow::ensure!(status.success(), "eval cases: {status}");
    Ok(())
}

fn version_cmd(args: &[String]) -> anyhow::Result<()> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    version::run(&gates::root(), &args).map_err(anyhow::Error::msg)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(name) = args.first() else {
        print_help();
        return ExitCode::FAILURE;
    };
    if matches!(name.as_str(), "help" | "-h" | "--help") {
        print_help();
        return ExitCode::SUCCESS;
    }
    let Some((_, _, command)) = COMMANDS.iter().find(|(n, _, _)| n == name) else {
        eprintln!("xtask: unknown command `{name}`\n");
        print_help();
        return ExitCode::FAILURE;
    };
    match command(&args[1..]) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("xtask {name}: error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    eprintln!("Usage: cargo xtask <command> [args]\n\nCommands:");
    for (name, about, _) in COMMANDS {
        eprintln!("  {name:<12} {about}");
    }
    eprintln!("  {:<12} Show this list", "help");
}
