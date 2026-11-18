import type { KitModuleManifest } from '@kition/kit-editor'

/** The engine, from this package's own folder, on the host's canvas color. Called after the theme is applied. */
export function engineManifest(): KitModuleManifest {
  const canvas = getComputedStyle(document.documentElement).getPropertyValue('--kit-canvas').trim()
  return {
    kitId: 'kit.pdf',
    base: new URL('./engine/', document.baseURI).href,
    module: 'kition_pdf_kit',
    documentClass: 'PdfDocument',
    // For Chinese fonts a PDF names but does not embed (build-engine.sh copies this one from kition-fonts).
    fonts: [new URL('./fonts/DroidSansFallbackFull.ttf', document.baseURI).href],
    setup: /^#[0-9a-f]{6}$/i.test(canvas) ? [{ method: 'setBackdrop', args: [canvas] }] : [],
  }
}
