export const THUMB_WIDTH = 132
export const THUMB_HEIGHT = 180
export const THUMB_ROW_HEIGHT = 240
export const THUMB_MAX_ROWS = 20
export const THUMB_MAX_BYTES = 2 * 1024 * 1024

/**
 * What thumbnails are drawn from: the engine's `render`, and its page stamps when it has them
 * (`pageStamps`: a page whose stamp is unchanged draws as it did, so its thumbnail stays).
 */
export type ThumbnailSource = {
  render: (x: number, y: number, scale: number, width: number, height: number) => Promise<Uint8Array>
  stamps?: () => Promise<PageStamps>
}
/** The engine's page stamps at its `revision` (its edit counter), one per page in page order. */
export type PageStamps = { revision: number; stamps: string[] }
export type ThumbnailArea = { x0: number; y0: number; x1: number; y1: number }
/** A page to show; `stamp` is its stamp now, when the source has stamps. */
export type ThumbnailRequest = { index: number; area: ThumbnailArea; stamp?: string }
export type Thumbnail = { width: number; height: number; pixels: Uint8Array } | { error: string }
/** A drawn thumbnail and what it was drawn from: the page's stamp, or the revision without one. */
type Entry = { image: Thumbnail; stamp?: string; revision: number }

/** The engine's `pageStamps` answer (JSON text), or null when it is not one. */
export function parseStamps(answer: unknown): PageStamps | null {
  try {
    const value = typeof answer === 'string' ? JSON.parse(answer) : null
    if (!value || !Number.isFinite(value.revision) || !Array.isArray(value.stamps) || !value.stamps.every((stamp: unknown) => typeof stamp === 'string')) return null
    return { revision: value.revision, stamps: value.stamps }
  } catch {
    return null
  }
}

export function thumbnailSize(area: ThumbnailArea) {
  const width = area.x1 - area.x0
  const height = area.y1 - area.y0
  if (![area.x0, area.y0, width, height].every(Number.isFinite) || width <= 0 || height <= 0) throw new Error('The page has invalid dimensions')
  const scale = Math.min(THUMB_WIDTH / width, THUMB_HEIGHT / height)
  return { width: Math.max(1, Math.round(width * scale)), height: Math.max(1, Math.round(height * scale)), scale }
}

export function thumbnailWindow(count: number, top: number, height: number) {
  const start = Math.max(0, Math.min(Math.max(0, count - 1), Math.floor(top / THUMB_ROW_HEIGHT) - 1))
  const end = Math.min(count, start + THUMB_MAX_ROWS, Math.ceil((top + height) / THUMB_ROW_HEIGHT) + 1)
  return { start, end: Math.max(start, end) }
}

/**
 * One active worker call; new scroll demand replaces queued work and bounds retained pixels.
 * After an edit a page keeps its thumbnail while its stamp is unchanged, and a changed page shows
 * its old thumbnail until the new one arrives (never a blank in between). A source without stamps
 * draws every page again after each revision, its old thumbnail still shown meanwhile.
 */
export class ThumbnailQueue {
  private source: ThumbnailSource | null = null
  private revision = -1
  private generation = 0
  private entries = new Map<number, Entry>()
  private requests: ThumbnailRequest[] = []
  private active = false
  private disposed = false
  private timer: ReturnType<typeof setTimeout> | null = null
  private notify: () => void
  /** Thumbnails drawn since the queue was made (each one worker render). */
  renders = 0

  constructor(notify: () => void) { this.notify = notify }

  get bytes() { return [...this.entries.values()].reduce((sum, item) => sum + ('pixels' in item.image ? item.image.pixels.byteLength : 0), 0) }
  get size() { return this.entries.size }
  /** The thumbnail to show for page `index` of `source`: the current one, or the one it replaces. */
  get(index: number, source: ThumbnailSource | null) {
    return source === this.source ? this.entries.get(index)?.image : undefined
  }

  update(source: ThumbnailSource | null, revision: number, requests: ThumbnailRequest[]) {
    if (this.disposed) return
    if (source !== this.source) {
      this.source = source
      this.entries.clear()
      this.generation += 1
    }
    if (revision !== this.revision) {
      // What is being drawn may be of the state before: it is not kept.
      this.revision = revision
      this.generation += 1
    }
    this.requests = [...new Map(requests.map((item) => [item.index, item])).values()].slice(0, THUMB_MAX_ROWS + 1)
    const wanted = new Set(this.requests.map((item) => item.index))
    for (const index of this.entries.keys()) if (!wanted.has(index)) this.entries.delete(index)
    this.notify()
    this.schedule()
  }

  retry(index: number) {
    if (this.disposed) return
    this.entries.delete(index)
    this.notify()
    this.schedule()
  }

  dispose() {
    this.disposed = true
    this.generation += 1
    if (this.timer) clearTimeout(this.timer)
    this.entries.clear()
    this.requests = []
  }

  /** Whether page `request`'s thumbnail is the one to show now. */
  private current(request: ThumbnailRequest) {
    const entry = this.entries.get(request.index)
    if (!entry) return false
    return request.stamp !== undefined ? entry.stamp === request.stamp : entry.revision === this.revision
  }

  private schedule() {
    if (this.active || this.timer || this.disposed || !this.source || !this.requests.some((item) => !this.current(item))) return
    // Give the document viewport's immediate render priority over its thumbnails.
    this.timer = setTimeout(() => { this.timer = null; void this.next() }, 150)
  }

  private async next() {
    const source = this.source
    const request = this.requests.find((item) => !this.current(item))
    if (!source || !request || this.disposed) return
    this.active = true
    const generation = this.generation
    let result: Thumbnail
    // With stamps, the page's stamp read just before and just after the render (the worker runs
    // requests in order): when the engine's revision is the same, the pixels are that stamp's.
    let stamp: string | undefined
    let settled = true
    try {
      const { width, height, scale } = thumbnailSize(request.area)
      const withStamps = request.stamp !== undefined && source.stamps
      const before = withStamps ? source.stamps!() : null
      const rendering = source.render(request.area.x0, request.area.y0, scale, width, height)
      const after = withStamps ? source.stamps!() : null
      const pixels = await rendering
      if (before && after) {
        const [first, last] = await Promise.all([before, after])
        settled = first.revision === last.revision
        stamp = last.stamps[request.index]
      }
      if (pixels.byteLength !== width * height * 4) throw new Error('The thumbnail renderer returned incomplete pixels')
      result = { width, height, pixels: pixels.buffer.byteLength === pixels.byteLength ? pixels : pixels.slice() }
    } catch (error) {
      result = { error: error instanceof Error ? error.message : String(error) }
      stamp = request.stamp
    }
    this.active = false
    if (settled && !this.disposed && generation === this.generation && this.requests.some((item) => item.index === request.index)) {
      if ('pixels' in result) this.renders += 1
      const replaced = this.entries.get(request.index)?.image
      const kept = this.bytes - (replaced && 'pixels' in replaced ? replaced.pixels.byteLength : 0)
      if ('pixels' in result && kept + result.pixels.byteLength > THUMB_MAX_BYTES) result = { error: 'The thumbnail memory limit was reached' }
      this.entries.set(request.index, { image: result, stamp, revision: this.revision })
      this.notify()
    }
    this.schedule()
  }
}
