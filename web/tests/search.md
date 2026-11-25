# PDF search lifecycle and highlight geometry

Run from `web/` with an already built, source-identified editor engine. This
harness never builds the engine or downloads fixtures:

```sh
node --test src/searchSession.test.mjs src/searchGeometry.test.mjs
node tests/search.mjs \
  --manifest <fixture-directory>/manifest-pinned.json \
  --browser chromium \
  --engine-dir <isolated-engine-package> \
  --out <ignored-report-directory>/search-chromium.json \
  --screenshot-dir <ignored-original-fixture-output>
node tests/search.mjs \
  --manifest <fixture-directory>/manifest-pinned.json \
  --browser webkit \
  --engine-dir <isolated-engine-package> \
  --out <ignored-report-directory>/search-webkit.json \
  --screenshot-dir <ignored-original-fixture-output>
```

Run browsers serially. The report records the engine JavaScript/WASM hashes,
harness hashes, fixture hash, browser version, and optional
`KITION_ENGINE_REVISION` / `KITION_ENGINE_PROFILE` provenance.
A binary hash alone does not prove the source revision or build profile.

`--engine-dir` is optional. When supplied, the shared
[engine override helper](engine-override.mjs) serves those exact JavaScript and
WASM bytes through Vite middleware before its static handler, including Worker
fetches. It records hashes and served counts without copying over the ordinary
package. Omit the option to use `public/engine/` as it stands. This allows search
regressions to run against an isolated new engine without republishing it.

## Original fixture contract

The native `search_geometry` integration test in the automation crate exports
an original Type3 PDF when `KITION_PDF_SEARCH_GEOMETRY_OUT` names an ignored output
directory. It has eight pages: full MediaBox and nonzero CropBox origins,
each at 0, 90, 180, and 270 degrees. Two ASCII lines have independently authored
filled-rectangle glyph geometry and ToUnicode data; no external font assets
or private corpus content are used. Both the full multiline query and a
partial multiline query have two expected rectangles per page.

The coordinator adds the exported PDF's SHA-256 as `sha256` to a separate
`manifest-pinned.json`. The original manifest remains unchanged. The browser
harness requires schema 1, `coordinateSpace: page-local-top-left-points`, eight
pages, `file`, and the pinned hash. Per-page `width`, `height`, `rotation`,
`cropBox`, and `queries[].rects` define the expected page-local geometry.
The fixture path is resolved relative to the manifest; reports omit local
absolute paths. No fixture binary is committed by this workflow.

## Coverage and limits

Each browser has 96 geometry cases: eight pages, three zoom levels
(50%, 100%, 200%), two device pixel ratios (1 and 2), and two queries. Every
case requires both highlight rectangles. Geometry comes from authored fixture
coordinates, the engine's document page origin, and the real viewport's current
view; it is compared against DOM bounds in CSS pixels with a 0.75-pixel
allowance. DPR must not multiply CSS highlight dimensions.

The harness captures the actual viewport through a temporary page-side method
wrapper after clicking Actual Size. Exact zoom levels then call that viewport's
`zoomTo`; these are harness-driven real viewport changes, not tests of typing
percentages into a UI zoom control. Search entry and result navigation use the
real FindBar. No shared SDK source or WASM code is changed.

Every geometry case also waits for the actual canvas backing dimensions at
the requested DPR. It samples each visible authored rectangle's center from
that canvas, requiring at least one visible center, RGB channels at most 64,
and alpha 255. Recorded pixel coordinates and colors establish visible center
coverage only. They do not certify whole-glyph, whole-page, or general font
fidelity. DOM highlights are separate from canvas pixels. Animation-frame waits
alone are not treated as proof the worker pixels reached the canvas.

Ten lifecycle checks per DPR cover repeated Find returning focus to the existing
query without changing it, preventing browser Find while that input is focused,
next/previous and keyboard wrapping,
current-revision cache reuse, clearing and no-results, 200 ms debounce, IME
composition suppression, stale async replies, edit revision invalidation,
close/focus restoration, Escape, keyboard reopening, and document replacement.
The stale-reply test holds actual completed `text.find` replies in a temporary
page wrapper while allowing a newer request to complete; it does not replace
engine output with invented match geometry.

Document replacement saves the synthetic edit first, then opens the same
original fixture under a second filename through the development host. That
host remounts the iframe, so this is a remount cleanup check; it does not claim
to test an in-place document identity swap. FindBar additionally keys session
cleanup to both document identity and revision in its component code.

Query caching retains at most 32 entries and 65,536 aggregate units, counting
query characters, matches, rectangles, and coordinate values. This is a
structural retention budget, not a precise JavaScript heap-byte measurement.
Matched text and other unused engine payload fields are removed before
retention. Large answers remain navigable but are not cached. In-flight worker
requests are logically invalidated; the SDK does not provide physical request
cancellation. Failed or unavailable queries are not cached as empty successes.

Reports explicitly distinguish passed, failed, and untested geometry and
lifecycle cases. A failed prerequisite does not turn remaining cases into
passes. The optional screenshot is limited to the editor iframe displaying
this original fixture, with the repository's attributed Inter font injected
for UI text. Keep it in ignored output directories; never use this screenshot
path with private documents. The harness does not modify repository font assets.
