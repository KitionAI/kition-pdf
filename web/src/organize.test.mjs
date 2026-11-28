import test from 'node:test'
import assert from 'node:assert/strict'
import { currentAfterDelete, dropGap, moveParams, orderAfterDrop, pageTargets, placeAfterMove, rangeSelection, toggleSelection } from './organize.ts'

test('a page command acts on the selected pages, else on the current page', () => {
  assert.deepEqual(pageTargets([], 4), [4])
  assert.deepEqual(pageTargets([5, 1, 5, 3], 0), [1, 3, 5])
})

test('Shift+click selects a run from the anchor either way; Cmd+click toggles, starting from the current page', () => {
  assert.deepEqual(rangeSelection(2, 5), [2, 3, 4, 5])
  assert.deepEqual(rangeSelection(5, 2), [2, 3, 4, 5])
  assert.deepEqual(toggleSelection([], 1, 4), [1, 4])
  assert.deepEqual(toggleSelection([1, 4], 1, 4), [1])
  assert.deepEqual(toggleSelection([3], 0, 0), [0, 3])
})

test('the gap under the pointer is the nearest row boundary, kept to the list', () => {
  assert.equal(dropGap(-30, 240, 8), 0)
  assert.equal(dropGap(100, 240, 8), 0)
  assert.equal(dropGap(130, 240, 8), 1)
  assert.equal(dropGap(9999, 240, 8), 8)
  assert.equal(dropGap(Number.NaN, 240, 8), 0)
})

test('dropping page 3 above page 1 is one page.move to the front', () => {
  assert.deepEqual(orderAfterDrop([2], 0, 5), [2, 0, 1, 3, 4])
  assert.deepEqual(moveParams([2], 0, 5), { pages: [3], to: 1 })
  // Pages 1 and 2 dropped after page 4: they go after it, in their order.
  assert.deepEqual(orderAfterDrop([0, 1], 4, 5), [2, 3, 0, 1, 4])
  assert.deepEqual(moveParams([1, 0], 4, 5), { pages: [1, 2], to: 3 })
  // Two pages apart, gathered at the end.
  assert.deepEqual(moveParams([0, 2], 5, 5), { pages: [1, 3], to: 4 })
})

test('a drop that leaves the order as it was, or moves every page, is no edit', () => {
  assert.equal(moveParams([2], 2, 5), null)
  assert.equal(moveParams([2], 3, 5), null)
  assert.equal(moveParams([1, 2], 1, 5), null)
  assert.equal(moveParams([0, 1, 2], 0, 3), null)
  assert.equal(moveParams([], 0, 3), null)
})

test('the current page follows its page through a move', () => {
  const order = orderAfterDrop([2], 0, 5)
  assert.equal(placeAfterMove(order, 2), 0)
  assert.equal(placeAfterMove(order, 0), 1)
  assert.equal(placeAfterMove(order, 4), 4)
})

test('after a delete the current page stays on its page, or goes to the page after the deleted run', () => {
  assert.equal(currentAfterDelete(5, [1, 2], 8), 3)
  assert.equal(currentAfterDelete(1, [1, 2], 8), 1)
  assert.equal(currentAfterDelete(0, [3], 8), 0)
  // The last pages deleted: the page before them.
  assert.equal(currentAfterDelete(7, [6, 7], 8), 5)
})
