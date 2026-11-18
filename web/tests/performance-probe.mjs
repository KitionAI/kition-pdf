/** Installed before any page script. Holds metadata and WeakMaps, never pixel copies. */
export function installPerformanceProbe() {
  const now = () => performance.timeOrigin + performance.now()
  const buffers = new WeakMap(), bitmaps = new WeakMap(), visible = new WeakMap()
  let requestId = 0
  const probe = window.__kitionPerf = {
    schema: 1, pages: [], requests: [], requestCount: 0, counters: { renderBuffersMapped: 0, bitmapWritesMapped: 0, visibleDrawsMapped: 0 }, goal: { id: 'cold-open-0', page: 1, phase: 'cold-open', status: 'pending', started: null },
    revision: null, dirty: null,
    arm(sample) { this.goal = { ...sample, status: 'pending', started: null, revisionBefore: this.revision } },
    start() { if (this.goal?.status === 'pending' && this.goal.started === null) this.goal.started = now() },
    resources() {
      const canvases = [...document.querySelectorAll('canvas')]
      const list = document.querySelector('[data-testid="pdf-thumbs"]')
      return { at: now(), domCanvasCount: canvases.length, domCanvasRgbaEstimate: canvases.reduce((sum, canvas) => sum + canvas.width * canvas.height * 4, 0), thumbnailRetainedRgbaBytes: list ? Number(list.dataset.cacheBytes) : 0, thumbnailCacheEntries: list ? Number(list.dataset.cacheCount) : 0, revision: this.revision, dirty: this.dirty }
    },
  }
  document.addEventListener('change', event => { if (event.target?.id === 'file') probe.openStarted = now() }, true)
  document.addEventListener('keydown', event => { if (event.key === 'Enter' && event.target?.dataset?.testid === 'pdf-page-field') probe.start() }, true)
  document.addEventListener('click', event => { if (event.target?.closest('[data-testid^="pdf-fit-"], [data-testid="pdf-actual-size"]')) probe.start() }, true)
  const overlaps = (request, goal) => {
    const area = probe.pages[goal.page - 1]?.area
    if (!area) return false
    const { x0, y0, x1, y1 } = request.area
    return x1 > area.x && x0 < area.x + area.width && y1 > area.y && y0 < area.y + area.height
  }
  const belongs = request => {
    const goal = probe.goal
    return goal?.status === 'pending' && goal.started !== null && request.started >= goal.started && overlaps(request, goal)
  }
  const OriginalWorker = window.Worker
  window.Worker = class extends OriginalWorker {
    constructor(...args) {
      super(...args)
      const pending = new Map()
      const send = this.postMessage.bind(this)
      this.postMessage = (message, ...rest) => {
        const request = message?.request
        if (request?.method === 'open') {
          probe.engineModuleUrl = new URL(`${request.manifest.module}.js`, request.manifest.base).href
          try { probe.goal.started ??= parent.__kitionPerf?.openStarted ?? now() } catch { probe.goal.started ??= now() }
          pending.set(message.id, { type: 'open' })
        } else if (['execute', 'tool', 'replicaImport', 'modelBatch'].includes(request?.method)) {
          pending.set(message.id, { type: 'state' })
        } else if (request?.method === 'render' || request?.method === 'renderArea') {
          const { method, width, height } = request
          const area = method === 'renderArea' ? { x0: request.x0, y0: request.y0, x1: request.x1, y1: request.y1 } : { x0: request.x, y0: request.y, x1: request.x + width / request.scale, y1: request.y + height / request.scale }
          const record = { id: ++requestId, method, area, width, height, scale: request.scale ?? null, started: now(), status: 'pending', revision: probe.revision }
          pending.set(message.id, { type: 'render', record })
          probe.requests.push(record)
          probe.requestCount++
          if (probe.requests.length > 256) probe.requests.shift()
        }
        return send(message, ...rest)
      }
      this.addEventListener('message', event => {
        const item = pending.get(event.data?.id)
        if (!item) return
        pending.delete(event.data.id)
        const { ok, value } = event.data
        if (item.type !== 'render') {
          const info = item.type === 'open' ? value : value?.info
          if (ok && info) {
            if (Array.isArray(info.inspected?.pages)) probe.pages = info.inspected.pages.map(page => ({ area: { ...page.area } }))
            probe.revision = info.revision; probe.dirty = info.dirty
          }
          return
        }
        const record = item.record
        record.completed = now(); record.status = ok ? 'passed' : 'failed'
        if (ok && value instanceof ArrayBuffer) { buffers.set(value, record); probe.counters.renderBuffersMapped++ }
        else if (!ok) {
          record.error = String(event.data.error)
          if (belongs(record) && (record.width > 200 || record.height > 200)) Object.assign(probe.goal, { status: 'failed', error: record.error, request: record })
        }
      })
      this.addEventListener('error', () => pending.clear())
      const terminate = this.terminate.bind(this)
      this.terminate = () => { pending.clear(); return terminate() }
    }
  }
  const put = CanvasRenderingContext2D.prototype.putImageData
  CanvasRenderingContext2D.prototype.putImageData = function (data, ...args) {
    const result = put.call(this, data, ...args)
    const record = buffers.get(data.data.buffer)
    if (record) { record.bitmapWritten = now(); bitmaps.set(this.canvas, record); probe.counters.bitmapWritesMapped++ }
    return result
  }
  const draw = CanvasRenderingContext2D.prototype.drawImage
  CanvasRenderingContext2D.prototype.drawImage = function (source, ...args) {
    const result = draw.call(this, source, ...args)
    if (this.canvas.dataset?.testid !== 'kit-canvas-surface') return result
    const record = bitmaps.get(source)
    if (!record) return result
    probe.counters.visibleDrawsMapped++
    visible.set(this.canvas, record.id)
    if (!belongs(record)) return result
    const canvas = this.canvas, goal = probe.goal, drawn = now()
    requestAnimationFrame(() => requestAnimationFrame(() => {
      if (probe.goal !== goal || goal.status !== 'pending' || visible.get(canvas) !== record.id || !canvas.isConnected) return
      const status = document.querySelector('[data-testid="pdf-status"]')?.textContent ?? ''
      if (!status.startsWith(`Page ${goal.page} of `)) return
      if (goal.action === 'edit' && (!(record.revision > goal.revisionBefore) || probe.dirty !== true)) return
      const presented = now()
      Object.assign(goal, { status: 'passed', request: { ...record }, revisionAfter: probe.revision, drawn, presented, presentMs: presented - goal.started, endToFrameOpportunityMs: presented - goal.started, drawToRafMs: presented - drawn, renderMs: record.completed - record.started, actionToRequestMs: record.started - goal.started, actionToDrawMs: drawn - goal.started })
    }))
    return result
  }
}
