type Area = { x0: number; y0: number; x1: number; y1: number }
export type SelectionPoint = { page: number; point: [number, number] }
/** What a range's ends widen to: two quick clicks a word, three the line, four the page. */
export type SelectionUnit = 'word' | 'line' | 'page'
/**
 * A `text.select` request. An end is a page-local point (`anchor`, `focus`) or a glyph index
 * (`anchorGlyph`, `focusGlyph`) from an earlier answer's `ends`; `unit` widens both ends; `step`
 * moves the focus end by `stepBy` glyphs, words or lines (Shift with the arrow keys).
 */
export type SelectionRequest = {
  page: number
  endPage: number
  mode: 'range' | 'column'
  anchor?: [number, number]
  focus?: [number, number]
  anchorGlyph?: number
  focusGlyph?: number
  unit?: SelectionUnit
  step?: number
  stepBy?: 'glyph' | 'word' | 'line'
  /** Answer the range's ends (`SelectionResult.ends`), for a Shift+click or Shift+arrow after it. */
  ends?: boolean
}
/** One end of a range as the engine resolved it: its page (1-based) and glyph index. */
export type SelectionEnd = { page: number; glyph: number }
export type SelectionResult = {
  text: string
  pages: Array<{ page: number; text: string; rects: number[][] }>
  /** A range's ends (a column has none); null on a page without text. */
  ends?: { anchor: SelectionEnd | null; head: SelectionEnd | null }
}

/** The unit `clicks` quick presses select: none for one, then a word, the line, the page. */
export function clickUnit(clicks: number | undefined): SelectionUnit | undefined {
  const count = Number.isFinite(clicks) ? Math.floor(clicks as number) : 1
  return count >= 4 ? 'page' : count === 3 ? 'line' : count === 2 ? 'word' : undefined
}

/**
 * The request that moves a range's focus end `by` glyphs (`stepBy` word: words; line: lines) from
 * where the engine last put it; null without both ends.
 */
export function stepRequest(result: SelectionResult | null, by: number, stepBy: 'glyph' | 'word' | 'line'): SelectionRequest | null {
  const anchor = result?.ends?.anchor
  const head = result?.ends?.head
  if (!anchor || !head || !Number.isFinite(by) || by === 0) return null
  return { page: anchor.page, anchorGlyph: anchor.glyph, endPage: head.page, focusGlyph: head.glyph, mode: 'range', step: Math.trunc(by), ...(stepBy !== 'glyph' ? { stepBy } : {}), ends: true }
}

/**
 * How far a drag near an edge of the pages' view scrolls them each frame, in CSS pixels (negative:
 * up): nothing farther than `edge` inside, more the nearer the pointer and beyond the edge.
 */
export function edgeScroll(pointer: number, start: number, end: number, edge = EDGE_PX): number {
  if (![pointer, start, end].every(Number.isFinite) || end - start < 4 * edge) return 0
  const before = pointer - start, after = end - pointer
  if (before < edge) return -Math.min(EDGE_MAX_STEP, Math.ceil((edge - before) / 3))
  if (after < edge) return Math.min(EDGE_MAX_STEP, Math.ceil((edge - after) / 3))
  return 0
}
/** Within this many CSS pixels of the view's top or bottom a selection drag scrolls. */
export const EDGE_PX = 32
/** The most a frame of edge scrolling moves, in CSS pixels. */
export const EDGE_MAX_STEP = 40

/** Canvas tools already supply document coordinates; crop/rotation belong to the engine. */
export function selectionPoint(pages: readonly { area: Area }[], x: number, y: number, nearest = false): SelectionPoint | null {
  if (!Number.isFinite(x) || !Number.isFinite(y)) return null
  let best: SelectionPoint | null = null, distance = Infinity
  pages.forEach(({ area }, index) => {
    if (!Object.values(area).every(Number.isFinite) || area.x1 <= area.x0 || area.y1 <= area.y0) return
    const px = Math.max(area.x0, Math.min(area.x1, x)), py = Math.max(area.y0, Math.min(area.y1, y))
    const next = Math.hypot(x - px, y - py)
    if (next < distance && (nearest || next === 0)) {
      best = { page: index + 1, point: [px - area.x0, py - area.y0] }
      distance = next
    }
  })
  return best
}

/** Ignore editable fields even if a PDF selection remains visible. */
export function isTextControl(target: EventTarget | null): boolean {
  return target instanceof Element && !!target.closest('input, textarea, select, [contenteditable]:not([contenteditable="false"])')
}

/** Coalesce pointer samples to one running request and one latest pending endpoint. */
export class SelectionSession {
  private generation = 0
  private busy = false
  private disposed = false
  private pending: { request: SelectionRequest; generation: number } | null = null
  private execute: (request: SelectionRequest) => Promise<SelectionResult | null>
  private notify: (result: SelectionResult | null) => void
  private failed: (error?: unknown) => void

  constructor(
    execute: (request: SelectionRequest) => Promise<SelectionResult | null>,
    notify: (result: SelectionResult | null) => void,
    failed: (error?: unknown) => void,
  ) { this.execute = execute; this.notify = notify; this.failed = failed }

  request(request: SelectionRequest) {
    if (this.disposed) return
    this.pending = { request, generation: ++this.generation }
    this.notify(null)
    void this.drain()
  }

  /**
   * Selects with the first of `requests` the engine answers (Select All: the whole document, then
   * the current page when the engine refuses a span that long); fails only when none answers.
   */
  choose(requests: readonly SelectionRequest[]) {
    if (this.disposed) return
    const generation = ++this.generation
    this.pending = null
    this.notify(null)
    const live = () => !this.disposed && generation === this.generation
    void (async () => {
      let failure: unknown
      for (const request of requests) {
        try {
          const result = await this.execute(request)
          if (!live()) return
          if (result) { this.notify(result); return }
        } catch (error) {
          if (!live()) return
          failure = error
        }
      }
      if (live()) this.failed(failure)
    })()
  }

  clear() {
    this.generation++
    this.pending = null
    if (!this.disposed) this.notify(null)
  }

  dispose() { this.clear(); this.disposed = true }

  private async drain() {
    if (this.busy || this.disposed) return
    this.busy = true
    try {
      while (this.pending && !this.disposed) {
        const { request, generation } = this.pending
        this.pending = null
        try {
          const result = await this.execute(request)
          if (this.disposed || generation !== this.generation) continue
          this.notify(result)
          if (!result) this.failed()
        } catch (error) {
          if (!this.disposed && generation === this.generation) { this.notify(null); this.failed(error) }
        }
      }
    } finally { this.busy = false }
  }
}
