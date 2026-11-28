import { useImperativeHandle, useLayoutEffect, useRef, useState, type Ref } from 'react'
import { TesseraIcons } from '@kition/sdk/editor'
import { IconButton, useKitEditor } from '@kition/sdk/editor'
import { SearchSession, type Match } from './searchSession'


const ChevronDown = TesseraIcons.ChevronDownIcon
const ChevronUp = TesseraIcons.ChevronUpIcon
const X = TesseraIcons.XIcon
const CaseSensitive = TesseraIcons.CaseSensitiveIcon
const WholeWord = TesseraIcons.WholeWordIcon

export type { Match } from './searchSession'
/** What the editor's keys do to an open find bar: Cmd+G and Shift+Cmd+G, Cmd+F with text selected. */
export type FindControl = { step: (by: number) => void; seed: (text: string) => void }
/** The find options (the session's: they last while the editor is open). */
export type FindOptions = { caseSensitive: boolean; wholeWords: boolean }

type Props = {
  document: object | null
  revision: string | number
  inputRef: Ref<HTMLInputElement>
  /** The text it opens with (the selection when Cmd+F opened it). */
  initial?: string
  control?: Ref<FindControl>
  /** The current match; `jump` is false when a document change only refreshed the matches. */
  onMatch: (match: Match | null, jump: boolean) => void
  /** Every match of the query (null: none yet, or the query cleared), for marking them all. */
  onMatches?: (matches: Match[] | null) => void
  options?: FindOptions
  onOptions?: (options: FindOptions) => void
  /** Earlier queries, newest last: Up in the field brings them back (Down goes forward again). */
  history?: readonly string[]
  /** A query to keep in the history (the bar closed, or Enter went to a match). */
  onRemember?: (text: string) => void
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
export function FindBar({ document, revision, inputRef, initial = '', control, onMatch, onMatches, options = NO_OPTIONS, onOptions, history = [], onRemember, onClose }: Props) {
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
  const all = useRef(onMatches)
  all.current = onMatches
  // Where Up and Down are in the history (its length: the query being written).
  const recall = useRef<number | null>(null)
  const searchedOptions = useRef(options)

  useLayoutEffect(() => {
    const previous = selection.current.matches?.[selection.current.index] ?? null
    // A document change keeps the current match; new options search afresh, from the first.
    const sameOptions = searchedOptions.current.caseSensitive === options.caseSensitive && searchedOptions.current.wholeWords === options.wholeWords
    refreshing.current = searchedDocument.current === document && document != null && sameOptions ? previous : null
    searchedDocument.current = document
    searchedOptions.current = options
    const asked = { ...(options.caseSensitive ? { caseSensitive: true } : {}), ...(options.wholeWords ? { wholeWords: true } : {}) }
    const search = new SearchSession(
      (value) => query<{ matches: Match[] }>('text.find', { query: value, ...asked }),
      (found) => {
        const kept = refreshing.current
        // While a refresh runs, the matches shown stay until its answer replaces them.
        if (kept && found === null) return
        refreshing.current = null
        const next = kept && found?.length ? keptMatch(found, kept) : 0
        selection.current = { matches: found, index: next }
        setMatches(found)
        setIndex(next)
        all.current?.(found)
        callback.current(found?.[next] ?? null, !kept)
      },
    )
    session.current = search
    search.search(currentText.current, composing.current || !document)
    return () => {
      search.dispose()
      session.current = null
    }
  }, [document, revision, query, options.caseSensitive, options.wholeWords])

  // Closing the bar takes its matches off the page.
  useLayoutEffect(() => () => {
    selection.current = { matches: null, index: 0 }
    all.current?.(null)
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
    if (currentText.current.trim()) onRemember?.(currentText.current)
    session.current?.clear()
    onClose()
  }
  /** Up (back) and Down (forward) through the earlier queries; past the newest, the field is empty. */
  const walk = (by: -1 | 1) => {
    if (!history.length) return false
    const at = recall.current ?? history.length
    const next = Math.max(0, Math.min(history.length, at + by))
    if (next === at) return true
    recall.current = next
    find(history[next] ?? '')
    return true
  }
  const toggle = (key: keyof FindOptions) => onOptions?.({ ...options, [key]: !options[key] })

  return (
    <div className="kui-bar" data-testid="pdf-find-bar">
      <input
        ref={inputRef}
        className="kui-field"
        style={{ width: 240 }}
        autoFocus
        value={text}
        aria-label={t('findPlaceholder')}
        placeholder={history.length && !text ? t('findRecall', { text: shorten(history[history.length - 1]) }) : t('findPlaceholder')}
        onChange={(event) => { recall.current = null; find(event.target.value) }}
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
          if (event.key === 'Enter') {
            event.preventDefault()
            if (currentText.current.trim()) onRemember?.(currentText.current)
            step(event.shiftKey ? -1 : 1)
          }
          if (event.key === 'Escape') { event.preventDefault(); close() }
          if ((event.key === 'ArrowUp' || event.key === 'ArrowDown') && !event.shiftKey && !event.altKey && !command) {
            if (walk(event.key === 'ArrowUp' ? -1 : 1)) event.preventDefault()
          }
        }}
        data-testid="pdf-find-text"
      />
      <span className="kui-muted" aria-live="polite" data-testid="pdf-find-count">
        {matches == null ? '' : matches.length ? `${index + 1}/${matches.length}` : t('noMatches')}
      </span>
      <IconButton icon={ChevronUp} label={t('findPrevious')} disabled={!matches?.length} onClick={() => step(-1)} data-testid="pdf-find-previous" />
      <IconButton icon={ChevronDown} label={t('findNext')} disabled={!matches?.length} onClick={() => step(1)} data-testid="pdf-find-next" />
      <IconButton icon={CaseSensitive} label={t('findCase')} pressed={options.caseSensitive} aria-pressed={options.caseSensitive} onClick={() => toggle('caseSensitive')} data-testid="pdf-find-case" />
      <IconButton icon={WholeWord} label={t('findWholeWords')} pressed={options.wholeWords} aria-pressed={options.wholeWords} onClick={() => toggle('wholeWords')} data-testid="pdf-find-whole" />
      <span style={{ flex: 1 }} />
      <IconButton icon={X} label={t('close')} onClick={close} data-testid="pdf-find-close" />
    </div>
  )
}

const NO_OPTIONS: FindOptions = { caseSensitive: false, wholeWords: false }

/** A remembered query as the empty field's hint shows it. */
const shorten = (text: string) => (text.length > 24 ? `${text.slice(0, 23)}…` : text)
