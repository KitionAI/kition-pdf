import { test } from 'node:test'
import assert from 'node:assert/strict'
import { dragIsBox, pagePoint, pickSpec, pickedRect, pickedValue } from './pick.ts'

const point = { type: 'array', items: { type: 'number' }, minItems: 2, maxItems: 2 }
const rect = { type: 'array', items: { type: 'number' }, minItems: 4, maxItems: 4 }
const points = { type: 'array', items: point, minItems: 1, maxItems: 2048 }
const page = { type: 'integer', required: true }

test('only a point, a rectangle or points of a command that takes a page are picked', () => {
  assert.deepEqual(pickSpec('link.add', 'rect', rect, { page, rect }), { key: 'rect', kind: 'rect', min: 1, max: 1 })
  assert.deepEqual(pickSpec('fillSign.add', 'at', point, { page, at: point }), { key: 'at', kind: 'point', min: 1, max: 1 })
  // No page to fill in: nothing to pick (the page is where the value is measured from).
  assert.equal(pickSpec('x.y', 'rect', rect, { rect }), null)
  // page.insertBlank's `at` is a page number, and a crop's four numbers are no rectangle on the page.
  assert.equal(pickSpec('page.insertBlank', 'at', { type: 'integer' }, { page, at: { type: 'integer' } }), null)
  assert.equal(pickSpec('content.set', 'crop', rect, { page, crop: rect }), null)
  assert.equal(pickSpec('x.y', 'rect', point, { page, rect: point }), null)
})

test('points take what the measurement needs', () => {
  assert.deepEqual(pickSpec('measure.distance', 'points', points, { page, points }), { key: 'points', kind: 'points', min: 2, max: 2 })
  assert.deepEqual(pickSpec('measure.area', 'points', points, { page, points }), { key: 'points', kind: 'points', min: 3, max: 200 })
  assert.deepEqual(pickSpec('comment.add', 'points', points, { page, points }), { key: 'points', kind: 'points', min: 1, max: 200 })
})

test('picked values are page points, kept on the page, to 1/100 pt', () => {
  assert.deepEqual(pagePoint([-5, 900.123], 612, 792), [0, 792])
  assert.deepEqual(pagePoint([100.456, 200.004], 612, 792), [100.46, 200])
  assert.deepEqual(pickedRect([250, 160], [100, 100]), [100, 100, 250, 160])
  assert.deepEqual(pickedValue('point', [[1, 2]]), [1, 2])
  assert.deepEqual(pickedValue('rect', [[5, 9], [1, 2]]), [1, 2, 5, 9])
  assert.deepEqual(pickedValue('points', [[1, 2], [3, 4]]), [[1, 2], [3, 4]])
  assert.equal(pickedValue('rect', [[1, 2]]), null)
  assert.equal(dragIsBox({ clientX: 0, clientY: 0 }, { clientX: 5, clientY: 40 }), false)
  assert.equal(dragIsBox({ clientX: 0, clientY: 0 }, { clientX: 6, clientY: 6 }), true)
})
