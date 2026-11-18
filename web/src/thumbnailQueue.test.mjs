import assert from 'node:assert/strict'
import test from 'node:test'
import { ThumbnailQueue, thumbnailSize, thumbnailWindow, THUMB_MAX_BYTES, THUMB_MAX_ROWS } from './thumbnailQueue.ts'

const request = (index) => ({ index, area: { x0: 0, y0: index * 600, x1: 400, y1: index * 600 + 600 } })
const pixels = async (_x, _y, _scale, width, height) => new Uint8Array(width * height * 4)
const advance = async (t) => { t.mock.timers.tick(150); await Promise.resolve(); await Promise.resolve() }
function setup(t) {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const queue = new ThumbnailQueue(() => {})
  t.after(() => queue.dispose())
  return queue
}

test('thumbnail dimensions and virtual window are bounded for extreme pages/documents', () => {
  assert.deepEqual(thumbnailSize({ x0: 0, y0: 0, x1: 1, y1: 1e10 }), { width: 1, height: 180, scale: 180 / 1e10 })
  assert.throws(() => thumbnailSize({ x0: 0, y0: 0, x1: NaN, y1: 10 }))
  const visible = thumbnailWindow(10000, 5000 * 240, 20000)
  assert.equal(visible.start, 4999)
  assert.equal(visible.end - visible.start, THUMB_MAX_ROWS)
})

test('prioritizes current demand and never renders or retains the whole document', async (t) => {
  const queue = setup(t)
  const calls = []
  const source = { render: async (...args) => { calls.push(args[1] / 600); return pixels(...args) } }
  queue.update(source, 1, [request(500), ...Array.from({ length: 1000 }, (_, i) => request(i))])
  for (let i = 0; i < 25; i++) await advance(t)
  assert.equal(calls[0], 500)
  assert.equal(calls.length, THUMB_MAX_ROWS + 1)
  assert.ok(queue.bytes <= THUMB_MAX_BYTES)
  queue.update(source, 1, [request(900)])
  assert.equal(queue.size, 0)
  await advance(t)
  assert.equal(queue.size, 1)
})

test('replaces pending demand while keeping only one worker call active', async (t) => {
  const queue = setup(t)
  let finish
  const calls = []
  const source = { render: (...args) => { calls.push(args[1] / 600); return new Promise(resolve => { finish = () => resolve(new Uint8Array(args[3] * args[4] * 4)) }) } }
  queue.update(source, 1, [request(0), request(1), request(2)])
  await advance(t)
  queue.update(source, 1, [request(50), request(51)])
  await advance(t)
  assert.deepEqual(calls, [0])
  finish()
  await Promise.resolve(); await Promise.resolve()
  assert.equal(queue.size, 0)
  await advance(t)
  assert.deepEqual(calls, [0, 50])
})

for (const kind of ['revision', 'document', 'dispose']) test(`discards obsolete pixels after ${kind} change`, async (t) => {
  const queue = setup(t)
  let finish
  const source = { render: (...args) => new Promise(resolve => { finish = () => resolve(new Uint8Array(args[3] * args[4] * 4)) }) }
  queue.update(source, 1, [request(0)])
  await advance(t)
  if (kind === 'dispose') queue.dispose()
  else queue.update(kind === 'document' ? { render: pixels } : source, 2, [])
  finish()
  await Promise.resolve(); await Promise.resolve()
  assert.equal(queue.size, 0)
  assert.equal(queue.get(0, source, 1), undefined)
})

test('keeps an explicit failed result without retry loops, then retries on demand', async (t) => {
  const queue = setup(t)
  let calls = 0
  const source = { render: async (...args) => { calls++; if (calls === 1) throw new Error('synthetic page failure'); return pixels(...args) } }
  queue.update(source, 1, [request(0)])
  await advance(t)
  assert.deepEqual(queue.get(0, source, 1), { error: 'synthetic page failure' })
  await advance(t)
  assert.equal(calls, 1)
  queue.retry(0)
  await advance(t)
  assert.ok('pixels' in queue.get(0, source, 1))
  assert.equal(calls, 2)
})

test('rejects malformed pixel payloads and does not retain an oversized backing buffer', async (t) => {
  const queue = setup(t)
  const malformed = { render: async () => new Uint8Array(1) }
  queue.update(malformed, 1, [request(0)])
  await advance(t)
  assert.match(queue.get(0, malformed, 1).error, /incomplete pixels/)
  const oversized = { render: async (_x, _y, _scale, width, height) => new Uint8Array(new ArrayBuffer(8 * 1024 * 1024), 0, width * height * 4) }
  queue.update(oversized, 2, [request(0)])
  await advance(t)
  const image = queue.get(0, oversized, 2)
  assert.equal(image.pixels.buffer.byteLength, image.width * image.height * 4)
})

test('an obsolete revision rejection does not replace the new revision state', async (t) => {
  const queue = setup(t)
  let reject
  const source = { render: () => new Promise((_resolve, fail) => { reject = fail }) }
  queue.update(source, 1, [request(0)])
  await advance(t)
  queue.update(source, 2, [])
  reject(new Error('old revision'))
  await Promise.resolve(); await Promise.resolve()
  assert.equal(queue.size, 0)
})
