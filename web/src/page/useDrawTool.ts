import { useMemo, useRef, useState } from 'react'
import type { KitPointer, KitTool } from '@kition/sdk/editor'
import { HANDLE_CURSOR, hitTest, onText, pageAt, type Area, type PageComment, type PageObjects, type Rect } from './geometry'
import { AREA_MIN_PX, PLACE_DRAG_PX, lineEnd, lineLongEnough, onPage, rectQuadOf, round2, shapeBigEnough, shapeRect, strokePoints, toolDef, type ToolId, type ToolStyle } from './tools'

type KitToolContext = Parameters<KitTool['begin']>[1]
type ToScreen = (x: number, y: number) => { x: number; y: number }

type Options = {
  tool: ToolId
  pages: readonly { area: Area }[]
  objects: PageObjects
  /** A page's text lines (null: not known yet), to tell a press on the text from one on the blank page. */
  lines: (page: number) => Rect[] | null
  chosen: PageComment | null
  zoom: () => number
  /** The tool's look now (the options bar's). */
  style: () => ToolStyle
  /** The page layer's own tool: a shape tool's press on the selected comment's handle resizes it. */
  pageTool: KitTool
  /** Text selection, which a markup tool's drag is. */
  text: KitTool
  /** A place tool's click (or a text box's drag): a card opens there. `rect`: a dragged text box. */
  place: (tool: ToolId, page: number, point: [number, number], rect: Rect | null) => void
  /** A markup tool's drag ended: mark the text between the two pointers (or, highlighting the blank page, the dragged area). */
  mark: (tool: ToolId, start: KitPointer, end: KitPointer) => void
  /** A shape or a stroke to make: `comment.add`'s geometry on `page` (the style is added by the caller). */
  add: (tool: ToolId, page: number, geometry: Record<string, unknown>) => void
}

const travelled = (a: KitPointer, b: KitPointer) => Math.hypot(b.clientX - a.clientX, b.clientY - a.clientY)

/**
 * The comment tools' pointer tool (stage 5): a sticky note, a text box or page text is placed with
 * a click (a text box also with a drag), which opens a card there; a highlight, underline or
 * strikethrough is a text selection that marks what it went over at the release; a rectangle or
 * an oval is dragged out (Shift: a square, a circle), a line or an arrow drawn from the press to
 * the release (Shift: level or plumb), each previewed in the canvas's overlay as it is dragged, and
 * made with one `comment.add` at the release (one undo step); a drawing is one stroke per drag.
 * A press on the selected comment's handle still resizes it while a shape tool is on. Escape during
 * a drag (the canvas cancels the gesture) makes nothing.
 */
export function useDrawTool(options: Options): KitTool {
  const live = useRef(options)
  live.current = options
  const kind = toolDef(options.tool).kind
  const resting = kind === 'markup' ? 'text' : 'crosshair'
  // A shape tool's cursor turns into a resize cursor over the selected comment's handle.
  const [cursor, setCursor] = useState('crosshair')
  const cursorRef = useRef(cursor)
  const showCursor = (next: string) => {
    if (cursorRef.current === next) return
    cursorRef.current = next
    setCursor(next)
  }
  const shown = kind === 'shape' ? cursor : resting

  const base = useMemo(() => {
    const local = (page: number, pointer: KitPointer): [number, number] => {
      const area = live.current.pages[page - 1]?.area
      if (!area) return [pointer.x, pointer.y]
      return onPage([pointer.x - area.x0, pointer.y - area.y0], area.x1 - area.x0, area.y1 - area.y0)
    }
    const toDocument = (page: number, point: readonly [number, number]): [number, number] => {
      const area = live.current.pages[page - 1]?.area
      return area ? [area.x0 + point[0], area.y0 + point[1]] : [point[0], point[1]]
    }
    const handleHit = (pointer: KitPointer) => {
      const { pages, objects, chosen, zoom } = live.current
      const at = pageAt(pages, pointer.x, pointer.y)
      if (!at) return null
      const hit = hitTest(objects, chosen, at.page, at.point, zoom())
      return hit.kind === 'handle' ? hit : null
    }
    const paintStyle = (context: CanvasRenderingContext2D, style: ToolStyle, zoom: number, minimum = 1) => {
      context.strokeStyle = style.color
      context.globalAlpha = Math.max(0.1, Math.min(1, style.opacity))
      context.lineWidth = Math.max(minimum, style.width * zoom)
      context.lineCap = 'round'
      context.lineJoin = 'round'
    }
    const tool: Omit<KitTool, 'cursor'> = {
      id: 'pdf-draw',
      begin(start, context) {
        const o = live.current
        const id = o.tool
        const def = toolDef(id)
        const at = pageAt(o.pages, start.x, start.y)
        // The highlighter on the blank page (lines known, none under the press) drags out an area.
        const area = def.kind === 'markup' && id === 'highlight' && at ? (() => {
          const known = o.lines(at.page)
          return known && !onText(known, at.point) ? at.page : null
        })() : null
        if (def.kind === 'markup' && area == null) {
          const gesture = o.text.begin(start, context)
          return {
            move: (pointer, ctx) => gesture?.move?.(pointer, ctx),
            end: (pointer, ctx) => {
              gesture?.end?.(pointer, ctx)
              live.current.mark(id, start, pointer)
            },
            cancel: (ctx) => gesture?.cancel?.(ctx),
          }
        }
        if (!at) return
        const page = at.page
        if (def.kind === 'shape' && handleHit(start)) return o.pageTool.begin(start, context)
        const from = local(page, start)
        const zoom = () => live.current.zoom()
        const clear = (ctx: KitToolContext) => ctx.overlay.clear()
        if (area != null) {
          const box = (pointer: KitPointer) => shapeRect(from, local(page, pointer))
          return {
            move(pointer, ctx) {
              const rect = box(pointer)
              ctx.overlay.draw((paint, toScreen) => {
                const a = toScreen(...toDocument(page, [rect[0], rect[1]]))
                const b = toScreen(...toDocument(page, [rect[2], rect[3]]))
                paint.fillStyle = live.current.style().color
                paint.globalAlpha = 0.4
                paint.fillRect(a.x, a.y, b.x - a.x, b.y - a.y)
              })
            },
            end(pointer, ctx) {
              const rect = box(pointer)
              const z = zoom()
              if ((rect[2] - rect[0]) * z < AREA_MIN_PX || (rect[3] - rect[1]) * z < AREA_MIN_PX) {
                clear(ctx)
                return
              }
              live.current.add(id, page, { quads: [rectQuadOf(rect)] })
            },
            cancel: clear,
          }
        }
        if (def.kind === 'place') {
          let far = false
          return {
            move(pointer, ctx) {
              if (id !== 'textbox') return
              if (travelled(start, pointer) >= PLACE_DRAG_PX) far = true
              if (!far) return
              const rect = shapeRect(from, local(page, pointer))
              ctx.overlay.draw((paint, toScreen) => {
                const a = toScreen(...toDocument(page, [rect[0], rect[1]]))
                const b = toScreen(...toDocument(page, [rect[2], rect[3]]))
                // The box's frame is the editor's (the theme's primary colour), not the comment's.
                paint.strokeStyle = getComputedStyle(document.documentElement).getPropertyValue('--kit-primary').trim() || 'currentColor'
                paint.setLineDash([4, 3])
                paint.lineWidth = 1
                paint.strokeRect(a.x + 0.5, a.y + 0.5, b.x - a.x, b.y - a.y)
              })
            },
            end(pointer, ctx) {
              clear(ctx)
              const rect = id === 'textbox' && far ? shapeRect(from, local(page, pointer)) : null
              const dragged = rect && Math.abs(pointer.clientX - start.clientX) >= PLACE_DRAG_PX && Math.abs(pointer.clientY - start.clientY) >= PLACE_DRAG_PX ? rect.map(round2) as Rect : null
              live.current.place(id, page, [round2(from[0]), round2(from[1])], dragged)
            },
            cancel: clear,
          }
        }
        if (def.kind === 'ink') {
          const points: [number, number][] = [from]
          const drawStroke = (ctx: KitToolContext) => ctx.overlay.draw((paint, toScreen) => {
            paintStyle(paint, live.current.style(), zoom())
            paint.beginPath()
            points.forEach((point, i) => {
              const p = toScreen(...toDocument(page, point))
              if (i === 0) paint.moveTo(p.x, p.y)
              else paint.lineTo(p.x, p.y)
            })
            if (points.length === 1) {
              const p = toScreen(...toDocument(page, points[0]))
              paint.lineTo(p.x + 0.1, p.y)
            }
            paint.stroke()
          })
          return {
            move(pointer, ctx) {
              points.push(local(page, pointer))
              drawStroke(ctx)
            },
            end(pointer, ctx) {
              points.push(local(page, pointer))
              drawStroke(ctx)
              live.current.add(id, page, { strokes: [strokePoints(points, 0.5 / Math.max(0.1, zoom()))] })
            },
            cancel: clear,
          }
        }
        // A rectangle, an oval, a line or an arrow.
        const geometry = (pointer: KitPointer) => {
          const to = local(page, pointer)
          if (id === 'line' || id === 'arrow') return { line: [from, lineEnd(from, to, pointer.shift)] as const, rect: null }
          return { line: null, rect: shapeRect(from, to, pointer.shift) }
        }
        const preview = (pointer: KitPointer, ctx: KitToolContext) => {
          const shape = geometry(pointer)
          ctx.overlay.draw((paint, toScreen: ToScreen) => {
            paintStyle(paint, live.current.style(), zoom())
            if (shape.line) {
              const a = toScreen(...toDocument(page, shape.line[0]))
              const b = toScreen(...toDocument(page, shape.line[1]))
              paint.beginPath()
              paint.moveTo(a.x, a.y)
              paint.lineTo(b.x, b.y)
              if (id === 'arrow') {
                const angle = Math.atan2(b.y - a.y, b.x - a.x)
                const size = Math.max(8, paint.lineWidth * 4)
                for (const turn of [Math.PI * 0.85, -Math.PI * 0.85]) {
                  paint.moveTo(b.x, b.y)
                  paint.lineTo(b.x + size * Math.cos(angle + turn), b.y + size * Math.sin(angle + turn))
                }
              }
              paint.stroke()
              return
            }
            const rect = shape.rect!
            const a = toScreen(...toDocument(page, [rect[0], rect[1]]))
            const b = toScreen(...toDocument(page, [rect[2], rect[3]]))
            paint.beginPath()
            if (id === 'oval') paint.ellipse((a.x + b.x) / 2, (a.y + b.y) / 2, Math.abs(b.x - a.x) / 2, Math.abs(b.y - a.y) / 2, 0, 0, Math.PI * 2)
            else paint.rect(a.x, a.y, b.x - a.x, b.y - a.y)
            paint.stroke()
          })
        }
        return {
          move: preview,
          end(pointer, ctx) {
            const shape = geometry(pointer)
            if (shape.line) {
              if (!lineLongEnough(shape.line[0], shape.line[1])) {
                clear(ctx)
                return
              }
              preview(pointer, ctx)
              live.current.add(id, page, { from: shape.line[0].map(round2), to: shape.line[1].map(round2) })
              return
            }
            if (!shape.rect || !shapeBigEnough(shape.rect)) {
              clear(ctx)
              return
            }
            preview(pointer, ctx)
            live.current.add(id, page, { rect: shape.rect.map(round2) })
          },
          cancel: clear,
        }
      },
      hover(pointer) {
        const o = live.current
        if (toolDef(o.tool).kind !== 'shape') return
        const hit = handleHit(pointer)
        showCursor(hit ? HANDLE_CURSOR[hit.handle] : 'crosshair')
      },
    }
    return tool
  }, [])
  return useMemo(() => ({ ...base, id: `pdf-draw-${options.tool}`, cursor: shown }) as KitTool, [base, shown, options.tool])
}
