import { TesseraIcons } from '@kition/sdk/editor'
import { RibbonButton, RibbonGroup, RibbonItem, RibbonRow, RibbonStack, toast, useKitEditor } from '@kition/sdk/editor'
import { pageFromField, type PdfInfo } from '../pdf'
import type { MarkupType } from '../markup'
import { focusPages, selectOnFocus } from '../focus'


const ChevronFirst = TesseraIcons.ChevronFirstIcon
const ChevronLast = TesseraIcons.ChevronLastIcon
const ChevronLeft = TesseraIcons.ChevronLeftIcon
const ChevronRight = TesseraIcons.ChevronRightIcon
const Copy = TesseraIcons.CopyIcon
const CopyPlus = TesseraIcons.CopyPlusIcon
const FilePlus2 = TesseraIcons.FilePlusCornerIcon
const Highlighter = TesseraIcons.HighlighterIcon
const MessageSquarePlus = TesseraIcons.MessageSquarePlusIcon
const RotateCcw = TesseraIcons.RotateCcwIcon
const RotateCw = TesseraIcons.RotateCwIcon
const Scan = TesseraIcons.ScanIcon
const ScanSearch = TesseraIcons.ScanSearchIcon
const StretchHorizontal = TesseraIcons.StretchHorizontalIcon
const Trash2 = TesseraIcons.TrashIcon
const Type = TesseraIcons.TypeIcon
const Underline = TesseraIcons.UnderlineIcon
const Strikethrough = TesseraIcons.StrikethroughIcon
const BookOpen = TesseraIcons.BookOpenIcon
/** The text selection tool's icon, here and in the Focus bar. */
export const SelectTool = TesseraIcons.TextCursorIcon
const FileIcon = TesseraIcons.FileIcon

export function HomeTab({ info, current, onPage, onZoom, selectionAvailable, onCopySelection, onMark }: {
  info: PdfInfo
  current: number
  onPage: (index: number) => void
  onZoom: (to: 'width' | 'page' | 'actual') => void
  selectionAvailable: boolean
  onCopySelection: () => void
  /** Marks the selected text (or the find bar's match) at once; false when there is none. */
  onMark: (type: MarkupType) => boolean
}) {
  const { run, ask, enabled, phrase: p, t } = useKitEditor()
  const count = info.pages.length
  const page = [current + 1]
  const editable = info.editable
  const label = info.pages[current]?.label ?? String(current + 1)

  // With text selected (or a match found) the markup buttons mark it; else they ask what to mark.
  const markup = (type: MarkupType) => {
    if (!onMark(type)) ask('comment.add', { type, find: '' })
  }

  const copyText = async () => {
    const answer = await run('text.extract', { pages: page })
    if (!answer) return
    const parsed = JSON.parse(answer) as { text?: string; pages?: Array<{ text?: string }> }
    const text = parsed.text ?? (parsed.pages ?? []).map((item) => item.text ?? '').join('\n')
    await navigator.clipboard.writeText(text).then(
      () => toast('success', t('copied')),
      () => toast('error', t('commandFailed')),
    )
  }

  return (
    <>
      <RibbonGroup label={p('Navigate')} icon={BookOpen}>
        <RibbonStack>
          <RibbonRow>
            <RibbonButton icon={ChevronFirst} size="icon" label={p('First page')} title={p('First page')} disabled={current === 0} onClick={() => onPage(0)} />
            <RibbonButton icon={ChevronLeft} size="icon" label={p('Previous page')} title={p('Previous page')} disabled={current === 0} onClick={() => onPage(current - 1)} data-testid="pdf-previous" primary />
            <RibbonItem primary>
              <input
                className="kui-ribbon-field pdf-page-field"
                key={current}
                defaultValue={label}
                aria-label={t('page')}
                {...selectOnFocus}
                onKeyDown={(event) => {
                  const field = event.currentTarget
                  if (event.key === 'Escape') {
                    event.preventDefault()
                    field.value = label
                    focusPages()
                    return
                  }
                  if (event.key !== 'Enter') return
                  event.preventDefault()
                  const index = pageFromField(field.value, info.pages, count)
                  // Something that names no page: the field shows the current page again.
                  if (index === null) {
                    field.value = label
                    field.select()
                    return
                  }
                  // The keyboard goes back to the pages before the field is made again for the new page.
                  focusPages()
                  onPage(index)
                }}
                onBlur={(event) => {
                  event.currentTarget.value = label
                  delete event.currentTarget.dataset.selecting
                }}
                data-testid="pdf-page-field"
              />
              <span className="kui-muted pdf-page-count">/ {count}</span>
            </RibbonItem>
            <RibbonButton icon={ChevronRight} size="icon" label={p('Next page')} title={p('Next page')} disabled={current >= count - 1} onClick={() => onPage(current + 1)} data-testid="pdf-next" primary />
            <RibbonButton icon={ChevronLast} size="icon" label={p('Last page')} title={p('Last page')} disabled={current >= count - 1} onClick={() => onPage(count - 1)} />
          </RibbonRow>
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={p('Zoom')} icon={ScanSearch}>
        <RibbonButton icon={StretchHorizontal} size="large" label={t('fitWidth')} onClick={() => onZoom('width')} data-testid="pdf-fit-width" primary />
        <RibbonStack>
          <RibbonButton icon={Scan} label={t('fitPage')} onClick={() => onZoom('page')} data-testid="pdf-fit-page" primary />
          <RibbonButton icon={ScanSearch} label={t('actualSize')} onClick={() => onZoom('actual')} data-testid="pdf-actual-size" primary />
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={p('Page')} icon={FileIcon}>
        <RibbonStack>
          <RibbonButton icon={RotateCcw} label={t('rotateLeft')} disabled={!editable || !enabled('page.rotate')} onClick={() => void run('page.rotate', { degrees: -90, pages: page })} data-testid="pdf-rotate-left" primary />
          <RibbonButton icon={RotateCw} label={t('rotateRight')} disabled={!editable || !enabled('page.rotate')} onClick={() => void run('page.rotate', { degrees: 90, pages: page })} data-testid="pdf-rotate-right" primary />
        </RibbonStack>
        <RibbonStack>
          <RibbonButton icon={FilePlus2} label={t('insertPage')} disabled={!editable || !enabled('page.insertBlank')} onClick={() => void run('page.insertBlank', { at: current + 2 })} data-testid="pdf-insert-page" primary />
          <RibbonButton icon={CopyPlus} label={t('duplicatePage')} disabled={!editable || !enabled('page.duplicate')} onClick={() => void run('page.duplicate', { pages: page })} data-testid="pdf-duplicate-page" />
          <RibbonButton icon={Trash2} label={t('deletePage')} disabled={!editable || count < 2 || !enabled('page.delete')} onClick={() => void run('page.delete', { pages: page })} data-testid="pdf-delete-page" primary />
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={p('Comment')} icon={MessageSquarePlus}>
        <RibbonButton icon={MessageSquarePlus} size="large" label={t('stickyNote')} disabled={!editable || !enabled('comment.add')} onClick={() => ask('comment.add')} data-testid="pdf-sticky-note" primary />
        <RibbonStack>
          <RibbonButton icon={Highlighter} label={t('highlightText')} disabled={!editable || !enabled('comment.add')} onClick={() => markup('highlight')} data-testid="pdf-highlight" primary />
          <RibbonButton icon={Type} label={t('addText')} disabled={!editable || !enabled('page.addText')} onClick={() => ask('page.addText')} data-testid="pdf-add-text" primary />
          <RibbonButton icon={Copy} label={t('copyText')} disabled={!enabled('text.extract')} onClick={() => void copyText()} data-testid="pdf-copy-text" />
        </RibbonStack>
        <RibbonStack>
          <RibbonButton icon={Underline} label={t('underline')} disabled={!editable || !enabled('comment.add')} onClick={() => markup('underline')} data-testid="pdf-underline" />
          <RibbonButton icon={Strikethrough} label={t('strikethrough')} disabled={!editable || !enabled('comment.add')} onClick={() => markup('strikeout')} data-testid="pdf-strikeout" />
        </RibbonStack>
      </RibbonGroup>
      <RibbonGroup label={p('Select text')} icon={SelectTool}>
        <RibbonButton icon={Copy} label={p('Copy selection')} title={p('Drag to select text. Alt-drag selects a column. Hold Space to pan.')} disabled={!selectionAvailable} onClick={onCopySelection} data-testid="pdf-copy-selection" primary />
      </RibbonGroup>
    </>
  )
}
