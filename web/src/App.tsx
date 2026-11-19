import { useCallback, useId, useMemo, useRef, useState, type KeyboardEvent, type RefObject } from 'react'
import { Scrollbar } from '@kition/tessera-ui'
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
  useConflictReport,
  useKitEditorState,
  useKitSession,
  type FrameTab,
  type HostConnection,
  type KitCanvasHandle,
  type KitCanvasView,
  type Phrases,
  type RasterRect,
  type RasterView,
} from '@kition/kit-editor'
import { engineManifest } from './engine'
import { useEngineBackdrop } from './backdrop'
import { CATALOGS } from './strings'
import { TABS, commandDefaults, describe, pageWidthArea, pagesPanBounds, wholePageArea } from './pdf'
import { HomeTab, SelectTool } from './ribbon/HomeTab'
import { SidePanel } from './SidePanel'
import { FindBar, type Match } from './FindBar'
import { SearchHighlights } from './SearchHighlights'
import { searchPanTarget } from './searchGeometry'
import { SelectionHighlights } from './SelectionHighlights'
import { useTextSelection } from './useTextSelection'
import type { SelectionRequest, SelectionResult } from './textSelection'
import { withRewriteLease } from './rewriteLease'

export function App({ host, phrase }: { host: HostConnection; phrase: Phrases }) {
  const t = useMemo(() => createKitStrings(CATALOGS, host.init.locale), [host])
  const manifest = useMemo(engineManifest, [])
  // An agent step that rewrites the whole file takes the whole document's lease too (UI-L2).
  const session = useKitSession(host, manifest, describe, {
    agentLeaseScope: (command, _params, engine) => engine.invoke<string>('leaseScope', command),
  })
  const [current, setCurrent] = useState(0)
  const defaults = useMemo(() => commandDefaults(current), [current])
  const editor = useKitEditorState(session, host, t, 'pdf', phrase, defaults)
  const { state } = session
  // Collaborators' edits a full rewrite could not keep are conflict objects: the host is told of them.
  const { doc } = session
  const readConflicts = useMemo(() => (doc ? () => doc.invoke<string>('kdmConflicts') : null), [doc])
  useConflictReport(host, readConflicts, session.change.revision)
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
        return withRewriteLease(leases, scope, command?.mutates ?? true, () => editor.run(id, params), refuse)
      },
    }),
    [doc, editor, leases, t],
  )
  // The one tool, text selection, named in the Focus bar (the editor has no engine tools).
  const selectTool = useMemo(() => ({ label: phrase('Select text'), icon: SelectTool }), [phrase])
  const canvas = useRef<KitCanvasHandle>(null)
  useEngineBackdrop(doc, canvas)
  const viewMemory = useRef<RasterView | null>(null)
  const region = useRef<HTMLDivElement>(null)
  const searchOpener = useRef<HTMLElement | null>(null)
  const searchInput = useRef<HTMLInputElement>(null)
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
  const selectText = useCallback(async (request: SelectionRequest) => doc ? JSON.parse(await doc.execute('text_select', request)) as SelectionResult : null, [doc])
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

  const showMatch = useCallback((match: Match | null) => {
    if (match) selection.clear()
    setSearchMatch(match ? { doc, revision: session.change.revision, match } : null)
    if (match) goTo(match.page - 1, match.rects[0] ?? null)
  }, [doc, session.change.revision, goTo, selection.clear])

  const zoomTo = (to: 'width' | 'page' | 'actual') => {
    const page = pages[current]?.area
    if (to === 'actual') canvas.current?.zoomTo(1)
    else if (page) canvas.current?.show((to === 'width' ? pageWidthArea(page) : wholePageArea(page))!)
  }

  const keyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const command = event.metaKey || event.ctrlKey
    if (command && event.key.toLowerCase() === 'f') {
      selection.clear()
      if (!finding) searchOpener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null
      else searchInput.current?.focus({ preventScroll: true })
      setFinding(true)
      return true
    }
    if (event.key === 'Escape') { selection.clear(); return !!selection.selected }
    if (!command && (event.key === 'PageDown' || event.key === 'PageUp')) {
      goTo(current + (event.key === 'PageDown' ? 1 : -1))
      return true
    }
    return false
  }

  const closeSearch = () => {
    setSearchMatch(null)
    setFinding(false)
    const opener = searchOpener.current
    const target = opener?.isConnected ? opener : region.current
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
        zoom={zoom}
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
        panels={state ? <SidePanel info={state} current={current} onPage={goTo} /> : null}
      >
        {finding ? (
          <FrameChrome>
            <FindBar document={doc} revision={session.change.revision} inputRef={searchInput} onMatch={showMatch} onClose={closeSearch} />
          </FrameChrome>
        ) : null}
        <div className="pdf-workspace">
          <div ref={region} id={viewportId} className="pdf-pages" tabIndex={-1} onCopy={selection.onCopy}>
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
