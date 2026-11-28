import { useMemo, useRef } from 'react'
import type { KitTool } from '@kition/sdk/editor'
import { pageAt, type Area } from './geometry'
import { pagePoint } from './pick'
import { PLACE_DRAG_PX } from './tools'
import type { FillMark } from '../ribbon/FillSignGroup'

/**
 * The Fill & Sign tool (stage 10): a click on a page places what is chosen there (text opens a
 * card at the click, a check or a cross goes on centred on it); a drag places nothing, as in the
 * reference. The tool stays on for the next click.
 */
export function useFillTool(options: { mark: FillMark | null; pages: readonly { area: Area }[]; place: (mark: FillMark, page: number, point: [number, number]) => void }): KitTool {
  const live = useRef(options)
  live.current = options
  return useMemo<KitTool>(() => ({
    id: 'pdf-fill',
    cursor: 'crosshair',
    begin(start) {
      const { mark, pages } = live.current
      const at = pageAt(pages, start.x, start.y)
      if (!mark || !at) return
      const area = pages[at.page - 1].area
      const point = pagePoint(at.point, area.x1 - area.x0, area.y1 - area.y0)
      return {
        end(pointer) {
          if (Math.hypot(pointer.clientX - start.clientX, pointer.clientY - start.clientY) >= PLACE_DRAG_PX) return
          live.current.place(mark, at.page, point)
        },
      }
    },
  }), [])
}
