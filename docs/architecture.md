# Kition PDF architecture

> **Last reviewed:** 2026-11-17 · **Last updated:** 2026-11-17 · **Target:** Adobe Acrobat Pro (Acrobat DC, continuous track 26.002.21931, macOS)

How Kition PDF is built today. The design rationale and the planned crates live in
`plan/architecture.md` and `plan/adr/` (local-only); this file describes what is on `main`. Each
crate's `README.md` has the detail.

## Shape

One Cargo workspace (`Cargo.toml`, edition 2024, Rust ≥ 1.90, `MIT OR Apache-2.0`), 29 library
crates in `crates/`, the kit's two bindings in `bindings/`, its editor in `web/` (React on
kition-sdk's `@kition/sdk/editor`), build tooling in `tools/xtask/`, and patched third-party crates in
`vendor/`.

| Binary | What it is |
|---|---|
| `bindings/headless` (`kition-pdf`) | The kit's headless executable: Kition starts it with `--kition-kit`, and agents reach it through the KHP `pdf_*` tools of Kition's kits MCP server |
| `bindings/wasm` (`kition-pdf-kit`) | The engine compiled to WebAssembly for the kit's editor (`web/`) |

This repository has no desktop app and no command-line tool.

## Layers

`cargo xtask layers` enforces the layering (`tools/xtask/src/layers.rs`). A crate may depend only on
crates in lower layers; L0–L1 crates are standalone and publishable.

| Layer | Crates today | Role |
|---|---|---|
| L0 foundation | `filters`, `crypt` | Stream filters (Flate, LZW, predictors, ASCII85, ASCIIHex, RunLength); standard security handler R2–R6 (RC4, AES-128/256, SASLprep) |
| L1 objects | `cos` | Lazy, tolerant parser with xref repair; copy-on-write object graph; incremental and full writers with object streams; decryption on load and re-encryption on save |
| L2 core model | `content`, `model`, `fonts` | Content-stream operators with source spans and a serializer; typed page tree with inherited attributes and display geometry (`/UserUnit`, `/Rotate`); font metrics and encodings for generated appearances |
| L3 core services | `render`, `annot`, `forms`, `js`, `xfa` | Rendering and inspection (bootstrap: vendored `hayro`, `lopdf`); annotation builders and appearance streams; AcroForm model, filling, appearances; sandboxed JavaScript (boa) and FormCalc; XFA template layout and data |
| L4 features | `edit`, `organize`, `redact`, `xfdf`, `sign`, `ocr`, `create`, `export`, `optimize`, `preflight`, `a11y`, `compare`, `measure`, `print` | One crate per Acrobat tool family |
| L5 bridge | `kdm-model` | The `kit.pdf` KDM bridge (Kition's own): the document as a kition-sdk replica, a base file and an overlay of edited objects |
| L6 façade | `engine` | Session, open documents, edit history, the command registry and tool catalogue. Frontends never reach past it |
| L7 platform and bindings | `platform`, `automation`, `kition-pdf-kit`, `kition-pdf` | OS services; headless JSON-Schema tools; the engine the kit's editor runs in the page (WebAssembly) and the headless executable for agents, both on those tools |

Planned crates that the layer table already reserves (`arlington`, `color`, `ops`, `text`, `ml`,
`security`, `prepress`, `search`, `media`, `ai`, `viewport`, `tools`, `ui-common`) don't exist yet;
neither does a `geom` crate.

## Data model and editing

The PDF object graph *is* the model (`crates/engine/engine/README.md` §The editing model):

1. Each open document holds a `kition_pdf_cos::Document`: the parsed objects plus a copy-on-write
   overlay. Unknown data is preserved because nothing is rebuilt that wasn't understood.
2. An edit runs on a clone; on success the previous state goes onto the undo stack. Clones share
   unchanged data, so snapshots are cheap.
3. After every edit an incremental write produces the working file (original bytes plus one
   revision); the view is refreshed from those bytes, so what you see is what Save writes.
4. Saves are incremental and atomic. A full rewrite happens only where it must: redaction, PDF/A
   conversion, print imposition, a changed password, Save As with garbage collection.
5. Autosave and crash recovery keep encrypted documents encrypted. Signed documents are protected
   from rewrites.

## Rendering and text

- Pages are rasterized on the CPU by the vendored `hayro` family (`hayro`, `hayro-interpret`,
  `hayro-syntax`, `hayro-jbig2`), with local patches for appearance states, layers, resource
  limits, cancellation, tiles and `/UserUnit` (`vendor/README.md`). A render pool renders visible
  pages first, then neighbours and thumbnails; tiles keep deep zoom sharp; a watchdog skips
  pathological pages.
- The kit's editor (`web/`) shows the rasters; the headless executable returns them as PNG
  previews.
- Text extraction (reading order, columns, vertical CJK, rotated text) feeds find, select, copy,
  redaction, export and compare; word-F1 0.98 against `pdftotext` on the corpus.
- The replacement (M2, ADR-0004) is the `model` crate, our own font engine and DisplayList devices;
  only the first pieces of `model` exist.

## File I/O

- PDF in: any revision chain, xref tables or streams, hybrid files, damaged files (repaired and
  logged), all standard-security revisions. Image codecs (DCT, JPX, JBIG2, CCITT) decode through
  the bootstrap renderer.
- PDF out: incremental or full saves, object streams, re-encryption, resource de-duplication when
  combining.
- Other formats: images and text in (`create`); PNG/JPEG/TIFF, text, RTF, Word and HTML out
  (`export`, `render`); FDF/XFDF/XML/CSV/TXT form and comment data (`xfdf`); PKCS #12, X.509,
  PEM/DER certificates (`sign`). Full matrix: [file-format-parity.md](parity/file-format-parity.md).

## Agent control

Every user-facing feature is reachable headlessly (`AGENTS.md` §3):

- the **command registry** (`crates/engine/engine/src/commands.rs`) is what the editor's menus and
  shortcuts are built from;
- the **automation tool table** (`crates/engine/automation`) is what the SDK (`bindings/wasm`)
  offers agents as commands, which reach them as the KHP `pdf_*` tools on `kition-pdf`.

## Safety

- `unsafe_code = "forbid"` workspace-wide; crates deny `unwrap`/`expect`/`panic!`; engine entry
  points keep a last-resort `catch_unwind` (`AGENTS.md` §4).
- Every input-derived size is bounded (decompression, images, recursion, content per page).
- `crates/formats/filters/fuzz` is a cargo-fuzz target for the stream decoders; every crash fix
  comes with a synthetic regression test (`AGENTS.md` §4).

## Build and quality gates

`cargo xtask ci` runs fmt, clippy (`-D warnings`), tests, the layering check, the WASM check,
`xtask assets` (asset licences and hashes), `xtask parity` (checklist claims must cite real commands,
tools and tests) and `xtask deny` (dependency licences). Other tasks: `xtask golden` (private
corpora rendered and compared with Poppler), `xtask demo-pdf`, `xtask models` (OCR models),
`xtask version`, and `xtask eval` (real-model cases; only a maintainer starts it). This repository's only
workflow is the Package workflow (`.github/workflows/package.yml`), which builds and publishes the
kit's package on GitHub with Kition's reusable `kit-package.yml`.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-17 | major | First public version |
