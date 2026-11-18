import assert from 'node:assert/strict'

export const CACHE_BUDGET = 64 * 1024 * 1024
export const PAGE_SIDE = 1024
export const PAGE_BYTES = PAGE_SIDE * PAGE_SIDE * 4
export const PAGE_COUNT = 48
export const COUNTERS = ['hits', 'misses', 'admissions', 'evictions', 'replacements', 'retentionBypasses', 'rendererCreations', 'displayInvalidations', 'fullRenderRequests', 'uncachedTileRequests', 'largePageBypasses']

export function pageColor(page) { return [40 + (page * 37) % 180, 40 + (page * 61) % 180, 40 + (page * 83) % 180, 255] }

/** Contributor-original solid rectangles, generated in memory; no embedded assets. */
export function cacheFixture() {
  const objects = ['', '<< /Type /Catalog /Pages 2 0 R >>', `<< /Type /Pages /Count ${PAGE_COUNT} /Kids [${Array.from({ length: PAGE_COUNT }, (_, i) => `${3 + i * 2} 0 R`).join(' ')}] >>`]
  for (let index = 0; index < PAGE_COUNT; index++) {
    const color = pageColor(index + 1).slice(0, 3).map(value => (value / 255).toFixed(8)).join(' ')
    const marks = `${color} rg 0 0 ${PAGE_SIDE} ${PAGE_SIDE} re f\n`
    objects.push(`<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${PAGE_SIDE} ${PAGE_SIDE}] /Resources << >> /Contents ${4 + index * 2} 0 R >>`)
    objects.push(`<< /Length ${marks.length} >>\nstream\n${marks}endstream`)
  }
  let body = '%PDF-1.7\n'
  const offsets = [0]
  for (let index = 1; index < objects.length; index++) { offsets.push(body.length); body += `${index} 0 obj\n${objects[index]}\nendobj\n` }
  const xref = body.length
  body += `xref\n0 ${objects.length}\n0000000000 65535 f \n${offsets.slice(1).map(offset => `${String(offset).padStart(10, '0')} 00000 n \n`).join('')}`
  return Buffer.from(`${body}trailer\n<< /Size ${objects.length} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`)
}

export function cachePlan() {
  const steps = []
  const add = (phase, page, action = 'navigate', expectation = 'any') => steps.push({ id: `${phase}-${steps.length}`, phase, page, action, expectation, status: 'untested' })
  add('cold-open', 1, 'open')
  add('actual-size', 1, 'zoom')
  add('prime', 1); add('prime', 2)
  add('warm-hit', 1, 'navigate', 'hit'); add('warm-hit', 2, 'navigate', 'hit'); add('warm-hit', 1, 'navigate', 'hit')
  for (let page = 3; page <= PAGE_COUNT; page++) add('pressure', page)
  add('evicted-revisit', 1, 'navigate', 'miss'); add('evicted-revisit', 2, 'navigate', 'miss')
  add('reconstructed-hit', 1, 'navigate', 'hit')
  add('edit-invalidation', 1, 'edit', 'invalidate')
  add('post-edit-miss', 2, 'navigate', 'miss')
  add('post-edit-hit', 1, 'navigate', 'hit')
  return steps
}

export function validateSnapshot(snapshot) {
  assert.equal(snapshot.schema, 1, 'Unsupported diagnostics schema')
  assert.equal(snapshot.scope, 'wasm-instance-thread-aggregate')
  assert.equal(snapshot.counterLimit, Number.MAX_SAFE_INTEGER)
  assert.equal(snapshot.saturated, false, 'Saturated diagnostic counters cannot prove exact deltas')
  for (const key of [...COUNTERS, 'liveCaches', 'residentBytes', 'residentEntries', 'capacityBytes', 'lastRendererGeneration', 'lastDisplayRevision']) assert.ok(Number.isSafeInteger(snapshot[key]) && snapshot[key] >= 0, `Invalid diagnostic field ${key}`)
  assert.equal(snapshot.liveCaches, 1, 'Per-document interpretation requires exactly one live cache')
  assert.equal(snapshot.capacityBytes, CACHE_BUDGET, 'This workload targets the real 64 MiB budget')
  assert.ok(snapshot.residentBytes <= snapshot.capacityBytes, 'Retained page bytes exceed the configured budget')
  assert.ok(snapshot.residentEntries <= CACHE_BUDGET / PAGE_BYTES, 'More full 1024-square pages retained than the budget permits')
  return snapshot
}

export function counterDelta(before, after) {
  validateSnapshot(before); validateSnapshot(after)
  return Object.fromEntries(COUNTERS.map(key => {
    const value = after[key] - before[key]
    assert.ok(Number.isSafeInteger(value) && value >= 0, `Counter ${key} reset or overflowed`)
    return [key, value]
  }))
}

export function assertLookup(expectation, delta) {
  assert.equal(delta.uncachedTileRequests, 0, 'The measured operation used a tile path')
  assert.equal(delta.largePageBypasses, 0, 'The measured operation bypassed the whole-page path')
  assert.equal(delta.retentionBypasses, 0, 'A small synthetic page unexpectedly bypassed cache retention')
  assert.equal(delta.replacements, 0, 'A lookup unexpectedly replaced a resident cache key')
  if (expectation === 'hit') {
    assert.equal(delta.hits, 1); assert.equal(delta.misses, 0)
    assert.equal(delta.fullRenderRequests, 0); assert.equal(delta.admissions, 0); assert.equal(delta.evictions, 0)
  } else if (expectation === 'miss' || expectation === 'invalidate') {
    assert.equal(delta.hits, 0); assert.equal(delta.misses, 1)
    assert.equal(delta.fullRenderRequests, 1); assert.equal(delta.admissions, 1)
  } else assert.equal(delta.hits + delta.misses, 1, 'A single-page view must perform exactly one full-page lookup')
  if (expectation === 'invalidate') { assert.equal(delta.displayInvalidations, 1); assert.equal(delta.rendererCreations, 1) }
  else { assert.equal(delta.displayInvalidations, 0); assert.equal(delta.rendererCreations, 0) }
}

export function summarizeCache(steps) {
  const statuses = ['passed', 'failed', 'timeout', 'untested']
  for (const step of steps) assert.ok(statuses.includes(step.status), 'Unknown sample status')
  const counts = Object.fromEntries(statuses.map(status => [status, steps.filter(step => step.status === status).length]))
  return { planned: steps.length, ...counts, completionRate: steps.length ? counts.passed / steps.length : null }
}

export function parseCacheOptions(args) {
  const options = { browser: 'chromium', timeoutMs: 120000 }
  const keys = new Map([['--out', 'out'], ['--browser', 'browser'], ['--engine-dir', 'engineDir'], ['--timeout-ms', 'timeoutMs']])
  const seen = new Set()
  for (let index = 0; index < args.length; index++) {
    if (args[index] === '--help') { options.help = true; continue }
    const key = keys.get(args[index])
    if (!key || seen.has(key)) throw new Error('Unknown or duplicate option')
    seen.add(key)
    const value = args[++index]
    if (!value || value.startsWith('--')) throw new Error('Missing option value')
    options[key] = key === 'timeoutMs' ? Number(value) : value
  }
  if (!options.help && !options.out) throw new Error('--out is required')
  if (!['chromium', 'webkit'].includes(options.browser)) throw new Error('Use chromium or webkit')
  if (!Number.isSafeInteger(options.timeoutMs) || options.timeoutMs <= 0) throw new Error('Timeout must be a positive integer')
  return options
}
