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

/** The pages (1-based, ascending) a render area reaches, in the order the engine looks them up. */
export function pagesReached(area, pages) {
  return pages.flatMap((page, index) => area.x1 > page.area.x && area.x0 < page.area.x + page.area.width && area.y1 > page.area.y && area.y0 < page.area.y + page.area.height ? [index + 1] : [])
}

/**
 * The engine's whole-page LRU as the harness knows it. The editor renders ahead: a 1:1 frame
 * reaches past the view along the scrolled axis (kit-editor's raster viewport), so one render
 * request looks up every page its area reaches, in page order, and admits each page it misses.
 * `known` lists identified residents, least recently used first; `unknown` counts residents
 * admitted before the model started, which are older than every known one.
 */
export function cacheModel(residentEntries, capacity = CACHE_BUDGET / PAGE_BYTES) {
  assert.ok(Number.isSafeInteger(residentEntries) && residentEntries >= 0 && residentEntries <= capacity, 'Invalid starting resident count')
  return { capacity, known: [], unknown: residentEntries }
}

export const modelResidents = model => model.known.length + model.unknown

/**
 * Replays one render's lookups (`pages` from pagesReached) against the model and returns the
 * exact deltas it predicts, with each page's outcome. A page that is neither known resident
 * nor known absent (residents of unknown identity remain) is uncertain: the observed hit count
 * says how many uncertain pages hit. When only some did, which ones is unknown, so the replay
 * accepts it only where no eviction can occur (then every count and the resulting LRU order are
 * the same whichever hit) and marks those pages 'uncertain'. `certain` is false whenever the
 * observation, not the model, decided an outcome.
 */
export function replayLookups(model, pages, observedHits) {
  const known = new Set(model.known)
  const uncertain = model.unknown > 0 ? pages.filter(page => !known.has(page)) : []
  const uncertainHits = observedHits - pages.filter(page => known.has(page)).length
  const absent = model.unknown > 0 ? 0 : pages.filter(page => !known.has(page)).length
  const partial = uncertainHits > 0 && uncertainHits < uncertain.length
  assert.ok(uncertainHits >= 0 && uncertainHits <= uncertain.length, `${observedHits} hits observed cannot come from pages ${pages.join(', ')}`)
  assert.ok(!partial || modelResidents(model) + absent + uncertain.length - uncertainHits <= model.capacity, `Cache state is ambiguous for pages ${uncertain.join(', ')}: ${observedHits} hits observed at capacity`)
  const delta = { hits: 0, misses: 0, admissions: 0, evictions: 0, fullRenderRequests: 0 }
  const outcomes = new Map()
  let unresolvedHits = uncertainHits
  for (const page of pages) {
    const position = model.known.indexOf(page)
    const uncertainHit = position < 0 && uncertain.includes(page) && unresolvedHits > 0
    if (position >= 0 || uncertainHit) {
      if (position >= 0) model.known.splice(position, 1)
      else { model.unknown -= 1; unresolvedHits -= 1 }
      model.known.push(page)
      delta.hits += 1; outcomes.set(page, partial && uncertainHit ? 'uncertain' : 'hit')
      continue
    }
    delta.misses += 1; delta.fullRenderRequests += 1; delta.admissions += 1; outcomes.set(page, partial && uncertain.includes(page) ? 'uncertain' : 'miss')
    if (modelResidents(model) >= model.capacity) {
      if (model.unknown > 0) model.unknown -= 1
      else model.known.shift()
      delta.evictions += 1
    }
    model.known.push(page)
  }
  return { pages, delta, outcomes, certain: uncertain.length === 0 }
}

/**
 * Exact counter deltas for one measured step: the LRU replay of every page the render reached,
 * and the plan's expectation (hit, miss, invalidate) for the requested page itself.
 */
export function assertLookup(step, delta, replay) {
  assert.equal(delta.uncachedTileRequests, 0, 'The measured operation used a tile path')
  assert.equal(delta.largePageBypasses, 0, 'The measured operation bypassed the whole-page path')
  assert.equal(delta.retentionBypasses, 0, 'A small synthetic page unexpectedly bypassed cache retention')
  assert.equal(delta.replacements, 0, 'A lookup unexpectedly replaced a resident cache key')
  assert.ok(replay.pages.includes(step.page), 'The measured render did not reach the requested page')
  for (const key of Object.keys(replay.delta)) assert.equal(delta[key], replay.delta[key], `Counter ${key} differs from the LRU replay of pages ${replay.pages.join(', ')}`)
  if (step.expectation !== 'any') {
    assert.ok(replay.certain, `Page ${step.page} must be known resident or known absent before a ${step.expectation} step`)
    assert.equal(replay.outcomes.get(step.page), step.expectation === 'hit' ? 'hit' : 'miss', `Page ${step.page} did not ${step.expectation === 'hit' ? 'hit' : 'miss'} the cache`)
  }
  if (step.expectation === 'invalidate') { assert.equal(delta.displayInvalidations, 1); assert.equal(delta.rendererCreations, 1) }
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
