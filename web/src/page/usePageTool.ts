import { useMemo, useRef, useState } from 'react'
import type { KitPointer, KitTool } from '@kition/sdk/editor'
import {
  CLICK_PX,
  asRect,
  cursorFor,
  hitTest,
  keptOnPage,
  movable,
  moveSlipped,
  pageAt,
  resizeSlipped,
  resized,
  shifted,
  type Area,
  type Hit,
  type PageComment,
  type PageLink,
  type PageObjects,
  type Rect,
} from './geometry'

type KitToolContext = Parameters<KitTool['begin']>[1]
type KitGesture = Exclude<ReturnType<KitTool['begin']>, void>

/** A comment being dragged: where its frame is drawn until the release (and until the change shows). */
export type Dragging = { page: number; index: number; id: string | null; rect: Rect; kind: 'move' | 'resize' }
/** The comment under a resting pointer, for its tip. */
export type Hovering = { comment: PageComment; clientX: number; clientY: number }

type Options = {
  pages: readonly { area: Area }[]
  objects: PageObjects
  /** A page's text lines for the cursor (null: not known). */
  lines: (page: number) => Rect[] | null
  /** The canvas's zoom now. */
  zoom: () => number
  /** Text selection, which a press on the text or the blank page (or a drag over a link or a highlight) starts. */
  text: KitTool
  clearText: () => void
  chosen: PageComment | null
  choose: (item: PageComment | null) => void
  move: (item: PageComment, dx: number, dy: number) => void
  resize: (item: PageComment, rect: Rect) => void
  follow: (link: PageLink) => void
  /** A comment double-clicked: its text opens on the page. */
  open: (item: PageComment) => void
  onDrag: (drag: Dragging | null) => void
  onHover: (hover: Hovering | null) => void
}

const travelled = (a: KitPointer, b: KitPointer) => Math.hypot(b.clientX - a.clientX, b.clientY - a.clientY)

/**
 * The page's pointer tool (stage 4): a press goes, in order, to the selected comment's handles
 * (resize), a form field (taken, so it starts no text selection; filling on the page is stage 6),
 * a comment (chosen, and dragged it moves), a link (a click follows it), a highlight (a click
 * chooses it), and else to text selection, which a press on the blank page starts too (clearing
 * the chosen comment). A drag only moves the frame drawn over the page; the release sends one
 * `comment.edit` (`move`, or `rect` for a resize), so a gesture is one undo step, and Escape during
 * the drag (the canvas cancels the gesture) sends nothing. The cursor follows what the pointer is
 * over: the text cursor on text, a move cursor on a comment, a resize cursor on a handle, a hand
 * on a link, an arrow elsewhere.
 */
export function usePageTool(options: Options): KitTool {
  const live = useRef(options)
  live.current = options
  const [cursor, setCursor] = useState('text')
  const cursorRef = useRef(cursor)
  const showCursor = (next: string) => {
    if (cursorRef.current === next) return
    cursorRef.current = next
    setCursor(next)
  }

  const base = useMemo(() => {
    const where = (pointer: KitPointer) => {
      const { pages, objects, chosen, zoom } = live.current
      const at = pageAt(pages, pointer.x, pointer.y)
      const hit: Hit = at ? hitTest(objects, chosen, at.page, at.point, zoom()) : { kind: 'off' }
      return { at, hit }
    }
    /** Text selection for this press; `after` runs on a release that did not travel (a click). */
    const select = (start: KitPointer, context: KitToolContext, after?: () => void): KitGesture | void => {
      const gesture = live.current.text.begin(start, context)
      if (!after) return gesture
      let far = false
      return {
        move: (pointer, ctx) => {
          if (travelled(start, pointer) >= CLICK_PX) far = true
          gesture?.move?.(pointer, ctx)
        },
        end: (pointer, ctx) => {
          gesture?.end?.(pointer, ctx)
          if (!far && travelled(start, pointer) < CLICK_PX) after()
        },
        cancel: (ctx) => gesture?.cancel?.(ctx),
      }
    }
    const pageSize = (page: number) => {
      const area = live.current.pages[page - 1]?.area
      return area ? [area.x1 - area.x0, area.y1 - area.y0] : [Infinity, Infinity]
    }
    const tool: Omit<KitTool, 'cursor'> = {
      id: 'pdf-page',
      begin(start, context) {
        const { hit } = where(start)
        const o = live.current
        o.onHover(null)
        switch (hit.kind) {
          case 'handle': {
            const from = asRect(hit.comment.rect)
            if (!from) return
            const item = hit.comment
            let to = from
            return {
              move(pointer) {
                to = resized(from, hit.handle, pointer.x - start.x, pointer.y - start.y, pointer.shift)
                live.current.onDrag({ page: item.page, index: item.index, id: item.id, rect: to, kind: 'resize' })
              },
              end(pointer) {
                to = resized(from, hit.handle, pointer.x - start.x, pointer.y - start.y, pointer.shift)
                if (resizeSlipped(from, to)) live.current.onDrag(null)
                else live.current.resize(item, to)
              },
              cancel: () => live.current.onDrag(null),
            }
          }
          case 'field':
            o.clearText()
            o.choose(null)
            return {}
          case 'comment': {
            const item = hit.comment
            o.clearText()
            o.choose(item)
            const from = asRect(item.rect)
            if (!movable(item) || !from) return {}
            const [width, height] = pageSize(item.page)
            let shown = false
            const offset = (pointer: KitPointer) => keptOnPage(from, pointer.x - start.x, pointer.y - start.y, width, height)
            return {
              move(pointer) {
                if (!shown && travelled(start, pointer) < CLICK_PX) return
                shown = true
                const [dx, dy] = offset(pointer)
                live.current.onDrag({ page: item.page, index: item.index, id: item.id, rect: shifted(from, dx, dy), kind: 'move' })
              },
              end(pointer) {
                const [dx, dy] = offset(pointer)
                if (!shown || moveSlipped(dx, dy)) live.current.onDrag(null)
                else live.current.move(item, dx, dy)
              },
              cancel: () => live.current.onDrag(null),
            }
          }
          case 'link': {
            const link = hit.link
            return select(start, context, () => live.current.follow(link))
          }
          case 'markup': {
            const item = hit.comment
            return select(start, context, () => live.current.choose(item))
          }
          default:
            return select(start, context)
        }
      },
      hover(pointer) {
        const { at, hit } = where(pointer)
        const o = live.current
        showCursor(cursorFor(hit, at && hit.kind === 'page' ? o.lines(at.page) : null, at?.point ?? null))
        o.onHover(hit.kind === 'comment' || hit.kind === 'markup' ? { comment: hit.comment, clientX: pointer.clientX, clientY: pointer.clientY } : null)
      },
      leave() {
        live.current.onHover(null)
      },
      doubleClick(pointer) {
        const { hit } = where(pointer)
        if ((hit.kind === 'comment' || hit.kind === 'markup') && hit.comment.type !== 'FreeText') live.current.open(hit.comment)
      },
    }
    return tool
  }, [])
  return useMemo(() => ({ ...base, cursor }) as KitTool, [base, cursor])
}
