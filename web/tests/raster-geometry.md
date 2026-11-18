# Browser glyph and hairline geometry

This harness consumes the same contributor-original ASCII PDFs and geometric
manifest exported by the native renderer tests. The coordinating task pins a
`sha256` on every fixture before browser execution; the harness refuses missing
hashes or changed bytes. No private PDF, external font program, screenshot, or
reference bitmap is used.

```sh
KITION_GOLDEN_ENGINE_REVISION=<verified-source-digest> \
KITION_GOLDEN_ENGINE_PROFILE=<verified-build-profile> \
node tests/raster-geometry.mjs \
  --manifest <sha-pinned-manifest.json> --out <report.json> --browser chromium
```

Run browsers serially when scheduled. Each fixture gets a fresh browser and
document. The test captures the real editor's existing `KitEngine` during a
visible Actual Size render, then uses that same worker for direct raster calls
at scale 1, 2, and 4 (72, 144, and 288 DPI). No second engine or page-thread WASM
instance is created. The default fingerprinted engine package is used unchanged.

## Geometry oracle

The schema declares PDF user coordinates with Y pointing up, rotation zero,
page dimensions, stroke endpoints and widths, and final Type3 glyph placement
matrices including character advances. Glyph rectangles use explicit
`x0,y0,x1,y1` format. The browser transforms these coordinates into raster pixel
coordinates; glyph checks invert the matrix at each pixel center.

Stroke checks measure support along the entire body, excluding two device pixels
at each endpoint. Ink over 0.01 counts as support, including faint quarter-point
antialiasing. Allowed normal support is half the theoretical width plus a
1.5-pixel edge tolerance; a hairline has a half-pixel device-width allowance.
Missing support, displaced ink centroids beyond 1.5 pixels, and significant ink
outside the guard produce separate findings.

Glyph checks require deep expected interiors to have darkness at least `1 - 32/255`
and exterior guards to stay at or below 5/255. Pixels within 1.5 device pixels of
a transformed rectangle boundary are excluded from these binary judgments.
Shared internal rectangle edges are conservatively excluded too. Missing glyph
support, incorrect interior geometry, and displaced/exterior ink are reported
separately. Absence from the expected region cannot by itself distinguish a
deleted glyph from one moved entirely beyond that region.

Coverage mass, bounding boxes, centroids, and glyph edge-zone coverage are raw
observations. Their exact values are not compared as a pass/fail antialias oracle.
RGBA fingerprints allow later investigation without assuming native and browser
backends must produce bit-identical edges.

The browser uses exact distances to transformed rectangle edges and local
region guards. The native checks use a conservative inverse-transform norm and
also inspect the full-page exterior. Their fixture geometry and ink thresholds
match, but their sampled populations and metrics are not claimed identical.

## Interface evidence and limits

Pixel geometry is evaluated directly on returned worker RGBA bytes. A separate
editor check requires Actual Size at DPR 1, followed by a fresh completed canvas
render after the direct calls. The exact-buffer presentation probe confirms that
the worker pixels reach the live canvas and remain current through two animation
frame opportunities; substantial dark fixture ink must also exist on that canvas.
This is interface sanity, not a claim that every direct scale was displayed.

All planned scale samples retain passed/failed/timed-out/untested status. Geometry
failures preserve regional metrics and allow the remaining scales to run; a
worker error or timeout stops that fixture's remaining direct calls. Errors,
cleanup, manifest/engine/harness hashes, and browser versions remain reportable.

The fixtures cover original Type3 shape placement and controlled thin strokes.
They do not certify general font shaping, proprietary fonts, arbitrary PDF
transparency, corpus fidelity, performance, or physical display presentation.

Prepare-only checks:

```sh
node --test tests/raster-geometry-support.test.mjs
node --check tests/raster-geometry.mjs
```
