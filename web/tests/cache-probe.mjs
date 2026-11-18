/** Add after performance-probe: count pending requests without retaining request bodies. */
export function installCacheQuietProbe() {
  const probe = window.__kitionCacheQuiet = { pending: 0, dispatched: 0, settled: 0, lastActivity: performance.now() }
  const OriginalWorker = window.Worker
  window.Worker = class extends OriginalWorker {
    constructor(...args) {
      super(...args)
      const pending = new Set()
      const send = this.postMessage.bind(this)
      this.postMessage = (message, ...rest) => {
        if (['render', 'renderArea'].includes(message?.request?.method)) {
          pending.add(message.id); probe.pending++; probe.dispatched++; probe.lastActivity = performance.now()
        }
        return send(message, ...rest)
      }
      this.addEventListener('message', event => {
        if (!pending.delete(event.data?.id)) return
        probe.pending--; probe.settled++; probe.lastActivity = performance.now()
      })
      const cleanup = () => { probe.pending -= pending.size; pending.clear(); probe.lastActivity = performance.now() }
      this.addEventListener('error', cleanup)
      const terminate = this.terminate.bind(this)
      this.terminate = () => { cleanup(); return terminate() }
    }
  }
}
