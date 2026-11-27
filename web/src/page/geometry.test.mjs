import test from 'node:test'
import assert from 'node:assert/strict'
import { clientToDocument, cursorFor, hitTest, keptOnPage, moveSlipped, pageAt, resizeSlipped, resized, screenBox } from './geometry.ts'

const pages = [{ area: { x0: 40, y0: 40, x1: 652, y1: 832 } }, { area: { x0: 40, y0: 856, x1: 832, y1: 1468 } }]
const objects = {
  comments: [
    { id: 'square', page: 1, index: 1, type: 'Square', rect: [100, 100, 200, 200] },
    { id: 'note', page: 1, index: 2, type: 'Text', rect: [150, 150, 170, 170] },
    { id: 'mark', page: 1, index: 3, type: 'Highlight', rect: [300, 300, 400, 312] },
    { id: 'locked', page: 2, index: 1, type: 'Square', rect: [10, 10, 50, 50], locked: true },
  ],
  fields: [{ name: 'city', page: 1, type: 'text', rect: [400, 50, 500, 70] }],
  links: [{ page: 1, index: 1, rect: [300, 300, 400, 312], to_page: 2 }, { page: 1, index: 2, rect: [300, 400, 400, 412], url: 'https://example.org' }],
}

test('a document point finds its page and the page-local point; between pages none', () => {
  assert.deepEqual(pageAt(pages, 140, 140), { page: 1, point: [100, 100] })
  assert.deepEqual(pageAt(pages, 50, 860), { page: 2, point: [10, 4] })
  assert.equal(pageAt(pages, 100, 845), null)
  assert.equal(pageAt(pages, Number.NaN, 0), null)
})

test('a press goes to the handles of the chosen comment, a field, the topmost comment, a link, a highlight, then the page', () => {
  const square = objects.comments[0]
  assert.equal(hitTest(objects, square, 1, [200, 200], 1).kind, 'handle')
  assert.equal(hitTest(objects, square, 1, [200, 200], 1).handle, 'se')
  assert.equal(hitTest(objects, null, 1, [200, 200], 1).kind, 'comment')
  assert.equal(hitTest(objects, null, 1, [450, 60], 1).kind, 'field')
  // The note lies over the square: the last on the page is on top.
  assert.equal(hitTest(objects, null, 1, [160, 160], 1).comment.id, 'note')
  // A link under a highlight is followed; a highlight alone is chosen.
  assert.equal(hitTest(objects, null, 1, [350, 305], 1).kind, 'link')
  assert.equal(hitTest({ ...objects, links: [] }, null, 1, [350, 305], 1).kind, 'markup')
  assert.equal(hitTest(objects, null, 1, [350, 405], 1).link.url, 'https://example.org')
  assert.equal(hitTest(objects, null, 1, [550, 600], 1).kind, 'page')
  // Three CSS pixels round a comment still reach it: fewer points at a higher zoom.
  assert.equal(hitTest(objects, null, 1, [97.5, 120], 1).kind, 'comment')
  assert.equal(hitTest(objects, null, 1, [97.5, 120], 4).kind, 'page')
  // A locked square has no handles.
  assert.equal(hitTest(objects, objects.comments[3], 2, [50, 50], 1).kind, 'comment')
})

test('the cursor says what is under the pointer', () => {
  assert.equal(cursorFor({ kind: 'handle', handle: 'ne' }, null, null), 'nesw-resize')
  assert.equal(cursorFor({ kind: 'comment', comment: objects.comments[0] }, null, null), 'move')
  assert.equal(cursorFor({ kind: 'comment', comment: objects.comments[3] }, null, null), 'default')
  assert.equal(cursorFor({ kind: 'link' }, null, null), 'pointer')
  assert.equal(cursorFor({ kind: 'page' }, [[10, 10, 100, 20]], [50, 15]), 'text')
  assert.equal(cursorFor({ kind: 'page' }, [[10, 10, 100, 20]], [50, 50]), 'default')
  assert.equal(cursorFor({ kind: 'page' }, null, [50, 50]), 'text')
})

test('a handle drag resizes from the opposite side, turns over past it, and keeps the ratio with Shift', () => {
  assert.deepEqual(resized([100, 100, 200, 200], 'se', 50, 20), [100, 100, 250, 220])
  assert.deepEqual(resized([100, 100, 200, 200], 'n', 0, -10), [100, 90, 200, 200])
  assert.deepEqual(resized([100, 100, 200, 200], 'e', -150, 0), [50, 100, 100, 200])
  assert.deepEqual(resized([100, 100, 200, 150], 'se', 100, 0, true), [100, 100, 300, 200])
  assert.deepEqual(resized([100, 100, 200, 150], 'nw', 50, 0, true), [150, 125, 200, 150])
  const thin = resized([100, 100, 200, 200], 'e', -100, 0)
  assert.ok(thin[2] - thin[0] >= 2)
})

test('small moves and resizes are slips; a move keeps a comment on its page', () => {
  assert.equal(moveSlipped(2, -2), true)
  assert.equal(moveSlipped(2.5, 0), false)
  assert.equal(resizeSlipped([0, 0, 10, 10], [0, 0, 14, 13]), true)
  assert.equal(resizeSlipped([0, 0, 10, 10], [0, 0, 15, 10]), false)
  assert.deepEqual(keptOnPage([10, 10, 50, 50], -30, 600, 612, 792), [-10, 600])
  assert.deepEqual(keptOnPage([10, 10, 50, 50], 0, 800, 612, 792), [0, 742])
  assert.deepEqual(keptOnPage([-10, 10, 50, 50], -30, 0, 612, 792), [-30, 0])
})

test('screen boxes and client points follow the view', () => {
  const view = { x: 20, y: 30, zoom: 2 }
  assert.deepEqual(screenBox(pages[0].area, [100, 100, 200, 150], view), { left: 240, top: 220, width: 200, height: 100 })
  assert.equal(screenBox(pages[0].area, [0, 0, 1, 1], { x: 0, y: 0, zoom: 0 }), null)
  assert.deepEqual(clientToDocument(250, 120, { left: 10, top: 20 }, view), [140, 80])
})
