import { useImperativeHandle, useLayoutEffect, useRef, useState, type Ref } from 'react'
import { TesseraIcons } from '@kition/sdk/editor'
import { IconButton, useKitEditor } from '@kition/sdk/editor'
import { SearchSession, type Match } from './searchSession'


const ChevronDown = TesseraIcons.ChevronDownIcon
const ChevronUp = TesseraIcons.ChevronUpIcon
const X = TesseraIcons.XIcon

export type { Match } from './searchSession'
/** What the editor's keys do to an open find bar: Cmd+G and Shift+Cmd+G, Cmd+F with text selected. */
export type FindControl = { step: (by: number) => void; seed: (text: string) => void }

type Props = {
  document: object | null
  revision: string | number
  inputRef: Ref<HTMLInputElement>
  /** The text it opens with (the selection when Cmd+F opened it). */
  initial?: string
  control?: Ref<FindControl>
  /** The current match; `jump` is false when a document change only refreshed the matches. */
  onMatch: (match: Match | null, jump: boolean) => void
  onClose: () => void
}

/**
 * The match a refresh keeps current: the same one if it is still there, else the first one at or
 * after it in reading order (page, then top), else the last one.
 */
export function keptMatch(found: readonly Match[], previous: Match): number {
  const top = (match: Match) => match.rects[0]?.[1] ?? 0
  const left = (match: Match) => match.rects[0]?.[0] ?? 0
  const same = found.findIndex((match) => match.page === previous.page && Math.abs(top(match) - top(previous)) < 2 && Math.abs(left(match) - left(previous)) < 2)
  if (same >= 0) return same
  const after = found.findIndex((match) => match.page > previous.page || (match.page === previous.page && top(match) >= top(previous) - 2))
  return after >= 0 ? after : Math.max(0, found.length - 1)
}

/**
 * Finds text with all page-local rectangles; engine page numbers remain one-based. A change of the
 * query searches again and goes to the first match; a change of the document (an edit, an undo)
 * only refreshes the matches: the current one and the view stay where they are.
 */
export function FindBar({ document, revision, inputRef, initial = '', control, onMatch, onClose }: Props) {
  const { query, t } = useKitEditor()
  const [text, setText] = useState(initial)
  const [matches, setMatches] = useState<Match[] | null>(null)
  const [index, setIndex] = useState(0)
  const session = useRef<SearchSession | null>(null)
  const currentText = useRef(initial)
  const composing = useRef(false)
  const selection = useRef({ matches: null as Match[] | null, index: 0 })
  // The current match a refresh after a document change keeps (null: the next answer jumps).
  const refreshing = useRef<Match | null>(null)
  const searchedDocument = useRef<object | null>(null)
  const callback = useRef(onMatch)
  callback.current = onMatch

  useLayoutEffect(() => {
    const previous = selection.current.matches?.[selection.current.index] ?? null
    refreshing.current = searchedDocument.current === document && document != null ? previous : null
    searchedDocument.current = document
    const search = new SearchSession(
      (value) => query<{ matches: Match[] }>('text.find', { query: value }),
      (found) => {
        const kept = refreshing.current
        // While a refresh runs, the matches shown stay until its answer replaces them.
        if (kept && found === null) return
        refreshing.current = null
        const next = kept && found?.length ? keptMatch(found, kept) : 0
        selection.current = { matches: found, index: next }
        setMatches(found)
        setIndex(next)
        callback.current(found?.[next] ?? null, !kept)
      },
    )
    session.current = search
    search.search(currentText.current, composing.current || !document)
    return () => {
      search.dispose()
      session.current = null
    }
  }, [document, revision, query])

  // Closing the bar takes its match off the page.
  useLayoutEffect(() => () => {
    selection.current = { matches: null, index: 0 }
    callback.current(null, false)
  }, [])

  const find = (value: string) => {
    refreshing.current = null
    currentText.current = value
    setText(value)
    session.current?.search(value, composing.current || !document)
  }
  const step = (by: number) => {
    if (composing.current) return
    const { matches: found, index: previous } = selection.current
    if (!found?.length) return
    refreshing.current = null
    const next = (previous + by + found.length) % found.length
    selection.current.index = next
    setIndex(next)
    callback.current(found[next], true)
  }
  useImperativeHandle(control, () => ({ step, seed: (value: string) => { if (value && value !== currentText.current) find(value) } }))
  const close = () => {
    session.current?.clear()
    onClose()
  }

  return (
    <div className="kui-bar" data-testid="pdf-find-bar">
      <input
        ref={inputRef}
        className="kui-field"
        style={{ width: 240 }}
        autoFocus
        value={text}
        aria-label={t('findPlaceholder')}
        placeholder={t('findPlaceholder')}
        onChange={(event) => find(event.target.value)}
        onCompositionStart={() => {
          composing.current = true
          session.current?.search(currentText.current, true)
        }}
        onCompositionEnd={(event) => {
          composing.current = false
          find(event.currentTarget.value)
        }}
        onKeyDown={(event) => {
          const command = event.ctrlKey || event.metaKey
          if (command && event.key.toLowerCase() === 'f') { event.preventDefault(); event.stopPropagation(); event.currentTarget.select(); return }
          if (composing.current || event.nativeEvent.isComposing || event.keyCode === 229) return
          if ((command && event.key.toLowerCase() === 'g') || event.key === 'F3') { event.preventDefault(); event.stopPropagation(); step(event.shiftKey ? -1 : 1); return }
          if (event.key === 'Enter') { event.preventDefault(); step(event.shiftKey ? -1 : 1) }
          if (event.key === 'Escape') { event.preventDefault(); close() }
        }}
        data-testid="pdf-find-text"
      />
      <span className="kui-muted" aria-live="polite" data-testid="pdf-find-count">
        {matches == null ? '' : matches.length ? `${index + 1}/${matches.length}` : t('noMatches')}
      </span>
      <IconButton icon={ChevronUp} label={t('findPrevious')} disabled={!matches?.length} onClick={() => step(-1)} data-testid="pdf-find-previous" />
      <IconButton icon={ChevronDown} label={t('findNext')} disabled={!matches?.length} onClick={() => step(1)} data-testid="pdf-find-next" />
      <span style={{ flex: 1 }} />
      <IconButton icon={X} label={t('close')} onClick={close} data-testid="pdf-find-close" />
    </div>
  )
}
