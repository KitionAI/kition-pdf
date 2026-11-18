import type { RasterView } from '@kition/kit-editor'
import type { PdfInfo } from './pdf'
import type { SelectionResult } from './textSelection'
import { searchScreenRects } from './searchGeometry'

export function SelectionHighlights({ selected, pages, view }: { selected: SelectionResult | null; pages: PdfInfo['pages']; view: RasterView | null }) {
  if (!selected?.text) return null
  return <div className="pdf-selection-highlights" data-testid="pdf-selection-highlights" aria-hidden="true">
    {selected.pages.flatMap(item => searchScreenRects(pages[item.page - 1]?.area, item.rects, view).map((style, index) => (
      <span key={`${item.page}-${index}`} className="pdf-selection-highlight" data-testid="pdf-selection-highlight" data-page={item.page} style={style} />
    )))}
  </div>
}
