import { TesseraIcons } from '@kition/kit-editor'
import { RibbonButton, RibbonGroup, RibbonItem, RibbonRow, RibbonStack, toast, useKitEditor } from '@kition/kit-editor'
import type { PdfInfo } from '../pdf'


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
const BookOpen = TesseraIcons.BookOpenIcon
/** The text selection tool's icon, here and in the Focus bar. */
export const SelectTool = TesseraIcons.TextCursorIcon
const FileIcon = TesseraIcons.FileIcon

export function HomeTab({ info, current, onPage, onZoom, selectionAvailable, onCopySelection }: { info: PdfInfo; current: number; onPage: (index: number) => void; onZoom: (to: 'width' | 'page' | 'actual') => void; selectionAvailable: boolean; onCopySelection: () => void }) {
  const { run, ask, enabled, phrase: p, t } = useKitEditor()
  const count = info.pages.length
  const page = [current + 1]
  const editable = info.editable

  const copyText = async () => {
    const answer = await run('text_extract', { pages: page })
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
                defaultValue={info.pages[current]?.label ?? current + 1}
                aria-label={t('page')}
                onKeyDown={(event) => {
                  if (event.key !== 'Enter') return
                  const wanted = event.currentTarget.value.trim()
                  const index = info.pages.findIndex((item) => item.label === wanted)
                  const number = Number(wanted)
                  onPage(index >= 0 ? index : Number.isFinite(number) ? Math.max(0, Math.min(count - 1, number - 1)) : current)
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
          <RibbonButton icon={ScanSearch} label={t('actualSize')} onClick={() => onZoom('actual')} data-testid="pdf-actual-size" />
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={p('Page')} icon={FileIcon}>
        <RibbonStack>
          <RibbonButton icon={RotateCcw} label={t('rotateLeft')} disabled={!editable || !enabled('page_rotate')} onClick={() => void run('page_rotate', { degrees: -90, pages: page })} data-testid="pdf-rotate-left" primary />
          <RibbonButton icon={RotateCw} label={t('rotateRight')} disabled={!editable || !enabled('page_rotate')} onClick={() => void run('page_rotate', { degrees: 90, pages: page })} data-testid="pdf-rotate-right" primary />
        </RibbonStack>
        <RibbonStack>
          <RibbonButton icon={FilePlus2} label={t('insertPage')} disabled={!editable || !enabled('page_insert_blank')} onClick={() => void run('page_insert_blank', { at: current + 2 })} data-testid="pdf-insert-page" primary />
          <RibbonButton icon={CopyPlus} label={t('duplicatePage')} disabled={!editable || !enabled('page_duplicate')} onClick={() => void run('page_duplicate', { pages: page })} data-testid="pdf-duplicate-page" />
          <RibbonButton icon={Trash2} label={t('deletePage')} disabled={!editable || count < 2 || !enabled('page_delete')} onClick={() => void run('page_delete', { pages: page })} data-testid="pdf-delete-page" primary />
        </RibbonStack>
      </RibbonGroup>

      <RibbonGroup label={p('Comment')} icon={MessageSquarePlus}>
        <RibbonButton icon={MessageSquarePlus} size="large" label={t('stickyNote')} disabled={!editable || !enabled('comment_add')} onClick={() => ask('comment_add')} data-testid="pdf-sticky-note" primary />
        <RibbonStack>
          <RibbonButton icon={Highlighter} label={t('highlightText')} disabled={!editable || !enabled('comment_add')} onClick={() => ask('comment_add', { type: 'highlight', find: '' })} data-testid="pdf-highlight" primary />
          <RibbonButton icon={Type} label={t('addText')} disabled={!editable || !enabled('page_add_text')} onClick={() => ask('page_add_text')} data-testid="pdf-add-text" primary />
          <RibbonButton icon={Copy} label={t('copyText')} disabled={!enabled('text_extract')} onClick={() => void copyText()} data-testid="pdf-copy-text" />
        </RibbonStack>
      </RibbonGroup>
      <RibbonGroup label={p('Select text')} icon={SelectTool}>
        <RibbonButton icon={Copy} label={p('Copy selection')} title={p('Drag to select text. Alt-drag selects a column. Hold Space to pan.')} disabled={!selectionAvailable} onClick={onCopySelection} data-testid="pdf-copy-selection" primary />
      </RibbonGroup>
    </>
  )
}
