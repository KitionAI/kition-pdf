# Mixed dot and dash hairline controls

This guide covers the single-line test packet. The
[disconnected straight-subpath extension](separated-dash-caps.md) covers multiple
independently started straight lines under the same mixed pattern.

The focused renderer path handles one finite, noncollapsed straight segment with
solid color, zero width, round or square caps, and both zero and positive on-runs.
Dash lengths and phase are evaluated in user space before affine transformation.
The resulting segments are stroked together at one device pixel, preserving
clipping and single-paint opacity. A semantic zero event uses a small segment
along the transformed tangent; real short positive runs are not snapped to zero.

Compound and curved paths, nonzero width, text, pattern and Type3 contexts retain
their existing handling. A phase exactly at the end of a positive on-entry and an
endpoint coinciding with an explicit zero event retain the compatibility fallback;
these boundary cap semantics are not asserted by this matrix. Positive-only,
pure-dot and zero-gap-only patterns retain their existing routes. This is focused
synthetic coverage, not full-corpus or universal malformed-input acceptance.

The contributor-original matrix contains 13 sample/control pairs: round and square
caps, anisotropy, shear, rotation, reflection, a 3:4 source tangent, phases inside a
gap and inside an on-run, a large equivalent phase, an odd array, clipping and alpha
overlap. Controls contain explicitly authored solid segments and dots, independent
of the production interval walker. For example, `[0 12 6 12] 3 d` has dots at 27 and
57 and positive runs 9–15, 39–45 and 69–75 on an 84-unit line. Square-dot control
deltas are binary-exact after PDF float parsing so their direction remains correct.

Use a separate worktree and build directory. From the repository root, export the
PDFs into a new ignored directory beneath that worktree's `target/`:

```sh
export CARGO_TARGET_DIR="$PWD/target/mixed-dash-checks"
export CARGO_BUILD_JOBS=2
export KITION_PDF_MIXED_DASH_OUT="$PWD/target/mixed-dash-fixtures"
cargo +1.95.0 test --locked --offline -p kition-pdf-render \
  --test mixed_dashed_hairline \
  --config 'profile.dev.opt-level=0' --config 'profile.dev.debug=0' \
  --config 'profile.dev.package."*".opt-level=0' \
  --config 'profile.dev.package."*".debug=0' \
  -- --nocapture --test-threads=2
node web/tests/dash-manifest.mjs \
  --raw "$KITION_PDF_MIXED_DASH_OUT/manifest.raw.json" \
  --out "$KITION_PDF_MIXED_DASH_OUT/manifest-pinned-base.json"
```

The exporter refuses to replace different existing fixture bytes. Its test is
`mixed_dash_hairlines_keep_semantic_dots_and_positive_runs_under_transforms`.
An old renderer is expected to fail the new sample/control comparisons; retain
that baseline separately from final verification.

The frozen general dash enricher carries a provenance note specific to the prior
matrix. Preserve its output, then correct only that note for this matrix:

```sh
python3 - <<'PY'
import json
import os
from pathlib import Path

root = Path(os.environ["KITION_PDF_MIXED_DASH_OUT"])
manifest = json.loads((root / "manifest-pinned-base.json").read_text())
manifest["enrichment"] = (
    "Pinned PDF hashes from the original mixed-dash exporter; "
    "authored probe thresholds preserved without overrides"
)
text = json.dumps(manifest, indent=2) + "\n"
output = root / "manifest-pinned.json"
try:
    with output.open("x") as stream:
        stream.write(text)
except FileExistsError:
    assert output.read_text() == text, "Refuse to replace a different pinned manifest"
PY
```

Reuse the existing real-editor Worker harness with an isolated engine package and
frozen shared SDK source. Set the source/profile metadata to the actual tested
build, and use separate reports for each browser:

```sh
cd web
node tests/dash.mjs \
  --manifest "$KITION_PDF_MIXED_DASH_OUT/manifest-pinned.json" \
  --engine-dir <isolated-engine-package> --shared-dir <frozen-sdk-source> \
  --browser chromium --out <report-directory>/mixed-dash-chromium.json
```

Repeat with `--browser webkit` and a different output. The prior 52-PDF matrix and
its helper files remain separate. See [the dash harness guide](../../web/tests/dash.md)
for lifecycle, timeout and source-fingerprint checks.

Each backend plans 26 PDFs, 78 rasters, 176 authored regions, 276 point probes and
39 pairs at scales 1, 2 and 4. Isolated dots and positive runs have independent ink
checks; clipped-away ink and gaps have white probes. The alpha-overlap region is
asserted only at scale 1, with four explicit exclusions across the two PDFs at the
other scales. Full-raster pair comparison still applies at every scale.

Complete RGBA comparison permits an inclusive per-channel difference of 2 and
requires opaque alpha. Exact equality is reported separately. Positive-run width,
centroid and continuity checks retain the existing fixed criteria; short intervals
are explicitly excluded. Passed, failed, timeout and untested results stay distinct.
External renderer disagreement is retained, not used to relax these criteria.
