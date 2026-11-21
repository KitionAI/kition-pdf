# Development

How to build Kition PDF from source, run its tests, take the README's pictures and publish the
kit's package. To use PDF, start with the [README](../README.md).

## Repository layout

| Path | Contents |
| --- | --- |
| `crates/` | The engine (Rust), in groups: `kdm`, `engine`, `formats`, `render`, `support`; compiled to WebAssembly for the editor |
| `bindings/` | The engine's two outlets: `wasm` (the in-page editor's engine, behind Kition's `KitDocument` contract) and `headless` (the `kition-pdf` program for Kition's agent and `kition-kits-mcp`) |
| `web/` | The kit's editor (React, on kition-sdk's `@kition/sdk/editor`) |
| `agent/` | The agent guide and checks (KHP) |
| `conformance/` | The sample PDFs the conformance checks open (KHP) |
| `kit.json` | The kit manifest (KHP) |
| `features.yaml`, `cases/` | The feature catalogue and its cases ([quality.md](quality.md)) |
| `docs/` | Documents: entry documents at the first level, parity documents in `docs/parity/`, feature documents in `docs/features/` |
| `assets/` | Fonts and demo data the engine and its tools use; `readme/`: the README's pictures |
| `tools/` | The repository's own tools: `tools/xtask` (`cargo xtask`) and `tools/parity` (the Acrobat feature list `cargo xtask parity` checks) |
| `vendor/` | Patched third-party crates the workspace uses in place of the published ones ([vendor/README.md](../vendor/README.md)) |

## Build and test

Clone [kition-fonts](https://github.com/KitionAI/kition-fonts) next to this repository first
(`git clone https://github.com/KitionAI/kition-fonts ../kition-fonts`): some tests add its fonts
the way a host does, and `pnpm engine` copies a font from it. Keep `KITION_FONTS_DIR` unset. Set
at build time, it builds in the Japanese and Arabic faces of the checkout it names, and a test that
needs a face the checkout lacks fails and names it
([crates/support/fonts/README.md](../crates/support/fonts/README.md)).

```sh
cargo test --workspace
cargo build --release -p kition-pdf                    # the headless kit: target/release/kition-pdf
cd web && pnpm install && pnpm engine && pnpm test:flow
```

`pnpm engine` builds `bindings/wasm` into `web/public/engine`. `pnpm dev` serves `dev.html`, the
editor in a development host; `web/package.json` lists the other browser tests (`test:modes`,
`test:search`, `test:selection`, `test:save`, …).

The check every change passes is `cargo xtask ci` ([quality.md](quality.md)); `cargo xtask --help`
lists the other tasks.

Kition's kit CI runs on GitHub on every push to `ci/**`: the Rust checks, `cargo xtask ci`, and
each `test:*` script of `web/` as its own job, except the scripts `.github/kit-ci.json` excludes.
`test:performance` is excluded because it needs a private corpus: a manifest of local PDFs that
are never committed or uploaded ([web/tests/performance.md](../web/tests/performance.md)). Run
it on a machine that has the corpus, one benchmark at a time after the other tests:

```sh
cd web && pnpm engine
node tests/performance.mjs --manifest <corpus-manifest.json> --out <report.json> --browser chromium
```

`test:search` and `test:selection` are excluded because their probe (`web/tests/search-probe.mjs`)
patches the editor's `RasterViewport` to read the view it shows, and `@kition/sdk/editor` does not
export that class (only its types), so they cannot find it in the package, installed or linked,
until it does. The other browser harnesses observe the live editor
through `KitEngine` (`@kition/sdk/editor`) and `KitPeer` (`@kition/sdk/protocol`), which
`web/tests/sdk-editor.mjs` and `web/tests/sdk-protocol.mjs` import through Vite.

Two files under `docs/` are read by `cargo xtask assets`: [attribution.toml](attribution.toml), the
list of every asset with its source, license and SHA-256, and [attribution.md](attribution.md),
generated from it by `cargo xtask assets --write`.

### The headless kit for an agent

kition-sdk's `kition-kits-mcp` serves `kition-pdf` as MCP tools (`pdf_inspect`, `pdf_run`,
`pdf_check`, …) on a folder of PDFs, for any MCP client; it finds `kit.json` above the
executable:

```sh
cargo build --release -p kition-pdf
cargo install --git https://github.com/KitionAI/kition-sdk kition-kits --bin kition-kits-mcp
kition-kits-mcp --workspace <folder> --kit target/release/kition-pdf
```

[agent/guide.md](../agent/guide.md) is the guide a model reads first, and
[writing-format.md](writing-format.md) lists every command.

### The editor's dependencies

The editor uses two packages: kition-sdk's `@kition/sdk` (its subpaths `/editor`, `/host`,
`/protocol` and `/dev-host`) and Kition Tessera's `@kition/tessera` (`/ui`). By default
`pnpm install` in `web/` installs them from npm, at the versions the committed
`web/pnpm-lock.yaml` pins, so the editor needs no other checkout. Until their first release they
come from the rehearsal copies, `@kitionstage/sdk` and `@kitionstage/tessera`, which
`web/package.json` installs under the final names by npm alias. To develop against local
checkouts instead, run `KITION_LINK=1 pnpm install` in `web/`: `web/.pnpmfile.cjs` then links
kition-sdk's `packages/sdk` and kition-tessera's `packages/tessera` from the checkouts next to this
repository (or the ones `KITION_SDK` and `KITION_TESSERA` name). Both packages' exports point at
their built `dist/`, so build them first (`pnpm --filter @kition/sdk build` in kition-sdk,
`pnpm build` in kition-tessera's `packages/tessera`). The linked install rewrites
`web/pnpm-lock.yaml`; do not commit a lockfile changed that way.

## Take the README pictures

[`web/scripts/readme-shots.mjs`](../web/scripts/readme-shots.mjs) takes the editor pictures in
`assets/readme/` with kition-sdk's shared runner (`tools/readme-shots/` there): the development
host in Classic mode, light theme, English, 1440 × 900 at device scale 2, on the kition-showcase
samples `reviewed-quarterly-report`, `confidential-exhibit-bates`, `volunteer-handbook-outline`,
`swim-club-registration-form` and `redacted-incident-memo`. It needs kition-sdk and
kition-showcase checkouts next to this repository (or `KITION_SDK` and `KITION_SHOWCASE`), the
engine in `web/public/` (`pnpm engine`), and no HTTP proxy set:

```sh
cd web
node scripts/readme-shots.mjs                 # every picture, or name some: hero forms
cd ../assets/readme
pngquant --quality 70-90 --force --ext .png hero.png pages.png bookmarks.png forms.png redaction.png
```

Each picture stays under 400 KB. The `made-*.png` pictures are copies of kition-showcase samples'
`thumb.png`. Every file in `assets/readme/` has an entry in [attribution.toml](attribution.toml)
with its SHA-256: after retaking a picture, update its `sha256` there and run
`cargo xtask assets --write`, then `cargo xtask assets`.

## Publish

Kition ships the kit's package, `kition-pdf-<version>.tgz` (the `pdf-ui/` folder), a release
asset of this repository tagged `package-v<version>`. It is built on GitHub, never on a developer
machine: the Package workflow (`.github/workflows/package.yml`, which calls Kition's reusable
`kit-package.yml`) builds the commit it is started on, with `gh workflow run package.yml -R
KitionAI/kition-pdf`, and publishes the Release with the package's lock entry,
`kition-pdf-<version>.json`. Kition pins it with `node scripts/pin-kits.mjs pdf` (see
`docs/kits.md` there). A version is published once: bump `version` in `kit.json` to publish a
change.
