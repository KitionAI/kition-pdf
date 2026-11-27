/**
 * What the page layer lays over the pages (HTML over the canvas, never in the rendered pixels):
 * the selected comment's frame and handles, the tip of the comment under the pointer, and the card
 * a comment's text is written in.
 */
import { useEffect, useRef, useState } from 'react'
import { Button, useKitEditor, type RasterView } from '@kition/sdk/editor'
import { HANDLES, asRect, handlePoint, resizable, screenBox, type Area, type PageComment, type Rect } from './geometry'
import type { Dragging, Hovering } from './usePageTool'

/** The frame round the selected comment: 3 px outside its rectangle, with handles when it resizes; during a drag, where the drag puts it, and a faint frame where it was. */
export function SelectionFrame({ comment, area, view, drag }: { comment: PageComment; area: Area | undefined; view: RasterView | null; drag: Dragging | null }) {
  const rect = asRect(comment.rect)
  if (!rect) return null
  const moving = drag && drag.page === comment.page && drag.index === comment.index && drag.id === comment.id ? drag.rect : null
  const shown: Rect = moving ?? rect
  const box = screenBox(area, shown, view)
  if (!box) return null
  const before = moving ? screenBox(area, rect, view) : null
  return (
    <>
      {before ? <div className="pdf-comment-ghost" style={{ left: before.left - 3, top: before.top - 3, width: before.width + 6, height: before.height + 6 }} aria-hidden="true" /> : null}
      <div className="pdf-comment-outline" style={{ left: box.left - 3, top: box.top - 3, width: box.width + 6, height: box.height + 6 }} data-testid="pdf-comment-outline" data-dragging={moving ? drag?.kind : undefined} aria-hidden="true" />
      {resizable(comment)
        ? HANDLES.map((handle) => {
            const [x, y] = handlePoint(shown, handle)
            const point = screenBox(area, [x, y, x, y], view)
            return point ? <span key={handle} className="pdf-comment-handle" style={{ left: point.left, top: point.top }} data-testid="pdf-comment-handle" data-handle={handle} aria-hidden="true" /> : null
          })
        : null}
    </>
  )
}

/** The tip of the comment under a resting pointer: its author, then its text (its type when it has no author). */
export function CommentTip({ hover, box }: { hover: Hovering; box: DOMRect | null }) {
  const { phrase: p } = useKitEditor()
  const { comment } = hover
  if (!box || (!comment.author && !comment.contents)) return null
  return (
    <div className="pdf-comment-tip" role="tooltip" style={{ left: hover.clientX - box.left + 12, top: hover.clientY - box.top + 18 }} data-testid="pdf-comment-tip">
      <strong>{comment.author || p(comment.type)}</strong>
      {comment.contents ? <span>{comment.contents}</span> : null}
    </div>
  )
}

/**
 * The card a comment's text is written in, beside it on the page: a comment's own text (a
 * double-click, Edit text in its menu) or a new sticky note's (Add a sticky note here). Post (or
 * Cmd+Enter) keeps it, Cancel (or Escape) changes nothing.
 */
export function NoteCard({ area, rect, view, title, initial, onPost, onCancel }: {
  area: Area | undefined
  rect: Rect
  view: RasterView | null
  title: string
  initial: string
  onPost: (text: string) => void
  onCancel: () => void
}) {
  const { t } = useKitEditor()
  const [text, setText] = useState(initial)
  // The caret starts after the text, to go on writing.
  const field = useRef<HTMLTextAreaElement>(null)
  useEffect(() => {
    const element = field.current
    element?.setSelectionRange(element.value.length, element.value.length)
  }, [])
  const box = screenBox(area, rect, view)
  if (!box) return null
  return (
    <div className="pdf-note-card" style={{ left: box.left + box.width + 8, top: box.top }} data-testid="pdf-note-card" role="dialog" aria-label={title}
      onPointerDown={(event) => event.stopPropagation()}>
      <div className="pdf-note-card-title">{title}</div>
      <textarea
        ref={field}
        className="kui-field"
        value={text}
        autoFocus
        rows={4}
        placeholder={t('commentPlaceholder')}
        aria-label={title}
        onChange={(event) => setText(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            event.preventDefault()
            event.stopPropagation()
            onCancel()
          } else if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
            event.preventDefault()
            event.stopPropagation()
            onPost(text)
          }
        }}
        data-testid="pdf-note-text"
      />
      <div className="pdf-selection-compose-actions">
        <Button onClick={onCancel} data-testid="pdf-note-cancel">{t('cancel')}</Button>
        <Button variant="primary" onClick={() => onPost(text)} data-testid="pdf-note-post">{t('post')}</Button>
      </div>
    </div>
  )
}
