export const THUMB_WIDTH = 132
export const THUMB_HEIGHT = 180
export const THUMB_ROW_HEIGHT = 240
export const THUMB_MAX_ROWS = 20
export const THUMB_MAX_BYTES = 2 * 1024 * 1024

export type ThumbnailSource = { render: (x: number, y: number, scale: number, width: number, height: number) => Promise<Uint8Array> }
export type ThumbnailArea = { x0: number; y0: number; x1: number; y1: number }
export type ThumbnailRequest = { index: number; area: ThumbnailArea }
export type Thumbnail = { width: number; height: number; pixels: Uint8Array } | { error: string }

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

/** One active worker call; new scroll demand replaces queued work and bounds retained pixels. */
export class ThumbnailQueue {
  private source: ThumbnailSource | null = null
  private revision = -1
  private generation = 0
  private entries = new Map<number, Thumbnail>()
  private requests: ThumbnailRequest[] = []
  private active = false
  private disposed = false
  private timer: ReturnType<typeof setTimeout> | null = null
  private notify: () => void

  constructor(notify: () => void) { this.notify = notify }

  get bytes() { return [...this.entries.values()].reduce((sum, item) => sum + ('pixels' in item ? item.pixels.byteLength : 0), 0) }
  get size() { return this.entries.size }
  get(index: number, source: ThumbnailSource | null, revision: number) {
    return source === this.source && revision === this.revision ? this.entries.get(index) : undefined
  }

  update(source: ThumbnailSource | null, revision: number, requests: ThumbnailRequest[]) {
    if (this.disposed) return
    if (source !== this.source || revision !== this.revision) {
      this.source = source
      this.revision = revision
      this.generation += 1
      this.entries.clear()
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

  private schedule() {
    if (this.active || this.timer || this.disposed || !this.source) return
    // Give the document viewport's immediate render priority over its thumbnails.
    this.timer = setTimeout(() => { this.timer = null; void this.next() }, 150)
  }

  private async next() {
    const source = this.source
    const request = this.requests.find((item) => !this.entries.has(item.index))
    if (!source || !request || this.disposed) return
    this.active = true
    const generation = this.generation
    let result: Thumbnail
    try {
      const { width, height, scale } = thumbnailSize(request.area)
      const pixels = await source.render(request.area.x0, request.area.y0, scale, width, height)
      if (pixels.byteLength !== width * height * 4) throw new Error('The thumbnail renderer returned incomplete pixels')
      result = { width, height, pixels: pixels.buffer.byteLength === pixels.byteLength ? pixels : pixels.slice() }
    } catch (error) {
      result = { error: error instanceof Error ? error.message : String(error) }
    }
    this.active = false
    if (!this.disposed && generation === this.generation && this.requests.some((item) => item.index === request.index)) {
      if ('pixels' in result && this.bytes + result.pixels.byteLength > THUMB_MAX_BYTES) result = { error: 'The thumbnail memory limit was reached' }
      this.entries.set(request.index, result)
      this.notify()
    }
    this.schedule()
  }
}
