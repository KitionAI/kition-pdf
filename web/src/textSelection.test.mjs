import assert from 'node:assert/strict'
import test from 'node:test'
import { selectionPoint, SelectionSession } from './textSelection.ts'

const pages = [{ area: { x0: 20, y0: 40, x1: 220, y1: 240 } }, { area: { x0: 40, y0: 260, x1: 200, y1: 500 } }]
const request = n => ({ page: 1, anchor: [0, 0], focus: [n, n], end_page: 1, mode: 'range' })
const result = text => ({ text, pages: [{ page: 1, text, rects: [[1, 2, 3, 4]] }] })
const flush = async () => { await Promise.resolve(); await Promise.resolve(); await Promise.resolve() }

test('document points subtract page origins exactly once and reject pasteboard starts', () => {
  assert.deepEqual(selectionPoint(pages, 50, 290), { page: 2, point: [10, 30] })
  assert.equal(selectionPoint(pages, 10, 80), null)
  assert.equal(selectionPoint(pages, 100, 250), null)
  assert.deepEqual(selectionPoint(pages, 10, 290, true), { page: 2, point: [0, 30] })
  assert.deepEqual(selectionPoint(pages, 250, 900, true), { page: 2, point: [160, 240] })
})

test('invalid page geometry and nonfinite pointers do not become selection coordinates', () => {
  const invalid = [{ area: { x0: 10, y0: 0, x1: 0, y1: 1 } }, { area: { x0: 0, y0: 0, x1: Infinity, y1: 10 } }]
  assert.equal(selectionPoint(invalid, 1, 1, true), null)
  assert.equal(selectionPoint(pages, NaN, 1, true), null)
  assert.equal(selectionPoint([], 0, 0, true), null)
})

test('a pointer flood retains only the newest pending request and never publishes an obsolete answer', async () => {
  const calls = [], finish = [], updates = []
  const session = new SelectionSession(r => { calls.push(r); return new Promise(resolve => finish.push(resolve)) }, r => updates.push(r), () => assert.fail('unexpected failure'))
  session.request(request(1))
  for (let n = 2; n <= 100; n++) session.request(request(n))
  assert.equal(calls.length, 1)
  finish.shift()(result('old')); await flush()
  assert.equal(updates.at(-1), null)
  assert.deepEqual(calls.map(c => c.focus[0]), [1, 100])
  finish.shift()(result('latest')); await flush()
  assert.equal(updates.at(-1).text, 'latest')
  session.dispose()
})

test('clear cancels queued work and late results, then allows a new drag', async () => {
  let finish
  const calls = [], updates = []
  const session = new SelectionSession(r => { calls.push(r); return new Promise(resolve => { finish = resolve }) }, r => updates.push(r), () => assert.fail('unexpected failure'))
  session.request(request(1)); session.request(request(2)); session.clear()
  finish(result('cancelled')); await flush()
  assert.equal(calls.length, 1); assert.equal(updates.at(-1), null)
  session.request(request(3)); finish(result('new')); await flush()
  assert.equal(updates.at(-1).text, 'new')
})

test('a disposed document session cannot publish, report a late error, or execute queued endpoints', async () => {
  let reject
  const calls = [], updates = [], failures = []
  const session = new SelectionSession(r => { calls.push(r); return new Promise((_, fail) => { reject = fail }) }, r => updates.push(r), () => failures.push(true))
  session.request(request(1)); session.request(request(2)); session.dispose(); session.request(request(3))
  reject(new Error('old document')); await flush()
  assert.equal(calls.length, 1); assert.equal(updates.at(-1), null); assert.deepEqual(failures, [])
})

test('current failures are reported without trapping future requests or retaining clipboard text', async () => {
  let attempt = 0
  const updates = [], failures = []
  const session = new SelectionSession(async () => {
    attempt++
    if (attempt === 1) throw new Error('unavailable')
    return attempt === 2 ? null : result('recovered')
  }, r => updates.push(r), () => failures.push(true))
  session.request(request(1)); await flush()
  session.request(request(2)); await flush()
  assert.deepEqual(failures, [true, true]); assert.equal(updates.at(-1), null)
  session.request(request(3)); await flush()
  assert.equal(updates.at(-1).text, 'recovered')
})
