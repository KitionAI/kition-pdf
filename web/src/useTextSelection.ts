import { useCallback, useLayoutEffect, useMemo, useRef, useState, type ClipboardEvent } from 'react'
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

  const onCopy = (event: ClipboardEvent) => {
    if (!selected?.text || isTextControl(event.target)) return
    event.clipboardData.setData('text/plain', selected.text)
    event.preventDefault()
  }
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
  return { tool, selected, clear, onCopy, copy }
}
