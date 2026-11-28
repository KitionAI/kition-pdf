/**
 * The page layer's geometry (stage 4 of "PDF usable by hand"): what lies under the pointer on a
 * page, a selected comment's handles, and the rectangle a drag makes. Every rectangle here is
 * page-local, as the engine's `comment.list`, `form.listFields`, `link.list` and `text.listLines`
 * answer them: points from the displayed page's top-left, y down, `[left, top, right, bottom]`,
 * already turned by the page's rotation. A page's `area` places it in the document, the space the
 * canvas's pointer and view are in, so a page-local point is the pointer less the area's corner.
 */

export type Rect = [number, number, number, number]
export type Area = { x0: number; y0: number; x1: number; y1: number }
export type View = { x: number; y: number; zoom: number }

/** A comment as the page layer needs it (a `comment.list` root). */
export type PageComment = {
  id: string | null
  page: number
  index: number
  type: string
  rect?: number[] | null
  contents?: string | null
  author?: string | null
  color?: string | null
  locked?: boolean
}
/** One of a field's widgets (`form.listFields`'s `widgets`): where it is, its place in the tab order, whether it has an appearance to show, and a check box's or radio button's export value and state. */
export type FieldWidget = { page: number | null; rect: number[] | null; rotation?: number; tab?: number | null; appearance?: boolean; hidden?: boolean; export?: string | null; on?: boolean }
export type FieldOption = string | { value: string; label: string }
/** A form field as `form.listFields` answers it. */
export type PageField = {
  name: string
  page: number | null
  rect: number[] | null
  type: string
  value?: unknown
  read_only?: boolean
  required?: boolean
  tooltip?: string
  options?: FieldOption[]
  multiline?: boolean
  multi_select?: boolean
  editable?: boolean
  max_length?: number
  font_size?: number
  widgets?: FieldWidget[]
}

/** A field's widgets; an answer without them stands for its one widget (its page and rect). */
export function widgetsOf(field: PageField): FieldWidget[] {
  return field.widgets?.length ? field.widgets : [{ page: field.page, rect: field.rect }]
}
export type PageLink = { page: number; index: number; rect: number[]; to_page?: number; url?: string; other?: string }
export type PageObjects = { comments: PageComment[]; fields: PageField[]; links: PageLink[] }

export const NO_OBJECTS: PageObjects = { comments: [], fields: [], links: [] }

/** Text markup: it follows its text, so it is never moved, and a drag over it selects that text. */
export const MARKUPS = new Set(['Highlight', 'Underline', 'StrikeOut', 'Squiggly'])
/** The kinds `comment.edit`'s `rect` resizes (the engine refuses the others). */
export const RESIZABLE = new Set(['Square', 'Circle', 'FreeText', 'Stamp', 'Ink'])

/** How far round a comment a press, a hover or a right-click still reaches it, in CSS pixels. */
export const SLOP_PX = 3
/** A handle's half side as drawn, and how far from its centre a press still takes it, in CSS pixels. */
export const HANDLE_PX = 4
export const HANDLE_REACH_PX = 6
/** A move of at most this many points, or a resize whose corners each moved at most this, is a slip: nothing changes. */
export const SLIP_MOVE = 2
export const SLIP_RESIZE = 4
/** A press that travels less than this, in CSS pixels, is a click. */
export const CLICK_PX = 3
/** The smallest side a resize leaves, in points. */
export const MIN_SIDE = 2
/** The arrow keys move a selected comment this far (points); with Shift or Cmd, the far step. */
export const NUDGE = 1
export const NUDGE_FAR = 10

export function movable(item: PageComment) {
  return !item.locked && !MARKUPS.has(item.type) && item.type !== 'Caret'
}
export function resizable(item: PageComment) {
  return !item.locked && RESIZABLE.has(item.type)
}

/** A finite, non-empty rectangle from an answer's four numbers, ordered; null otherwise. */
export function asRect(value: readonly number[] | null | undefined): Rect | null {
  if (!value || value.length !== 4 || !value.every(Number.isFinite)) return null
  const rect: Rect = [Math.min(value[0], value[2]), Math.min(value[1], value[3]), Math.max(value[0], value[2]), Math.max(value[1], value[3])]
  return rect
}

export function inRect(rect: Rect, x: number, y: number, grow = 0) {
  return x >= rect[0] - grow && x <= rect[2] + grow && y >= rect[1] - grow && y <= rect[3] + grow
}

function validArea(area: Area | undefined): area is Area {
  return !!area && Object.values(area).every(Number.isFinite) && area.x1 > area.x0 && area.y1 > area.y0
}

/** The page (1-based) a document point is on and the page-local point, or null between and off the pages. */
export function pageAt(pages: readonly { area: Area }[], x: number, y: number): { page: number; point: [number, number] } | null {
  if (!Number.isFinite(x) || !Number.isFinite(y)) return null
  for (let index = 0; index < pages.length; index += 1) {
    const area = pages[index].area
    if (validArea(area) && x >= area.x0 && x <= area.x1 && y >= area.y0 && y <= area.y1) return { page: index + 1, point: [x - area.x0, y - area.y0] }
  }
  return null
}

/** The eight resize handles, corners and edge middles, clockwise from the top-left. */
export const HANDLES = ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w'] as const
export type Handle = (typeof HANDLES)[number]
export const HANDLE_CURSOR: Record<Handle, string> = { nw: 'nwse-resize', se: 'nwse-resize', ne: 'nesw-resize', sw: 'nesw-resize', n: 'ns-resize', s: 'ns-resize', e: 'ew-resize', w: 'ew-resize' }

export function handlePoint(rect: Rect, handle: Handle): [number, number] {
  const mx = (rect[0] + rect[2]) / 2
  const my = (rect[1] + rect[3]) / 2
  const x = handle.includes('w') ? rect[0] : handle.includes('e') ? rect[2] : mx
  const y = handle.includes('n') ? rect[1] : handle.includes('s') ? rect[3] : my
  return [x, y]
}

/** What a press or the pointer is over, in the order presses go: the selected comment's handles, a field, a comment, a link, the text or the blank page. */
export type Hit =
  | { kind: 'handle'; comment: PageComment; handle: Handle }
  | { kind: 'field'; field: PageField }
  | { kind: 'comment'; comment: PageComment }
  | { kind: 'link'; link: PageLink }
  | { kind: 'markup'; comment: PageComment }
  | { kind: 'page' }
  | { kind: 'off' }

/**
 * What is under page-local point `point` of `page` at `zoom`. The selected comment's handles come
 * first, then a field, then the topmost comment (the last on the page) other than text markup,
 * then a link, then text markup (a link under a highlight is followed, the highlight chosen only
 * where there is no link), then the page itself.
 */
export function hitTest(objects: PageObjects, chosen: PageComment | null, page: number, point: [number, number], zoom: number): Hit {
  const [x, y] = point
  const z = zoom > 0 && Number.isFinite(zoom) ? zoom : 1
  const slop = SLOP_PX / z
  if (chosen && chosen.page === page && resizable(chosen)) {
    const rect = asRect(chosen.rect)
    if (rect) {
      const reach = HANDLE_REACH_PX / z
      for (const handle of HANDLES) {
        const [hx, hy] = handlePoint(rect, handle)
        if (Math.abs(x - hx) <= reach && Math.abs(y - hy) <= reach) return { kind: 'handle', comment: chosen, handle }
      }
    }
  }
  for (let i = objects.fields.length - 1; i >= 0; i -= 1) {
    const field = objects.fields[i]
    for (const widget of widgetsOf(field)) {
      const rect = asRect(widget.rect)
      if (widget.page === page && !widget.hidden && rect && inRect(rect, x, y)) return { kind: 'field', field }
    }
  }
  let markup: PageComment | null = null
  for (let i = objects.comments.length - 1; i >= 0; i -= 1) {
    const comment = objects.comments[i]
    const rect = asRect(comment.rect)
    if (comment.page !== page || !rect || !inRect(rect, x, y, slop)) continue
    if (MARKUPS.has(comment.type)) {
      markup ??= comment
      continue
    }
    return { kind: 'comment', comment }
  }
  for (let i = objects.links.length - 1; i >= 0; i -= 1) {
    const link = objects.links[i]
    const rect = asRect(link.rect)
    if (link.page === page && rect && inRect(rect, x, y)) return { kind: 'link', link }
  }
  if (markup) return { kind: 'markup', comment: markup }
  return { kind: 'page' }
}

/** Whether a page-local point is on a line of text (`lines` from `text.listLines`); null lines: not known yet, so text. */
export function onText(lines: readonly Rect[] | null, point: [number, number]) {
  if (!lines) return true
  return lines.some((rect) => inRect(rect, point[0], point[1], 1))
}

/** The pointer's cursor over a hit. */
export function cursorFor(hit: Hit, lines: readonly Rect[] | null, point: [number, number] | null): string {
  switch (hit.kind) {
    case 'handle':
      return HANDLE_CURSOR[hit.handle]
    case 'comment':
      return movable(hit.comment) ? 'move' : 'default'
    case 'link':
      return 'pointer'
    case 'markup':
    case 'field':
    case 'off':
      return 'default'
    case 'page':
      return point && onText(lines, point) ? 'text' : 'default'
  }
}

/**
 * The rectangle a handle drag makes from `rect`, the handle moved by (`dx`, `dy`) points: the
 * opposite corner or edge stays; past it the rectangle turns over. With `keep` a corner keeps the
 * width-to-height ratio (Shift). Never thinner than `MIN_SIDE`.
 */
export function resized(rect: Rect, handle: Handle, dx: number, dy: number, keep = false): Rect {
  let [l, t, r, b] = rect
  const w = r - l
  const h = b - t
  const west = handle.includes('w')
  const east = handle.includes('e')
  const north = handle.includes('n')
  const south = handle.includes('s')
  if (keep && (west || east) && (north || south) && w > 0 && h > 0) {
    // The larger relative change sets the scale; the opposite corner stays.
    const sx = (east ? w + dx : w - dx) / w
    const sy = (south ? h + dy : h - dy) / h
    const scale = Math.abs(sx - 1) > Math.abs(sy - 1) ? sx : sy
    const nw = w * scale
    const nh = h * scale
    if (east) r = l + nw
    else l = r - nw
    if (south) b = t + nh
    else t = b - nh
  } else {
    if (west) l += dx
    if (east) r += dx
    if (north) t += dy
    if (south) b += dy
  }
  const out: Rect = [Math.min(l, r), Math.min(t, b), Math.max(l, r), Math.max(t, b)]
  if (out[2] - out[0] < MIN_SIDE) out[2] = out[0] + MIN_SIDE
  if (out[3] - out[1] < MIN_SIDE) out[3] = out[1] + MIN_SIDE
  return out
}

/** Whether a resize from `before` to `after` is a slip: every corner moved at most `SLIP_RESIZE` points. */
export function resizeSlipped(before: Rect, after: Rect) {
  return before.every((value, i) => Math.abs(value - after[i]) <= SLIP_RESIZE)
}

/** Whether a move of (`dx`, `dy`) points is a slip. */
export function moveSlipped(dx: number, dy: number) {
  return Math.abs(dx) <= SLIP_MOVE && Math.abs(dy) <= SLIP_MOVE
}

/**
 * A move of `rect` by (`dx`, `dy`) kept on a page `width` by `height` points: a comment that lies
 * on its page stays on it; one already partly off moves freely.
 */
export function keptOnPage(rect: Rect, dx: number, dy: number, width: number, height: number): [number, number] {
  const fits = rect[0] >= 0 && rect[1] >= 0 && rect[2] <= width && rect[3] <= height
  if (!fits) return [dx, dy]
  return [Math.max(-rect[0], Math.min(width - rect[2], dx)), Math.max(-rect[1], Math.min(height - rect[3], dy))]
}

export function shifted(rect: Rect, dx: number, dy: number): Rect {
  return [rect[0] + dx, rect[1] + dy, rect[2] + dx, rect[3] + dy]
}

/** A page-local rectangle on screen: CSS pixels from the canvas's top-left (not clipped to the page). */
export function screenBox(area: Area | undefined, rect: Rect, view: View | null) {
  if (!validArea(area) || !view || !(view.zoom > 0)) return null
  const box = { left: (area.x0 + rect[0] - view.x) * view.zoom, top: (area.y0 + rect[1] - view.y) * view.zoom, width: (rect[2] - rect[0]) * view.zoom, height: (rect[3] - rect[1]) * view.zoom }
  return Object.values(box).every(Number.isFinite) ? box : null
}

/** A point in the browser window (a pointer event's client point) as a document point, given the canvas's box on screen. */
export function clientToDocument(clientX: number, clientY: number, box: { left: number; top: number }, view: View): [number, number] {
  return [view.x + (clientX - box.left) / view.zoom, view.y + (clientY - box.top) / view.zoom]
}

/** The page layer's lists from the engine's answers, each page's in its order (paint order: the last is on top). */
export function objectsFrom(comments: unknown, fields: unknown, links: unknown): PageObjects {
  const list = <T>(value: unknown, key: string): T[] => {
    const items = value && typeof value === 'object' ? (value as Record<string, unknown>)[key] : null
    return Array.isArray(items) ? (items as T[]) : []
  }
  return {
    comments: list<PageComment>(comments, 'comments').filter((item) => item && typeof item.page === 'number'),
    fields: list<PageField>(fields, 'fields').filter((item) => item && typeof item.name === 'string'),
    links: list<PageLink>(links, 'links').filter((item) => item && typeof item.page === 'number'),
  }
}
