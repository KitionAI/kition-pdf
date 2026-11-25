# Transformed zero-width strokes in the real browser Worker

This PDF-only harness consumes the original 42-file native affine-stroke
manifest, pinned with a SHA256 for every PDF. Seven transforms, three directions,
and two widths are rendered at scales 1, 2, and 4. The planned populations are
63 zero-width raster checks, 63 descriptive nonzero-width observations, and
54 comparisons of transformed zero-width samples to the same browser's identity
transform. No corpus screenshot, downloaded fixture, font, or reference bitmap
is required. Generated fixture bytes remain outside tracked source files.

Run only when the coordinating task schedules the browser:

```sh
KITION_ENGINE_REVISION=<verified-source-digest> \
KITION_ENGINE_PROFILE=<verified-build-profile> \
node tests/hairline.mjs \
  --manifest <fixture-directory>/manifest-pinned.json \
  --engine-dir <isolated-engine-package> \
  --shared-dir <frozen-sdk-package> \
  --out <report.json> --browser chromium
```

`--shared-dir` is optional; omitting it runs on the `@kition/sdk` package the
editor has installed (`web/node_modules/@kition/sdk`). For an isolated run,
prepare a frozen copy of that package: a directory with its `package.json` and
`dist/` (for example `cp -R web/node_modules/@kition/sdk/. <frozen-sdk>`). Its
external dependencies (React, `@kition/tessera`) may be linked through a
`node_modules` symlink beside them; package exports must point into the frozen
`dist/lib`. No installation, download, source mutation, or snapshot creation is
performed by the harness.

Exact Vite aliases (`@kition/sdk/editor`, `/host`, `/protocol`, `/dev-host`, ...)
are derived from the frozen manifest's browser exports. The package is excluded
from dependency prebundling, with resolution guards in the server and Worker
plugins. Unknown subpaths and any resolution or file request into the installed
package fail the run. The snapshot is added to Vite's filesystem allowlist. Each
actual frozen module request records a portable relative module path, request
count and SHA256 of the raw bytes, which must match the initial snapshot
fingerprint. The report requires observed requests for the editor
(`dist/lib/kit-editor/index.js`), its engine Worker asset, and the host side
(`kit-host`, `kit-ui-protocol`, `kit-dev-host`). These request records
demonstrate which snapshot was served; they do not claim hashes of
Vite-transformed output.

The engine override is mandatory. Its JS and WASM bytes are hash-recorded and
served before Vite's static middleware, including Worker requests. Each file
gets a fresh browser context, initially null host connection, and unique name.
The test waits for that filename's completed host open before capturing the
existing editor `KitEngine` using a read-only `bookmark.list` command (the class
comes from `@kition/sdk/editor` through `tests/sdk-editor.mjs`, which the page
imports through Vite, so it is the class the editor runs). It checks
one clean 128-by-128 page and DPR 1. Raster calls use the inspected page origin,
not the surrounding editor's pasteboard origin. A request probe confirms that
the real Worker receives each render. No second engine or page-thread WASM
instance is created. Browser runs are serial within the harness.

The independent geometric oracle checks the affine-transformed local endpoints
against the authored page endpoints. It converts PDF Y-up geometry to pixel
Y-down coordinates and measures composited black ink over white. Before any
browser run, the following bounds were fixed:

- Full-segment and interior ink mass divided by length: 0.65 to 1.50 device pixels.
- Interior perpendicular ink centroid: within 0.65 device pixels of the line.
- Every one-pixel interior sample: at least 0.10 summed ink within 0.8 pixels
  along the tangent and 1.5 pixels along the normal. Two pixels at each endpoint
  are excluded. Neighbor support accommodates diagonal pixel stair steps.
- Ink farther than two pixels from the expected segment: at most the larger
  of 0.5 ink pixels and 0.5 percent of total mass.
- Same-target identity comparison: at most 10 percent relative mass difference
  and 0.25 pixels centroid difference; both samples must pass analytic checks.

These are bounded one-device-pixel checks for this backend, not universal
cross-renderer antialias thresholds. RGBA hashes are retained; exact hash equality
is an observation, not a required match. Nonzero 0.001-point widths are recorded
as `observed` with `unassessed-nonzero-width` fidelity, even when faint or absent.
They do not inherit zero-width assertions. Worker errors still fail execution.

Every planned sample starts `untested`; successful zero-width checks become
`passed` or `failed`. Timeouts remain `timeout`, and subsequent scales for that
document remain untested after an execution error. Geometry failures allow the
remaining scales to run. Reports preserve separate population counts, harness,
manifest, PDF and engine hashes, runtime/cleanup errors, asset requests, browser
version and source provenance. Missing provenance is explicitly unspecified.
The report is checkpointed after every document and finalized even on failure.
The editor source and the SDK package are fingerprinted before and after
execution: `web/src`, and the package's `package.json` and browser code
(`dist/lib`), from the installed package in live mode or the frozen copy in
snapshot mode.
Changes during the run fail the report and list their
portable source paths, preventing unnoticed concurrent edits from being treated
as stable evidence.

This scope excludes dashed strokes, pattern paint, stroke text, Type3 glyphs,
clipping, transparency, physical display presentation and performance claims.
It does not certify general PDF fidelity or the host's disk-save endpoint.

Preparation checks do not launch a browser or build an engine:

```sh
node --test tests/hairline-support.test.mjs
node --test tests/hairline-shared.test.mjs
node --check tests/hairline.mjs
```
