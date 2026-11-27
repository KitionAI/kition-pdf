import { TesseraIcons } from '@kition/sdk/editor'
import { MenuItem, RibbonButton, RibbonGroup, RibbonItem, RibbonMenu, RibbonRow, RibbonStack, toast, useKitEditor } from '@kition/sdk/editor'
import { pageFromField, type PdfInfo } from '../pdf'
import type { MarkupType } from '../markup'
import type { ToolId } from '../page/tools'
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
const More = TesseraIcons.EllipsisIcon

export function HomeTab({ info, current, onPage, onZoom, selectionAvailable, onCopySelection, onMark, tool, onTool }: {
  info: PdfInfo
  current: number
  onPage: (index: number) => void
  onZoom: (to: 'width' | 'page' | 'actual') => void
  selectionAvailable: boolean
  onCopySelection: () => void
  /** Marks the selected text (or the find bar's match) at once; false when there is none. */
  onMark: (type: MarkupType) => boolean
  /** The tool on the pages, and choosing one (the Comment group's buttons are tools). */
  tool: ToolId
  onTool: (id: ToolId) => void
}) {
  const { run, ask, enabled, phrase: p, t } = useKitEditor()
  const count = info.pages.length
  const page = [current + 1]
  const editable = info.editable
  const label = info.pages[current]?.label ?? String(current + 1)

  // With text selected (or a match found) the markup buttons mark it; else they turn the tool on,
  // to mark text by dragging over it (the parameters dialog is under More).
  const markup = (type: MarkupType) => {
    if (!onMark(type)) onTool(type)
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
        <RibbonButton icon={MessageSquarePlus} size="large" label={t('stickyNote')} pressed={tool === 'note'} disabled={!editable || !enabled('comment.add')} onClick={() => onTool(tool === 'note' ? 'select' : 'note')} data-testid="pdf-sticky-note" primary />
        <RibbonStack>
          <RibbonButton icon={Highlighter} label={t('highlightText')} pressed={tool === 'highlight'} disabled={!editable || !enabled('comment.add')} onClick={() => (tool === 'highlight' ? onTool('select') : markup('highlight'))} data-testid="pdf-highlight" primary />
          <RibbonButton icon={Type} label={t('addText')} pressed={tool === 'addText'} disabled={!editable || !enabled('page.addText')} onClick={() => onTool(tool === 'addText' ? 'select' : 'addText')} data-testid="pdf-add-text" primary />
          <RibbonButton icon={Copy} label={t('copyText')} disabled={!enabled('text.extract')} onClick={() => void copyText()} data-testid="pdf-copy-text" />
        </RibbonStack>
        <RibbonStack>
          <RibbonButton icon={Underline} label={t('underline')} pressed={tool === 'underline'} disabled={!editable || !enabled('comment.add')} onClick={() => (tool === 'underline' ? onTool('select') : markup('underline'))} data-testid="pdf-underline" />
          <RibbonButton icon={Strikethrough} label={t('strikethrough')} pressed={tool === 'strikeout'} disabled={!editable || !enabled('comment.add')} onClick={() => (tool === 'strikeout' ? onTool('select') : markup('strikeout'))} data-testid="pdf-strikeout" />
          {/* In a RibbonItem: in Single line a menu's own rows are drawn only under a primary item. */}
          <RibbonItem primary>
            <RibbonMenu icon={More} label={t('commentMore')} disabled={!editable} testId="pdf-comment-more" primary>
              <MenuItem icon={MessageSquarePlus} label={t('noteDialog')} disabled={!enabled('comment.add')} onSelect={() => ask('comment.add')} testId="pdf-note-dialog" />
              <MenuItem icon={Highlighter} label={t('highlightDialog')} disabled={!enabled('comment.add')} onSelect={() => ask('comment.add', { type: 'highlight', find: '' })} testId="pdf-highlight-dialog" />
              <MenuItem icon={Type} label={t('addTextDialog')} disabled={!enabled('page.addText')} onSelect={() => ask('page.addText')} testId="pdf-add-text-dialog" />
            </RibbonMenu>
          </RibbonItem>
        </RibbonStack>
      </RibbonGroup>
      <RibbonGroup label={p('Select text')} icon={SelectTool}>
        <RibbonButton icon={Copy} label={p('Copy selection')} title={p('Drag to select text. Alt-drag selects a column. Hold Space to pan.')} disabled={!selectionAvailable} onClick={onCopySelection} data-testid="pdf-copy-selection" primary />
      </RibbonGroup>
    </>
  )
}
