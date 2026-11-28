# Controlled browser baseline

Run one benchmark process at a time, after correctness suites and builds stop.
Use the already fingerprinted verification WASM. This is an unoptimized-build
baseline, not release performance acceptance or a comparison to an older binary.

```sh
KITION_ENGINE_REVISION=<verified-source-digest> \
KITION_ENGINE_PROFILE='dev opt-level=0/debug=0' \
node tests/performance.mjs \
  --manifest <absolute-manifest-path> \
  --out <report-path> --browser chromium
```

Set `KITION_ENGINE_PROFILE` from the build evidence. The value above
describes the current verification build; when omitted the report explicitly
states that the profile is unspecified and makes no release-performance claim.

The manifest has `cases`, each with an absolute local `path`, positive `pages`
(at least two), optional `sha256`, and ASCII `group`. Input paths and document
text are never copied into the report. A short harness validation can specify
`--cycles 1 --window-pages 2`; the default is 100 complete two-page roundtrips
(200 navigations) after a 40-page sequential window and first/middle/last jumps.
There are also three zoom operations and one unsaved comment refresh through
the host command protocol. Corpus files are never saved or modified.

## Scroll at device pixel ratio 2

`--scroll-pages 20` measures scrolling instead of the navigation samples: each case opens at
device pixel ratio 2, clicks Fit Width, and wheels down (one 60-pixel event every 16 ms) until the
status passes page 21, then back up to page 1 over pages already drawn. The report's `scroll`
records, for each direction, the repaints of the live page canvas per second, wheel event to next
repaint (p50, p95), and the worker render requests it cost. Numbers only: nothing passes or fails
on a duration.

## Completion definition

The browser init script runs before the editor and observes real engine worker
requests and response buffer identities. `putImageData` maps the same buffer to
its offscreen bitmap. A subsequent `drawImage` must use that bitmap on the live
editor canvas, match a render dispatched after the action, overlap the requested
page, and have the requested page active in the editor. Two animation-frame
callbacks must still find the same render displayed and the same operation
pending. Cached previews, footer updates, and worker replies alone cannot pass.
If the browser does not preserve the buffer identity, the sample times out;
there is no fallback to a weaker completion metric.

Navigation and zoom timing starts at captured keydown/click dispatch, excluding
Playwright actionability waits. Cold-open timing starts at the host file input's
change event. Edit refresh begins before the host command roundtrip and includes
that transport. All clocks use `performance.timeOrigin + performance.now()`.

`presentMs` aliases `endToFrameOpportunityMs`; it is the action-to-draw-plus-two-
frame-opportunities interval, not physical screen presentation. `renderMs` is
worker queue-inclusive request/reply time. Raw request area, size, revision,
timestamps, draw time, and frame-opportunity time are retained for each sample.

Every planned operation starts as untested. Renderer errors and timeouts stop
that case; remaining work stays untested. Successful-sample p50/p95 use nearest
rank, accompanied by planned/completed/failed/timed-out/untested counts and a
completion rate over all planned samples. Small-sample p95 may be the maximum.
No percentile treats a timeout as a fast result.

## Resources and provenance

Each case starts a fresh browser process, context, and worker at 1440 by 900,
DPR 1. Cold-open includes the default thumbnail sidebar; navigation switches to
bookmarks. OS file and Vite module caches are not reset. The report fingerprints
the harness, engine, manifest, document bytes and packaged fonts; it records
browser version, loaded fonts, generic platform/CPU-count/RAM, and start/end load.

The process sampler records the owned browser process tree every two seconds and
at phase boundaries, with the limitations of summed RSS made explicit. Worker
checkpoints import the exact already-loaded glue URL and call its cached init
to read existing WASM memory capacity; they never create a page-thread instance.
This is addressable linear-memory capacity, not live allocation or physical RSS.
Checkpoint evaluations occur between measured operations, every ten roundtrips
and at phase boundaries. No garbage collection is forced.

DOM canvas memory is only `width * height * 4`; thumbnail retained RGBA bytes
come from the queue's exposed accounting. Offscreen frames, GPU backing stores,
allocator fragmentation and shared-memory duplication need separate analysis.
The probe retains at most 256 numeric render-metadata records and uses WeakMaps
for buffer/bitmap identity, never pixel copies. Measurement completion does not
certify a memory plateau or approve performance budgets.

Continuous wheel scrolling, searching while navigating, rapid cancellation,
cache-hit/eviction counters and full-corpus fidelity are outside this baseline.

On macOS, browser helpers managed or reparented by XPC may lie outside the
launched process descendant tree. WebKit subtree coverage is not validated;
report its RSS as observed descendant RSS only, never total WebKit memory or a
cross-browser memory comparison. Linear-memory capacity is addressable capacity
and need not equal resident bytes, even with complete process coverage.
