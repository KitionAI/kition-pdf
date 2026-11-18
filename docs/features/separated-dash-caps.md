# Disconnected mixed dashed hairline controls

This focused extension handles an ordinary zero-width solid-color stroke whose
path consists of separate MoveTo + LineTo pairs, with mixed zero and positive
on-dashes and round or square caps. Each source subpath restarts the same dash
pattern and phase in user space. Its own transformed tangent determines dot-cap
direction. All resulting centerlines are painted together at one device pixel,
retaining clipping and single-paint opacity at crossings.

Connected vertices, curves, ClosePath, dangling moves and collapsed tangents stay
outside this extension. Exact phase at the end of a positive on-entry and a source
endpoint reaching an explicit zero-on entry retain compatibility fallback. Failure
in any subpath discards the entire prepared result before legacy stroking. Nonzero
widths, text, patterns and Type3 contexts keep their existing handling.

The implementation retains the aggregate source/restart estimate and source-element
bound. Pattern/phase preparation is shared; semantic events and terminal lookahead
have global budgets across the path, as do emitted elements and fallible allocation.
A subpath does not receive a fresh 250,000-event allowance. Explicit zero events
alone receive a tiny tangent segment; real positive runs are not snapped to zero.

The contributor-original matrix has eight sample/control pairs: round and square
caps with unequal subpath lengths, phase restart inside a gap, distinct 3:4 and
reversed tangents, an odd pattern, a shared shear transform, clipping and an alpha
crossing. Independent controls contain manually tabulated solid intervals and
dots. For example, `[0 12 6 12] 0 d` restarts with dots at 0 and 30 on both a
54-unit line and a 44-unit line; positive runs are 12-18 and 42-48 on the former,
12-18 and 42-44 on the latter. Square-dot controls use binary-exact endpoint
differences after PDF float parsing to preserve their authored directions.

Use a separate worktree and build directory. From the repository root, export into
a new ignored directory beneath that worktree's `target/`:

```sh
export CARGO_TARGET_DIR="$PWD/target/separated-dash-checks"
export CARGO_BUILD_JOBS=2
export KITION_PDF_SEPARATED_DASH_OUT="$PWD/target/separated-dash-fixtures"
cargo +1.95.0 test --locked --offline -p kition-pdf-render \
  --test separated_dashed_hairline \
  --config 'profile.dev.opt-level=0' --config 'profile.dev.debug=0' \
  --config 'profile.dev.package."*".opt-level=0' \
  --config 'profile.dev.package."*".debug=0' \
  -- --nocapture --test-threads=2
node web/tests/dash-manifest.mjs \
  --raw "$KITION_PDF_SEPARATED_DASH_OUT/manifest.raw.json" \
  --out "$KITION_PDF_SEPARATED_DASH_OUT/manifest-pinned-base.json"
```

The exporter refuses different existing fixture bytes. Its test is
`separated_mixed_dash_hairlines_restart_phase_and_keep_one_device_paint`.
Retain the expected failing prepatch baseline separately from final verification.

The frozen general enricher has a provenance note for an earlier matrix. Preserve
its exact output, then correct only that note for this packet:

```sh
python3 - <<'PY'
import json
import os
from pathlib import Path

root = Path(os.environ["KITION_PDF_SEPARATED_DASH_OUT"])
manifest = json.loads((root / "manifest-pinned-base.json").read_text())
manifest["enrichment"] = (
    "Pinned PDF hashes from the original disconnected-dash exporter; "
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
frozen shared SDK source. Record the actual tested source/profile metadata and use
separate reports for each browser:

```sh
cd web
node tests/dash.mjs \
  --manifest "$KITION_PDF_SEPARATED_DASH_OUT/manifest-pinned.json" \
  --engine-dir <isolated-engine-package> --shared-dir <frozen-sdk-source> \
  --browser chromium --out <report-directory>/separated-dash-chromium.json
```

Repeat with `--browser webkit` and a different report. See the existing
[dash harness guide](../../web/tests/dash.md) for lifecycle and source checks. The
prior [single-line mixed controls](mixed-dash-caps.md), general dash packet and
other frozen regression packets remain separate. Final acceptance also requires
all 297 earlier regression raster outputs to remain byte-identical, using their
frozen fixtures and prior accepted output hashes.

Each backend plans 16 PDFs, 48 rasters, 212 authored region checks, 264 point probes
and 24 sample/control comparisons at scales 1, 2 and 4. Every subpath has positive
ink and gap coverage. The alpha crossing at (55.5,64.5) uses one stroke and expects
gray 128 in rectangle (55,64,1,1) at scale 1. Four alpha-region checks are explicitly
excluded at scales 2 and 4; full-raster comparison still applies at those scales.

The frozen schema supports one analyticLine, so this disconnected packet uses
pointInk and regions without analyticLine. It does not claim full analytic width
or continuity coverage for every run. Existing point bounds and centroid tolerance
0.75 stay fixed. Full RGBA comparison permits an inclusive per-channel difference
of 2 and requires opaque alpha; exact equality is reported separately. Preserve
passed, failed, timeout and untested outcomes distinctly. External renderer
disagreement does not justify relaxing these controls or claiming corpus coverage.
