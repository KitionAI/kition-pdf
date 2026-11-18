# Pure dotted hairlines on separate straight subpaths

Ordinary zero-width color strokes can preserve pure dotted dash patterns on
multiple separate straight subpaths. Eligible patterns have an even number of
entries, zero in every on position, and strictly positive off entries. Each
subpath restarts the dash phase and supplies its own transformed cap direction.
The complete prepared path is painted once at one device-pixel width.

The implementation reuses the bounded semantic walk for mixed dashes. It keeps
the existing input, output and aggregate event budgets, finite-geometry checks,
and fallible allocation. Unsupported geometry or a terminal zero event on any
subpath rejects preparation for the whole stroke. It never paints a partial
prepared result. Single-line pure-dot behavior and the mixed-dash route retain
their previous behavior.

## Scope and independent controls

The contributor-original fixture in
`crates/render/render/tests/support/pure_zero_separated_fixtures.rs` contains
eight samples and eight independent undashed controls. No fonts, images or
external document assets are used. Literal scalar dot tables precede the
transform; the production dash walker does not generate the expected dots.

| Case | Independent checks |
| --- | --- |
| Round and square caps | Phase 3, unequal forward/reversed source lengths |
| Unequal gaps | `[0,8,0,12]`, phase 3, nonuniform dot spacing |
| Periodic phase | `[0,12]`, phase 27, equivalent to phase 3 |
| Diagonal direction | A 3:4 source direction and a reversed horizontal source |
| Shear | Distinct transformed directions, including a vertical source mapped to 1:2 |
| Clip | Retained dots, removed dots and empty regions |
| Alpha crossing | Unique `[0,12]` events, one paint at opacity 0.5 |

All raster cases end inside positive gaps, away from disputed terminal events.
Controls use direction-bearing tiny positive solid segments with independently
checked parsed coordinate differences. The test validates control ink and empty
regions before comparing complete images. A red sentinel checks continued paint.

At scales 1, 2 and 4, the packet has 48 rasters and 24 full-image comparisons.
The browser manifest describes 294 authored point checks and 200 region checks;
four alpha-region checks are explicitly outside their authored scale. Native
testing measures 147 reference-point masses and compares all sample pixels.
Same-backend comparison uses channel tolerance 2 and records exact equality
separately. Successful external rendering is separate from geometric agreement.

## Reproduce the native regression

From this repository, use a dedicated Cargo target and at most two build jobs:

```sh
mkdir -p target
export RUSTC="$(rustup which --toolchain 1.95.0 rustc)"
export RUSTDOC="$(rustup which --toolchain 1.95.0 rustdoc)"
export PATH="$(dirname "$RUSTC"):$PATH"
export CARGO_BUILD_JOBS=2
export CARGO_TARGET_DIR="$PWD/target/pure-zero-separated-build"
export KITION_PDF_PURE_ZERO_SEPARATED_OUT="$PWD/target/pure-zero-separated-fixtures"
rustup run 1.95.0 cargo test --locked --offline -p kition-pdf-render \
  --test pure_zero_separated_hairline \
  --config 'profile.dev.opt-level=0' --config 'profile.dev.debug=0' \
  --config 'profile.dev.package."*".opt-level=0' \
  --config 'profile.dev.package."*".debug=0' \
  -- --nocapture --test-threads=1
```

The export variable is optional. When supplied, it must name an absolute path
within this checkout's `target/`; the exporter refuses to replace different
evidence. Use a new export directory after changing fixtures or renderer output.
Verify `rustc --version` and `cargo clippy --version` with this environment:
selecting Cargo alone does not establish which compiler its child process uses.
The output contains original PDFs, raw RGBA and `manifest.raw.json`. Pin the
manifest with `web/tests/dash-manifest.mjs` before using the existing browser
harness. See [the dash harness guide](../../web/tests/dash.md) for isolated packages
and frozen shared sources. Schedule full native stress checks after heavy build
and browser jobs.

## Preserved limitations

The fixture uses unique zero events and keeps the isolated-dot area bounds away
from the crossing; coincident contours do not meet those bounds at scales 2 and 4. At the crossing it
checks the scale-1 gray pixel and full-image agreement at every scale.

Those checks establish the tested one-paint behavior, not general antialiasing
union correctness for overlapping caps. Pure-dot patterns with zero off gaps
use the disconnected-path fallback. Different subpaths can still have
coincident dots, so positive off gaps alone do not certify overlap antialiasing.
Connected, curved, closed or collapsed sources, exact terminal-zero events,
nonzero-width strokes, text, pattern and Type3 cases remain outside this repair.
The boundary diagnostics retain both disputed
mixed-dash boundary exclusions. No full-corpus fidelity or optimized-performance
claim follows from these synthetic tests.
