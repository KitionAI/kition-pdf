import { useMemo, useRef } from 'react'
import type { KitPointer, KitTool, KitToolContext } from '@kition/sdk/editor'
import { pageAt, type Area } from './geometry'
import { dragIsBox, pagePoint, pickedRect, type PickSpec } from './pick'

type Options = {
  spec: PickSpec | null
  pages: readonly { area: Area }[]
  /** The page the points picked so far are on (1-based), or null before the first. */
  page: number | null
  /** The points picked so far (page-local). */
  points: readonly (readonly [number, number])[]
  zoom: () => number
  /** A point or a box is picked: the dialog comes back with it. */
  done: (page: number, points: [number, number][]) => void
  /** One more point of several. */
  add: (page: number, point: [number, number]) => void
  /** A double click ends a list of points. */
  finish: () => void
}

/** Points closer than this many CSS pixels to the last one are the same click (a double click's second press). */
const SAME_POINT_PX = 4

/**
 * The pointer tool while a parameters dialog is put away to point at the page (stage 10): a
 * click picks a point; a drag picks a box (previewed dashed in the canvas's overlay; a drag too
 * short to be a box picks nothing and the tool stays); clicks pick several points on the page of
 * the first one (a click on another page counts where it falls on that page's edge), and a double
 * click ends them. Nothing here edits the document: the value goes back to the dialog.
 */
export function usePickTool(options: Options): KitTool {
  const live = useRef(options)
  live.current = options
  const tool = useMemo<KitTool>(() => {
    /** The pointer on `page`, page-local and kept on it. */
    const local = (page: number, pointer: KitPointer): [number, number] | null => {
      const area = live.current.pages[page - 1]?.area
      if (!area) return null
      return pagePoint([pointer.x - area.x0, pointer.y - area.y0], area.x1 - area.x0, area.y1 - area.y0)
    }
    const toDocument = (page: number, point: readonly [number, number]): [number, number] => {
      const area = live.current.pages[page - 1]?.area
      return area ? [area.x0 + point[0], area.y0 + point[1]] : [point[0], point[1]]
    }
    const primary = () => getComputedStyle(document.documentElement).getPropertyValue('--kit-primary').trim() || 'currentColor'
    /** The rubber band from the last point picked to the pointer. */
    const band = (pointer: KitPointer, context: KitToolContext) => {
      const { spec, page, points } = live.current
      const last = points.at(-1)
      if (spec?.kind !== 'points' || page == null || !last) return
      const to = local(page, pointer)
      if (!to) return
      context.overlay.draw((paint, toScreen) => {
        const a = toScreen(...toDocument(page, last))
        const b = toScreen(...toDocument(page, to))
        paint.strokeStyle = primary()
        paint.setLineDash([4, 3])
        paint.lineWidth = 1
        paint.beginPath()
        paint.moveTo(a.x, a.y)
        paint.lineTo(b.x, b.y)
        paint.stroke()
      })
    }
    return {
      id: 'pdf-pick',
      cursor: 'crosshair',
      begin(start, context) {
        const o = live.current
        const spec = o.spec
        if (!spec) return
        if (spec.kind === 'points') {
          const page = o.page ?? pageAt(o.pages, start.x, start.y)?.page
          if (page == null) return
          const point = local(page, start)
          if (!point) return
          const last = o.points.at(-1)
          const zoom = Math.max(0.01, o.zoom())
          if (last && o.page === page && Math.hypot(point[0] - last[0], point[1] - last[1]) * zoom < SAME_POINT_PX) return
          context.overlay.clear()
          o.add(page, point)
          return
        }
        const at = pageAt(o.pages, start.x, start.y)
        if (!at) return
        const page = at.page
        const from = local(page, start)
        if (!from) return
        if (spec.kind === 'point') {
          return { end: () => live.current.done(page, [from]) }
        }
        const clear = (ctx: KitToolContext) => ctx.overlay.clear()
        return {
          move(pointer, ctx) {
            const to = local(page, pointer)
            if (!to) return
            const rect = pickedRect(from, to)
            ctx.overlay.draw((paint, toScreen) => {
              const a = toScreen(...toDocument(page, [rect[0], rect[1]]))
              const b = toScreen(...toDocument(page, [rect[2], rect[3]]))
              paint.strokeStyle = primary()
              paint.setLineDash([4, 3])
              paint.lineWidth = 1
              paint.strokeRect(a.x + 0.5, a.y + 0.5, b.x - a.x, b.y - a.y)
            })
          },
          end(pointer, ctx) {
            clear(ctx)
            const to = local(page, pointer)
            if (!to || !dragIsBox(start, pointer)) return
            live.current.done(page, [from, to])
          },
          cancel: clear,
        }
      },
      hover(pointer, context) {
        band(pointer, context)
        return undefined
      },
      doubleClick(_pointer, context) {
        if (live.current.spec?.kind !== 'points') return
        context.overlay.clear()
        live.current.finish()
      },
    }
  }, [])
  return tool
}
