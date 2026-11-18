/** Observe and fault-inject the existing development host; do not replace its storage handler. */
export async function installSaveHostProbe() {
  const url = performance.getEntriesByType('resource').map(item => item.name).find(name => /\/kit-ui-protocol\/src\/index\.ts(?:\?|$)/.test(name))
  if (!url) throw new Error('Live protocol module is unavailable')
  const { KitPeer } = await import(url)
  const probe = window.__pdfSaveHost = {
    events: [], writes: [], failNext: false, holdNext: false, held: null, pending: null,
    event(type, detail = {}) { const entry = { sequence: this.events.length, type, ...detail }; this.events.push(entry); return entry },
    release() { const release = this.held; this.held = null; release?.() },
  }
  const handle = KitPeer.prototype.handle
  KitPeer.prototype.handle = function (method, handler) {
    if (method !== 'host/write') return handle.call(this, method, handler)
    return handle.call(this, method, async params => {
      const write = { index: probe.writes.length, mode: params.mode, size: params.bytes.byteLength, status: 'requested' }
      probe.writes.push(write); probe.event('host-write-requested', { write: write.index })
      if (probe.failNext) {
        probe.failNext = false; write.status = 'rejected'; probe.event('host-write-rejected', { write: write.index })
        throw new Error('Injected original-fixture host write failure')
      }
      const result = await handler(params)
      write.status = 'written'; write.version = result.version; probe.event('host-write-stored', { write: write.index })
      if (probe.holdNext) { probe.holdNext = false; await new Promise(resolve => { probe.held = resolve }) }
      write.status = 'acknowledged'; probe.event('host-write-acknowledged', { write: write.index })
      return result
    })
  }
}

/** Observe actual Worker serialization and markSaved calls without changing their output. */
export async function installSaveEngineProbe() {
  const url = performance.getEntriesByType('resource').map(item => item.name).find(name => /\/engine\/kitEngine\.ts(?:\?|$)/.test(name))
  if (!url) throw new Error('Live engine module is unavailable')
  const { KitEngine } = await import(url)
  const host = window.parent.__pdfSaveHost
  const probe = window.__pdfSaveEngine = { engine: null, serializations: [], marks: [] }
  const execute = KitEngine.prototype.execute
  KitEngine.prototype.execute = async function (...args) { probe.engine = this; return execute.apply(this, args) }
  const serialize = KitEngine.prototype.serialize
  KitEngine.prototype.serialize = async function (...args) {
    probe.engine = this
    const saved = await serialize.apply(this, args)
    const entry = { revision: saved.revision, size: saved.bytes.byteLength, dirty: this.info.dirty }
    probe.serializations.push(entry); host.event('worker-serialized', entry)
    return saved
  }
  const markSaved = KitEngine.prototype.markSaved
  KitEngine.prototype.markSaved = async function (revision) {
    const mark = { requestedRevision: revision, beforeRevision: this.info.revision, dirtyBefore: this.info.dirty }
    probe.marks.push(mark); host.event('worker-mark-started', mark)
    await markSaved.call(this, revision)
    Object.assign(mark, { afterRevision: this.info.revision, dirtyAfter: this.info.dirty })
    host.event('worker-mark-completed', mark)
  }
}
