# PDF pointer selection browser checks

Run from `web/`, serially, after the coordinator has built and fingerprinted a
compatible editor engine exposing `text.select`:

```sh
node --test tests/selection-support.test.mjs
node tests/selection.mjs \
  --manifest <original-fixture-directory>/manifest-pinned.json \
  --engine-dir <isolated-engine-package> \
  --browser chromium \
  --screenshot-dir <ignored-original-fixture-output> \
  --out <ignored-report-directory>/selection-chromium.json
node tests/selection.mjs \
  --manifest <original-fixture-directory>/manifest-pinned.json \
  --engine-dir <isolated-engine-package> \
  --browser webkit \
  --screenshot-dir <ignored-original-fixture-output> \
  --out <ignored-report-directory>/selection-webkit.json
```

The harness does not build, download, copy, or replace an engine package.
`--engine-dir` serves the specified JavaScript and WASM bytes through a Vite
middleware before its static asset handler, so Worker fetches receive the
isolated package too. Only the two named `/engine/` assets are served by that
override. The report records their hashes and served counts. Without the
option, the existing `public/engine/` package is used. Ordinary package files
are never changed. Optional provenance comes from
`KITION_ENGINE_REVISION` and `KITION_ENGINE_PROFILE`; binary hashes
alone do not establish a source revision or build profile.

## Fixture and expected results

Reuse the original SHA-pinned eight-page Type3 fixture described in
[search.md](search.md). It contains two ASCII lines and contributor-original
filled glyph rectangles, with no private document or external font assets.
The full MediaBox and nonzero CropBox variants each cover rotations of 0, 90,
180, and 270 degrees. The fixture binary remains in ignored output.

Full line rectangles and the independent partial-query rectangles in its
manifest determine A, B, C, and D geometry. A midpoint split is checked against
that partial oracle before use; no native selection answer is used to generate
expected geometry. Range endpoints are inclusive nearest-glyph points. Forward
and reversed full ranges produce `AB\nCD`; B-to-C produces `B\nC`. Column
selection uses an axis-aligned box containing A and C glyph centers. It returns
`C\nA` at 180 degrees, following visual row order, and `A\nC` at the other
rotations; vertical lines use the native API's reading-order fallback. Column
rectangles retain glyph reading order in every case.

## Matrix and visible evidence

Each browser plans 192 geometry cases: eight pages, three zoom levels
(50%, 100%, 200%), two DPR values (1 and 2), and four gestures (full forward,
full reverse, partial range, Alt column). All drags use real mouse events.
Exact zoom and positioning call the existing editor viewport, captured through
a temporary page probe; these are not tests of a zoom percentage input.

Every case checks the real `text.select` result's plain text, page order, and
rectangles against the authored expectations. It compares visible overlay
bounds in CSS pixels using the current viewport and page origins, without
multiplying CSS geometry by DPR. Each visible authored rectangle center must
also be dark and opaque in the actual canvas at the expected backing size.
This is center-coverage evidence, not a whole-glyph, whole-page, or general
font fidelity claim. Optional screenshots include only the editor iframe on
this original fixture, with attributed repository Inter injected for UI text;
keep them in ignored outputs. No corpus content is used or produced.

Fourteen lifecycle checks per DPR cover:

- Plain-text copy payload from both a synthetic event and the trusted event
  caused by real Ctrl/Cmd+C, plus separately reported native clipboard reads
  and Copy Selection button clipboard results.
- Real toolbar inputs and synthetic editable controls inside the copy event
  region retaining their own copy behavior.
- Escape, single clicks, pasteboard clicks, drag return to the anchor,
  pointercancel, and Escape during an active drag clearing selection.
- One active request plus a coalesced latest endpoint: a real worker reply is
  held in a page wrapper, superseded, and released before the latest request
  executes. A DOM observer rejects publication of the superseded geometry.
- Space and middle-button hand panning without issuing selection requests.
- Forward and reverse cross-page ranges producing `CD\nAB`, with separate
  page rectangles, and Alt dragging clamped to its anchor page.
- Edit revision invalidation, including an outstanding delayed reply.
- Starting selection while Find is open: closing the bar changes the canvas
  extent, so the drag follows the glyph's new screen position before release.
- Saving the original synthetic edit, then replacing the document through the
  host without carrying over selection or an enabled Copy Selection button.

The development host replaces its iframe on document replacement. That check
therefore certifies remount cleanup, not an in-place document swap. Mouse
coverage does not certify touch, pen, or accessibility input equivalence.

The clipboard event test uses the browser's `ClipboardEvent` and `DataTransfer`
and requires only `text/plain`. A document bubble observer also captures the
trusted Ctrl/Cmd+C event after React handled it, retaining its event object
for post-dispatch `defaultPrevented` inspection and its in-dispatch payload.
This verifies keyboard copy independently of read permission in both browsers.
Actual clipboard reads are a separate result:
unsupported or denied reads remain explicitly untested. Unexpected clipboard
contents are never printed into reports. The button test selects a different
substring from the keyboard test before checking the clipboard, preventing a
stale identical value from being mistaken for a successful button write.

Reports prepopulate all geometry and lifecycle cases as untested and mark
individual passes and failures. A prerequisite failure leaves remaining cases
untested. Page-side probes observe the live editor and delay actual replies;
they do not modify production code, shared SDK files, or WASM implementation.
