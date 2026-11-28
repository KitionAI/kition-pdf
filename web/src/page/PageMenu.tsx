/**
 * The pages' right-click menus (stage 4, L1 and C12): one for a comment (it is selected first),
 * one for selected text (a right-click on the selection), and the page's own (anywhere else on a
 * page). Rows that cannot run are off, never hidden, so the menu keeps its shape.
 */
import { ColorGrid, ContextMenu, MenuItem, MenuSeparator, TesseraIcons, useKitEditor, type ContextMenuPoint } from '@kition/sdk/editor'
import type { MarkupType } from '../markup'
import type { PageComment } from './geometry'

export type PageMenuTarget =
  | { kind: 'comment'; at: ContextMenuPoint; comment: PageComment }
  | { kind: 'selection'; at: ContextMenuPoint }
  | { kind: 'page'; at: ContextMenuPoint; page: number; point: [number, number] }

/** The comment colours a menu offers: yellow, orange, red, pink, purple, blue, light blue, green, grey, black. */
export const COMMENT_COLORS = ['#FFD100', '#FF8A00', '#E32222', '#F06292', '#8E44AD', '#1A73E8', '#4FC3F7', '#2E9E5B', '#8C8C8C', '#000000']

export type PageMenuActions = {
  copy: () => void
  mark: (type: MarkupType) => void
  redact: () => void
  findSelection: () => void
  editText: (item: PageComment) => void
  reply: (item: PageComment) => void
  setStatus: (item: PageComment) => void
  color: (item: PageComment, color: string) => void
  copyText: (item: PageComment) => void
  remove: (item: PageComment) => void
  properties: (item: PageComment) => void
  addNote: (page: number, point: [number, number]) => void
  addBookmark: (page: number) => void
  rotatePage: (page: number) => void
  deletePage: (page: number) => void
  fit: (to: 'page' | 'width') => void
  find: () => void
}

export function PageMenu({ target, onClose, actions, hasSelection, pageCount }: {
  target: PageMenuTarget | null
  onClose: () => void
  actions: PageMenuActions
  hasSelection: boolean
  pageCount: number
}) {
  const { t, enabled } = useKitEditor()
  if (!target) return null
  const mod = /Mac|iPhone|iPad/i.test(navigator.platform) ? '⌘' : 'Ctrl+'
  const canMark = enabled('comment.add')
  const rows = (() => {
    switch (target.kind) {
      case 'comment': {
        const item = target.comment
        const canEdit = enabled('comment.edit') && !item.locked
        return (
          <>
            <MenuItem icon={TesseraIcons.PencilIcon} label={t('menuEditText')} onSelect={() => actions.editText(item)} disabled={!canEdit} testId="pdf-menu-edit-text" />
            <MenuItem icon={TesseraIcons.MessageSquareIcon} label={t('reply')} onSelect={() => actions.reply(item)} disabled={!enabled('comment.reply')} testId="pdf-menu-reply" />
            <MenuItem label={t('menuSetStatus')} onSelect={() => actions.setStatus(item)} disabled={!enabled('comment.setStatus')} testId="pdf-menu-status" />
            <MenuItem icon={TesseraIcons.CopyIcon} label={t('menuCopyText')} onSelect={() => actions.copyText(item)} disabled={!item.contents} testId="pdf-menu-copy-text" />
            <MenuSeparator />
            <div className="pdf-menu-label" aria-hidden="true">{t('menuColor')}</div>
            {canEdit ? <ColorGrid colors={COMMENT_COLORS} onPick={(color) => { actions.color(item, color); onClose() }} testId="pdf-menu-colors" /> : null}
            <MenuSeparator />
            <MenuItem icon={TesseraIcons.TrashIcon} label={t('deleteComment')} shortcut="Delete" onSelect={() => actions.remove(item)} disabled={!enabled('comment.delete') || !!item.locked} testId="pdf-menu-delete" />
            <MenuItem label={t('menuProperties')} onSelect={() => actions.properties(item)} disabled={!canEdit} testId="pdf-menu-properties" />
          </>
        )
      }
      case 'selection':
        return (
          <>
            <MenuItem icon={TesseraIcons.CopyIcon} label={t('copy')} shortcut={`${mod}C`} onSelect={actions.copy} testId="pdf-menu-copy" />
            <MenuSeparator />
            <MenuItem icon={TesseraIcons.HighlighterIcon} label={t('highlight')} onSelect={() => actions.mark('highlight')} disabled={!canMark} testId="pdf-menu-highlight" />
            <MenuItem icon={TesseraIcons.UnderlineIcon} label={t('underline')} onSelect={() => actions.mark('underline')} disabled={!canMark} testId="pdf-menu-underline" />
            <MenuItem icon={TesseraIcons.StrikethroughIcon} label={t('strikethrough')} onSelect={() => actions.mark('strikeout')} disabled={!canMark} testId="pdf-menu-strikeout" />
            <MenuItem icon={TesseraIcons.EraserIcon} label={t('markRedaction')} onSelect={actions.redact} disabled={!enabled('redaction.mark')} testId="pdf-menu-redact" />
            <MenuSeparator />
            <MenuItem icon={TesseraIcons.SearchIcon} label={t('menuFindSelection')} onSelect={actions.findSelection} testId="pdf-menu-find-selection" />
          </>
        )
      case 'page': {
        const { page, point } = target
        return (
          <>
            <MenuItem icon={TesseraIcons.CopyIcon} label={t('copy')} shortcut={`${mod}C`} onSelect={actions.copy} disabled={!hasSelection} testId="pdf-menu-copy" />
            <MenuSeparator />
            <MenuItem icon={TesseraIcons.StickyNoteIcon} label={t('menuAddNote')} onSelect={() => actions.addNote(page, point)} disabled={!canMark} testId="pdf-menu-add-note" />
            <MenuItem icon={TesseraIcons.BookmarkIcon} label={t('menuAddBookmark')} onSelect={() => actions.addBookmark(page)} disabled={!enabled('bookmark.add')} testId="pdf-menu-add-bookmark" />
            <MenuSeparator />
            <MenuItem icon={TesseraIcons.RotateCwIcon} label={t('menuRotatePage')} onSelect={() => actions.rotatePage(page)} disabled={!enabled('page.rotate')} testId="pdf-menu-rotate-page" />
            <MenuItem icon={TesseraIcons.TrashIcon} label={t('menuDeletePage')} onSelect={() => actions.deletePage(page)} disabled={!enabled('page.delete') || pageCount < 2} testId="pdf-menu-delete-page" />
            <MenuSeparator />
            <MenuItem label={t('fitPage')} shortcut={`${mod}0`} onSelect={() => actions.fit('page')} testId="pdf-menu-fit-page" />
            <MenuItem label={t('fitWidth')} shortcut={`${mod}2`} onSelect={() => actions.fit('width')} testId="pdf-menu-fit-width" />
            <MenuSeparator />
            <MenuItem icon={TesseraIcons.SearchIcon} label={t('menuFind')} shortcut={`${mod}F`} onSelect={actions.find} testId="pdf-menu-find" />
          </>
        )
      }
    }
  })()
  return (
    <ContextMenu at={target.at} onClose={onClose} label={target.kind === 'comment' ? t('menuComment') : target.kind === 'selection' ? t('selectionActions') : t('menuPage')} testId={`pdf-menu-${target.kind}`}>
      {rows}
    </ContextMenu>
  )
}
