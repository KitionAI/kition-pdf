import type { KitCommand, KitEngineInfo, RasterRect } from '@kition/sdk/editor'

/** What the editor reads from the engine's `inspect()`. */
export type PdfInfo = {
  pages: Array<{ area: RasterRect; label: string }>
  editable: boolean
  readOnlyReason: string | null
  formFields: number
}

type Area = { x: number; y: number; width: number; height: number }

export function describe(info: KitEngineInfo): PdfInfo {
  const inspected = info.inspected as { pages?: Array<{ area?: Area; label?: string }>; editable?: boolean; readOnlyReason?: string | null; formFields?: number }
  return {
    pages: (inspected.pages ?? []).map((page, index) => ({
      area: page.area ? { x0: page.area.x, y0: page.area.y, x1: page.area.x + page.area.width, y1: page.area.y + page.area.height } : { x0: 0, y0: 0, x1: 0, y1: 0 },
      label: page.label ?? String(index + 1),
    })),
    editable: inspected.editable !== false,
    readOnlyReason: inspected.readOnlyReason ?? null,
    formFields: inspected.formFields ?? 0,
  }
}

/** A page's full width with a margin, from its top: the canvas opens and fits width with it. */
export function pageWidthArea(page: RasterRect | undefined): RasterRect | null {
  if (!page) return null
  const width = page.x1 - page.x0
  return { x0: page.x0 - 24, y0: page.y0 - 24, x1: page.x1 + 24, y1: page.y0 + width * 0.6 }
}

/**
 * Where panning stops: every page with the same margin `pageWidthArea` opens with, so a drag
 * never pulls the pages out of view (a document smaller than the view stays centred in it).
 */
export function pagesPanBounds(pages: readonly RasterRect[]): RasterRect | null {
  const shown = pages.filter((page) => page.x1 > page.x0 && page.y1 > page.y0)
  if (!shown.length) return null
  return {
    x0: Math.min(...shown.map((page) => page.x0)) - 24,
    y0: Math.min(...shown.map((page) => page.y0)) - 24,
    x1: Math.max(...shown.map((page) => page.x1)) + 24,
    y1: Math.max(...shown.map((page) => page.y1)) + 24,
  }
}

export function wholePageArea(page: RasterRect | undefined): RasterRect | null {
  return page ? { x0: page.x0 - 16, y0: page.y0 - 16, x1: page.x1 + 16, y1: page.y1 + 16 } : null
}

/**
 * Where the person is, as a command's dialog starts: the current page (1-based) for `page`,
 * `pages`, and a blank page's `at`; a point near the page's top-left for a comment or text.
 */
export function commandDefaults(current: number) {
  return (command: KitCommand): Record<string, unknown> => {
    const schema = command.schema ?? {}
    const values: Record<string, unknown> = {}
    if ('page' in schema) values.page = current + 1
    if ('pages' in schema) values.pages = [current + 1]
    if ('from' in schema && command.id === 'page.number') values.from = 1
    if ('at' in schema) values.at = command.id === 'page.insertBlank' ? current + 2 : [72, 72]
    if (command.id === 'comment.add') Object.assign(values, { type: 'note', contents: '' })
    return values
  }
}

/**
 * The editor's tabs, the engine menus each one gathers (`menu: [menu, group]`), and the engine
 * commands its Single line row shows (Office's Simplified Ribbon; a command a browser build does
 * not offer is simply not drawn).
 */
export const TABS: Array<{ id: string; label: string; menus: string[]; primary: readonly string[] }> = [
  { id: 'home', label: 'Home', menus: [], primary: [] },
  { id: 'comment', label: 'Comments', menus: ['Comment'], primary: ['comment.add', 'comment.reply', 'comment.edit', 'comment.delete', 'comment.setStatus', 'comment.hide'] },
  { id: 'edit', label: 'Edit', menus: ['Edit PDF', 'Text', 'Links'], primary: ['content.set', 'image.edit', 'object.move', 'text.find', 'text.select', 'text.edit', 'link.add', 'link.edit', 'link.delete'] },
  { id: 'pages', label: 'Pages', menus: ['Pages', 'Bookmarks'], primary: ['page.insertBlank', 'page.duplicate', 'page.delete', 'page.rotate', 'page.move', 'page.number', 'page.addText', 'bookmark.add', 'bookmark_rename', 'bookmark_delete'] },
  { id: 'forms', label: 'Forms', menus: ['Forms'], primary: ['form.addField', 'form.fill', 'form.reset', 'form.detectFields', 'form.setProps', 'form.deleteField', 'fillSign.add'] },
  { id: 'protect', label: 'Protect', menus: ['Protect', 'Redact'], primary: ['signature.list', 'redaction.mark', 'redaction.apply', 'redaction.clear'] },
  { id: 'document', label: 'Document', menus: ['Document', 'Measure', 'Accessibility', 'Tools'], primary: ['file.getInfo', 'doc_set_info', 'file.setWatermark', 'file.setHeaderFooter', 'file.protect', 'doc_optimize', 'measure.distance', 'measure.area', 'accessibility.check', 'ocr.recognize'] },
]

/**
 * The page a page field's text names (0-based): a page label (`iv`), or a number kept to the
 * document (`99` is the last page, `0` the first); null for text that names no page.
 */
export function pageFromField(text: string, pages: readonly { label: string }[], count: number): number | null {
  const wanted = text.trim()
  if (!wanted || count < 1) return null
  const index = pages.findIndex((item) => item.label === wanted)
  if (index >= 0) return index
  if (!/^\d+$/.test(wanted)) return null
  return Math.max(0, Math.min(count - 1, Number(wanted) - 1))
}
