import { useContext } from 'react'
import {
  CommandGroupButtons,
  RibbonButton,
  RibbonGroup,
  RibbonPresentationContext,
  RibbonStack,
  TesseraIcons,
  Tooltip,
  TooltipContent,
  TooltipTrigger,
  useKitEditor,
  type RibbonButtonProps,
} from '@kition/sdk/editor'
import type { PdfInfo } from '../pdf'

/** The page commands this tab draws by hand (the engine's others follow them in its groups). */
const DRAWN_PAGES = ['page.rotate', 'page.insertBlank', 'page.duplicate', 'page.delete', 'page.move', 'page.number']
const DRAWN_BOOKMARKS = ['bookmark.add']

/**
 * A ribbon button that, in Single line's row (where it is its icon alone), names itself with the
 * kit's tooltip as the pointer rests on it, not the browser's title.
 */
function NamedButton(props: RibbonButtonProps) {
  const presentation = useContext(RibbonPresentationContext)
  if (presentation !== 'primary') return <RibbonButton {...props} />
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <RibbonButton {...props} title={undefined} />
      </TooltipTrigger>
      <TooltipContent side="bottom" data-testid="pdf-ribbon-tip">{props.label}</TooltipContent>
    </Tooltip>
  )
}

/**
 * The Pages tab (stage 7, E9): the page commands act on the pages selected in the thumbnails (the
 * current page when none is), each with its own icon; Move pages and Number pages open their
 * dialogs from the selection; New bookmark writes its title in the Bookmarks panel. The engine's
 * other page and bookmark commands follow in their groups.
 */
export function PagesTab({ info, targets, onNewBookmark }: {
  info: PdfInfo
  /** The pages acted on (0-based): those selected in the thumbnails, else the current one. */
  targets: readonly number[]
  onNewBookmark: () => void
}) {
  const { run, ask, enabled, t, phrase: p } = useKitEditor()
  const pages = targets.map((index) => index + 1)
  const editable = info.editable
  const can = (command: string) => editable && enabled(command)
  return (
    <>
      <RibbonGroup label={p('Pages')} icon={TesseraIcons.FilesIcon}>
        <RibbonStack>
          <NamedButton icon={TesseraIcons.RotateCcwIcon} label={t('rotateLeft')} disabled={!can('page.rotate')} onClick={() => void run('page.rotate', { degrees: -90, pages })} data-testid="pdf-pages-rotate-left" primary />
          <NamedButton icon={TesseraIcons.RotateCwIcon} label={t('rotateRight')} disabled={!can('page.rotate')} onClick={() => void run('page.rotate', { degrees: 90, pages })} data-testid="pdf-pages-rotate-right" primary />
        </RibbonStack>
        <RibbonStack>
          <NamedButton icon={TesseraIcons.FilePlusCornerIcon} label={t('insertPage')} disabled={!can('page.insertBlank')} onClick={() => void run('page.insertBlank', { at: Math.max(...pages) + 1 })} data-testid="pdf-pages-insert" primary />
          <NamedButton icon={TesseraIcons.CopyPlusIcon} label={t('duplicatePage')} disabled={!can('page.duplicate')} onClick={() => void run('page.duplicate', { pages })} data-testid="pdf-pages-duplicate" primary />
          <NamedButton icon={TesseraIcons.TrashIcon} label={t('deletePage')} disabled={!can('page.delete') || pages.length >= info.pages.length} onClick={() => void run('page.delete', { pages })} data-testid="pdf-pages-delete" primary />
        </RibbonStack>
        <RibbonStack>
          <NamedButton icon={TesseraIcons.ArrowUpDownIcon} label={t('movePages')} disabled={!can('page.move')} onClick={() => ask('page.move', { pages })} data-testid="pdf-cmd-page.move" primary />
          <NamedButton icon={TesseraIcons.ListOrderedIcon} label={t('numberPages')} disabled={!can('page.number')} onClick={() => ask('page.number', { from: Math.min(...pages), to: Math.max(...pages) })} data-testid="pdf-pages-number" primary />
        </RibbonStack>
        <CommandGroupButtons path="Pages" exclude={DRAWN_PAGES} />
      </RibbonGroup>
      <RibbonGroup label={p('Bookmarks')} icon={TesseraIcons.BookmarkIcon}>
        <NamedButton icon={TesseraIcons.BookmarkIcon} label={t('newBookmark')} disabled={!can('bookmark.add')} onClick={onNewBookmark} data-testid="pdf-pages-new-bookmark" primary />
        <CommandGroupButtons path="Bookmarks" exclude={DRAWN_BOOKMARKS} />
      </RibbonGroup>
    </>
  )
}
