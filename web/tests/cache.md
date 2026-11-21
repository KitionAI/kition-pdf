# Whole-page cache browser evidence

This functional workload proves the PDF editor's actual 1:1 cache path using
opt-in numeric engine diagnostics. It does not change the shared SDK or host,
write a PDF, capture a screenshot, or use private corpus assets.

Run Chromium and WebKit serially. The coordinating task supplies a WASM build
with the PDF `cache-diagnostics` feature. Its default engine package can remain
untouched: `--engine-dir` serves exactly the two supplied glue/WASM files through
a Vite middleware installed before normal asset serving.

```sh
KITION_GOLDEN_ENGINE_REVISION=<verified-source-digest> \
KITION_GOLDEN_ENGINE_PROFILE='<verified-build-profile>; cache-diagnostics' \
node tests/cache.mjs --browser chromium \
  --engine-dir <diagnostics-engine-directory> --out <report-path>
```

The default engine directory is `public/engine`. A normal build without the
diagnostics export fails clearly rather than silently skipping assertions.
The report hashes the actual immutable bytes served, the fixture, and all
harness modules. No absolute local paths are written into the report.

## Deterministic workload

The test creates 48 original pages in memory, each a 1024 by 1024 solid-color
rectangle with a deterministic per-page color. Each whole-page image requires
4 MiB of RGBA storage. The real 64 MiB cache can retain 16 such pages.

After opening the real development host, the test switches from thumbnails to
bookmarks and waits for zero pending render requests plus 250 ms without render
activity. It clicks Actual Size at DPR 1. Every measured view must then use
`render` with scale exactly 1 and reach the requested PDF page. A second
observer confirms exactly one real render request in each isolated counter
interval; extra/background requests fail rather than contaminate data.

The editor renders ahead of the view: kit-editor's raster viewport extends each
frame past the view along the scrolled axis, so a request for page N at 1:1 can
also reach the end of page N-1. The engine looks up every page a request
reaches, in page order. The test therefore replays each request's pages against
a model of the engine's LRU (16 whole pages in 64 MiB) and requires the exact
hit, miss, admission, eviction and full-render deltas the replay predicts, and
the same resident count as the snapshot before and after each step. Residents
admitted before the measured steps (the cold open) are of unknown identity until
an observation resolves them; a hit, miss or invalidation step requires its
requested page to be known resident or known absent first.

The sequence primes pages 1 and 2, then revisits both to prove exact hits. It
visits pages 3 through 48 to exceed capacity, proves bounded occupancy and actual
eviction, and revisits pages 1 and 2 to prove misses and new whole-page renders.
A further revisit proves the reconstructed page now hits. An unsaved annotation
on page 1 must advance document revision and renderer generation, increment
display invalidations, discard the old resident set, and admit the new page.
Page 2 must then miss in the new revision; page 1 must hit.

The center pixel of every measured canvas must match that original page's
known color. This checks stable content on cache hits and reconstruction without
copying screenshots or retaining rendered pixel arrays. The annotation is away
from the sampled center; revision/counter evidence establishes invalidation,
not a claim about annotation appearance fidelity.

## Diagnostics and completion

The test calls `pdfCacheDiagnostics()` through the already loaded glue module
inside the same live editor Worker. It does not create another document or
renderer, call a shared protocol extension, reset counters, or mutate LRU order.
The snapshot is schema 1 with scope `wasm-instance-thread-aggregate`; exactly
one live cache is required before interpreting it as this document's cache.

Snapshots retain cumulative hits, misses, admissions, evictions, replacements,
retention bypasses, whole-page render requests, tile requests, large-page
bypasses, renderer creations, and display invalidations. Gauges retain current
resident bytes/entries/capacity and the most recent renderer generation/display
revision. Counter deltas must stay nonnegative; gauges must stay within budget.

`performance-probe.mjs` supplies completed-canvas evidence: the exact worker
response buffer must be written to a bitmap, drawn to the live editor canvas,
and remain the current operation's image for two animation-frame opportunities.
The requested page must be active and overlap that render. Edit completion also
requires an increased revision and dirty state. Footer updates and worker replies
alone do not qualify. Two frame opportunities are not physical screen scan-out.

Every planned step is counted as passed, failed, timed out, or untested. A
failure stops this case, preserving untouched later steps in the denominator.
Diagnostics absence, extra render activity, wrong render scale, and incorrect
counter deltas are explicit failures. Waits and cleanup are bounded. Timings
are functional diagnostics only; the quiet waits make them unsuitable for
latency targets or memory-plateau certification.

Prepare-only verification, without a build or browser benchmark:

```sh
node --test tests/cache-support.test.mjs
node --check tests/cache.mjs
node --check tests/cache-probe.mjs
```
