import { useCallback, useEffect, useRef, useState } from 'react'
import { NO_OBJECTS, asRect, objectsFrom, type PageObjects, type Rect } from './geometry'

type Engine = { execute: (command: string, params: Record<string, unknown>) => Promise<string> }

/**
 * What lies on the pages for the page layer to hit: every comment, form field and link with its
 * rectangle, read once per document change (`revision`) with the engine's own read commands. The
 * editor's reads change nothing (no revision, so the person's text selection and chosen comment
 * stay); until a change's lists arrive the previous ones answer. `lines(page)` answers a page's
 * text lines (`text.listLines`) for the pointer's cursor, read when first asked for in a revision,
 * or null while they are on the way (or when the engine cannot read them).
 */
export function usePageObjects(doc: Engine | null, revision: number) {
  const [state, setState] = useState<{ doc: Engine | null; revision: number; objects: PageObjects }>({ doc: null, revision: -1, objects: NO_OBJECTS })
  useEffect(() => {
    if (!doc) return undefined
    let live = true
    const read = (command: string) => doc.execute(command, {}).then((text) => JSON.parse(text) as unknown).catch(() => null)
    void Promise.all([read('comment.list'), read('form.listFields'), read('link.list')]).then(([comments, fields, links]) => {
      if (live) setState({ doc, revision, objects: objectsFrom(comments, fields, links) })
    })
    return () => { live = false }
  }, [doc, revision])
  const objects = state.doc === doc ? state.objects : NO_OBJECTS

  const lineCache = useRef<{ doc: Engine | null; revision: number; pages: Map<number, Rect[] | 'reading' | 'unknown'> }>({ doc: null, revision: -1, pages: new Map() })
  const lines = useCallback((page: number): Rect[] | null => {
    const cache = lineCache.current
    if (cache.doc !== doc || cache.revision !== revision) lineCache.current = { doc, revision, pages: new Map() }
    const pages = lineCache.current.pages
    const known = pages.get(page)
    if (known === 'reading' || known === 'unknown') return null
    if (known) return known
    if (!doc) return null
    pages.set(page, 'reading')
    doc.execute('text.listLines', { page }).then((text) => {
      const answer = JSON.parse(text) as { lines?: Array<{ rect?: number[] }> }
      pages.set(page, (answer.lines ?? []).map((line) => asRect(line.rect)).filter((rect): rect is Rect => rect != null))
    // Lines the engine cannot read: the whole page shows the text cursor, as before.
    }).catch(() => pages.set(page, 'unknown'))
    return null
  }, [doc, revision])
  return { objects, lines, fresh: state.doc === doc && state.revision === revision }
}
