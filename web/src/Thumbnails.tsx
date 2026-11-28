import { useEffect, useMemo, useRef, useState, type KeyboardEvent as ReactKeyboardEvent, type MouseEvent as ReactMouseEvent, type PointerEvent as ReactPointerEvent } from 'react'
import { Button, ContextMenu, MenuItem, MenuSeparator, TesseraIcons, useKitEditor, type ContextMenuPoint } from '@kition/sdk/editor'
import type { PdfInfo } from './pdf'
import { parseStamps, ThumbnailQueue, THUMB_ROW_HEIGHT, thumbnailWindow, type PageStamps, type Thumbnail, type ThumbnailSource } from './thumbnailQueue'
import { dropGap, pageTargets, rangeSelection, toggleSelection } from './organize'

/** What the thumbnails do to pages (0-based): each one command, one undo step (`App.tsx`). */
export type PageActions = {
  rotate: (pages: number[], degrees: number) => void
  remove: (pages: number[]) => void
  insertAfter: (page: number) => void
  duplicate: (pages: number[]) => void
  /** The pages dropped at gap `gap` (0 before the first page, the page count after the last). */
  move: (pages: number[], gap: number) => void
}

/** How far the pointer goes, in CSS pixels, before a press on a thumbnail becomes a drag. */
const DRAG_START = 5
/** Near the list's top or bottom edge (CSS pixels) a drag scrolls it. */
const EDGE = 32

/**
 * The page thumbnails (stage 7, E1–E5): a click goes to the page; Shift+click selects a run from
 * the last page clicked, Cmd+click adds a page or takes it out; a drag moves the selected pages
 * (or the page grabbed) where the line between thumbnails shows, one `page.move`; Delete and
 * Backspace delete the selected pages (never the last one); a right-click opens the pages' menu.
 * With no selection the current page is the one acted on. Only the visible rows and a small
 * overscan own canvases or queued work.
 */
export function Thumbnails({ info, current, onPage, selected, onSelect, actions }: {
  info: PdfInfo
  current: number
  onPage: (index: number) => void
  /** The pages selected (0-based); empty: none, the current page is the one acted on. */
  selected: readonly number[]
  onSelect: (pages: number[]) => void
  actions: PageActions
}) {
  const { session: { doc, change }, t, phrase: p, enabled } = useKitEditor()
  const region = useRef<HTMLDivElement>(null)
  const [view, setView] = useState({ top: 0, height: 600 })
  const [, changed] = useState(0)
  const queue = useMemo(() => new ThumbnailQueue(() => changed((value) => value + 1)), [])
  const count = info.pages.length
  const range = thumbnailWindow(count, view.top, view.height)
  const rows = info.pages.slice(range.start, range.end)
  const targets = pageTargets(selected, current)
  const anchor = useRef<number | null>(null)
  const [menu, setMenu] = useState<{ at: ContextMenuPoint; pages: number[] } | null>(null)
  const [drag, setDrag] = useState<{ pages: number[]; gap: number | null; x: number; y: number } | null>(null)
  // A press that became a drag: the click that follows it is not a click.
  const dragged = useRef(false)
  const canEdit = info.editable
  const can = (command: string) => canEdit && enabled(command)

  useEffect(() => () => queue.dispose(), [queue])
  useEffect(() => {
    const element = region.current
    if (!element) return
    const measure = () => setView({ top: element.scrollTop, height: element.clientHeight })
    const observer = new ResizeObserver(measure)
    observer.observe(element)
    measure()
    return () => observer.disconnect()
  }, [])
  // A new document starts with no anchor and no menu.
  useEffect(() => {
    anchor.current = null
    setMenu(null)
  }, [doc])

  useEffect(() => {
    const element = region.current
    if (!element || drag) return
    const top = current * THUMB_ROW_HEIGHT
    if (top < element.scrollTop || top + THUMB_ROW_HEIGHT > element.scrollTop + element.clientHeight) {
      element.scrollTop = Math.max(0, top - (element.clientHeight - THUMB_ROW_HEIGHT) / 2)
    }
    setView({ top: element.scrollTop, height: element.clientHeight })
    // Not during a drag: the list stays where the pointer has it.
  }, [current, doc])

  // The keyboard stays on the thumbnails after a delete or a move took the focused one away.
  const keepFocus = useRef(false)
  useEffect(() => {
    if (!keepFocus.current) return
    const active = document.activeElement
    if (active && active !== document.body && active.isConnected && region.current?.contains(active)) return
    keepFocus.current = false
    requestAnimationFrame(() => region.current?.querySelector<HTMLButtonElement>(`[data-testid="pdf-thumb-${Math.min(current, count - 1)}"]`)?.focus({ preventScroll: true }))
  })

  // The engine's page stamps (stage 3): after an edit only the pages whose stamp changed draw
  // again, each showing its old thumbnail until the new one arrives.
  const source = useMemo<ThumbnailSource | null>(() => doc ? {
    render: (x, y, scale, width, height) => doc.render(x, y, scale, width, height),
    stamps: async () => {
      const stamps = parseStamps(await doc.invoke<string>('pageStamps'))
      if (!stamps) throw new Error('The engine has no page stamps')
      return stamps
    },
  } : null, [doc])
  // A failed read (an engine without stamps) is `stamps: null`: every revision then draws anew.
  const [stamps, setStamps] = useState<{ source: ThumbnailSource; change: number; stamps: PageStamps | null } | null>(null)
  useEffect(() => {
    if (!source?.stamps) return
    let live = true
    const at = change.revision
    source.stamps().then((found) => found, () => null).then((found) => { if (live) setStamps({ source, change: at, stamps: found }) })
    return () => { live = false }
  }, [source, change.revision])
  // Until this revision's stamps are known the thumbnails stay as they are.
  const ready = stamps !== null && stamps.source === source && stamps.change === change.revision
  useEffect(() => {
    if (!ready) return
    const order = [current, ...Array.from({ length: range.end - range.start }, (_, offset) => range.start + offset)]
    const known = stamps?.stamps?.stamps.length === info.pages.length ? stamps.stamps.stamps : null
    // Without stamps (an engine that has none), `stamp` stays out and every revision draws anew.
    queue.update(source, change.revision, order.flatMap((index) => info.pages[index] ? [{ index, area: info.pages[index].area, ...(known ? { stamp: known[index] } : {}) }] : []))
  }, [queue, source, ready, stamps, change.revision, current, info.pages, range.start, range.end])

  const scrollTo = (index: number) => {
    const element = region.current
    if (!element) return
    const top = index * THUMB_ROW_HEIGHT
    if (top < element.scrollTop || top + THUMB_ROW_HEIGHT > element.scrollTop + element.clientHeight) element.scrollTop = Math.max(0, top - (element.clientHeight - THUMB_ROW_HEIGHT) / 2)
    setView({ top: element.scrollTop, height: element.clientHeight })
  }
  const focusThumb = (index: number) => requestAnimationFrame(() => region.current?.querySelector<HTMLButtonElement>(`[data-testid="pdf-thumb-${index}"]`)?.focus({ preventScroll: true }))
  /** The arrows, Home and End: the page goes, the selection with Shift grows from the anchor, else it is let go. */
  const stepTo = (index: number, extend: boolean) => {
    const wanted = Math.max(0, Math.min(count - 1, index))
    if (extend) onSelect(rangeSelection(anchor.current ?? current, wanted))
    else {
      anchor.current = wanted
      if (selected.length) onSelect([])
    }
    scrollTo(wanted)
    onPage(wanted)
    focusThumb(wanted)
  }

  /** Deletes `pages` unless they are every page (a document keeps one); the keyboard stays here. */
  const remove = (pages: number[]) => {
    if (!pages.length || pages.length >= count || !can('page.delete')) return
    keepFocus.current = true
    actions.remove(pages)
  }

  const click = (event: ReactMouseEvent<HTMLButtonElement>, index: number) => {
    // WebKit gives a clicked button no focus: the keys (Delete, the arrows) need it here.
    event.currentTarget.focus({ preventScroll: true })
    if (dragged.current) {
      dragged.current = false
      return
    }
    if (event.shiftKey) {
      onSelect(rangeSelection(anchor.current ?? current, index))
      return
    }
    if (event.metaKey || event.ctrlKey) {
      onSelect(toggleSelection(selected, current, index))
      anchor.current = index
      return
    }
    anchor.current = index
    if (selected.length) onSelect([])
    onPage(index)
  }

  const openMenu = (index: number, at: ContextMenuPoint) => {
    const shown = selected.length ? selected : [current]
    const pages = shown.includes(index) ? pageTargets(selected, current) : [index]
    if (!shown.includes(index)) {
      onSelect([index])
      anchor.current = index
    }
    setMenu({ at, pages })
  }

  const keyDown = (event: ReactKeyboardEvent<HTMLButtonElement>, index: number) => {
    const command = event.metaKey || event.ctrlKey
    const key = event.key
    const take = () => {
      event.preventDefault()
      event.stopPropagation()
    }
    if (!command && !event.altKey && (key === 'ArrowDown' || key === 'ArrowUp' || key === 'Home' || key === 'End')) {
      take()
      stepTo(key === 'ArrowDown' ? index + 1 : key === 'ArrowUp' ? index - 1 : key === 'Home' ? 0 : count - 1, event.shiftKey)
    } else if (!command && !event.altKey && !event.shiftKey && (key === 'Delete' || key === 'Backspace')) {
      take()
      remove(selected.length ? targets : [index])
    } else if (command && !event.altKey && !event.shiftKey && key.toLowerCase() === 'a') {
      take()
      onSelect(Array.from({ length: count }, (_, page) => page))
    } else if (key === 'Escape' && selected.length && !command) {
      take()
      onSelect([])
    } else if (key === 'ContextMenu' || (key === 'F10' && event.shiftKey)) {
      take()
      const box = event.currentTarget.getBoundingClientRect()
      openMenu(index, { x: box.left + box.width / 2, y: box.top + box.height / 2 })
    }
  }

  /** A press that moves far enough drags the selected pages (or the one pressed) to the gap under the pointer. */
  const pointerDown = (event: ReactPointerEvent<HTMLButtonElement>, index: number) => {
    dragged.current = false
    if (event.button !== 0 || !can('page.move') || count < 2) return
    const start = { x: event.clientX, y: event.clientY }
    const shown = selected.length ? selected : [current]
    const pages = shown.includes(index) ? pageTargets(selected, current) : [index]
    let live: { pages: number[]; gap: number | null; x: number; y: number } | null = null
    let pointer = start
    let frame = 0
    const gapAt = (x: number, y: number) => {
      const element = region.current
      if (!element) return null
      const box = element.getBoundingClientRect()
      if (x < box.left || x > box.right || y < box.top - 4 || y > box.bottom + 4) return null
      return dropGap(y - box.top + element.scrollTop, THUMB_ROW_HEIGHT, count)
    }
    const show = () => {
      if (!live) return
      live = { ...live, gap: gapAt(pointer.x, pointer.y), x: pointer.x, y: pointer.y }
      setDrag(live)
    }
    // Near the list's edges it scrolls, a little more the nearer the pointer.
    const scroll = () => {
      frame = 0
      const element = region.current
      if (!live || !element) return
      const box = element.getBoundingClientRect()
      const inside = pointer.x >= box.left && pointer.x <= box.right
      const up = pointer.y - box.top
      const down = box.bottom - pointer.y
      const speed = !inside ? 0 : up < EDGE ? -Math.ceil((EDGE - Math.max(0, up)) / 3) : down < EDGE ? Math.ceil((EDGE - Math.max(0, down)) / 3) : 0
      if (!speed) return
      element.scrollTop += speed
      setView({ top: element.scrollTop, height: element.clientHeight })
      show()
      frame = requestAnimationFrame(scroll)
    }
    const move = (moved: PointerEvent) => {
      pointer = { x: moved.clientX, y: moved.clientY }
      if (!live) {
        if (Math.hypot(pointer.x - start.x, pointer.y - start.y) < DRAG_START) return
        live = { pages, gap: null, x: pointer.x, y: pointer.y }
        dragged.current = true
        setMenu(null)
      }
      show()
      if (!frame) frame = requestAnimationFrame(scroll)
    }
    const stop = () => {
      window.removeEventListener('pointermove', move)
      window.removeEventListener('pointerup', up)
      window.removeEventListener('pointercancel', cancel)
      window.removeEventListener('keydown', escape, true)
      if (frame) cancelAnimationFrame(frame)
      setDrag(null)
    }
    const up = (released: PointerEvent) => {
      const done = live
      stop()
      if (!done) return
      // Let go outside the list: nothing moves.
      const gap = gapAt(released.clientX, released.clientY)
      if (gap === null) return
      keepFocus.current = true
      actions.move(done.pages, gap)
    }
    const cancel = () => stop()
    // Escape during a drag drops it; nothing moves.
    const escape = (key: KeyboardEvent) => {
      if (key.key !== 'Escape' || !live) return
      key.preventDefault()
      key.stopPropagation()
      live = null
      stop()
    }
    window.addEventListener('pointermove', move)
    window.addEventListener('pointerup', up)
    window.addEventListener('pointercancel', cancel)
    window.addEventListener('keydown', escape, true)
  }

  const isSelected = new Set(selected)
  const menuPages = menu?.pages ?? []
  const many = menuPages.length > 1
  return (
    <div ref={region} className="pdf-thumbs" data-testid="pdf-thumbs" role="list" aria-label={p('Page thumbnails')} aria-multiselectable="true"
      data-cache-bytes={queue.bytes} data-cache-count={queue.size} data-thumb-renders={queue.renders} data-dragging={drag ? 'true' : undefined}
      onScroll={(event) => setView({ top: event.currentTarget.scrollTop, height: event.currentTarget.clientHeight })}>
      <div className="pdf-thumbs-spacer" style={{ height: count * THUMB_ROW_HEIGHT }}>
        {rows.map((page, offset) => {
          const index = range.start + offset
          const image = queue.get(index, source)
          const error = image && 'error' in image ? image.error : null
          const chosen = isSelected.has(index)
          return (
            <div className="pdf-thumb-row" role="listitem" aria-posinset={index + 1} aria-setsize={count} key={index} style={{ top: index * THUMB_ROW_HEIGHT, height: THUMB_ROW_HEIGHT }}>
              <button type="button" className="pdf-thumb" data-current={index === current} aria-current={index === current ? 'page' : undefined}
                data-selected={chosen ? 'true' : undefined} data-dragged={drag?.pages.includes(index) ? 'true' : undefined}
                aria-label={`${p('Page')} ${page.label}${chosen ? ` (${t('pageSelected')})` : ''}`} data-testid={`pdf-thumb-${index}`}
                onClick={(event) => click(event, index)}
                onPointerDown={(event) => pointerDown(event, index)}
                onContextMenu={(event) => {
                  event.preventDefault()
                  event.currentTarget.focus({ preventScroll: true })
                  openMenu(index, { x: event.clientX, y: event.clientY })
                }}
                onKeyDown={(event) => keyDown(event, index)}>
                <ThumbCanvas image={image} />
                <span>{page.label}</span>
              </button>
              {error ? <div className="pdf-thumb-error" role="status" title={error} data-testid={`pdf-thumb-error-${index}`}>
                <span>{t('renderFailed')}</span>
                <Button onClick={() => queue.retry(index)} aria-label={`${t('retryRender')}: ${p('Page')} ${page.label}`}>{t('retryRender')}</Button>
              </div> : null}
            </div>
          )
        })}
        {drag && drag.gap !== null ? <div className="pdf-thumb-drop" style={{ top: drag.gap * THUMB_ROW_HEIGHT }} data-testid="pdf-thumb-drop" aria-hidden="true" /> : null}
      </div>
      {drag && drag.pages.length > 1 ? <div className="pdf-thumb-drag-count" style={{ left: drag.x + 12, top: drag.y + 12 }} aria-hidden="true">{t('pagesCount', { count: String(drag.pages.length) })}</div> : null}
      <ContextMenu at={menu?.at ?? null} onClose={() => setMenu(null)} label={t('menuPages')} testId="pdf-thumb-menu">
        <MenuItem icon={TesseraIcons.RotateCcwIcon} label={t('rotateLeft')} disabled={!can('page.rotate')} onSelect={() => actions.rotate(menuPages, -90)} testId="pdf-thumb-menu-rotate-left" />
        <MenuItem icon={TesseraIcons.RotateCwIcon} label={t('rotateRight')} disabled={!can('page.rotate')} onSelect={() => actions.rotate(menuPages, 90)} testId="pdf-thumb-menu-rotate-right" />
        <MenuSeparator />
        <MenuItem icon={TesseraIcons.FilePlusCornerIcon} label={t('insertBlankAfter')} disabled={!can('page.insertBlank')} onSelect={() => actions.insertAfter(Math.max(...menuPages))} testId="pdf-thumb-menu-insert" />
        <MenuItem icon={TesseraIcons.CopyPlusIcon} label={many ? t('duplicatePages', { count: String(menuPages.length) }) : t('duplicatePage')} disabled={!can('page.duplicate')} onSelect={() => actions.duplicate(menuPages)} testId="pdf-thumb-menu-duplicate" />
        <MenuSeparator />
        <MenuItem icon={TesseraIcons.TrashIcon} label={many ? t('deletePages', { count: String(menuPages.length) }) : t('deletePage')} shortcut="Delete" disabled={!can('page.delete') || menuPages.length >= count} onSelect={() => remove(menuPages)} testId="pdf-thumb-menu-delete" />
      </ContextMenu>
    </div>
  )
}

function ThumbCanvas({ image }: { image: Thumbnail | undefined }) {
  const ref = useRef<HTMLCanvasElement>(null)
  const pixels = image && 'pixels' in image ? image : null
  useEffect(() => {
    const canvas = ref.current
    if (!canvas) return
    canvas.width = pixels?.width ?? 1
    canvas.height = pixels?.height ?? 1
    if (pixels) canvas.getContext('2d')?.putImageData(new ImageData(new Uint8ClampedArray(pixels.pixels.buffer as ArrayBuffer, pixels.pixels.byteOffset, pixels.pixels.byteLength), pixels.width, pixels.height), 0, 0)
  }, [pixels])
  return <span className="pdf-thumb-preview"><canvas ref={ref} width={1} height={1} style={{ width: pixels?.width ?? 132, height: pixels?.height ?? 180 }} aria-hidden="true" /></span>
}
