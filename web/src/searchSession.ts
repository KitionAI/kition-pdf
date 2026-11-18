export type Match = { page: number; rects: number[][] }
export const SEARCH_DELAY_MS = 200
export const SEARCH_CACHE_ENTRIES = 32
// Counts query characters, matches, rectangles, and coordinate values, not estimated heap bytes.
export const SEARCH_CACHE_UNITS = 65_536

type Find = (text: string) => Promise<{ matches: Match[] } | null>
type Cached = { matches: Match[]; units: number }

/** One document revision's debounced search, with bounded retained query geometry. */
export class SearchSession {
  private generation = 0
  private timer: ReturnType<typeof setTimeout> | null = null
  private disposed = false
  private cache = new Map<string, Cached>()
  private find: Find
  private notify: (matches: Match[] | null) => void

  constructor(find: Find, notify: (matches: Match[] | null) => void) {
    this.find = find
    this.notify = notify
  }

  get size() { return this.cache.size }
  get units() { return [...this.cache.values()].reduce((sum, entry) => sum + entry.units, 0) }

  search(text: string, composing = false) {
    if (this.disposed) return
    this.invalidate()
    this.notify(null)
    if (composing || !text.trim()) return
    const generation = this.generation
    this.timer = setTimeout(() => {
      this.timer = null
      void this.run(text, generation)
    }, SEARCH_DELAY_MS)
  }

  /** Invalidates pending replies immediately, including between input and React's next effect. */
  clear() {
    if (this.disposed) return
    this.invalidate()
    this.notify(null)
  }

  dispose() {
    this.invalidate()
    this.disposed = true
    this.cache.clear()
  }

  private invalidate() {
    this.generation++
    if (this.timer !== null) clearTimeout(this.timer)
    this.timer = null
  }

  private async run(text: string, generation: number) {
    const cached = this.cache.get(text)
    if (cached) {
      this.cache.delete(text)
      this.cache.set(text, cached)
      this.notify(cached.matches)
      return
    }
    try {
      const answer = await this.find(text)
      if (this.disposed || generation !== this.generation) return
      if (!answer) { this.notify(null); return }
      // The engine also returns matched text; retain only navigation geometry.
      const matches = answer.matches.map(({ page, rects }) => ({ page, rects }))
      const units = text.length + matches.reduce((sum, match) => sum + 1 + match.rects.reduce((count, rect) => count + 1 + rect.length, 0), 0)
      if (units <= SEARCH_CACHE_UNITS) {
        this.cache.set(text, { matches, units })
        while (this.cache.size > SEARCH_CACHE_ENTRIES || this.units > SEARCH_CACHE_UNITS) {
          const oldest = this.cache.keys().next().value
          if (oldest === undefined) break
          this.cache.delete(oldest)
        }
      }
      this.notify(matches)
    } catch {
      // A failed request is never cached or reported as a successful empty search.
      if (!this.disposed && generation === this.generation) this.notify(null)
    }
  }
}
