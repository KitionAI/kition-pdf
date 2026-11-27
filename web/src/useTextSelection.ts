import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { toast, type KitTool } from '@kition/sdk/editor'
import type { PdfInfo } from './pdf'
import { isTextControl, selectionPoint, SelectionSession, type SelectionRequest, type SelectionResult } from './textSelection'

type Options = {
  document: object | null
  revision: number
  pages: PdfInfo['pages']
  execute: (request: SelectionRequest) => Promise<SelectionResult | null>
  onStart: () => void
  t: (key: string) => string
}

export function useTextSelection({ document: doc, revision, pages, execute, onStart, t }: Options) {
  const session = useRef<SelectionSession | null>(null)
  const gestureEpoch = useRef(0)
  const callbacks = useRef({ onStart, t })
  callbacks.current = { onStart, t }
  const [stored, setStored] = useState<{ doc: typeof doc; revision: number; result: SelectionResult } | null>(null)
  const selected = stored?.doc === doc && stored.revision === revision ? stored.result : null

  useLayoutEffect(() => {
    const current = new SelectionSession(
      request => doc ? execute(request) : Promise.resolve(null),
      result => setStored(result ? { doc, revision, result } : null),
      error => toast('error', error instanceof Error ? error.message : typeof error === 'string' ? error : callbacks.current.t('commandFailed')),
    )
    session.current = current
    setStored(null)
    return () => { current.dispose(); session.current = null }
  }, [doc, revision, execute])

  const clear = useCallback(() => { gestureEpoch.current++; session.current?.clear() }, [])
  const tool = useMemo<KitTool>(() => ({
    id: 'pdf-text-selection',
    cursor: 'text',
    begin(start) {
      const current = session.current
      const epoch = ++gestureEpoch.current
      current?.clear()
      callbacks.current.onStart()
      const anchor = selectionPoint(pages, start.x, start.y)
      if (!doc || !anchor || !current) return
      const mode = start.alt ? 'column' : 'range'
      let last = ''
      const update = (pointer: typeof start) => {
        if (session.current !== current || gestureEpoch.current !== epoch) return
        if (Math.hypot(pointer.clientX - start.clientX, pointer.clientY - start.clientY) < 3) { current.clear(); last = ''; return }
        const end = mode === 'column'
          ? selectionPoint([pages[anchor.page - 1]], pointer.x, pointer.y, true)
          : selectionPoint(pages, pointer.x, pointer.y, true)
        if (!end) return
        const request: SelectionRequest = { page: anchor.page, anchor: anchor.point, focus: end.point, endPage: mode === 'column' ? anchor.page : end.page, mode }
        const key = JSON.stringify(request)
        if (key === last) return
        last = key
        current.request(request)
      }
      return { move: update, end: update, cancel: () => { if (session.current === current && gestureEpoch.current === epoch) { gestureEpoch.current++; current.clear() } } }
    },
  }), [doc, pages])

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
    const whole: SelectionRequest = { page: 1, anchor: [0, 0], focus: [end.x1 - end.x0, end.y1 - end.y0], endPage: last, mode: 'range' }
    const index = Math.max(0, Math.min(last - 1, current)) + 1
    const one: SelectionRequest = { page: index, anchor: [0, 0], focus: [page.area.x1 - page.area.x0, page.area.y1 - page.area.y0], endPage: index, mode: 'range' }
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
  return { tool, selected, clear, selectAll, copy }
}
