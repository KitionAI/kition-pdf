import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { toast, type KitTool } from '@kition/sdk/editor'
import type { PdfInfo } from './pdf'
import { clickUnit, edgeScroll, isTextControl, selectionPoint, SelectionSession, stepRequest, type SelectionPoint, type SelectionRequest, type SelectionResult } from './textSelection'

type Options = {
  document: object | null
  revision: number
  pages: PdfInfo['pages']
  execute: (request: SelectionRequest) => Promise<SelectionResult | null>
  /** A press on the pages: where (a page and a page-local point), or null off every page. */
  onStart: (at: SelectionPoint | null) => void
  t: (key: string) => string
}

export function useTextSelection({ document: doc, revision, pages, execute, onStart, t }: Options) {
  const session = useRef<SelectionSession | null>(null)
  const gestureEpoch = useRef(0)
  const callbacks = useRef({ onStart, t })
  callbacks.current = { onStart, t }
  const [stored, setStored] = useState<{ doc: typeof doc; revision: number; result: SelectionResult } | null>(null)
  const selected = stored?.doc === doc && stored.revision === revision ? stored.result : null
  // While the mouse is held over the pages (the selection's action bar waits for the release).
  const [dragging, setDragging] = useState(false)

  useLayoutEffect(() => {
    const current = new SelectionSession(
      request => doc ? execute(request) : Promise.resolve(null),
      result => setStored(result ? { doc, revision, result } : null),
      error => toast('error', error instanceof Error ? error.message : typeof error === 'string' ? error : callbacks.current.t('commandFailed')),
    )
    session.current = current
    setStored(null)
    setDragging(false)
    return () => { current.dispose(); session.current = null }
  }, [doc, revision, execute])

  const clear = useCallback(() => { gestureEpoch.current++; extending.current = null; session.current?.clear() }, [])
  // The selection now, for Shift+click (it extends from the anchor) and Shift with the arrows.
  const latest = useRef(selected)
  latest.current = selected
  // Shift+arrow steps asked for before the engine answered the first: they add up on its ends.
  const extending = useRef<{ from: SelectionResult; stepBy: 'glyph' | 'word' | 'line'; step: number } | null>(null)
  const tool = useMemo<KitTool>(() => ({
    id: 'pdf-text-selection',
    cursor: 'text',
    begin(start, context) {
      const current = session.current
      const epoch = ++gestureEpoch.current
      extending.current = null
      const clicks = Math.max(1, start.clicks ?? 1)
      const unit = start.alt ? undefined : clickUnit(clicks)
      // Shift+click: the range runs on from the anchor the selection has (on any page).
      const from = start.shift && !start.alt && clicks === 1 ? latest.current?.ends?.anchor ?? null : null
      if (!from) current?.clear()
      const anchor = selectionPoint(pages, start.x, start.y)
      callbacks.current.onStart(anchor)
      if (!doc || !anchor || !current) return
      setDragging(true)
      const mode = start.alt ? 'column' : 'range'
      const base: Omit<SelectionRequest, 'endPage'> = from
        ? { page: from.page, anchorGlyph: from.glyph, mode: 'range', ends: true }
        : { page: anchor.page, anchor: anchor.point, mode, ...(unit ? { unit } : {}), ...(mode === 'range' ? { ends: true } : {}) }
      // A plain press waits for the pointer to travel; a Shift+click or a double click selects at once.
      const atOnce = !!from || !!unit
      let last = ''
      let pointer = start
      const update = (at: typeof start) => {
        pointer = at
        if (session.current !== current || gestureEpoch.current !== epoch) return
        if (!atOnce && Math.hypot(at.clientX - start.clientX, at.clientY - start.clientY) < 3) { current.clear(); last = ''; return }
        const end = mode === 'column'
          ? selectionPoint([pages[anchor.page - 1]], at.x, at.y, true)
          : selectionPoint(pages, at.x, at.y, true)
        if (!end) return
        const request: SelectionRequest = { ...base, focus: end.point, endPage: mode === 'column' ? anchor.page : end.page }
        const key = JSON.stringify(request)
        if (key === last) return
        last = key
        current.request(request)
      }
      // Held near the view's top or bottom edge (or past it), the drag scrolls the pages and the
      // selection follows the point now under the pointer.
      const surface = typeof document !== 'undefined' ? document.querySelector('[data-testid="kit-canvas-surface"]') : null
      let frame = 0
      const scroll = () => {
        frame = 0
        if (session.current !== current || gestureEpoch.current !== epoch || !surface) return
        const box = surface.getBoundingClientRect()
        const by = edgeScroll(pointer.clientY, box.top, box.bottom)
        if (!by) return
        const viewport = context.viewport
        const before = viewport.view
        // panBy moves the document with a hand's drag (screen pixels): up, to scroll down.
        viewport.panBy(0, -by)
        const after = viewport.view
        if (after.y !== before.y || after.x !== before.x) {
          const at = viewport.toImage(pointer.canvasX, pointer.canvasY)
          update({ ...pointer, x: at.x, y: at.y })
        }
        frame = requestAnimationFrame(scroll)
      }
      const watch = () => { if (!frame && surface) frame = requestAnimationFrame(scroll) }
      const stop = () => { if (frame) cancelAnimationFrame(frame); frame = 0 }
      if (atOnce) update(start)
      return {
        move: (at) => { update(at); watch() },
        end: (at) => { stop(); setDragging(false); update(at) },
        cancel: () => { stop(); setDragging(false); if (session.current === current && gestureEpoch.current === epoch) { gestureEpoch.current++; current.clear() } },
      }
    },
  }), [doc, pages])

  /**
   * Shift with the arrow keys: the focus end moves `by` glyphs (`stepBy` word: words, line: lines)
   * on its page, the anchor staying; answers whether there was a range to move.
   */
  const extend = useCallback((by: number, stepBy: 'glyph' | 'word' | 'line') => {
    const live = session.current
    const now = extending.current
    const base = now && now.stepBy === stepBy ? now : latest.current ? { from: latest.current, stepBy, step: 0 } : null
    if (!live || !base) return false
    const request = stepRequest(base.from, base.step + by, stepBy)
    if (!request) return false
    gestureEpoch.current++
    extending.current = { ...base, step: base.step + by }
    live.request(request)
    return true
  }, [])
  // An answer starts the next steps from where it put the ends.
  useEffect(() => { if (selected) extending.current = null }, [selected])

  /**
   * Select All (Cmd+A): every page's text when the engine takes a span that long (it refuses more
   * than 64 pages, or too much text), else the current page's (`current`, 0-based).
   */
  const selectAll = useCallback((current: number) => {
    const live = session.current
    const last = pages.length
    const page = pages[Math.max(0, Math.min(last - 1, current))]
    if (!doc || !live || !page) return
    gestureEpoch.current++
    const end = pages[last - 1].area
    const whole: SelectionRequest = { page: 1, anchor: [0, 0], focus: [end.x1 - end.x0, end.y1 - end.y0], endPage: last, mode: 'range', ends: true }
    const index = Math.max(0, Math.min(last - 1, current)) + 1
    const one: SelectionRequest = { page: index, anchor: [0, 0], focus: [page.area.x1 - page.area.x0, page.area.y1 - page.area.y0], endPage: index, mode: 'range', ends: true }
    live.choose(last > 1 ? [whole, one] : [one])
  }, [doc, pages])

  // Copy takes the selection wherever the keyboard is in the editor (a ribbon button, a thumbnail),
  // but never from a field, or over text selected in the page's own controls (a panel's list).
  const copied = useRef(selected)
  copied.current = selected
  useEffect(() => {
    const copy = (event: ClipboardEvent) => {
      const text = copied.current?.text
      if (!text || !event.clipboardData || isTextControl(event.target)) return
      const own = window.getSelection()
      if (own && !own.isCollapsed && own.toString()) return
      event.clipboardData.setData('text/plain', text)
      event.preventDefault()
    }
    document.addEventListener('copy', copy)
    return () => document.removeEventListener('copy', copy)
  }, [])
  const copy = () => {
    if (!selected?.text) return
    // Invoke the clipboard API in the click's activation, without an intervening engine await.
    try {
      void navigator.clipboard.writeText(selected.text).then(
        () => toast('success', t('copied')),
        () => toast('error', t('commandFailed')),
      )
    } catch { toast('error', t('commandFailed')) }
  }
  return { tool, selected, dragging, clear, selectAll, copy, extend }
}
