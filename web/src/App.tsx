import { useCallback, useEffect, useId, useMemo, useRef, useState, type KeyboardEvent, type MouseEvent as ReactMouseEvent, type RefObject } from 'react'
import { Scrollbar } from '@kition/tessera/ui'
import {
  CommandTab,
  HAND_TOOL,
  KitDocumentCanvas,
  ToolRail,
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
  type KitCommand,
  type KitCanvasView,
  type KitPointer,
  type KitTool,
  type Phrases,
  type RasterRect,
  type RasterView,
} from '@kition/sdk/editor'
import { engineManifest } from './engine'
import { useEngineBackdrop } from './backdrop'
import { CATALOGS } from './strings'
import { TABS, commandDefaults, describe, pageWidthArea, pagesPanBounds, wholePageArea, type Where } from './pdf'
import { HomeTab, SelectTool } from './ribbon/HomeTab'
import { SidePanel } from './SidePanel'
import { commentRef, sameComment, type CommentItem } from './comments'
import { ParamsDialog, type Asking } from './ParamsDialog'
import { SelectionBar } from './SelectionBar'
import { matchRuns, selectionBounds, selectionRuns, type MarkupRun, type MarkupType } from './markup'
import { FindBar, type FindControl, type Match } from './FindBar'
import { SearchHighlights } from './SearchHighlights'
import { searchPanTarget } from './searchGeometry'
import { SelectionHighlights } from './SelectionHighlights'
import { useTextSelection } from './useTextSelection'
import { selectionPoint, type SelectionRequest, type SelectionResult } from './textSelection'
import { withRewriteLease } from './rewriteLease'
import { newlyDropped } from './droppedEdits'
import { focusPages, keyboardFallback, keyboardLost, returnKeyboardLater } from './focus'
import { isTextControl } from './textSelection'
import { KEY_LINE, WHEEL_NOTCH_ZOOM, clampZoom, ladderStep, screenStep, scrollView, wheelNotches, zoomAround } from './view'
import { ZoomBar, type Fit } from './ZoomBar'
import { NUDGE, NUDGE_FAR, asRect, clientToDocument, hitTest, inRect, keptOnPage, movable, pageAt, type PageComment, type PageLink, type Rect } from './page/geometry'
import { usePageObjects } from './page/usePageObjects'
import { usePageTool, type Dragging, type Hovering } from './page/usePageTool'
import { CommentTip, NoteCard, SelectionFrame } from './page/PageOverlays'
import { PageMenu, type PageMenuActions, type PageMenuTarget } from './page/PageMenu'
import { LinkDialog } from './page/LinkDialog'
import { FormBar, FormFields, fillableCount, type FormFieldsHandle } from './page/FormFields'
import { useDrawTool } from './page/useDrawTool'
import { TOOL_LABELS, ToolOptions, railTools, toolIcon } from './page/ToolOptions'
import { DEFAULT_STYLES, fitTextBox, isToolId, styleParams, toolDef, type ToolId, type ToolStyle } from './page/tools'

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
  const defaults = useMemo(() => commandDefaults({ current, click: null, selection: null }), [current])
  const editor = useKitEditorState(session, host, t, 'pdf', phrase, defaults)
  const { state } = session
  const { doc } = session
  // A command that rewrites the whole file takes the whole document's lease while it runs, and no
  // edit runs while someone else holds it (`rewriteLease.ts`).
  const leases = useMemo(() => kitLeases(host), [host])
  // The comment chosen in the Comments panel: the Comments tab's commands that name one act on it.
  const [chosen, setChosen] = useState<CommentItem | null>(null)
  const chosenRef = useRef(chosen)
  chosenRef.current = chosen
  // What a parameters dialog starts from: the current page, the last press on a page, the selection.
  const where = useRef<Where>({ current: 0, click: null, selection: null })
  // The editor's own parameters dialog (`ParamsDialog.tsx`), in place of the SDK's.
  const [asking, setAsking] = useState<(Asking & { key: number }) | null>(null)
  const askCount = useRef(0)
  const openDialog = useCallback((command: KitCommand, values: Record<string, unknown> = {}, fixed?: Record<string, unknown>) => {
    askCount.current += 1
    const start = commandDefaults(where.current)(command)
    if (fixed) for (const key of Object.keys(fixed)) delete start[key]
    setAsking({ command, values: { ...start, ...values }, fixed, key: askCount.current })
  }, [])
  /** Runs an edit under the whole-document lease it needs; a refusal toasts and answers null. */
  const runLeased = useCallback(async (id: string, edit: () => Promise<string | null>, onRefuse?: (holder: string) => void) => {
    const command = editor.byId.get(id)
    const scope = doc ? await doc.invoke<string>('leaseScope', id).catch(() => '') : ''
    const refuse = onRefuse ?? ((holder: string) => toast('info', t('rewriting', { name: holder || t('someone') })))
    return withRewriteLease(leases, scope, command?.mutates ?? true, edit, refuse)
  }, [doc, editor.byId, leases, t])
  const leased = useMemo(
    () => {
      /** The chosen comment for a command that names one and was given none (`id`, or `page` and `index`). */
      const target = (id: string, params?: Record<string, unknown>) => {
        const item = chosenRef.current
        return COMMENT_COMMANDS.has(id) && item && !(params && ('id' in params || 'index' in params)) ? commentRef(item) : null
      }
      const ask = (id: string, values?: Record<string, unknown>) => {
        const command = editor.byId.get(id)
        if (!command) return
        // A command the host or a read-only session refuses gets no dialog: running it says why.
        if (!editor.enabledByEngine(id)) {
          void editor.run(id, values ?? {})
          return
        }
        const fixed = target(id, values) ?? undefined
        openDialog(command, id === 'comment.edit' && fixed && !values ? { contents: chosenRef.current?.contents ?? '' } : values, fixed)
      }
      return {
        ...editor,
        dialog: null,
        setDialog: (command: KitCommand | null, values?: Record<string, unknown>) => (command ? openDialog(command, values) : setAsking(null)),
        ask,
        // Commands that act on a comment wait for one to be chosen in the Comments panel.
        enabled: (id: string) => editor.enabled(id) && (!COMMENT_COMMANDS.has(id) || chosenRef.current != null),
        run: async (id: string, params?: Record<string, unknown>) => {
          const command = editor.byId.get(id)
          const fixed = target(id, params)
          // Editing the chosen comment asks what to change, starting from its text.
          if (fixed && id === 'comment.edit' && !params) {
            ask(id)
            return null
          }
          const given = fixed ? { ...fixed, ...params } : params
          // A command that needs values a person gives opens its dialog first.
          if (command && needsParams(command) && (!given || (fixed && !hasRequired(command, given)))) {
            if (editor.enabledByEngine(id)) {
              openDialog(command, params ?? {}, fixed ?? undefined)
              return null
            }
          }
          try {
            return await runLeased(id, () => editor.run(id, given ?? {}))
          } finally {
            // A parameters dialog that ran (or failed) leaves the keyboard on the body: the pages take it.
            returnKeyboardLater()
          }
        },
      }
    },
    // `chosen`: the commands that act on a comment turn on and off with it.
    [editor, openDialog, runLeased, chosen],
  )
  /** The dialog's Run: the command runs under its lease; a failure stays in the dialog (it throws). */
  const runDialog = useCallback(async (command: KitCommand, params: Record<string, unknown>) => {
    let refused: string | null = null
    await runLeased(command.id, () => editor.session.run(command.id, params), (holder) => { refused = t('rewriting', { name: holder || t('someone') }) })
    if (refused) throw new Error(refused)
    setAsking(null)
  }, [editor.session, runLeased, t])
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
  // The tool on the pages (stage 5), session state like the view: the page layer (Select), the
  // hand, or a comment tool; each comment tool's look; whether a one-shot tool stays on.
  const [toolId, setToolId] = useState<ToolId>('select')
  const toolRef = useRef(toolId)
  toolRef.current = toolId
  const [styles, setStyles] = useState<Record<ToolId, ToolStyle>>(DEFAULT_STYLES)
  const stylesRef = useRef(styles)
  stylesRef.current = styles
  const [keepTool, setKeepTool] = useState(false)
  const keepRef = useRef(keepTool)
  keepRef.current = keepTool
  // The opacity and width of comments this session made or changed (comment.list answers neither).
  const knownStyles = useRef(new Map<string, { opacity?: number; width?: number }>())
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
    onStart: (at) => {
      // A press with the highlighter (or underline, strikethrough) keeps the find bar: the next match is a Cmd+G away.
      if (toolDef(toolRef.current).kind !== 'markup') {
        setFinding(false)
        setSearchMatch(null)
        searchOpener.current = null
      }
      setChosen(null)
      where.current.click = at
    },
  })
  // A parameters dialog starts from what is in front of the person (`commandDefaults`).
  const bounds = selectionBounds(selection.selected)
  where.current.current = current
  where.current.selection = bounds && selection.selected ? { ...bounds, text: selection.selected.text } : null
  // A new document starts with no comment chosen and no press remembered.
  useEffect(() => {
    setChosen(null)
    where.current.click = null
  }, [doc])
  // The chosen comment follows the document's changes (its text, its place); gone, nothing is chosen.
  useEffect(() => {
    const item = chosenRef.current
    if (!doc || !item) return undefined
    let live = true
    doc.execute('comment.list', {}).then((text) => {
      if (!live) return
      const found = ((JSON.parse(text) as { comments?: CommentItem[] }).comments ?? []).find((other) => sameComment(other, item))
      if (!found) setChosen(null)
      else if (JSON.stringify(found) !== JSON.stringify(item)) setChosen(found)
    }).catch(() => undefined)
    return () => { live = false }
  }, [doc, session.change.revision])

  /**
   * Marks the selection (each page of it), else the find bar's current match, as a highlight,
   * underline or strikethrough at once; answers whether there was anything to mark.
   */
  const mark = (type: MarkupType, contents?: string) => {
    const runs = selection.selected?.text ? selectionRuns(selection.selected) : matchRuns(selectedMatch)
    if (!runs.length) return false
    selection.clear()
    void markRuns(type, runs, contents)
    return true
  }
  /** One markup comment per run (a page each), in its tool's look; answers the last one's result. */
  const markRuns = async (type: MarkupType, runs: readonly MarkupRun[], contents?: string) => {
    let last: string | null = null
    for (const run of runs) {
      const done = await leased.run('comment.add', { type, page: run.page, quads: run.quads, ...(contents ? { contents } : {}), ...styleParams(type, stylesRef.current[type]) })
      if (done == null) break
      last = done
    }
    return last
  }
  /** Chooses a comment in the Comments panel and shows where it is on its page. */
  const chooseComment = (item: CommentItem | null) => {
    setChosen(item)
    if (item) goTo(item.page - 1, item.rect ?? null)
  }
  // The selection's bar shows once the drag is let go, until Copy puts it away.
  const [barHidden, setBarHidden] = useState<unknown>(null)
  const showBar = !!selection.selected?.text && !selection.dragging && barHidden !== selection.selected && toolId === 'select'

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
    else if (page && to === 'width') {
      // The page's width and a 24 pt margin each side fill the view's width, whatever its height;
      // the page's top (and its margin) at the view's top.
      const handle = canvas.current
      const view = handle?.view()
      if (!handle || !view || !(view.width > 0)) return
      handle.zoomTo(clampZoom(view.width / (page.x1 - page.x0 + 48)))
      handle.panTo(page.x0 - 24, page.y0 - 24)
    } else if (page) canvas.current?.show(wholePageArea(page)!)
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
  const dialogOpen = !!asking
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

  // The page layer (web/src/page, stage 4): what is on the pages, the pointer tool that sends a
  // press to it, the selected comment's frame, its tip, its menu, the card its text is written in.
  const { objects, lines } = usePageObjects(doc, session.change.revision)
  const [drag, setDrag] = useState<Dragging | null>(null)
  const [hover, setHover] = useState<Hovering | null>(null)
  const [menu, setMenu] = useState<PageMenuTarget | null>(null)
  const [card, setCard] = useState<{ kind: 'edit'; comment: PageComment } | { kind: 'new'; tool: PlaceTool; page: number; point: [number, number]; rect: Rect | null } | null>(null)
  const [linkAsk, setLinkAsk] = useState<string | null>(null)
  const [replyAsk, setReplyAsk] = useState(0)
  // A drag's frame stays where the release put it until the chosen comment's new place is read back.
  const chosenPlace = chosen?.rect?.join(',') ?? ''
  useEffect(() => setDrag(null), [chosenPlace, doc])
  useEffect(() => {
    if (!drag) return undefined
    const late = window.setTimeout(() => setDrag(null), 4000)
    return () => window.clearTimeout(late)
  }, [drag])
  // A new document starts with no menu, card or link question.
  useEffect(() => {
    setMenu(null)
    setCard(null)
    setLinkAsk(null)
    setHover(null)
    setToolId('select')
    knownStyles.current = new Map()
  }, [doc])
  /** Chooses a comment on the page: the Comments panel follows, the view stays. */
  const choosePageComment = useCallback((item: PageComment | null) => {
    const now = chosenRef.current
    if (item && now && sameComment(now, item)) return
    if (!item && !now) return
    chosenRef.current = item as CommentItem | null
    setChosen(item as CommentItem | null)
  }, [])
  /** One `comment.edit` for a gesture's end (a move or a resize); a refused or failed one puts the frame back. */
  const editComment = useCallback((item: PageComment, change: Record<string, unknown>) => {
    void leased.run('comment.edit', { ...commentRef(item), ...change }).then((done) => { if (done == null) setDrag(null) }, () => setDrag(null))
  }, [leased])
  const pageTool = usePageTool({
    pages,
    objects,
    lines,
    zoom: () => canvas.current?.view()?.zoom ?? 1,
    text: selection.tool,
    clearText: selection.clear,
    chosen,
    choose: choosePageComment,
    move: (item, dx, dy) => editComment(item, { move: [round2(dx), round2(dy)] }),
    resize: (item, rect) => editComment(item, { rect: rect.map(round2) }),
    follow: (link) => followLink(link),
    open: (item) => {
      choosePageComment(item)
      setCard({ kind: 'edit', comment: item })
    },
    onDrag: setDrag,
    onHover: (next) => setHover((now) => (now?.comment === next?.comment ? now : next)),
  })

  /** Selects the comment `comment.add` answered (its page and place), remembering the look it was made with. */
  const chooseMade = async (answer: string | null, tool: ToolId) => {
    let made: { page?: number; index?: number } | null = null
    try { made = (JSON.parse(answer ?? '') as { comment?: { page?: number; index?: number } }).comment ?? null } catch { made = null }
    if (!doc || !made?.page || !made.index) return
    const list = await doc.execute('comment.list', { page: made.page }).then((text) => JSON.parse(text) as { comments?: PageComment[] }).catch(() => null)
    const found = (list?.comments ?? []).find((item) => item.index === made.index)
    if (!found) return
    const style = stylesRef.current[tool]
    knownStyles.current.set(styleKey(found), { opacity: style.opacity, width: style.width })
    choosePageComment(found)
  }
  /** A one-shot tool that has made its comment goes back to Select, unless it is kept. */
  const finishedWith = (tool: ToolId) => {
    if (toolRef.current === tool && toolDef(tool).oneShot && !keepRef.current) setToolId('select')
  }
  /** Turns a tool on; a markup tool chosen with text selected marks it at once (the tool stays on). */
  const chooseTool = (id: ToolId) => {
    setHover(null)
    setMenu(null)
    if (toolDef(id).kind === 'markup' && selection.selected?.text) mark(id as MarkupType)
    setToolId(id)
    focusPages()
  }
  /** A markup tool's drag over text: what it went over is marked (each page), and the last one selected. */
  const markDrag = (tool: ToolId, start: KitPointer, end: KitPointer) => {
    if (Math.hypot(end.clientX - start.clientX, end.clientY - start.clientY) < 3) return
    const anchor = selectionPoint(pages, start.x, start.y)
    if (!anchor) return
    const column = start.alt
    const focus = column ? selectionPoint([pages[anchor.page - 1]], end.x, end.y, true) : selectionPoint(pages, end.x, end.y, true)
    if (!focus) return
    const request: SelectionRequest = { page: anchor.page, anchor: anchor.point, focus: focus.point, endPage: column ? anchor.page : focus.page, mode: column ? 'column' : 'range' }
    void (async () => {
      const result = await selectText(request).catch(() => null)
      selection.clear()
      const runs = selectionRuns(result)
      if (!runs.length) return
      const last = await markRuns(tool as MarkupType, runs)
      if (last) await chooseMade(last, tool)
    })()
  }
  /** A shape, a drawing or an area highlight: one `comment.add`, in the tool's look; a shape is selected once made. */
  const addDrawn = (tool: ToolId, page: number, geometry: Record<string, unknown>) => {
    const def = toolDef(tool)
    if (!def.type) return
    if (!def.selects) choosePageComment(null)
    void leased.run('comment.add', { type: def.type, page, ...geometry, ...styleParams(tool, stylesRef.current[tool]) }).then((answer) => {
      if (answer != null && def.selects) void chooseMade(answer, tool)
    })
  }
  const drawTool = useDrawTool({
    tool: toolId,
    pages,
    objects,
    lines,
    chosen,
    zoom: () => canvas.current?.view()?.zoom ?? 1,
    style: () => stylesRef.current[toolRef.current],
    pageTool,
    text: selection.tool,
    place: (tool, page, point, rect) => {
      if (tool === 'note' || tool === 'textbox' || tool === 'addText') setCard({ kind: 'new', tool, page, point, rect })
    },
    mark: markDrag,
    add: addDrawn,
  })
  const canvasTool: KitTool = toolId === 'select' ? pageTool : toolId === 'hand' ? HAND_TOOL : drawTool

  // Filling the form on the page (stage 6): a control over every field, the bar a document with
  // a form opens with, and whether the fields are highlighted (on at first, then as last set).
  const formFields = useRef<FormFieldsHandle>(null)
  const [highlightFields, setHighlightFields] = useState(true)
  const [formBarClosed, setFormBarClosed] = useState<unknown>(null)
  const fieldCount = fillableCount(objects.fields)
  const fillField = useCallback((name: string, value: unknown) => leased.run('form.fill', { values: { [name]: value } }), [leased])
  const canvasSurface = useCallback(() => region.current?.querySelector('[data-testid="kit-canvas-surface"]') ?? null, [])
  /** Brings a document rectangle into view when it is not all in it: a little below the view's top. */
  const reveal = useCallback((rect: Rect) => {
    const handle = canvas.current
    const view = handle?.view()
    if (!handle || !view || !(view.zoom > 0)) return
    const width = view.width / view.zoom
    const height = view.height / view.zoom
    const margin = 16 / view.zoom
    const across = rect[0] >= view.x + margin && rect[2] <= view.x + width - margin
    const down = rect[1] >= view.y + margin && rect[3] <= view.y + height - margin
    if (across && down) return
    handle.panTo(across ? view.x : rect[0] - width * 0.2, down ? view.y : rect[1] - height * 0.3)
  }, [])
  /** A text box's width for its text: Helvetica at its size, as the engine sets it. */
  const measureContext = useRef<CanvasRenderingContext2D | null>(null)
  const measureLine = (line: string) => {
    measureContext.current ??= document.createElement('canvas').getContext('2d')
    const context = measureContext.current
    if (!context) return line.length * 6
    context.font = '12px Helvetica, Arial, sans-serif'
    return context.measureText(line).width
  }
  // The options bar: a comment tool's look for what it makes next; with Select, the selected comment's.
  const barComment = toolId === 'select' ? chosen : null
  const restyle = (change: Partial<ToolStyle>) => {
    const def = toolDef(toolId)
    const item = chosen
    if (def.kind !== 'view') {
      setStyles((now) => ({ ...now, [toolId]: { ...now[toolId], ...change } }))
      // A colour also goes to the selected comment of the kind this tool makes.
      if (change.color && item && !item.locked && item.type === def.subtype && editor.enabled('comment.edit')) void leased.run('comment.edit', { ...commentRef(item), color: change.color })
    } else if (item && !item.locked) {
      const key = styleKey(item)
      void leased.run('comment.edit', { ...commentRef(item), ...change }).then((done) => {
        if (done != null && (change.opacity != null || change.width != null)) knownStyles.current.set(key, { ...knownStyles.current.get(key), ...(change.opacity != null ? { opacity: change.opacity } : {}), ...(change.width != null ? { width: change.width } : {}) })
      })
    }
    focusPages()
  }
  const rail = useMemo(() => railTools(t), [t])

  // Views a link left, for Alt+Left (Cmd+[) to go back to and Alt+Right (Cmd+]) forward again.
  const history = useRef<{ back: Array<{ x: number; y: number }>; forward: Array<{ x: number; y: number }> }>({ back: [], forward: [] })
  useEffect(() => { history.current = { back: [], forward: [] } }, [doc])
  const followLink = (link: PageLink) => {
    if (typeof link.to_page === 'number') {
      const view = canvas.current?.view()
      if (view) {
        history.current.back = [...history.current.back.slice(-199), { x: view.x, y: view.y }]
        history.current.forward = []
      }
      goTo(link.to_page - 1)
    } else if (link.url) setLinkAsk(link.url)
    else toast('info', t('linkOther', { kind: link.other ?? '?' }))
  }
  const stepHistory = (by: -1 | 1) => {
    const view = canvas.current?.view()
    const { back, forward } = history.current
    const to = by < 0 ? back.at(-1) : forward.at(-1)
    if (!view || !to) return
    if (by < 0) history.current = { back: back.slice(0, -1), forward: [...forward, { x: view.x, y: view.y }] }
    else history.current = { back: [...back, { x: view.x, y: view.y }], forward: forward.slice(0, -1) }
    canvas.current?.panTo(to.x, to.y)
  }
  /** A web address goes to the host (it opens the browser); a host that opens none says so. */
  const openUrl = (url: string) => {
    setLinkAsk(null)
    focusPages()
    if (!/^(https?:|mailto:)/i.test(url)) {
      toast('info', t('linkRefused', { url }))
      return
    }
    if (!host.init.capabilities?.includes('open-url')) {
      toast('info', t('linkNoHost', { url }))
      return
    }
    void host.peer.request('host/openUrl', { url }).catch(() => toast('error', t('linkOpenFailed')))
  }

  /** The right-click on the pages: a comment's menu (it is chosen), the selection's, or the page's. */
  const contextMenu = (event: ReactMouseEvent<HTMLDivElement>) => {
    const target = event.target instanceof Element ? event.target : null
    // A field's own menu stays the browser's (the note card's text, the selection bar's).
    if (!target || isTextControl(target) || target.closest('.pdf-selection-bar, .pdf-note-card')) return
    event.preventDefault()
    const surface = region.current?.querySelector('[data-testid="kit-canvas-surface"]')
    const view = canvas.current?.view()
    if (!surface || !view) return
    const [x, y] = clientToDocument(event.clientX, event.clientY, surface.getBoundingClientRect(), view)
    const at = pageAt(pages, x, y)
    setHover(null)
    if (!at) {
      setMenu(null)
      return
    }
    const point = { x: event.clientX, y: event.clientY }
    const hit = hitTest(objects, chosen, at.page, at.point, view.zoom)
    if (hit.kind === 'comment' || hit.kind === 'markup' || hit.kind === 'handle') {
      choosePageComment(hit.comment)
      setMenu({ kind: 'comment', at: point, comment: hit.comment })
      return
    }
    const onSelection = selection.selected?.pages.some((item) => item.page === at.page && item.rects.some((rect) => {
      const r = asRect(rect)
      return !!r && inRect(r, at.point[0], at.point[1], 2)
    }))
    setMenu(onSelection ? { kind: 'selection', at: point } : { kind: 'page', at: point, page: at.page, point: at.point })
  }
  const menuActions: PageMenuActions = {
    copy: () => selection.copy(),
    mark: (type) => void mark(type),
    findSelection: () => openFind(),
    editText: (item) => setCard({ kind: 'edit', comment: item }),
    reply: (item) => {
      choosePageComment(item)
      setReplyAsk((n) => n + 1)
    },
    setStatus: (item) => {
      choosePageComment(item)
      leased.ask('comment.setStatus')
    },
    color: (item, color) => void leased.run('comment.edit', { ...commentRef(item), color }),
    copyText: (item) => {
      try {
        void navigator.clipboard.writeText(item.contents ?? '').then(() => toast('success', t('copied')), () => toast('error', t('commandFailed')))
      } catch { toast('error', t('commandFailed')) }
    },
    remove: (item) => {
      choosePageComment(null)
      void leased.run('comment.delete', commentRef(item))
    },
    properties: (item) => {
      choosePageComment(item)
      leased.ask('comment.edit')
    },
    addNote: (page, point) => setCard({ kind: 'new', tool: 'note', page, point, rect: null }),
    addBookmark: (page) => leased.ask('bookmark.add', { page }),
    rotatePage: (page) => void leased.run('page.rotate', { pages: [page], degrees: 90 }),
    deletePage: (page) => void leased.run('page.delete', { pages: [page] }),
    fit: (to) => zoomTo(to),
    find: () => openFind(),
  }
  /** The card's Post: a comment's new text, or a new sticky note where the menu was asked for. */
  const postCard = (text: string) => {
    const open = card
    setCard(null)
    focusPages()
    if (!open) return
    if (open.kind === 'edit') {
      if (text !== (open.comment.contents ?? '')) void leased.run('comment.edit', { ...commentRef(open.comment), contents: text })
      return
    }
    const { tool, page, point } = open
    const style = styleParams(tool, stylesRef.current[tool])
    if (tool === 'note') {
      void leased.run('comment.add', { type: 'note', page, at: point.map(round2), contents: text, ...style }).then((answer) => { if (answer != null) void chooseMade(answer, tool) })
    } else if (tool === 'textbox') {
      // A text box without text is nothing.
      if (!text.trim()) return
      const area = pages[page - 1]?.area
      const rect = open.rect ?? fitTextBox(text, point, measureLine, area ? area.x1 - area.x0 : Infinity, area ? area.y1 - area.y0 : Infinity)
      void leased.run('comment.add', { type: 'textbox', page, rect, contents: text, ...style }).then((answer) => { if (answer != null) void chooseMade(answer, tool) })
    } else {
      if (!text.trim()) return
      void leased.run('page.addText', { page, at: point.map(round2), text, ...(style.color ? { color: style.color } : {}) })
    }
    finishedWith(tool)
  }
  const cardPlace: { page: number; rect: Rect } | null = card
    ? card.kind === 'edit'
      ? (() => { const rect = asRect(card.comment.rect); return rect ? { page: card.comment.page, rect } : null })()
      : { page: card.page, rect: card.rect ?? [card.point[0], card.point[1], card.point[0] + 20, card.point[1] + 20] }
    : null
  const cardTitle = !card ? '' : card.kind === 'edit' ? t('editComment') : card.tool === 'textbox' ? t('textBox') : card.tool === 'addText' ? t('pageText') : t('stickyNote')
  const canvasBox = hover ? region.current?.querySelector('.kui-canvas')?.getBoundingClientRect() ?? null : null

  /** Opens the find bar (Cmd+F, a menu's Find), with the selected text as what it looks for. */
  const openFind = () => {
    const picked = (selection.selected?.text ?? '').replace(/\s+/g, ' ').trim()
    const seed = picked.length > 0 && picked.length <= 200 ? picked : ''
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
  }

  const keyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const command = event.metaKey || event.ctrlKey
    const key = event.key.length === 1 ? event.key.toLowerCase() : event.key
    const plain = !command && !event.altKey && !event.shiftKey
    if (command && !event.altKey && key === 'f') {
      // Text selected on the page is what the find bar looks for.
      openFind()
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
      // Then the comment selected on the page (or in the panel), then a comment tool (back to
      // Select), then the text selection.
      if (chosen) {
        choosePageComment(null)
        return true
      }
      if (toolId !== 'select' && toolId !== 'hand') {
        chooseTool('select')
        return true
      }
      selection.clear()
      return !!selection.selected
    }
    // Tab from the pages goes to the form's first field (Shift+Tab its last) from the current page on.
    if (key === 'Tab' && !command && !event.altKey && toolId === 'select' && event.target instanceof Element && (event.target === region.current || event.target.classList.contains('kui-app'))) {
      if (formFields.current?.enter(event.shiftKey, current + 1)) return true
    }
    // V and H: Select and the hand (the only tools with a key of their own).
    if (plain && (key === 'v' || key === 'h') && !isTextControl(event.target) && !(event.target instanceof Element && event.target.closest('[role="menu"], [role="listbox"], [role="dialog"]'))) {
      chooseTool(key === 'v' ? 'select' : 'hand')
      return true
    }
    // Undo and Redo right after an edit: the list of what can run may not have caught up with the
    // edit yet (the SDK's shortcut then does nothing), so the engine is asked, as the SDK does for
    // its keys without Cmd. When the list already allows it, the SDK's own shortcut runs it.
    if (command && !event.altKey && key === 'z') {
      const id = event.shiftKey ? 'edit.redo' : 'edit.undo'
      if (editor.enabled(id) || !editor.enabledByEngine(id) || !doc) return false
      const open = doc
      void open.disabledCommands().then((off) => {
        if (!off.includes(id)) void leased.run(id, {})
      }).catch(() => undefined)
      return true
    }
    // Copy with the keyboard off the pages (a button, the frame): some engines (WebKit on Linux)
    // send no copy event there without a page selection, so the editor asks for one itself.
    if (command && !event.altKey && !event.shiftKey && key === 'c' && selection.selected?.text && !(event.target instanceof Node && region.current?.contains(event.target))) {
      if (!document.execCommand('copy')) selection.copy()
      return true
    }
    // Delete takes the comment chosen in the Comments panel away, wherever the keyboard is but in
    // a field, a list or a menu of its own (the thumbnails); not while a comment tool is on.
    const viewing = toolDef(toolId).kind === 'view'
    if (plain && viewing && (key === 'Delete' || key === 'Backspace') && chosen && event.target instanceof Element && !isTextControl(event.target) && !event.target.closest('.pdf-thumbs, [role="menu"], [role="listbox"], [role="dialog"]')) {
      void leased.run('comment.delete', commentRef(chosen))
      setChosen(null)
      return true
    }
    // The arrow keys move the selected comment a point (Shift or Cmd: ten) while the pages (or
    // nothing) hold the keyboard; each press is one undo step.
    const arrow = key === 'ArrowLeft' ? [-1, 0] : key === 'ArrowRight' ? [1, 0] : key === 'ArrowUp' ? [0, -1] : key === 'ArrowDown' ? [0, 1] : null
    const onPages = event.target instanceof Element && (event.target === document.body || event.target.classList.contains('kui-app') || !!region.current?.contains(event.target)) && !isTextControl(event.target)
    if (arrow && chosen && viewing && !event.altKey && onPages && movable(chosen)) {
      const rect = asRect(chosen.rect)
      const area = pages[chosen.page - 1]?.area
      if (rect && area && editor.enabled('comment.edit')) {
        const step = event.shiftKey || command ? NUDGE_FAR : NUDGE
        const [dx, dy] = keptOnPage(rect, arrow[0] * step, arrow[1] * step, area.x1 - area.x0, area.y1 - area.y0)
        if (dx || dy) editComment(chosen, { move: [dx, dy] })
      }
      return true
    }
    // Back and forward through the views links left (Alt+Left / Alt+Right, Cmd+[ / Cmd+]).
    if ((event.altKey && !command && !event.shiftKey && (key === 'ArrowLeft' || key === 'ArrowRight')) || (command && !event.altKey && !event.shiftKey && (key === '[' || key === ']'))) {
      if (isTextControl(event.target)) return false
      stepHistory(key === 'ArrowLeft' || key === '[' ? -1 : 1)
      return true
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
        state ? <HomeTab info={state} current={current} onPage={goTo} onZoom={zoomTo} selectionAvailable={!!selection.selected?.text} onCopySelection={selection.copy} onMark={mark} tool={toolId} onTool={chooseTool} /> : null
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
        focusTool={toolId === 'select' ? selectTool : { label: t(TOOL_LABELS[toolId]), icon: toolIcon(toolId) }}
        rail={state ? <ToolRail tools={rail} active={toolId} onSelect={(id) => { if (isToolId(id)) chooseTool(id) }} /> : null}
        options={state ? (
          <ToolOptions
            tool={toolId}
            style={styles[toolId]}
            comment={barComment}
            known={barComment ? knownStyles.current.get(styleKey(barComment)) ?? null : null}
            keep={keepTool}
            onColor={(color) => restyle({ color })}
            onOpacity={(opacity) => restyle({ opacity })}
            onWidth={(width) => restyle({ width })}
            onKeep={setKeepTool}
          />
        ) : null}
        status={
          state ? (
            <>
              <span data-testid="pdf-status">{t('pageOf', { page: current + 1, pages: pages.length })}</span>
              {!state.editable && state.readOnlyReason ? <span>{t('readOnly', { reason: state.readOnlyReason })}</span> : null}
            </>
          ) : null
        }
        statusEnd={canvasView ? <ZoomBar zoom={zoom} onZoom={setZoomLevel} onFit={zoomTo} /> : null}
        panels={state ? <SidePanel info={state} current={current} onPage={goTo} chosen={chosen} onChoose={chooseComment} reply={replyAsk} /> : null}
      >
        {finding ? (
          <FrameChrome>
            <FindBar document={doc} revision={session.change.revision} inputRef={searchInput} initial={findSeed} control={findControl} onMatch={showMatch} onClose={closeSearch} />
          </FrameChrome>
        ) : null}
        <div className="pdf-workspace">
          <div ref={region} id={viewportId} className="pdf-pages" tabIndex={-1} onContextMenu={contextMenu}>
            <KitDocumentCanvas canvas={canvas} viewMemory={viewMemory} initialArea={pageWidthArea(pages[0]?.area)} panBounds={panBounds} framed={false} onView={follow} tool={canvasTool}>
              {selectedMatch ? <SearchHighlights page={pages[selectedMatch.page - 1]?.area} pageNumber={selectedMatch.page} rects={selectedMatch.rects} view={canvasView} /> : null}
              <SelectionHighlights selected={selection.selected} pages={pages} view={canvasView} />
              <FormFields
                fields={objects.fields}
                pages={pages}
                view={canvasView}
                active={toolId === 'select'}
                highlight={highlightFields}
                canFill={leased.enabled('form.fill')}
                fill={fillField}
                onEnter={() => {
                  selection.clear()
                  choosePageComment(null)
                  setMenu(null)
                }}
                onReadOnly={(field) => toast('info', t('fieldReadOnly', { name: field.tooltip || field.name }))}
                reveal={reveal}
                surface={canvasSurface}
                handle={formFields}
              />
              {chosen?.rect ? <SelectionFrame comment={chosen} area={pages[chosen.page - 1]?.area} view={canvasView} drag={drag} /> : null}
              {hover && !drag && !menu && !card ? <CommentTip hover={hover} box={canvasBox} /> : null}
              {card && cardPlace ? (
                <NoteCard
                  key={card.kind === 'edit' ? `edit-${card.comment.id ?? card.comment.index}` : `new-${card.tool}-${card.page}-${card.point.join(',')}`}
                  area={pages[cardPlace.page - 1]?.area}
                  rect={cardPlace.rect}
                  view={canvasView}
                  title={cardTitle}
                  initial={card.kind === 'edit' ? card.comment.contents ?? '' : ''}
                  onPost={postCard}
                  onCancel={() => { setCard(null); focusPages() }}
                />
              ) : null}
              {showBar && selection.selected ? (
                <SelectionBar
                  key={JSON.stringify(selection.selected.pages.map((item) => item.rects))}
                  selected={selection.selected}
                  pages={pages}
                  view={canvasView}
                  onMark={(type, contents) => void mark(type, contents)}
                  onCopy={() => {
                    selection.copy()
                    setBarHidden(selection.selected)
                    focusPages()
                  }}
                />
              ) : null}
            </KitDocumentCanvas>
          </div>
          {fieldCount > 0 && formBarClosed !== doc ? (
            <FormBar count={fieldCount} highlight={highlightFields} onHighlight={setHighlightFields} onClose={() => setFormBarClosed(doc)} />
          ) : null}
          <PageMenu target={menu} onClose={() => setMenu(null)} actions={menuActions} hasSelection={!!selection.selected?.text} pageCount={pages.length} />
          {linkAsk ? <LinkDialog url={linkAsk} onOpen={() => openUrl(linkAsk)} onClose={() => { setLinkAsk(null); focusPages() }} /> : null}
          {asking ? <ParamsDialog key={asking.key} asking={asking} onRun={(params) => runDialog(asking.command, params)} onClose={() => setAsking(null)} /> : null}
          {canvasView && panBounds ? <PageScrollbars view={canvasView} bounds={panBounds} controls={viewportId} canvas={canvas} vertical={t('verticalScroll')} horizontal={t('horizontalScroll')} /> : null}
        </div>
      </EditorFrame>
    </KitEditorProvider>
  )
}

/** A point value as a command takes it: to 1/100 pt. */
const round2 = (value: number) => Math.round(value * 100) / 100

/** The tools that open a card where they are clicked. */
type PlaceTool = 'note' | 'textbox' | 'addText'

/** What the editor remembers a comment's look under: its id, else its page and place. */
const styleKey = (item: Pick<PageComment, 'id' | 'page' | 'index'>) => item.id ?? `${item.page}/${item.index}`

/** The Comments tab's commands that act on one comment (`id`, or `page` and `index`). */
const COMMENT_COMMANDS = new Set(['comment.delete', 'comment.edit', 'comment.reply', 'comment.setStatus', 'comment.mark', 'comment.lock', 'comment.previewImage'])

/** Whether `params` gives every value `command` requires. */
function hasRequired(command: KitCommand, params: Record<string, unknown>) {
  return Object.entries((command.schema ?? {}) as Record<string, { required?: boolean }>).every(([key, node]) => !node.required || params[key] !== undefined)
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
