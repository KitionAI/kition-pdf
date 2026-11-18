import type { RasterRect, RasterView } from '@kition/kit-editor'
import { searchScreenRects } from './searchGeometry'

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
