import type { KitModuleManifest } from '@kition/sdk/editor'
import { canvasColor } from './backdrop'

/** The engine, from this package's own folder, on the host's canvas color. Called after the theme is applied. */
export function engineManifest(): KitModuleManifest {
  const canvas = canvasColor()
  return {
    kitId: 'kit.pdf',
    base: new URL('./engine/', document.baseURI).href,
    module: 'kition_pdf_kit',
    documentClass: 'PdfDocument',
    // For Chinese fonts a PDF names but does not embed (build-engine.sh copies this one from kition-fonts).
    fonts: [new URL('./fonts/DroidSansFallbackFull.ttf', document.baseURI).href],
    setup: canvas ? [{ method: 'setBackdrop', args: [canvas] }] : [],
  }
}
