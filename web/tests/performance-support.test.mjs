import test from 'node:test'
import assert from 'node:assert/strict'
import { nearestRank, summarize, planSamples, parsePerformanceOptions } from './performance-support.mjs'

test('nearest rank preserves tails and uses one-based ceiling ranks', () => {
  assert.equal(nearestRank([100, 1, 2, 3], 0.5), 2)
  assert.equal(nearestRank([100, 1, 2, 3], 0.95), 100)
  assert.equal(nearestRank(Array.from({ length: 100 }, (_, i) => i + 1), 0.95), 95)
  assert.equal(nearestRank([], 0.5), null)
  assert.throws(() => nearestRank([NaN], 0.5))
})
test('failure and skipped work remain in the planned denominator', () => {
  const result = summarize([{ status: 'passed', presentMs: 20 }, { status: 'failed' }, { status: 'timeout' }, { status: 'untested' }])
  assert.deepEqual([result.planned, result.completed, result.failed, result.timedOut, result.untested], [4, 1, 1, 1, 1])
  assert.equal(result.completionRate, 0.25)
  assert.equal(result.successfulPresentMs.p95, 20)
  assert.equal(summarize([{ status: 'timeout' }]).successfulPresentMs.p50, null)
  assert.throws(() => summarize([{ status: 'passed' }]))
})
test('default plan contains 100 complete two-page roundtrips plus 40-page sweep', () => {
  const plan = planSamples(1044)
  assert.equal(plan.filter(s => s.phase === 'repeated-pair').length, 200)
  assert.equal(plan.filter(s => s.phase === 'sequential-window').length, 40)
  assert.deepEqual(plan.filter(s => s.cycle === 100).map(s => s.page), [1, 1044])
  assert.equal(new Set(plan.map(s => s.id)).size, plan.length)
  assert.equal(summarize(plan).untested, plan.length)
})
test('CLI defaults and invalid inputs cannot silently reduce the benchmark', () => {
  assert.equal(parsePerformanceOptions(['--manifest', 'cases.json', '--out', 'out.json']).cycles, 100)
  for (const args of [['--cycles', '0'], ['--cycles', '2.5'], ['--browser', 'bogus'], ['--cycles', '1', '--cycles', '2']]) {
    assert.throws(() => parsePerformanceOptions(['--manifest', 'cases.json', '--out', 'out.json', ...args]))
  }
})
