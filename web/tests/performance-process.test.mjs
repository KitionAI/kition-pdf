import assert from 'node:assert/strict'
import test from 'node:test'
import { processTreeRss } from './performance-process.mjs'

test('browser RSS includes nested children but excludes unrelated processes', () => {
  assert.deepEqual(processTreeRss('40 20 7\n10 1 100\n20 10 200\n99 1 99999\n30 10 300', 10), {
    rssSumBytes: 607 * 1024, processCount: 4,
  })
})

test('closed root is unavailable rather than zero memory', () => {
  assert.equal(processTreeRss('20 1 500', 10), null)
})

test('cycles do not double-count and malformed snapshots cannot masquerade as measurements', () => {
  assert.deepEqual(processTreeRss('10 20 5\n20 10 7', 10), { rssSumBytes: 12 * 1024, processCount: 2 })
  for (const text of ['10 1 NaN', '10 1 -1', '10 1 1.5', '10 1 100 extra']) {
    assert.throws(() => processTreeRss(text, 10), /Invalid numeric/)
  }
})
