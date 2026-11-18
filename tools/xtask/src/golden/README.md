# Local golden PDF runner

Rust-only inventory and native PageRenderer baseline against an **external** Poppler
`pdftoppm` executable. Corpus PDFs and generated images stay in private local output;
never commit or publish them. The runner does not add fonts or extract embedded fonts.

```sh
cargo xtask golden inventory \
  --manifest /private/tmp/pdf-golden/manifest.json \
  --cache /private/tmp/pdf-golden/cache \
  --sources ../golden_ppt ../golden_excel.zip ../golden_word.zip
cargo xtask golden run \
  --manifest /private/tmp/pdf-golden/manifest.json \
  --out /private/tmp/pdf-golden/smoke --suite smoke
cargo xtask golden run \
  --manifest /private/tmp/pdf-golden/manifest.json \
  --out /private/tmp/pdf-golden/full --suite full --resume
```

Inventory accepts directories, individual PDFs, and ZIPs. ZIPs support stored and
DEFLATE entries, correct unflagged UTF-8 names, retain raw entry bytes as hex, reject
unsafe paths/symlinks/duplicates, exclude `__MACOSX` and `._*`, and cap PDFs at 512 MiB
and the corpus at 8 GiB. Extracted PDFs use SHA-256 filenames in the explicit cache.
Directory symlinks are not followed. Schema 2 is a native inventory. Every document is inspected in an isolated
process (default 120 seconds). Metadata failures remain in the manifest.

`smoke` deterministically chooses first, middle (rounded up), and last pages of **each
selected file**, removing duplicates. This can be a large suite: `--group golden_word`
and `--limit-files 1` select its first sorted document. Record the manifest and selected
pages for repeatability. `full` chooses every page. `--dpi` defaults to 72 (max 288).
`--timeout` defaults to 60 seconds **per process**; native and reference each get this
budget. Run uses a cold isolated native process per page, then external Poppler. It
validates all selected source SHA-256 values, keeps going on individual failures, and
returns failure after writing the report if any task failed. Missing corpus/Poppler
fails loudly. One page's render and text extraction share a native timeout budget.

`report.json` includes per-group counts, every selected file/page, native errors and
warnings, timings, raster-byte memory proxy, extracted text length/hash/excerpt and a
self-find check. Native warnings count as failures, not silent passes. `index.html`
shows native/reference/difference images. RGB uses white composition, CropBox, PDF
rotation, and matching DPI. Different dimensions are explicit geometry failures.
Metrics are absolute RGB channel error, fraction of pixels with any channel delta
above 16, and **global** luminance SSIM (not windowed SSIM). SSIM below 0.99 or pixel
fraction above 1% flags a review candidate, not a correctness verdict or failure gate.
Poppler antialiasing and host fallback fonts can produce legitimate differences.

`--resume` reuses only successful results with all three images and a matching
fingerprint (runner binary hash, revision, platform, Poppler version, DPI, timeout and
schema), after rechecking PDF hashes. Failures and warnings are retried. A binary
rebuild invalidates checkpoints; do not change the binary during a run. Reports and
page checkpoints are atomic. The output directory belongs to one run at a time.

Current limits: no SDK/WASM, save workflow, Ghostscript cross-check, windowed SSIM,
peak RSS, or warm-cache timing. Metadata covers all page sizes/CropBoxes/rotation,
font kind/embedding/subsetting/encoding, and document annotation/link/form counts.
Raw MediaBox/UserUnit, ToUnicode, images and transparency object features are explicitly
unmeasured. Text self-find is a plumbing check, not a labeled accuracy assessment.
The per-page process deliberately rereads the PDF to guarantee timeout isolation.
