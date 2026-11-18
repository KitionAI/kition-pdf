import { execFile } from 'node:child_process'
import { promisify } from 'node:util'

const exec = promisify(execFile)

/** ps RSS is KiB on macOS/Linux. Sum the task-owned tree, never unrelated processes. */
export function processTreeRss(output, rootPid) {
  const rows = output.trim().split('\n').filter(Boolean).map(line => {
    const values = line.trim().split(/\s+/).map(Number)
    if (values.length !== 3 || values.some(value => !Number.isSafeInteger(value) || value < 0)) {
      throw new Error('Invalid numeric process snapshot')
    }
    return { pid: values[0], parent: values[1], rss: values[2] * 1024 }
  })
  if (!rows.some(row => row.pid === rootPid)) return null
  const children = new Map()
  for (const row of rows) {
    const list = children.get(row.parent) ?? []
    list.push(row)
    children.set(row.parent, list)
  }
  const pending = [rows.find(row => row.pid === rootPid)]
  const seen = new Set()
  let rssSumBytes = 0
  while (pending.length) {
    const row = pending.pop()
    if (seen.has(row.pid)) continue
    seen.add(row.pid)
    rssSumBytes += row.rss
    pending.push(...(children.get(row.pid) ?? []))
  }
  return { rssSumBytes, processCount: seen.size }
}

/** Observational sampling: RSS includes shared pages repeatedly; it is not unique/live memory. */
export function createProcessSampler(rootPid, { intervalMs = 2000 } = {}) {
  if (!Number.isSafeInteger(rootPid) || rootPid <= 0) throw new Error('A launched browser PID is required')
  if (!Number.isFinite(intervalMs) || intervalMs < 250) throw new Error('Sampling interval must be at least 250 ms')
  const samples = []
  const started = performance.now()
  let timer
  let active = false
  let chain = Promise.resolve()
  const sample = label => {
    chain = chain.then(async () => {
      const before = performance.now()
      const entry = { label, elapsedMs: Math.round(before - started), status: 'unavailable' }
      samples.push(entry)
      if (!['darwin', 'linux'].includes(process.platform)) {
        entry.reason = 'Process RSS sampling is implemented only for macOS and Linux'
        return
      }
      try {
        const { stdout } = await exec('ps', ['-axo', 'pid=,ppid=,rss='], { timeout: 3000, maxBuffer: 8 * 1024 * 1024 })
        const snapshot = processTreeRss(stdout, rootPid)
        if (!snapshot) entry.reason = 'The launched browser root process is no longer present'
        else Object.assign(entry, snapshot, { status: 'sampled' })
      } catch (error) {
        entry.reason = `Process snapshot unavailable (${error.code ?? 'invalid snapshot'})`
      } finally {
        entry.probeMs = Math.round(performance.now() - before)
      }
    })
    return chain
  }
  const schedule = () => {
    if (!active) return
    timer = setTimeout(async () => {
      await sample('periodic')
      schedule()
    }, intervalMs)
    timer.unref()
  }
  return {
    async start() {
      if (active) throw new Error('Sampler already started')
      active = true
      await sample('browser-start')
      schedule()
    },
    checkpoint: sample,
    async stop() {
      active = false
      clearTimeout(timer)
      await chain
      const values = samples.filter(entry => entry.status === 'sampled').map(entry => entry.rssSumBytes)
      return {
        metric: 'Sum of resident-set bytes for the launched browser process and its descendants',
        limitations: [
          'Shared resident pages can be counted more than once; this is not unique physical memory.',
          'Worker memory is included only through its owning browser processes, not attributed separately.',
          'Observed high water is a lower bound on transient peaks at the sampling interval.',
          'A process can start or exit between snapshots; no global host memory claim is made.',
          'Sampling does not force garbage collection or identify live allocations.',
        ],
        intervalMs,
        status: values.length ? 'sampled' : 'unavailable',
        firstRssSumBytes: values[0] ?? null,
        lastRssSumBytes: values.at(-1) ?? null,
        observedHighWaterRssSumBytes: values.length ? Math.max(...values) : null,
        samples,
      }
    },
  }
}
