/** Observe the real editor. Delay only selected synthetic search replies after worker completion. */
export async function installSearchProbe() {
  const moduleUrl = (file) => performance.getEntriesByType('resource').map(entry => entry.name).find(name => new RegExp(`/engine/${file}\\.ts(?:\\?|$)`).test(name))
  const engineUrl = moduleUrl('kitEngine'), viewportUrl = moduleUrl('rasterViewport')
  if (!engineUrl || !viewportUrl) throw new Error('Live editor modules are unavailable')
  const [{ KitEngine }, { RasterViewport }] = await Promise.all([import(engineUrl), import(viewportUrl)])
  const probe = window.__pdfSearch = {
    calls: [], held: [], holdQuery: null, viewport: null, engine: null,
    release() { const held = this.held.splice(0); this.holdQuery = null; for (const resolve of held) resolve() },
  }
  const execute = KitEngine.prototype.execute
  KitEngine.prototype.execute = async function (command, params = {}) {
    probe.engine = this
    if (command !== 'text_find') return execute.call(this, command, params)
    const call = { query: params.query, started: performance.now(), status: 'pending' }
    probe.calls.push(call)
    try {
      const result = await execute.call(this, command, params)
      call.workerCompleted = performance.now()
      if (probe.holdQuery === params.query) await new Promise(resolve => probe.held.push(resolve))
      call.status = 'returned'
      return result
    } catch (error) { call.status = 'failed'; throw error }
  }
  const zoom = RasterViewport.prototype.zoomTo
  RasterViewport.prototype.zoomTo = function (...args) {
    probe.viewport = this
    return zoom.apply(this, args)
  }
}

export function searchSnapshot() {
  const probe = window.__pdfSearch
  const overlay = document.querySelector('[data-testid="pdf-search-highlights"]')
  const canvas = document.querySelector('[data-testid="kit-canvas-surface"]')
  const surface = canvas.getBoundingClientRect()
  const bounds = overlay?.getBoundingClientRect()
  return {
    view: probe.viewport ? { ...probe.viewport.view } : null,
    pages: probe.engine?.info.inspected.pages.map(page => page.area),
    dpr: devicePixelRatio,
    count: document.querySelector('[data-testid="pdf-find-count"]')?.textContent ?? null,
    page: overlay ? Number(overlay.dataset.page) : null,
    overlay: bounds ? { left: bounds.left - surface.left, top: bounds.top - surface.top, width: bounds.width, height: bounds.height, pointerEvents: getComputedStyle(overlay).pointerEvents, overflow: getComputedStyle(overlay).overflow, ariaHidden: overlay.getAttribute('aria-hidden') } : null,
    canvas: { width: surface.width, height: surface.height, backingWidth: canvas.width, backingHeight: canvas.height },
    rects: [...document.querySelectorAll('[data-testid="pdf-search-highlight"]')].map(element => {
      const rect = element.getBoundingClientRect()
      return { left: rect.left - surface.left, top: rect.top - surface.top, width: rect.width, height: rect.height }
    }),
    calls: probe.calls.map(call => ({ ...call })),
  }
}

/** Poll authored rectangle centers in the actual canvas, independently of the overlay DOM. */
export function searchInkSamples({ page, rects, zoom, dpr }) {
  const probe = window.__pdfSearch
  const canvas = document.querySelector('[data-testid="kit-canvas-surface"]')
  const area = probe?.engine?.info.inspected.pages[page - 1]?.area
  const view = probe?.viewport?.view
  if (!canvas || !area || !view || Math.abs(view.zoom - zoom) > 1e-8) return null
  const bounds = canvas.getBoundingClientRect()
  if (Math.abs(canvas.width - Math.round(bounds.width * dpr)) > 1 || Math.abs(canvas.height - Math.round(bounds.height * dpr)) > 1) return null
  const context = canvas.getContext('2d')
  if (!context) return null
  const samples = rects.flatMap(([x0, y0, x1, y1], rectangle) => {
    const x = (area.x + (x0 + x1) / 2 - view.x) * zoom
    const y = (area.y + (y0 + y1) / 2 - view.y) * zoom
    if (x < 0 || y < 0 || x >= bounds.width || y >= bounds.height) return []
    const pixelX = Math.min(canvas.width - 1, Math.floor(x * canvas.width / bounds.width))
    const pixelY = Math.min(canvas.height - 1, Math.floor(y * canvas.height / bounds.height))
    const rgba = [...context.getImageData(pixelX, pixelY, 1, 1).data]
    return [{ rectangle, css: [x, y], pixel: [pixelX, pixelY], rgba }]
  })
  if (!samples.length || samples.some(({ rgba }) => rgba[3] !== 255 || rgba.slice(0, 3).some(channel => channel > 64))) return null
  return { backing: [canvas.width, canvas.height], cssExtent: [bounds.width, bounds.height], samples }
}
