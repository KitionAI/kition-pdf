# Original dashed hairline controls

These contributor-original PDFs test ordinary solid-color zero-width paths under
finite affine transforms. They separate user-space dash placement from a one
pixel device-space stroke. They do not establish corpus fidelity, nonzero-width,
text-stroke, pattern, or Type3 behavior.

Export the native fixture matrix into a fresh ignored directory, then pin its
existing bytes without overwriting discovery evidence:

```sh
cd web
node tests/dash-manifest.mjs \
  --raw <fixture-dir>/manifest.raw.json \
  --out <fixture-dir>/manifest-pinned.json
node --test tests/dash-support.test.mjs
node tests/dash.mjs \
  --manifest <fixture-dir>/manifest-pinned.json \
  --engine-dir <isolated-engine-package> \
  --shared-dir <frozen-sdk-source> \
  --out <report-directory>/dash-chromium.json \
  --browser chromium
```

Repeat with `--browser webkit` and a separate report. Use a dedicated engine
package and frozen shared SDK checkout; ordinary packages are not replaced.
Record source and engine fingerprints with the reports. The browser uses a fresh
context for each PDF, observes actual editor Worker requests, and renders scales
1, 2 and 4. Operations have a 20-second bound and cleanup a separate 60-second
bound. Timeouts, failures and untested entries stay distinct. The full planned
raster, authored-region, point-probe and pair denominators exist before launch.

Straight-line checks independently derive dash intervals from local length,
array and phase, then transform endpoints. Odd patterns repeat twice. Interior
checks exclude two device pixels at each endpoint and require at least two
remaining pixels. On-runs require equivalent width 0.65 to 1.5 device pixels,
centroid within 0.65 pixels and continuous interior support. Gaps allow at most
0.1 equivalent pixel width. Short intervals are explicitly excluded. Caps and
joins additionally rely on hand-segmented solid-width-zero controls; the scalar
interior method does not infer endpoint cap geometry.

Curves, closed paths, clipping and zero gaps use authored positive ink probes,
negative white regions and full-raster controls, rather than deriving curve
arclength from rendered pixels. Point probe radii are device pixels. Optional
`region.scales` limits a region to its declared scales; other scales list it in
`excludedRegions`, never as passed or untested. This is needed for a uniform
one-pixel alpha-cross rectangle authored only at scale 1. Complete control-pair
comparison still covers alpha behavior at all three scales.

Every sample and its control are rendered by the same backend. Complete RGBA
comparison permits an inclusive per-channel difference of 2 and requires opaque
alpha throughout both rasters. Exact byte equality is reported separately as
`rgbaEqual`; tolerant pairs must not be described as exact. Independent geometry
and authored checks must also pass. Cross-renderer pixel equality is not assumed;
external disagreements remain failures or documented observations.

The browser harness retains RGBA in memory only for pair comparison and reports
hashes and measurements. It exercises the real editor Worker but does not prove
physical screen presentation. Sanitized invalid/overbudget patterns belong to a
separate fallback classification; their expected metadata describes solid output,
not the invalid source pattern. Fixture manifest and method changes must be
recorded before freezing the final matrix.
