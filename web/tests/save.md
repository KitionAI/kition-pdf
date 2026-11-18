# PDF save and reopen browser checks

Run from `web/`, serially with other browser suites:

```sh
node tests/save.mjs \
  --manifest <fixture-directory>/manifest-pinned.json \
  --engine-dir <isolated-save-engine-package> \
  --browser chromium \
  --out <ignored-report-directory>/save-chromium.json \
  --artifact-dir <new-ignored-output-directory>/chromium
```

Repeat with `--browser webkit`, a separate report, and a fresh artifact directory.
The output directory must not already contain `original.pdf` or the named saved
snapshots. Existing artifacts are never replaced. The runner launches Vite and
the selected browser, closes them on success or failure, and exits nonzero for
any failed or untested planned check. It never builds, downloads, or replaces an
ordinary engine package. `engine-override.mjs` serves the supplied JS/WASM only
for this Vite instance, including Worker requests.
Each reopen waits for a new host connection as well as the ready status, so an
asynchronous file read cannot reuse the previous same-name document's readiness.

The pinned input is the existing contributor-original, Apache-2.0 eight-page
Type3 fixture. Its SHA-256 is
`3a40455a0f4f9e0de1f66d3e3456b1d663c59ba3aa7ff51295b7f35b1666c2b1`.
It contains original filled-rectangle glyphs for `AB` and `CD`, four rotations,
and full/cropped page bounds. No external font, corpus document, image, or
screenshot is used. The original and successfully host-written output bytes
are copied only into the requested ignored artifact directory.

## Independently authored expectations

Fourteen planned checks per browser cover:

1. Clean opening, the eight authored page dimensions, and empty outlines/comments.
2. Add bookmark 1 at page 1, undo to no bookmarks, and redo to bookmark 1.
3. Save through the actual host `document/save` request. The real Worker
   serializes; the development host stores the bytes through `host/write`;
   acknowledgement precedes Worker `markSaved`, and the document becomes clean.
4. Add bookmark 2 at page 2 and reject one host write. The request rejects, no
   `markSaved` call occurs, stored bytes/version remain unchanged, and both
   bookmarks remain present in the dirty live document.
5. Click the real Save toolbar button. Retry stores both bookmarks and clears
   dirty state. The output begins with every byte of the preceding saved file.
6. Reopen that saved file and read back exactly bookmarks 1 and 2.
7. Add bookmark 3 at page 3, save, and hold the host acknowledgement after its
   real storage handler succeeds. The live document stays dirty and no
   `markSaved` call occurs while the acknowledgement is held.
8. Add bookmark 4 at page 4 through the real agent protocol while that
   acknowledgement is pending; the live revision increases.
9. Release the old acknowledgement. `markSaved` receives the older serialized
   revision, the save result says `saved: false`, and bookmark 4 remains dirty.
   The stored bytes still equal the snapshot containing only bookmarks 1-3.
10. Save again; all four bookmarks become clean. Because the prior stale save
    was not rebased, this file preserves the last acknowledged clean baseline
    (the two-bookmark file) as its prefix.
11. Reopen the held snapshot and require exactly bookmarks 1-3.
12. Reopen the latest snapshot and require exactly bookmarks 1-4.
13. Add a rectangle annotation on page 1 with top-left coordinates
    `[170, 150, 210, 180]`, red stroke, width 2, and the original test text
    `Original save rectangle`; use the real Save toolbar button.
14. Reopen and require the annotation rectangle/text and all four bookmarks,
    clean state, and unchanged authored page dimensions.

Bookmarks use titles `Original save bookmark 1` through
`Original save bookmark 4`, target pages 1-4, and no children. They do not alter
page content. The annotation is far from page 1's authored glyph rectangles
`[50, 44, 74, 64]` and `[50, 84, 74, 104]`. External raster comparison should
measure its changed-pixel bounds and allow only a small explicitly recorded
stroke/antialias margin around the annotation; this browser harness does not
claim such a pixel comparison passed.

## Evidence and scope

Reports contain planned/passed/failed/untested counts, input/harness/shared
source/engine hashes, isolated asset request counts, observed write/serialize/
markSaved order, and output hashes. Set `KITION_GOLDEN_ENGINE_REVISION` and
`KITION_GOLDEN_ENGINE_PROFILE` when source provenance is known. Every recorded
successful snapshot is the development host's stored byte array, not a second
serialization made just for the test. A `manifest.json` beside the artifacts
uses this external-oracle shape:

```json
{
  "schema": 1,
  "documents": [{
    "id": "original-save-chromium",
    "original": "original.pdf",
    "sha256": "<input-sha256>",
    "outputs": [{
      "id": "bookmark-first",
      "file": "bookmark-first.pdf",
      "kind": "unchanged",
      "prefixOf": "original.pdf"
    }]
  }]
}
```

All file references resolve relative to the manifest. Annotation output has
`kind: "annotation"` and `annotation: { page: 1, rect: [170, 150, 210, 180], ... }`.
Each output also records expected bookmarks, SHA-256, size, host version, and
the measured exact-prefix result. The separate external oracle should check
text, unchanged-page rasters, the bounded annotation region, and prefix bytes.
The runner records written bytes before asserting prefix preservation, so a
failed prefix check retains its evidence.

The page probe wraps the existing `KitPeer.handle('host/write')` registration
before opening the editor. Ordinary writes invoke its original handler; one
write throws before storage and one holds its original successful result.
Engine method wrappers observe actual Worker output and forward calls intact.
No test-dependent production behavior or SDK source edit is introduced.

This verifies the actual PDF SDK Worker, Kit UI save sequencing, protocol
requests, and development-host **in-memory** storage. It does not exercise
Kition's `PUT /v1/kits/files`, disk atomicity/durability, permissions, disk-full
recovery, process crashes, encrypted files, signatures, or DocChecksum. The
development host advertises no replica persistence capability, and reopen
remounts the iframe; neither replica saves nor in-place replacement is covered.
External rendering results remain separately classified.

## Direct WASM SDK acknowledgement regression

Run `node tests/save-sdk.mjs --engine-dir <isolated-save-engine-package>
--manifest <fixture-directory>/manifest-pinned.json --out <ignored-report.json>`
from `web/` (join the command onto one line). This loads the real artifact in
Node without a browser Worker revision guard. It checks stale acknowledgements
after local edits, undo and remote imports; an acknowledgement with no prepared
save; explicit DocChecksum rewrite refusal; and exact incremental prefixes plus
replica identity/version across three saves and reopened replicas. No product
engine runs on a browser page thread.

The SDK API has no save-operation identifier. A caller must serialize save and
acknowledgement operations; these tests do not certify arbitrary reordered
acknowledgements of multiple overlapping save preparations. The UI's existing
Worker compares the serialized revision before calling `markSaved`; direct SDK
checks establish the PDF implementation's own protection.
