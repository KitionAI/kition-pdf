import test from 'node:test'
import assert from 'node:assert/strict'
import { CACHE_BUDGET, PAGE_BYTES, COUNTERS, cachePlan, cacheFixture, counterDelta, assertLookup, summarizeCache, parseCacheOptions } from './cache-support.mjs'

const snapshot = extras => ({ schema: 1, scope: 'wasm-instance-thread-aggregate', counterLimit: Number.MAX_SAFE_INTEGER, saturated: false, ...Object.fromEntries(COUNTERS.map(key => [key, 0])), liveCaches: 1, residentBytes: PAGE_BYTES, residentEntries: 1, capacityBytes: CACHE_BUDGET, lastRendererGeneration: 1, lastDisplayRevision: 0, ...extras })
test('exact hit and miss deltas reject mixed or uncached work', () => {
  const before = snapshot()
  assertLookup('hit', counterDelta(before, snapshot({ hits: 1 })))
  assertLookup('miss', counterDelta(before, snapshot({ misses: 1, admissions: 1, fullRenderRequests: 1 })))
  assert.throws(() => assertLookup('hit', counterDelta(before, snapshot({ hits: 1, misses: 1 }))))
  assert.throws(() => assertLookup('hit', counterDelta(before, snapshot({ hits: 1, uncachedTileRequests: 1 }))))
})
test('invalid gauges and counter resets cannot masquerade as bounded cache behavior', () => {
  assert.throws(() => counterDelta(snapshot({ hits: 2 }), snapshot({ hits: 1 })))
  assert.throws(() => counterDelta(snapshot(), snapshot({ residentBytes: CACHE_BUDGET + 1 })))
  assert.throws(() => counterDelta(snapshot(), snapshot({ liveCaches: 2 })))
  assert.throws(() => counterDelta(snapshot(), snapshot({ residentEntries: 17 })))
  assert.throws(() => counterDelta(snapshot(), snapshot({ saturated: true })))
})
test('the fixed plan crosses capacity and retains failure/untested denominators', () => {
  const steps = cachePlan()
  assert.equal(steps.filter(step => step.phase === 'pressure').length, 46)
  assert.ok(46 * PAGE_BYTES > CACHE_BUDGET)
  assert.deepEqual(steps.filter(step => step.phase === 'evicted-revisit').map(step => step.page), [1, 2])
  steps[0].status = 'passed'; steps[1].status = 'timeout'
  const summary = summarizeCache(steps)
  assert.equal(summary.planned, summary.passed + summary.failed + summary.timeout + summary.untested)
  assert.equal(summary.completionRate, 1 / steps.length)
})
test('fixture is deterministic ASCII geometry and CLI cannot silently ignore overrides', () => {
  assert.deepEqual(cacheFixture(), cacheFixture())
  assert.match(cacheFixture().toString(), /\/Count 48/)
  assert.equal(parseCacheOptions(['--out', 'result.json', '--engine-dir', 'engine']).engineDir, 'engine')
  assert.throws(() => parseCacheOptions(['--out', 'result.json', '--timeout-ms', '0']))
  assert.throws(() => parseCacheOptions(['--out', 'a', '--out', 'b']))
})
