import type { RasterRect, RasterView } from '@kition/sdk/editor'
import { searchScreenRects } from './searchGeometry'
import type { Match } from './searchSession'

/** Transient UI over the live canvas; never part of PDF pixels or saved content. */
export function SearchHighlights({ page, pageNumber, rects, view }: { page: RasterRect | undefined; pageNumber: number; rects: number[][]; view: RasterView | null }) {
  const boxes = searchScreenRects(page, rects, view)
  if (!boxes.length) return null
  return (
    <div className="pdf-search-highlights" data-testid="pdf-search-highlights" data-page={pageNumber} aria-hidden="true">
      {boxes.map((style, index) => <span key={index} className="pdf-search-highlight" data-testid="pdf-search-highlight" style={style} />)}
    </div>
  )
}

/** The most other matches marked at once (the engine answers at most 500). */
const MATCHES_SHOWN = 500

/**
 * Every match of the find bar's query on the pages in view, faint (the current one is drawn by
 * `SearchHighlights`, stronger); pages out of view mark none, so a long document marks only a few.
 */
export function FoundMatches({ matches, current, pages, view }: {
  matches: readonly Match[]
  current: Match | null
  pages: readonly { area: RasterRect }[]
  view: (RasterView & { width: number; height: number }) | null
}) {
  if (!view || !(view.zoom > 0) || !matches.length) return null
  const top = view.y, bottom = view.y + view.height / view.zoom
  const left = view.x, right = view.x + view.width / view.zoom
  const shown = (page: number) => {
    const area = pages[page - 1]?.area
    return !!area && area.y1 >= top && area.y0 <= bottom && area.x1 >= left && area.x0 <= right
  }
  const boxes: Array<{ key: string; page: number; style: ReturnType<typeof searchScreenRects>[number] }> = []
  for (const [index, match] of matches.entries()) {
    if (boxes.length >= MATCHES_SHOWN) break
    if (match === current || !shown(match.page)) continue
    for (const [part, style] of searchScreenRects(pages[match.page - 1]?.area, match.rects, view).entries()) boxes.push({ key: `${index}-${part}`, page: match.page, style })
  }
  if (!boxes.length) return null
  return (
    <div className="pdf-search-matches" data-testid="pdf-search-matches" aria-hidden="true">
      {boxes.map(({ key, page, style }) => <span key={key} className="pdf-search-match" data-testid="pdf-search-match" data-page={page} style={style} />)}
    </div>
  )
}
