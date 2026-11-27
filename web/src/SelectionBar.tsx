/**
 * The bar beside a text selection once the drag is let go: Add comment (a highlight with a note),
 * Highlight, Underline, Strikethrough and Copy. A markup marks the selection at once, each page of
 * it, and clears it; Copy copies and puts the bar away, keeping the selection. Hidden while the
 * mouse is held; Add comment is off for a selection across pages.
 */
import { useState } from 'react'
import { Button, IconButton, TesseraIcons, useKitEditor, type KitCanvasView, type RasterView } from '@kition/sdk/editor'
import type { PdfInfo } from './pdf'
import type { SelectionResult } from './textSelection'
import { selectionEnd, type MarkupType } from './markup'
import { searchScreenRects } from './searchGeometry'

const Comment = TesseraIcons.MessageSquarePlusIcon
const Highlighter = TesseraIcons.HighlighterIcon
const Underline = TesseraIcons.UnderlineIcon
const Strikethrough = TesseraIcons.StrikethroughIcon
const Copy = TesseraIcons.CopyIcon

/** The bar's gap below the selection's last line, and its size, in CSS pixels. */
const GAP = 6
const BAR_WIDTH = 190
const BAR_HEIGHT = 44

export function SelectionBar({ selected, pages, view, onMark, onCopy }: {
  selected: SelectionResult
  pages: PdfInfo['pages']
  view: KitCanvasView | null
  onMark: (type: MarkupType, contents?: string) => void
  onCopy: () => void
}) {
  const { t, enabled } = useKitEditor()
  const [composing, setComposing] = useState(false)
  const [note, setNote] = useState('')
  const end = selectionEnd(selected)
  const box = end ? searchScreenRects(pages[end.page - 1]?.area, [end.rect], view)[0] : undefined
  if (!box) return null
  const canMark = enabled('comment.add')
  const onePage = selected.pages.filter((item) => item.rects.length).length === 1
  const post = () => {
    onMark('highlight', note.trim() || undefined)
    setComposing(false)
    setNote('')
  }
  // Below the selection's end, its right edge on the line's (above it at the view's foot).
  const below = box.top + box.height + GAP
  const top = view && below + BAR_HEIGHT > view.height ? box.top - GAP - BAR_HEIGHT : below
  const style = { left: Math.max(BAR_WIDTH, box.left + box.width), top: Math.max(4, top) }
  return (
    <div className="pdf-selection-bar" style={style} data-testid="pdf-selection-bar" role="toolbar" aria-label={t('selectionActions')}
      onPointerDown={(event) => event.stopPropagation()}>
      {composing ? (
        <div className="pdf-selection-compose" data-testid="pdf-selection-compose">
          <textarea
            className="kui-field"
            value={note}
            autoFocus
            rows={3}
            placeholder={t('commentPlaceholder')}
            aria-label={t('addComment')}
            onChange={(event) => setNote(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Escape') {
                event.preventDefault()
                event.stopPropagation()
                setComposing(false)
              } else if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
                event.preventDefault()
                post()
              }
            }}
            data-testid="pdf-selection-note"
          />
          <div className="pdf-selection-compose-actions">
            <Button onClick={() => setComposing(false)}>{t('cancel')}</Button>
            <Button variant="primary" onClick={post} data-testid="pdf-selection-post">{t('post')}</Button>
          </div>
        </div>
      ) : (
        <>
          <IconButton icon={Comment} label={t('addComment')} title={t('addComment')} disabled={!canMark || !onePage} onClick={() => setComposing(true)} data-testid="pdf-bar-comment" />
          <IconButton icon={Highlighter} label={t('highlight')} title={t('highlight')} disabled={!canMark} onClick={() => onMark('highlight')} data-testid="pdf-bar-highlight" />
          <IconButton icon={Underline} label={t('underline')} title={t('underline')} disabled={!canMark} onClick={() => onMark('underline')} data-testid="pdf-bar-underline" />
          <IconButton icon={Strikethrough} label={t('strikethrough')} title={t('strikethrough')} disabled={!canMark} onClick={() => onMark('strikeout')} data-testid="pdf-bar-strikeout" />
          <IconButton icon={Copy} label={t('copy')} title={t('copy')} onClick={onCopy} data-testid="pdf-bar-copy" />
        </>
      )}
    </div>
  )
}

/** A thin frame round the comment chosen in the Comments panel, where it is on the page. */
export function CommentOutline({ page, rect, view }: { page: PdfInfo['pages'][number] | undefined; rect: number[]; view: RasterView | null }) {
  const box = searchScreenRects(page?.area, [rect], view)[0]
  if (!box) return null
  return <div className="pdf-comment-outline" style={{ left: box.left - 3, top: box.top - 3, width: box.width + 6, height: box.height + 6 }} data-testid="pdf-comment-outline" aria-hidden="true" />
}
