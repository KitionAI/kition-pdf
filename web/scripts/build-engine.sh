#!/bin/sh
# Builds the PDF engine (bindings/wasm) to WebAssembly into public/engine, where the
# editor loads it from its own package. Needs wasm-pack.
# A test engine (tests/ci.mjs cache) sets KITION_PDF_ENGINE_OUT to build elsewhere, with the
# bindings' KITION_PDF_ENGINE_FEATURES (for example cache-diagnostics); it gets no fonts.
set -eu
web="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$web/.." && pwd)"
if toolchain="$(rustc --print sysroot 2>/dev/null)" && [ -d "$toolchain/lib" ]; then
  export DYLD_FALLBACK_LIBRARY_PATH="$toolchain/lib${DYLD_FALLBACK_LIBRARY_PATH:+:$DYLD_FALLBACK_LIBRARY_PATH}"
fi
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo/target}"
# A dependency that compiles C for the target needs a WebAssembly toolchain: Apple's clang cannot
# target WebAssembly, so use Homebrew's LLVM when it is there and no compiler is set.
if [ -z "${CC_wasm32_unknown_unknown:-}" ] && command -v brew >/dev/null 2>&1; then
  for formula in llvm@21 llvm; do
    llvm="$(brew --prefix "$formula" 2>/dev/null || true)"
    if [ -n "$llvm" ] && [ -x "$llvm/bin/clang" ]; then
      export CC_wasm32_unknown_unknown="$llvm/bin/clang" AR_wasm32_unknown_unknown="$llvm/bin/llvm-ar"
      break
    fi
  done
fi
# On Linux, the newest installed LLVM (/usr/lib/llvm-*).
if [ -z "${CC_wasm32_unknown_unknown:-}" ]; then
  llvm="$(ls -d /usr/lib/llvm-*/bin 2>/dev/null | sort -V | tail -n 1)"
  if [ -n "$llvm" ] && [ -x "$llvm/clang" ]; then
    export CC_wasm32_unknown_unknown="$llvm/clang" AR_wasm32_unknown_unknown="$llvm/llvm-ar"
  fi
fi
cd "$repo"
out="${KITION_PDF_ENGINE_OUT:-$web/public/engine}"
wasm-pack build bindings/wasm --target web --release --no-typescript --out-dir "$out" ${KITION_PDF_ENGINE_FEATURES:+-- --features "$KITION_PDF_ENGINE_FEATURES"}
rm -f "$out/.gitignore" "$out/package.json"
[ -z "${KITION_PDF_ENGINE_OUT:-}" ] || exit 0
# PDFs often name Chinese fonts they do not embed; the editor gives the engine this face for them
# (from the kition-fonts checkout next to the repository, or KITION_FONTS_DIR) before it opens.
# Not Noto CJK, which AGENTS.md rules out.
fonts="${KITION_FONTS_DIR:-$repo/../kition-fonts}/fonts/droid-sans-fallback"
rm -rf "$web/public/fonts"
if [ -f "$fonts/DroidSansFallbackFull.ttf" ]; then
  mkdir -p "$web/public/fonts"
  cp "$fonts/DroidSansFallbackFull.ttf" "$web/public/fonts/"
  cp "$fonts/NOTICE" "$web/public/fonts/DroidSansFallback-NOTICE.txt"
else
  echo "warning: no kition-fonts checkout at $fonts; Chinese text in fonts a PDF does not embed will not show" >&2
fi
