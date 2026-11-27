type Area = { x0: number; y0: number; x1: number; y1: number }
export type SelectionPoint = { page: number; point: [number, number] }
export type SelectionRequest = { page: number; anchor: [number, number]; focus: [number, number]; endPage: number; mode: 'range' | 'column' }
export type SelectionResult = { text: string; pages: Array<{ page: number; text: string; rects: number[][] }> }

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
