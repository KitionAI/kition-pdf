# Type3 recursion through the real editor Worker

This PDF-only harness consumes the native contributor-original two-page Type3
fixtures. It uses the existing editor Worker, an isolated engine package and
the frozen `@kition/sdk` package guard from the hairline/mesh harnesses when
`--shared-dir` is given.
The coordinator schedules execution; preparation never builds an engine or
opens a browser.

```sh
KITION_ENGINE_REVISION=<verified-source-digest> \
KITION_ENGINE_PROFILE=<verified-build-profile> \
node tests/type3.mjs \
  --manifest <fixture-directory>/manifest.json \
  --engine-dir <isolated-engine-package> \
  --shared-dir <frozen-sdk-package> \
  --out <report.json> --browser chromium --timeout-ms 20000
```

Manifest schema 1 requires 40-by-40-point PDF Y-up pages, scales `[1,2]`,
`pageCount: 2`, and `resetPage` with index 1 and authored color regions.
Cases require portable IDs/filenames, SHA-256, `isStress`, `glyphKind`
(`d0`, `d1` or `mixed`), `expectedTruncation`, regions, and optional
`expectedMatch`. A truncated case must point to a clean control for recovery.
Each region is `{rect:[x,y,width,height], rgb:[r,g,b], tolerance}`.

For finite cases, the first page renders at scales 1 and 2, then the clean
second page renders through the same Worker, then the first-page renders repeat.
Every pixel center in each authored region is asserted. Full RGBA hashes must
match explicit clean controls and the repeated sample at the same scale.
No image threshold is weakened to accommodate a regression.

For cyclic cases, native rendering still verifies later-paint pixels and the
specific warning flag. The SDK deliberately rejects a partial page carrying a
warning, so the browser must receive the existing actionable rendering-budget
error at both scales on both passes. Returned pixels or a timeout cannot pass
that expectation. The editor must expose its render-failure alert and a visible
retry control. The harness observes the retry control without triggering an
automatic retry. The clean second page must still render in the same Worker.
Finally, a clean control document opens in the same browser context and must
render without an error alert, with matching regions and full-RGBA control hash.

The existing read-only `bookmark_list` capture obtains the editor's engine;
initial rendering only needs to settle, not succeed, before expected-error
checks. A fresh browser context and initially null host connection isolate each
case. Initial fitted rendering is part of opening the real editor. Worker
request counts prove that each explicit call crossed the Worker boundary.

Reports record all planned successful-raster, expected-rejection, region,
control, repeat, reset-page, error-UI and clean-document recovery populations
before execution. Samples remain untested if their prerequisite failed.
Expected rejection is a passing assertion, not a passing raster. Browser error
text does not establish the detailed native warning count or show the pixels
of the rejected partial page. Source snapshots, required module requests,
fixture hashes, package fingerprints, runtime errors and cleanup failures are
checked as in the existing mesh harness. No shared SDK files are edited.

Preparation checks:

```sh
node --test tests/type3-support.test.mjs
node --check tests/type3.mjs
```

These fixtures exercise recursion, finite resource rebinding, same-font
different-CharProc calls, sibling transforms, d0/d1 behavior and state reset.
They do not certify every Type3 font or PDF fidelity. Returned Worker pixels
do not prove physical screen presentation.

Resource cleanup has a separate 60-second bound with per-resource elapsed times
and status in the report. Engine/render operations retain the configured
20-second bound. A cleanup failure still fails the overall run.
