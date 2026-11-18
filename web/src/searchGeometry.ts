type Area = { x0: number; y0: number; x1: number; y1: number }
type View = { x: number; y: number; zoom: number }

/** Search rectangles are page-local, top-left points, already rotated by the engine. */
export function searchDocumentRects(page: Area | undefined, rects: readonly number[][]): Area[] {
  if (!page || !Object.values(page).every(Number.isFinite) || page.x1 <= page.x0 || page.y1 <= page.y0) return []
  const width = page.x1 - page.x0, height = page.y1 - page.y0
  return rects.flatMap((rect) => {
    if (rect.length !== 4 || !rect.every(Number.isFinite)) return []
    const [left, top, right, bottom] = rect
    const x0 = Math.max(0, left), y0 = Math.max(0, top)
    const x1 = Math.min(width, right), y1 = Math.min(height, bottom)
    if (x1 <= x0 || y1 <= y0) return []
    return [{ x0: page.x0 + x0, y0: page.y0 + y0, x1: page.x0 + x1, y1: page.y0 + y1 }]
  })
}

/** HTML overlays use CSS pixels; the canvas alone accounts for device pixel ratio. */
export function searchScreenRects(page: Area | undefined, rects: readonly number[][], view: View | null) {
  if (!view || ![view.x, view.y, view.zoom].every(Number.isFinite) || view.zoom <= 0) return []
  return searchDocumentRects(page, rects).flatMap((rect) => {
    const box = { left: (rect.x0 - view.x) * view.zoom, top: (rect.y0 - view.y) * view.zoom, width: (rect.x1 - rect.x0) * view.zoom, height: (rect.y1 - rect.y0) * view.zoom }
    return Object.values(box).every(Number.isFinite) ? [box] : []
  })
}

/** Keep an already-visible horizontal match in place; reveal off-screen columns. */
export function searchPanTarget(page: Area, rect: number[] | null, view: View, viewport: { width: number; height: number }) {
  const width = viewport.width / view.zoom, height = viewport.height / view.zoom
  const match = rect ? searchDocumentRects(page, [rect])[0] : undefined
  let x = page.x1 - page.x0 <= width ? view.x : page.x0 - 16
  if (match) {
    x = match.x0 >= view.x && match.x1 <= view.x + width ? view.x : match.x0 - Math.min(width / 3, 24 / view.zoom)
  }
  return { x, y: match ? match.y0 - height / 3 : page.y0 - 16 }
}
