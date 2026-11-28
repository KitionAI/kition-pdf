/**
 * Organizing pages from the thumbnails (stage 7): which pages a page command acts on, how a
 * click with Shift or Cmd changes the selection, where a dragged set of pages lands and what
 * `page.move` is told, and which page is current after a move or a delete. Pages are 0-based
 * here; the engine's commands count from 1.
 */

/** The pages a page command acts on: the selected ones in order, else the current page. */
export function pageTargets(selected: readonly number[], current: number): number[] {
  return selected.length ? [...new Set(selected)].sort((a, b) => a - b) : [current]
}

/** Shift+click: every page from the anchor to `index`, in order. */
export function rangeSelection(anchor: number, index: number): number[] {
  const [from, to] = anchor <= index ? [anchor, index] : [index, anchor]
  return Array.from({ length: to - from + 1 }, (_, offset) => from + offset)
}

/** Cmd+click: `index` joins the selection or leaves it; with none, the current page counts as selected. */
export function toggleSelection(selected: readonly number[], current: number, index: number): number[] {
  const start = selected.length ? selected : [current]
  const next = start.includes(index) ? start.filter((page) => page !== index) : [...start, index]
  return [...new Set(next)].sort((a, b) => a - b)
}

/** The gap between rows a point is nearest (0 before the first row, `count` after the last). */
export function dropGap(offset: number, rowHeight: number, count: number): number {
  if (!(rowHeight > 0) || !Number.isFinite(offset)) return 0
  return Math.max(0, Math.min(count, Math.round(offset / rowHeight)))
}

/** The order of the pages once `pages` are put at the gap `gap` of the order before the move. */
export function orderAfterDrop(pages: readonly number[], gap: number, count: number): number[] {
  const moving = [...new Set(pages)].filter((page) => page >= 0 && page < count).sort((a, b) => a - b)
  const rest = Array.from({ length: count }, (_, index) => index).filter((index) => !moving.includes(index))
  const at = Math.max(0, Math.min(rest.length, gap - moving.filter((page) => page < gap).length))
  return [...rest.slice(0, at), ...moving, ...rest.slice(at)]
}

/**
 * What `page.move` is given for `pages` dropped at gap `gap`: the pages (from 1) and `to`, their
 * first one's place (from 1) among the pages left; null when the drop changes nothing.
 */
export function moveParams(pages: readonly number[], gap: number, count: number): { pages: number[]; to: number } | null {
  const moving = [...new Set(pages)].filter((page) => page >= 0 && page < count).sort((a, b) => a - b)
  if (!moving.length || moving.length >= count) return null
  const order = orderAfterDrop(moving, gap, count)
  if (order.every((page, index) => page === index)) return null
  return { pages: moving.map((page) => page + 1), to: order.indexOf(moving[0]) + 1 }
}

/** Where page `index` is after a move: its place in `order` (as `orderAfterDrop` answers). */
export function placeAfterMove(order: readonly number[], index: number): number {
  const place = order.indexOf(index)
  return place < 0 ? index : place
}

/**
 * The current page after `deleted` are deleted: the same page, one place earlier for each deleted
 * page before it; a deleted current page gives way to the page after the deleted run it was in.
 */
export function currentAfterDelete(current: number, deleted: readonly number[], count: number): number {
  const gone = new Set(deleted)
  const left = count - gone.size
  if (left <= 0) return 0
  let page = current
  while (gone.has(page) && page < count) page += 1
  if (page >= count) {
    page = current
    while (gone.has(page) && page > 0) page -= 1
  }
  const before = [...gone].filter((index) => index < page).length
  return Math.max(0, Math.min(left - 1, page - before))
}
