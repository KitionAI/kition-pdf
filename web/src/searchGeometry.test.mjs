import test from 'node:test'
import assert from 'node:assert/strict'
import { searchDocumentRects, searchScreenRects, searchPanTarget } from './searchGeometry.ts'

test('page-local multi-line rectangles gain the document page offset once', () => {
  const page = { x0: 50, y0: 600, x1: 250, y1: 900 }
  assert.deepEqual(searchDocumentRects(page, [[10, 20, 40, 30], [10, 50, 60, 60]]), [
    { x0: 60, y0: 620, x1: 90, y1: 630 }, { x0: 60, y0: 650, x1: 110, y1: 660 },
  ])
})

test('zoom and panning convert points to CSS pixels without a DPR factor', () => {
  const page = { x0: 30, y0: 400, x1: 230, y1: 700 }
  for (const zoom of [0.5, 1, 2]) assert.deepEqual(searchScreenRects(page, [[20, 40, 80, 60]], { x: 10, y: 420, zoom }), [
    { left: 40 * zoom, top: 20 * zoom, width: 60 * zoom, height: 20 * zoom },
  ])
})

test('off-page portions are clipped and invalid engine geometry is rejected', () => {
  const page = { x0: 10, y0: 20, x1: 110, y1: 220 }
  const rects = [[-4, -2, 120, 240], [2, 2, 1, 9], [1, 1, NaN, 3], [0, 0, Infinity, 1], [101, 1, 103, 2], [1, 2, 3]]
  assert.deepEqual(searchDocumentRects(page, rects), [page])
  assert.deepEqual(searchScreenRects(page, [[0, 0, 4, 4]], { x: 0, y: 0, zoom: 0 }), [])
  assert.deepEqual(searchDocumentRects(undefined, [[0, 0, 1, 1]]), [])
})

test('match navigation reveals a distant column and keeps a visible column still', () => {
  const page = { x0: 10, y0: 500, x1: 2010, y1: 1500 }
  const view = { x: 0, y: 400, zoom: 2 }, viewport = { width: 800, height: 600 }
  assert.deepEqual(searchPanTarget(page, [1500, 100, 1600, 120], view, viewport), { x: 1498, y: 500 })
  assert.deepEqual(searchPanTarget(page, [100, 100, 200, 120], view, viewport), { x: 0, y: 500 })
  assert.deepEqual(searchPanTarget(page, null, view, viewport), { x: -6, y: 484 })
})
