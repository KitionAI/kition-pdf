/**
 * Text markup from what the person picked: a text selection's lines, or the find bar's current
 * match. `text.select` and `text.find` answer page-local rectangles (points from the displayed
 * page's top-left, y down: `[left, top, right, bottom]`), and `comment.add` takes `quads` in the
 * same space, four corners each in the order upper-left, upper-right, lower-left, lower-right (as
 * the engine's own `find` path builds them), so each line rectangle is one quad.
 */
import type { SelectionResult } from './textSelection'

export type MarkupType = 'highlight' | 'underline' | 'strikeout'
/** One `comment.add` of a markup: a page (1-based) and its quads. */
export type MarkupRun = { page: number; quads: number[][] }

/** A page-local rectangle as a quad; null for one that is not a finite, non-empty rectangle. */
export function rectQuad(rect: readonly number[]): number[] | null {
  if (rect.length !== 4 || !rect.every(Number.isFinite)) return null
  const [left, top, right, bottom] = [Math.min(rect[0], rect[2]), Math.min(rect[1], rect[3]), Math.max(rect[0], rect[2]), Math.max(rect[1], rect[3])]
  if (right - left <= 0 || bottom - top <= 0) return null
  return [left, top, right, top, left, bottom, right, bottom]
}

function runOf(page: number, rects: readonly number[][]): MarkupRun | null {
  const quads = rects.flatMap((rect) => rectQuad(rect) ?? [])
  return quads.length ? { page, quads: chunk(quads) } : null
}

function chunk(flat: number[]): number[][] {
  const out: number[][] = []
  for (let i = 0; i + 8 <= flat.length; i += 8) out.push(flat.slice(i, i + 8))
  return out
}

/** A selection's markup: one run per page it has lines on (a selection across pages marks each). */
export function selectionRuns(selected: SelectionResult | null): MarkupRun[] {
  if (!selected?.text) return []
  return selected.pages.flatMap((item) => runOf(item.page, item.rects) ?? [])
}

/** The find bar's current match as one run. */
export function matchRuns(match: { page: number; rects: number[][] } | null): MarkupRun[] {
  if (!match) return []
  const run = runOf(match.page, match.rects)
  return run ? [run] : []
}

/** Where a selection ends on screen, for its action bar: the last line's rectangle (page-local) and its page. */
export function selectionEnd(selected: SelectionResult | null): { page: number; rect: number[] } | null {
  if (!selected?.text) return null
  for (let i = selected.pages.length - 1; i >= 0; i -= 1) {
    const item = selected.pages[i]
    const rect = item.rects.at(-1)
    if (rect && rectQuad(rect)) return { page: item.page, rect }
  }
  return null
}

/** The smallest page-local rectangle round a selection's lines on one page, or null across pages. */
export function selectionBounds(selected: SelectionResult | null): { page: number; rect: number[] } | null {
  const pages = (selected?.text ? selected.pages : []).filter((item) => item.rects.some((rect) => rectQuad(rect)))
  if (pages.length !== 1) return null
  const rects = pages[0].rects.filter((rect) => rectQuad(rect))
  return {
    page: pages[0].page,
    rect: [Math.min(...rects.map((r) => Math.min(r[0], r[2]))), Math.min(...rects.map((r) => Math.min(r[1], r[3]))), Math.max(...rects.map((r) => Math.max(r[0], r[2]))), Math.max(...rects.map((r) => Math.max(r[1], r[3])))],
  }
}
