# Patch-mesh culling through the real browser Worker

This PDF-only harness uses the native contributor-original Coons and tensor
patch fixtures on 40-by-40-point pages at scales 1 and 2. It verifies each
manifest-authored color region and compares explicitly paired fixtures by their
complete same-target RGBA hashes. Corpus documents, screenshots, fonts and
cross-renderer antialias equality are outside this scope.

The coordinating task schedules browser runs serially and pins the native
fixture manifest before execution:

```sh
KITION_GOLDEN_ENGINE_REVISION=<verified-source-digest> \
KITION_GOLDEN_ENGINE_PROFILE=<verified-build-profile> \
node tests/mesh.mjs \
  --manifest <fixture-directory>/manifest-pinned.json \
  --engine-dir <isolated-engine-package> \
  --shared-dir <frozen-shared-sdk> \
  --out <report.json> --browser chromium --timeout-ms 60000
```

The frozen shared directory and isolated engine are mandatory. The harness
reuses the exact six-package alias/request guard described in
[the hairline harness](hairline.md), including actual Worker module requests,
source SHA checks and before/after snapshots. Ordinary engine packages and live
shared source remain untouched. Each fixture gets a fresh browser context,
initially null host connection and unique filename. The test captures the
existing editor engine via read-only `bookmark_list`, then uses that engine's
real Worker to render from the inspected page origin. No page-thread WASM or
second document engine is created. Automatic editor renders finish before direct
raster sampling begins.

The schema explicitly declares PDF Y-up coordinates and two scales. Each case
contains `id`, `file`, pinned `sha256`, `shadingType` (6 or 7), `isStress`, optional
`expectedMatch`, and `regions`. A region contains `rect: [x,y,width,height]`,
`rgb: [r,g,b]`, and a declared channel `tolerance`. Every pixel center inside the
authored rectangle must have all RGB channels within tolerance and alpha 255.
The harness neither shrinks rectangles nor averages away isolated wrong pixels.
A region with no sampled pixel centers fails. Region coordinates must therefore
already exclude any antialias boundary that the fixture author does not intend
to assert. Explicit control pairs must also have passing region checks and
identical full RGBA hashes at the same scale.

The fixture set can distinguish clearly off-page stress from visible patches,
Coons interior overshoot, tensor interior controls, affine placement, clipping,
background preservation and later page painting. The inflated-edge control
preserves this rasterizer's existing 1.025 triangle expansion; that artificial
edge coverage is not a normative PDF visibility assertion. Consult the native
fixture manifest for the exact planned cases and region coordinates.

The report establishes its case, raster, region and comparison denominators
before execution. Every sample starts `untested`. Geometry failures allow the
remaining scales to execute. A Worker failure or timeout leaves later scales
untested. Source changes, missing isolated assets, absent required frozen
modules, runtime errors and cleanup failures fail the overall report. Reports
are checkpointed after each document and retain portable provenance hashes and
separate passed/failed/timed-out/untested counts.

Open/capture and direct render durations are descriptive measurements, not
performance acceptance thresholds or a guarantee that a specific native branch
ran. The operation timeout is explicit in the report. Browser completion does
not replace native culling and budget regression tests, and returned pixels do
not prove physical screen presentation.

Preparation checks create no browser or engine build:

```sh
node --test tests/mesh-support.test.mjs
node --check tests/mesh.mjs
```
