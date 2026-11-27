import { useCallback, useEffect, useId, useMemo, useRef, useState, type KeyboardEvent, type RefObject } from 'react'
import { Scrollbar } from '@kition/tessera/ui'
import {
  CommandTab,
  KitDocumentCanvas,
  KitEditorProvider,
  EditorFrame,
  FrameChrome,
  createKitStrings,
  kitLeases,
  needsParams,
  toast,
  useKitEditorState,
  useKitSession,
  type FrameTab,
  type HostConnection,
  type KitCanvasHandle,
  type KitCanvasView,
  type Phrases,
  type RasterRect,
  type RasterView,
} from '@kition/sdk/editor'
import { engineManifest } from './engine'
import { useEngineBackdrop } from './backdrop'
import { CATALOGS } from './strings'
import { TABS, commandDefaults, describe, pageWidthArea, pagesPanBounds, wholePageArea } from './pdf'
import { HomeTab, SelectTool } from './ribbon/HomeTab'
import { SidePanel } from './SidePanel'
import { FindBar, type FindControl, type Match } from './FindBar'
import { SearchHighlights } from './SearchHighlights'
import { searchPanTarget } from './searchGeometry'
import { SelectionHighlights } from './SelectionHighlights'
import { useTextSelection } from './useTextSelection'
import type { SelectionRequest, SelectionResult } from './textSelection'
import { withRewriteLease } from './rewriteLease'
import { newlyDropped } from './droppedEdits'
import { focusPages, keyboardFallback, keyboardLost, returnKeyboardLater } from './focus'
import { isTextControl } from './textSelection'
import { KEY_LINE, WHEEL_NOTCH_ZOOM, clampZoom, ladderStep, screenStep, scrollView, wheelNotches, zoomAround } from './view'
import { ZoomBar, type Fit } from './ZoomBar'

export function App({ host, phrase }: { host: HostConnection; phrase: Phrases }) {
  const t = useMemo(() => createKitStrings(CATALOGS, host.init.locale), [host])
  const manifest = useMemo(engineManifest, [])
  // An agent step that rewrites the whole file takes the whole document's lease too (UI-L2).
  // A PDF changed outside Kition opens with its stored replica all the same: the engine records
  // what the file changed as one import, so the document keeps its identity and object ids (UI-R1).
  const session = useKitSession(host, manifest, describe, {
    agentLeaseScope: (command, _params, engine) => engine.invoke<string>('leaseScope', command),
  })
  const [current, setCurrent] = useState(0)
  const defaults = useMemo(() => commandDefaults(current), [current])
  const editor = useKitEditorState(session, host, t, 'pdf', phrase, defaults)
  const { state } = session
  const { doc } = session
  // A command that rewrites the whole file takes the whole document's lease while it runs, and no
  // edit runs while someone else holds it (`rewriteLease.ts`).
  const leases = useMemo(() => kitLeases(host), [host])
  const leased = useMemo(
    () => ({
      ...editor,
      run: async (id: string, params?: Record<string, unknown>) => {
        const command = editor.byId.get(id)
        // A command that opens its dialog first is leased when the dialog runs it.
        if (!params && command && needsParams(command)) return editor.run(id, params)
        const scope = doc ? await doc.invoke<string>('leaseScope', id).catch(() => '') : ''
        const refuse = (holder: string) => toast('info', t('rewriting', { name: holder || t('someone') }))
        try {
          return await withRewriteLease(leases, scope, command?.mutates ?? true, () => editor.run(id, params), refuse)
        } finally {
          // A parameters dialog that ran (or failed) leaves the keyboard on the body: the pages take it.
          returnKeyboardLater()
        }
      },
    }),
    [doc, editor, leases, t],
  )
  // Edits an undo or redo of a whole-file rewrite dropped (`droppedEdits.ts`): the person is told.
  const dropped = useRef<{ doc: unknown; count: number }>({ doc: null, count: 0 })
  useEffect(() => {
    if (!doc) return
    let live = true
    doc.invoke<number>('kdmDropped').then((now) => {
      if (!live) return
      const seen = dropped.current.doc === doc ? dropped.current.count : 0
      dropped.current = { doc, count: now }
      const lost = newlyDropped(seen, now)
      if (lost > 0) toast('warning', t('droppedEdits', { count: String(lost) }))
    }).catch(() => undefined)
    return () => { live = false }
  }, [doc, session.change.revision, t])
  // The one tool, text selection, named in the Focus bar (the editor has no engine tools).
  const selectTool = useMemo(() => ({ label: phrase('Select text'), icon: SelectTool }), [phrase])
  const canvas = useRef<KitCanvasHandle>(null)
  useEngineBackdrop(doc, canvas)
  const viewMemory = useRef<RasterView | null>(null)
  const region = useRef<HTMLDivElement>(null)
  const searchOpener = useRef<HTMLElement | null>(null)
  const searchInput = useRef<HTMLInputElement>(null)
  const findControl = useRef<FindControl>(null)
  const [findSeed, setFindSeed] = useState('')
  const [zoom, setZoom] = useState(1)
  const [tab, setTab] = useState('home')
  const [finding, setFinding] = useState(false)
  // The canvas's view and size: the search and selection highlights and the scrollbars follow it.
  const [canvasView, setCanvasView] = useState<KitCanvasView | null>(null)
  const viewportId = useId()
  const [searchMatch, setSearchMatch] = useState<{ doc: typeof doc; revision: number; match: Match } | null>(null)
  const selectedMatch = finding && searchMatch?.doc === doc && searchMatch.revision === session.change.revision ? searchMatch.match : null
  const pages = state?.pages ?? []
  const panBounds = useMemo(() => pagesPanBounds((state?.pages ?? []).map((page) => page.area)), [state?.pages])
  const selectText = useCallback(async (request: SelectionRequest) => doc ? JSON.parse(await doc.execute('text.select', request)) as SelectionResult : null, [doc])
  const selection = useTextSelection({ document: doc, revision: session.change.revision, pages, execute: selectText, t,
    onStart: () => { setFinding(false); setSearchMatch(null); searchOpener.current = null },
  })

  // The current page: the one in the middle of the view.
  const follow = useCallback(
    (view: KitCanvasView) => {
      setZoom(view.zoom)
      setCanvasView((previous) => previous?.x === view.x && previous.y === view.y && previous.zoom === view.zoom && previous.width === view.width && previous.height === view.height ? previous : { x: view.x, y: view.y, zoom: view.zoom, width: view.width, height: view.height })
      const height = (region.current?.clientHeight ?? window.innerHeight) / view.zoom
      const middle = view.y + height / 2
      let best = 0
      pages.forEach((page, index) => {
        if (page.area.y0 <= middle) best = index
      })
      setCurrent((previous) => (previous === best ? previous : best))
    },
    [pages],
  )

  /** Goes to a page at the current zoom, its top a little below the view's top. */
  const goTo = useCallback(
    (index: number, rect: number[] | null = null) => {
      const page = pages[Math.max(0, Math.min(pages.length - 1, index))]
      const view = viewMemory.current
      if (!page) return
      if (!view) {
        canvas.current?.show(pageWidthArea(page.area)!)
        return
      }
      const target = searchPanTarget(page.area, rect, view, { width: region.current?.clientWidth ?? window.innerWidth, height: region.current?.clientHeight ?? window.innerHeight })
      canvas.current?.panTo(target.x, target.y)
      setCurrent(index)
    },
    [pages],
  )

  // A match the person asked for (a new query, Next, Previous) is brought into view; one a
  // document change only refreshed (an edit while the find bar is open) leaves the view alone.
  const showMatch = useCallback((match: Match | null, jump: boolean) => {
    if (match && jump) selection.clear()
    setSearchMatch(match ? { doc, revision: session.change.revision, match } : null)
    if (match && jump) goTo(match.page - 1, match.rects[0] ?? null)
  }, [doc, session.change.revision, goTo, selection.clear])

  const zoomTo = (to: Fit) => {
    const page = pages[current]?.area
    if (to === 'actual') canvas.current?.zoomTo(1)
    else if (page) canvas.current?.show((to === 'width' ? pageWidthArea(page) : wholePageArea(page))!)
  }
  const setZoomLevel = (value: number) => canvas.current?.zoomTo(clampZoom(value))

  // The keyboard is the pages' as soon as a document opens, before anything is clicked (K1).
  const focusedDoc = useRef<unknown>(null)
  const ready = !!state
  useEffect(() => {
    if (!doc || !ready || focusedDoc.current === doc) return
    if (keyboardLost() && focusPages()) focusedDoc.current = doc
  }, [doc, ready])
  // A key that still finds the body holding the keyboard goes to the pages (K2's safety net).
  useEffect(() => {
    document.addEventListener('keydown', keyboardFallback, true)
    return () => document.removeEventListener('keydown', keyboardFallback, true)
  }, [])
  // A parameters dialog closed with Escape or Cancel gives the keyboard back too.
  const dialogOpen = !!editor.dialog
  const wasDialogOpen = useRef(false)
  useEffect(() => {
    if (wasDialogOpen.current && !dialogOpen) returnKeyboardLater()
    wasDialogOpen.current = dialogOpen
  }, [dialogOpen])

  // A mouse wheel's notch with Cmd (Ctrl) zooms one step around the pointer; a trackpad's small
  // steps and a pinch keep the canvas's own zoom. Heard before the canvas, in the capture phase.
  useEffect(() => {
    const element = region.current
    if (!element) return undefined
    const wheel = (event: WheelEvent) => {
      if (!event.ctrlKey && !event.metaKey) return
      const surface = event.target instanceof Element ? event.target.closest('[data-testid="kit-canvas-surface"]') : null
      const notches = wheelNotches(event, window.devicePixelRatio)
      const handle = canvas.current
      const view = handle?.view()
      if (!surface || notches === null || !handle || !view) return
      event.preventDefault()
      event.stopPropagation()
      zoomAround(handle, surface, view.zoom * WHEEL_NOTCH_ZOOM ** -notches, event.clientX, event.clientY)
    }
    element.addEventListener('wheel', wheel, { capture: true, passive: false })
    return () => element.removeEventListener('wheel', wheel, { capture: true })
  }, [doc])

  // Space scrolls a screen when it is tapped; held while dragging, it pans (the canvas's hand).
  const spaceTap = useRef<{ up: boolean; dragged: boolean } | null>(null)
  const scrollScreen = useCallback((direction: 1 | -1) => {
    const view = canvas.current?.view()
    if (view) scrollView(canvas.current!, 0, direction * screenStep(view.height))
  }, [])
  useEffect(() => {
    const element = region.current
    const up = (event: globalThis.KeyboardEvent) => {
      if (event.key !== ' ') return
      const tap = spaceTap.current
      spaceTap.current = null
      if (tap && !tap.dragged && !isTextControl(event.target)) scrollScreen(tap.up ? -1 : 1)
    }
    const press = () => { if (spaceTap.current) spaceTap.current.dragged = true }
    document.addEventListener('keyup', up)
    element?.addEventListener('pointerdown', press, true)
    return () => {
      document.removeEventListener('keyup', up)
      element?.removeEventListener('pointerdown', press, true)
    }
  }, [doc, scrollScreen])

  /** Goes `by` pages from the current one, its top at the view's top. */
  const turnPage = (by: number) => goTo(Math.max(0, Math.min(pages.length - 1, current + by)))

  const keyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const command = event.metaKey || event.ctrlKey
    const key = event.key.length === 1 ? event.key.toLowerCase() : event.key
    const plain = !command && !event.altKey && !event.shiftKey
    if (command && !event.altKey && key === 'f') {
      // Text selected on the page is what the find bar looks for.
      const chosen = (selection.selected?.text ?? '').replace(/\s+/g, ' ').trim()
      const seed = chosen.length > 0 && chosen.length <= 200 ? chosen : ''
      selection.clear()
      if (!finding) {
        searchOpener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null
        setFindSeed(seed)
      } else {
        if (seed) findControl.current?.seed(seed)
        searchInput.current?.focus({ preventScroll: true })
        searchInput.current?.select()
      }
      setFinding(true)
      return true
    }
    if (finding && ((command && !event.altKey && key === 'g') || (!command && !event.altKey && key === 'F3'))) {
      findControl.current?.step(event.shiftKey ? -1 : 1)
      return true
    }
    // Escape goes back one layer at a time: the find bar, then the selection (dialogs and menus
    // take theirs first); with nothing to cancel it is the frame's (leaving Focus).
    if (plain && key === 'Escape') {
      const focusMode = region.current?.closest('.kui-app')?.getAttribute('data-mode') === 'focus'
      if (finding && !focusMode) {
        closeSearch()
        return true
      }
      selection.clear()
      return !!selection.selected
    }
    if (command && !event.altKey && !event.shiftKey && key === 'a') {
      selection.selectAll(current)
      return true
    }
    // Zoom: Cmd+0 fits the current page, Cmd+1 is 100 %, Cmd+2 fits its width; Cmd+= and Cmd+- step the ladder.
    if (command && !event.altKey && (key === '0' || key === '1' || key === '2')) {
      zoomTo(key === '0' ? 'page' : key === '1' ? 'actual' : 'width')
      return true
    }
    if (command && !event.altKey && (key === '=' || key === '+' || key === '-')) {
      const view = canvas.current?.view()
      if (view) setZoomLevel(ladderStep(view.zoom, key === '-' ? -1 : 1))
      return true
    }
    return scrollKey(event, command)
  }

  /** The keys that scroll and turn pages; not while a menu, list box or slider has the keyboard (they move in it), nor for a key a control took (a thumbnail's arrows). */
  const scrollKey = (event: KeyboardEvent<HTMLDivElement>, command: boolean) => {
    if (event.isDefaultPrevented() || event.nativeEvent.defaultPrevented || event.altKey) return false
    const target = event.target instanceof Element ? event.target : null
    if (target?.closest('[role="menu"], [role="listbox"], [role="slider"], [role="scrollbar"], .kui-scrollbar, input, select, textarea')) return false
    const handle = canvas.current
    const view = handle?.view()
    if (!handle || !view || !panBounds) return false
    switch (event.key) {
      case ' ':
        if (command) return false
        // Scrolls when the key comes up (a tap); a drag while it is held pans instead.
        if (!event.repeat) spaceTap.current = { up: event.shiftKey, dragged: false }
        return true
      case 'ArrowDown':
      case 'ArrowUp': {
        const down = event.key === 'ArrowDown'
        if (command) handle.panTo(view.x, down ? panBounds.y1 : panBounds.y0)
        else if (!event.shiftKey) scrollView(handle, 0, down ? KEY_LINE : -KEY_LINE)
        else return false
        return true
      }
      case 'PageDown':
      case 'PageUp': {
        const down = event.key === 'PageDown'
        if (command) turnPage(down ? 1 : -1)
        else scrollView(handle, 0, (down ? 1 : -1) * screenStep(view.height))
        return true
      }
      case 'Home':
      case 'End':
        handle.panTo(view.x, event.key === 'End' ? panBounds.y1 : panBounds.y0)
        return true
      case 'ArrowRight':
      case 'ArrowLeft': {
        if (event.shiftKey) return false
        const right = event.key === 'ArrowRight'
        // Sideways while the pages are wider than the view and there is more on that side; else the next page.
        const width = view.width / view.zoom
        const more = right ? view.x + width < panBounds.x1 - 0.5 : view.x > panBounds.x0 + 0.5
        if (!command && panBounds.x1 - panBounds.x0 > width + 0.5 && more) scrollView(handle, right ? KEY_LINE : -KEY_LINE, 0)
        else turnPage(right ? 1 : -1)
        return true
      }
      default:
        return false
    }
  }

  const closeSearch = () => {
    setSearchMatch(null)
    setFinding(false)
    const opener = searchOpener.current
    const target = opener?.isConnected && opener !== document.body ? opener : region.current
    target?.focus({ preventScroll: true })
    if (document.activeElement !== target) region.current?.focus({ preventScroll: true })
    searchOpener.current = null
  }

  const tabs: FrameTab[] = TABS.map((item) => ({
    id: item.id,
    label: phrase(item.label),
    view: item.id === 'home' || item.id === 'comment',
    ribbon:
      item.id === 'home' ? (
        state ? <HomeTab info={state} current={current} onPage={goTo} onZoom={zoomTo} selectionAvailable={!!selection.selected?.text} onCopySelection={selection.copy} /> : null
      ) : (
        <>
          {item.menus.map((menu) => (
            <CommandTab key={menu} menu={menu} primary={item.primary} />
          ))}
        </>
      ),
  }))
  const active = TABS.find((item) => item.id === tab) ?? TABS[0]

  return (
    <KitEditorProvider editor={leased}>
      <EditorFrame
        tabs={tabs}
        tab={active.id}
        onTab={setTab}
        canvas={canvas}
        onKeyDown={keyDown}
        focusTool={selectTool}
        status={
          state ? (
            <>
              <span data-testid="pdf-status">{t('pageOf', { page: current + 1, pages: pages.length })}</span>
              {!state.editable && state.readOnlyReason ? <span>{t('readOnly', { reason: state.readOnlyReason })}</span> : null}
            </>
          ) : null
        }
        statusEnd={canvasView ? <ZoomBar zoom={zoom} onZoom={setZoomLevel} onFit={zoomTo} /> : null}
        panels={state ? <SidePanel info={state} current={current} onPage={goTo} /> : null}
      >
        {finding ? (
          <FrameChrome>
            <FindBar document={doc} revision={session.change.revision} inputRef={searchInput} initial={findSeed} control={findControl} onMatch={showMatch} onClose={closeSearch} />
          </FrameChrome>
        ) : null}
        <div className="pdf-workspace">
          <div ref={region} id={viewportId} className="pdf-pages" tabIndex={-1}>
            <KitDocumentCanvas canvas={canvas} viewMemory={viewMemory} initialArea={pageWidthArea(pages[0]?.area)} panBounds={panBounds} framed={false} onView={follow} tool={selection.tool}>
              {selectedMatch ? <SearchHighlights page={pages[selectedMatch.page - 1]?.area} pageNumber={selectedMatch.page} rects={selectedMatch.rects} view={canvasView} /> : null}
              <SelectionHighlights selected={selection.selected} pages={pages} view={canvasView} />
            </KitDocumentCanvas>
          </div>
          {canvasView && panBounds ? <PageScrollbars view={canvasView} bounds={panBounds} controls={viewportId} canvas={canvas} vertical={t('verticalScroll')} horizontal={t('horizontalScroll')} /> : null}
        </div>
      </EditorFrame>
    </KitEditorProvider>
  )
}

/** Scrollbars over the pages' bounds: the vertical one always (where the pages are in a long document), the horizontal one when the pages are wider than the view. */
function PageScrollbars({ view, bounds, controls, canvas, vertical, horizontal }: {
  view: KitCanvasView
  bounds: RasterRect
  controls: string
  canvas: RefObject<KitCanvasHandle | null>
  vertical: string
  horizontal: string
}) {
  const height = view.height / view.zoom
  const width = view.width / view.zoom
  const verticalMax = Math.max(0, bounds.y1 - bounds.y0 - height)
  const horizontalMax = Math.max(0, bounds.x1 - bounds.x0 - width)
  return (
    <>
      <Scrollbar label={vertical} controls={controls} value={view.y - bounds.y0} max={verticalMax} page={height} onChange={(value) => canvas.current?.panTo(view.x, bounds.y0 + value)} />
      {horizontalMax > 0.5 ? <Scrollbar label={horizontal} controls={controls} orientation="horizontal" value={view.x - bounds.x0} max={horizontalMax} page={width} onChange={(value) => canvas.current?.panTo(bounds.x0 + value, view.y)} /> : null}
    </>
  )
}
