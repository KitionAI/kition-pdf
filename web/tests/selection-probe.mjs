/** Called after installSearchProbe; observes actual text_select replies and the live engine. */
export async function installSelectionProbe() {
  const url = performance.getEntriesByType('resource').map(entry => entry.name).find(name => /\/engine\/kitEngine\.ts(?:\?|$)/.test(name))
  if (!url) throw new Error('Live engine module is unavailable')
  const { KitEngine } = await import(url)
  const probe = window.__pdfSelection = {
    calls: [], active: 0, maxActive: 0, held: [], holdNext: false, pointerId: null, trustedCopyCount: 0, trustedCopy: null,
    release() { this.holdNext = false; for (const finish of this.held.splice(0)) finish() },
  }
  document.addEventListener('pointerdown', event => { if (event.target?.dataset?.testid === 'kit-canvas-surface') probe.pointerId = event.pointerId }, true)
  // React's root listener has handled this bubbling event before it reaches document.
  document.addEventListener('copy', event => {
    if (!event.isTrusted) return
    probe.trustedCopyCount++
    probe.trustedCopy = { event, text: event.clipboardData?.getData('text/plain') ?? '', types: [...(event.clipboardData?.types ?? [])] }
  })
  for (const name of ['render', 'renderArea']) {
    const original = KitEngine.prototype[name]
    KitEngine.prototype[name] = function (...args) { window.__pdfSearch.engine = this; return original.apply(this, args) }
  }
  const execute = KitEngine.prototype.execute
  KitEngine.prototype.execute = async function (command, params = {}) {
    if (command !== 'text.select') return execute.call(this, command, params)
    const call = { params: { ...params }, status: 'pending' }
    probe.calls.push(call); probe.active++; probe.maxActive = Math.max(probe.maxActive, probe.active)
    const hold = probe.holdNext
    if (hold) probe.holdNext = false
    try {
      const raw = await execute.call(this, command, params)
      call.result = JSON.parse(raw)
      if (hold) await new Promise(resolve => probe.held.push(resolve))
      call.status = 'returned'
      return raw
    } catch (error) { call.status = 'failed'; throw error }
    finally { probe.active-- }
  }
}

export function selectionSnapshot() {
  const canvas = document.querySelector('[data-testid="kit-canvas-surface"]')
  const bounds = canvas.getBoundingClientRect()
  const overlay = document.querySelector('[data-testid="pdf-selection-highlights"]')
  return {
    view: { ...window.__pdfSearch.viewport.view },
    pages: window.__pdfSearch.engine.info.inspected.pages.map(page => page.area),
    canvas: { width: bounds.width, height: bounds.height },
    overlay: overlay ? { pointerEvents: getComputedStyle(overlay).pointerEvents, ariaHidden: overlay.getAttribute('aria-hidden') } : null,
    rects: [...document.querySelectorAll('[data-testid="pdf-selection-highlight"]')].map(element => {
      const rect = element.getBoundingClientRect()
      return { page: Number(element.dataset.page), left: rect.left - bounds.left, top: rect.top - bounds.top, width: rect.width, height: rect.height }
    }),
    latest: window.__pdfSelection.calls.at(-1), active: window.__pdfSelection.active, maxActive: window.__pdfSelection.maxActive,
  }
}

export function dispatchCopy() {
  const clipboard = new DataTransfer()
  const event = new ClipboardEvent('copy', { clipboardData: clipboard, bubbles: true, cancelable: true })
  document.activeElement.dispatchEvent(event)
  return { prevented: event.defaultPrevented, text: clipboard.getData('text/plain'), types: [...clipboard.types] }
}
