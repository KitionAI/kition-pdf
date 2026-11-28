import { useEffect, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react'
import { ContextMenu, IconButton, MenuItem, MenuSeparator, TesseraIcons, useEngineQuery, useKitEditor, type ContextMenuPoint } from '@kition/sdk/editor'

type BookmarkItem = { path: number[]; title: string; page: number | null; open?: boolean; children?: BookmarkItem[] }
type Row = { item: BookmarkItem; depth: number; key: string; parent: string | null; hasChildren: boolean; expanded: boolean }

/** A new bookmark asked for (Cmd+B, a menu, the Pages tab): written in place, at the top level or under `parent`. */
export type BookmarkDraft = { key: number; parent: number[] | null; page?: number }

type Editing = { kind: 'rename'; key: string; path: number[]; title: string } | { kind: 'new'; parent: number[] | null; page: number | null; title: string }

const keyOf = (path: readonly number[]) => path.join('.')
const NO_BOOKMARKS: BookmarkItem[] = []

/**
 * The document's bookmarks (stage 7, F2–F4): a tree that folds and unfolds (as the file has it at
 * first, then as the person leaves it this session); a click goes to the page. A double-click or
 * F2 renames a bookmark in place (Enter or a click elsewhere keeps the title, Escape drops it);
 * Delete removes it and those under it; the arrows go through the rows (Right unfolds, Left
 * folds); a right-click offers Rename, Add a bookmark under it and Delete. A new bookmark goes to
 * the current page (or the page whose menu asked for it) and is titled in place before anything is added: Escape adds nothing. Each
 * edit is one command, one undo step.
 */
export function Bookmarks({ onPage, current, draft, onDraftTaken }: { onPage: (index: number) => void; current: number; draft: BookmarkDraft | null; onDraftTaken: () => void }) {
  const { t, run, enabled, session: { doc } } = useKitEditor()
  const items = useEngineQuery<{ bookmarks: BookmarkItem[] }>('bookmark.list', { limit: 1000 })?.bookmarks ?? NO_BOOKMARKS
  const [folded, setFolded] = useState<Map<string, boolean>>(new Map())
  const [editing, setEditingState] = useState<Editing | null>(null)
  // What is being written, as the handlers see it at once (a blur can follow an Enter before the next render).
  const editingRef = useRef<Editing | null>(null)
  const setEditing = (next: Editing | null) => {
    editingRef.current = next
    setEditingState(next)
  }
  const [, poke] = useState(0)
  const [menu, setMenu] = useState<{ at: ContextMenuPoint; row: Row } | null>(null)
  const [active, setActive] = useState<string | null>(null)
  const tree = useRef<HTMLDivElement>(null)
  // A row to give the keyboard to once the list has been read again: by its path, or by its place.
  const focusNext = useRef<{ key: string } | { index: number } | null>(null)

  // A new document starts with the file's own folding, nothing being written.
  useEffect(() => {
    setFolded(new Map())
    setEditing(null)
    setMenu(null)
  }, [doc])
  // A new bookmark asked for from outside the panel, taken once.
  useEffect(() => {
    if (!draft) return
    onDraftTaken()
    startNew(draft.parent, draft.page ?? null)
  }, [draft])

  const isOpen = (item: BookmarkItem) => folded.get(keyOf(item.path)) ?? item.open ?? false
  const rows: Row[] = []
  const walk = (list: BookmarkItem[], depth: number, parent: string | null) => {
    for (const item of list) {
      const hasChildren = (item.children ?? []).length > 0
      const newChild = editing?.kind === 'new' && editing.parent && keyOf(editing.parent) === keyOf(item.path)
      const expanded = (hasChildren || !!newChild) && (isOpen(item) || !!newChild)
      rows.push({ item, depth, key: keyOf(item.path), parent, hasChildren, expanded })
      if (expanded && hasChildren) walk(item.children ?? [], depth + 1, keyOf(item.path))
    }
  }
  walk(items, 0, null)

  useEffect(() => {
    const wanted = focusNext.current
    if (!wanted || editing) return
    const buttons = [...(tree.current?.querySelectorAll<HTMLButtonElement>('[data-testid="pdf-bookmark"]') ?? [])]
    const button = 'key' in wanted ? buttons.find((element) => element.dataset.path === wanted.key) : buttons[Math.max(0, Math.min(buttons.length - 1, wanted.index))]
    if (!button) {
      if (!buttons.length) {
        focusNext.current = null
        tree.current?.focus({ preventScroll: true })
      }
      return
    }
    focusNext.current = null
    setActive(button.dataset.path ?? null)
    button.focus({ preventScroll: true })
  })

  const canAdd = enabled('bookmark.add')
  const canRename = enabled('bookmark_rename')
  const canDelete = enabled('bookmark_delete')

  function startNew(parent: number[] | null, page: number | null = null) {
    if (!canAdd) return
    setMenu(null)
    if (parent) setFolded((now) => new Map(now).set(keyOf(parent), true))
    setEditing({ kind: 'new', parent, page, title: t('untitledBookmark') })
  }
  const rename = (row: Row) => {
    if (!canRename) return
    setMenu(null)
    setEditing({ kind: 'rename', key: row.key, path: row.item.path, title: row.item.title })
  }
  const remove = (row: Row) => {
    if (!canDelete) return
    setMenu(null)
    // The keyboard goes to the row after it (and those under it), else the one before; rows are
    // known by their paths, which the delete shifts.
    const index = rows.indexOf(row)
    const after = rows.slice(index + 1).find((other) => !other.key.startsWith(`${row.key}.`))
    const target = after ?? rows[index - 1]
    focusNext.current = target ? { key: keyOf(shiftedPath(target.item.path, row.item.path)) } : null
    void run('bookmark_delete', { path: row.item.path })
  }
  /** Enter or a click elsewhere: the title, trimmed, is kept; an empty or unchanged one changes nothing. */
  const commit = async () => {
    const now = editingRef.current
    if (!now) return
    setEditing(null)
    const title = now.title.trim()
    if (now.kind === 'rename') {
      focusNext.current = { key: now.key }
      if (title && title !== rows.find((row) => row.key === now.key)?.item.title) await run('bookmark_rename', { path: now.path, title })
      return
    }
    if (!title) return
    const siblings = now.parent ? findItem(items, now.parent)?.children ?? [] : items
    focusNext.current = { key: keyOf([...(now.parent ?? []), siblings.length + 1]) }
    await run('bookmark.add', { title, page: (now.page ?? current) + 1, ...(now.parent ? { parent: now.parent } : {}) })
  }
  const cancel = () => {
    const now = editingRef.current
    setEditing(null)
    focusNext.current = now?.kind === 'rename' ? { key: now.key } : rows.length ? { key: rows[0].key } : null
  }

  const toggle = (row: Row, open?: boolean) => {
    if (!row.hasChildren) return
    setFolded((now) => new Map(now).set(row.key, open ?? !row.expanded))
  }
  const focusRow = (index: number) => {
    const row = rows[Math.max(0, Math.min(rows.length - 1, index))]
    if (!row) return
    focusNext.current = { key: row.key }
    setActive(row.key)
    poke((n) => n + 1)
  }

  const keyDown = (event: ReactKeyboardEvent<HTMLButtonElement>, row: Row, index: number) => {
    const key = event.key
    const plain = !event.metaKey && !event.ctrlKey && !event.altKey
    let taken = true
    if (plain && key === 'ArrowDown') focusRow(index + 1)
    else if (plain && key === 'ArrowUp') focusRow(index - 1)
    else if (plain && key === 'Home') focusRow(0)
    else if (plain && key === 'End') focusRow(rows.length - 1)
    else if (plain && key === 'ArrowRight') {
      if (row.hasChildren && !row.expanded) toggle(row, true)
      else if (row.expanded) focusRow(index + 1)
    } else if (plain && key === 'ArrowLeft') {
      if (row.expanded) toggle(row, false)
      else if (row.parent) focusNext.current = { key: row.parent }
    } else if (plain && key === 'F2') rename(row)
    else if (plain && !event.shiftKey && (key === 'Delete' || key === 'Backspace')) remove(row)
    else if (key === 'ContextMenu' || (key === 'F10' && event.shiftKey)) {
      const box = event.currentTarget.getBoundingClientRect()
      setMenu({ at: { x: box.left + 24, y: box.bottom - 4 }, row })
    } else taken = false
    if (!taken) return
    event.preventDefault()
    event.stopPropagation()
    // The focus moves on the next render (`focusNext`).
    poke((n) => n + 1)
  }

  const field = (depth: number, testId: string) => editing ? (
    <div className="pdf-bookmark-row" style={{ paddingLeft: 4 + depth * 14 }}>
      <span className="pdf-bookmark-toggle" aria-hidden="true" />
      <input
        className="kui-field pdf-bookmark-field"
        value={editing.title}
        autoFocus
        onFocus={(event) => event.currentTarget.select()}
        aria-label={editing.kind === 'new' ? t('newBookmark') : t('renameBookmark')}
        onChange={(event) => setEditing({ ...editing, title: event.target.value })}
        onKeyDown={(event) => {
          if (event.key === 'Enter') {
            event.preventDefault()
            event.stopPropagation()
            void commit()
          } else if (event.key === 'Escape') {
            event.preventDefault()
            event.stopPropagation()
            cancel()
          }
        }}
        onBlur={() => void commit()}
        data-testid={testId}
      />
    </div>
  ) : null

  const shown: React.ReactNode[] = []
  rows.forEach((row, index) => {
    const { item, depth } = row
    if (editing?.kind === 'rename' && editing.key === row.key) {
      shown.push(<div key={row.key} role="treeitem" aria-level={depth + 1}>{field(depth, 'pdf-bookmark-title')}</div>)
    } else {
      shown.push(
        <div key={row.key} className="pdf-bookmark-row" role="treeitem" aria-level={depth + 1} aria-expanded={row.hasChildren ? row.expanded : undefined} style={{ paddingLeft: 4 + depth * 14 }}>
          {row.hasChildren ? (
            <button type="button" className="pdf-bookmark-toggle" tabIndex={-1} aria-label={row.expanded ? t('collapse') : t('expand')} onClick={() => toggle(row)} data-testid="pdf-bookmark-toggle">
              {row.expanded ? <TesseraIcons.ChevronDownIcon size={14} aria-hidden="true" /> : <TesseraIcons.ChevronRightIcon size={14} aria-hidden="true" />}
            </button>
          ) : <span className="pdf-bookmark-toggle" aria-hidden="true" />}
          <button
            type="button"
            className="pdf-list-item pdf-bookmark-title"
            tabIndex={(active ?? rows[0]?.key) === row.key ? 0 : -1}
            aria-disabled={item.page == null || undefined}
            onClick={(event) => {
              // WebKit gives a clicked button no focus: the keys (F2, Delete, the arrows) need it here.
              event.currentTarget.focus({ preventScroll: true })
              setActive(row.key)
              if (item.page != null) onPage(item.page - 1)
            }}
            onDoubleClick={() => rename(row)}
            onContextMenu={(event) => {
              event.preventDefault()
              event.currentTarget.focus({ preventScroll: true })
              setActive(row.key)
              setMenu({ at: { x: event.clientX, y: event.clientY }, row })
            }}
            onKeyDown={(event) => keyDown(event, row, index)}
            data-path={row.key}
            data-testid="pdf-bookmark"
          >
            <span className="pdf-bookmark-text">{item.title}</span>
            {item.page != null ? <span className="kui-muted pdf-bookmark-page">{item.page}</span> : null}
          </button>
        </div>,
      )
    }
    // A new bookmark under this one is written after its last child.
    if (editing?.kind === 'new' && editing.parent) {
      const parentKey = keyOf(editing.parent)
      let last = -1
      rows.forEach((other, place) => { if (other.key === parentKey || other.key.startsWith(`${parentKey}.`)) last = place })
      if (last === index) shown.push(<div key="new" role="treeitem" aria-level={editing.parent.length + 1}>{field(editing.parent.length, 'pdf-bookmark-title')}</div>)
    }
  })
  if (editing?.kind === 'new' && !editing.parent) shown.push(<div key="new" role="treeitem" aria-level={1}>{field(0, 'pdf-bookmark-title')}</div>)

  return (
    <div className="pdf-bookmarks">
      <div className="pdf-bookmarks-bar">
        <IconButton icon={TesseraIcons.BookmarkIcon} label={t('newBookmark')} disabled={!canAdd} onClick={() => startNew(null)} data-testid="pdf-bookmark-new" />
      </div>
      {!shown.length ? <p className="pdf-empty kui-muted">{t('noBookmarks')}</p> : (
        <div ref={tree} className="pdf-bookmark-tree" role="tree" aria-label={t('bookmarks')} tabIndex={-1} data-testid="pdf-bookmarks">
          {shown}
        </div>
      )}
      <ContextMenu at={menu?.at ?? null} onClose={() => setMenu(null)} label={t('bookmarks')} testId="pdf-bookmark-menu">
        {menu ? (
          <>
            <MenuItem icon={TesseraIcons.PencilIcon} label={t('renameBookmark')} shortcut="F2" disabled={!canRename} onSelect={() => rename(menu.row)} testId="pdf-bookmark-menu-rename" />
            <MenuItem icon={TesseraIcons.PlusIcon} label={t('addChildBookmark')} disabled={!canAdd} onSelect={() => startNew(menu.row.item.path)} testId="pdf-bookmark-menu-child" />
            <MenuSeparator />
            <MenuItem icon={TesseraIcons.TrashIcon} label={t('deleteBookmark')} shortcut="Delete" disabled={!canDelete} onSelect={() => remove(menu.row)} testId="pdf-bookmark-menu-delete" />
          </>
        ) : null}
      </ContextMenu>
    </div>
  )
}

/** Where the bookmark at `path` is once the one at `deleted` (and those under it) is gone. */
function shiftedPath(path: readonly number[], deleted: readonly number[]): number[] {
  const out = [...path]
  const level = deleted.length - 1
  if (out.length > level && deleted.slice(0, level).every((step, at) => out[at] === step) && out[level] > deleted[level]) out[level] -= 1
  return out
}

function findItem(items: readonly BookmarkItem[], path: readonly number[]): BookmarkItem | null {
  let list = items
  let found: BookmarkItem | null = null
  for (const step of path) {
    found = list[step - 1] ?? null
    if (!found) return null
    list = found.children ?? []
  }
  return found
}
