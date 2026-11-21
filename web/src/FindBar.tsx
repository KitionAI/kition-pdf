import { useLayoutEffect, useRef, useState, type Ref } from 'react'
import { TesseraIcons } from '@kition/sdk/editor'
import { IconButton, useKitEditor } from '@kition/sdk/editor'
import { SearchSession, type Match } from './searchSession'


const ChevronDown = TesseraIcons.ChevronDownIcon
const ChevronUp = TesseraIcons.ChevronUpIcon
const X = TesseraIcons.XIcon

export type { Match } from './searchSession'
type Props = {
  document: object | null
  revision: string | number
  inputRef: Ref<HTMLInputElement>
  onMatch: (match: Match | null) => void
  onClose: () => void
}

/** Finds text with all page-local rectangles; engine page numbers remain one-based. */
export function FindBar({ document, revision, inputRef, onMatch, onClose }: Props) {
  const { query, t } = useKitEditor()
  const [text, setText] = useState('')
  const [matches, setMatches] = useState<Match[] | null>(null)
  const [index, setIndex] = useState(0)
  const session = useRef<SearchSession | null>(null)
  const currentText = useRef('')
  const composing = useRef(false)
  const selection = useRef({ matches: null as Match[] | null, index: 0 })
  const callback = useRef(onMatch)
  callback.current = onMatch

  useLayoutEffect(() => {
    const search = new SearchSession(
      (value) => query<{ matches: Match[] }>('text_find', { query: value }),
      (found) => {
        selection.current = { matches: found, index: 0 }
        setMatches(found)
        setIndex(0)
        callback.current(found?.[0] ?? null)
      },
    )
    session.current = search
    search.search(currentText.current, composing.current || !document)
    return () => {
      search.dispose()
      session.current = null
      selection.current = { matches: null, index: 0 }
      callback.current(null)
    }
  }, [document, revision, query])

  const find = (value: string) => {
    currentText.current = value
    setText(value)
    session.current?.search(value, composing.current || !document)
  }
  const step = (by: number) => {
    if (composing.current) return
    const { matches: found, index: previous } = selection.current
    if (!found?.length) return
    const next = (previous + by + found.length) % found.length
    selection.current.index = next
    setIndex(next)
    callback.current(found[next])
  }
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
          if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'f') { event.preventDefault(); event.stopPropagation(); return }
          if (composing.current || event.nativeEvent.isComposing || event.keyCode === 229) return
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
