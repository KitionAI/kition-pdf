# Original embedded-font mapping through the browser Worker

This PDF-only harness uses contributor-original geometric font programs built
by the native `font_mapping` integration test. No corpus font is extracted,
decoded, copied or displayed. The glyphs are an empty outline, a left bar and
a right L on a 40-by-40-point page, tested at scales 1 and 2.

The coordinating task freezes the native matrix and schedules builds and
browser runs serially. From `web/`, enrich the native export without changing
its PDFs, fonts or raw manifest:

```sh
node tests/font-manifest.mjs \
  --raw <fixture-directory>/manifest.raw.json \
  --out <fixture-directory>/manifest-pinned.json
```

The enrichment pins both PDF and original font SHA256 values, keeps native
classification metadata and adds six explicit PDF Y-up measurement regions.
It refuses to replace a differing existing manifest. The two ToUnicode twins
have explicit same-backend full-RGBA comparisons; cross-font programs are not
assumed pixel-identical merely because their interiors agree.

Run each browser with an isolated compiled package and frozen shared SDK:

```sh
KITION_ENGINE_REVISION=<verified-source-digest> \
KITION_ENGINE_PROFILE=<verified-build-profile> \
node tests/font.mjs \
  --manifest <fixture-directory>/manifest-pinned.json \
  --engine-dir <isolated-engine-package> \
  --shared-dir <frozen-sdk-package> \
  --out <report.json> --browser chromium --timeout-ms 20000
```

`--shared-dir` is optional, as for [the hairline harness](hairline.md): without it the
harness runs on the installed `@kition/sdk` the editor resolves (what `pnpm test:font` does through
`tests/ci.mjs`, which also exports and pins the fixtures and runs both browsers).

Repeat with `--browser webkit` and a separate output file. The ordinary engine
and live shared sources remain untouched. Frozen alias/request guards come
from [the hairline harness](hairline.md). Fresh contexts, initially null host
connections and unique filenames avoid stale readiness. A read-only bookmark
command captures the existing editor engine; all samples use its real Worker.
No second document engine or page-thread WASM instance is created. The report
records source hashes before/after execution and actual isolated asset loads.

Left, right and foot probes identify the chosen geometric shape. A white
counter distinguishes the authored L from a solid block, a red continuation
marker proves later painting ran, and a white background checks localization.
Every pixel center in each rectangle is measured; no averaging hides wrong
pixels. A missing pixel population, transparent pixel or unknown combination
of glyph probes fails measurement integrity. Declared tolerances apply per
channel. Empty pixels cannot distinguish empty .notdef from a missing glyph;
native traced-GID and no-fallback checks provide that evidence separately.

A separate raw-CFF bounds matrix may declare `notdefShape: "right-l"` for an
original nonempty .notdef (the classifier also supports `left-bar` and `empty`).
This field changes the expected geometry only for `candidateGlyphId: 0`.
The reported semantic GID stays zero; a shared outline is not a reason to
rename it GID 2. Missing metadata retains the original empty .notdef behavior.
The invalid direct-mapped GID 3 remains empty even when the font's .notdef is
visible. Unknown shape declarations are rejected. The separately classified
`malformed-explicit-map-compatibility` controls assert only the retained
inverse-map behavior, including out-of-range results.

`format-control` cases assert the PDF 32000-1 section 9.7.4.2 mapping. Retained
malformed compatibility cases assert only the explicitly selected existing
behavior. The `malformed-wrapper-mapping-probe` has `assertMapping: false`:
left/right/foot colors are measured against a recorded candidate, but a
different recognized shape is a descriptive observation. Its `mapping.status`
is `observed`, never `passed`; marker/counter/background and measurement
integrity still must pass. A passing raster for this probe certifies a usable
observation, not normative agreement. Reports list mapping assertions and
observations separately from raster and region measurements.

The complete denominator is created before browser execution. Each raster,
region and exact comparison begins `untested`; exceptions and timeouts remain
distinct. Per-operation timeout defaults to 20 seconds. Cleanup has a separate
60-second bound and cleanup failure fails the report. Source drift, runtime
errors, missing isolated assets and missing frozen modules also fail it.
Returned RGBA does not establish physical screen presentation, corpus fidelity
or an optimized performance budget. No PDF.js runtime result is implied by
independent source/specification review or external Poppler/Ghostscript runs.

Preparation checks create no browser or engine build:

```sh
node --test tests/font-support.test.mjs
node --check tests/font.mjs
node --check tests/font-manifest.mjs
```
