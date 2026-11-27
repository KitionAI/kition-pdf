import test from 'node:test'
import assert from 'node:assert/strict'
import { errorField, humanize, initialText, pageList, parseValue, shownValue } from './params.ts'
import { matchRuns, rectQuad, selectionBounds, selectionEnd, selectionRuns } from './markup.ts'

const pages = { type: 'array', items: { type: 'integer', minimum: 1 } }
const point = { type: 'array', items: { type: 'number' }, minItems: 2, maxItems: 2 }
const rect = { type: 'array', items: { type: 'number' }, minItems: 4, maxItems: 4 }
const points = { type: 'array', items: point, minItems: 2 }
const words = { type: 'array', items: { type: 'string' } }

test('pages are typed as 3, 3, 5 or 2-4 (E8), and JSON still works', () => {
  assert.deepEqual(parseValue(pages, '3'), { value: [3] })
  assert.deepEqual(parseValue(pages, '3, 5'), { value: [3, 5] })
  assert.deepEqual(parseValue(pages, '2-4'), { value: [2, 3, 4] })
  assert.deepEqual(parseValue(pages, '1, 3 - 5'), { value: [1, 3, 4, 5] })
  assert.deepEqual(parseValue(pages, '4–2'), { value: [4, 3, 2] })
  assert.deepEqual(parseValue(pages, '3，5'), { value: [3, 5] })
  assert.deepEqual(parseValue(pages, '[3]'), { value: [3] })
  assert.deepEqual(parseValue(pages, ''), { value: undefined })
  assert.deepEqual(parseValue(pages, 'three'), { error: { kind: 'pages' } })
  assert.deepEqual(parseValue(pages, '1.5'), { error: { kind: 'pages' } })
  assert.equal(pageList('1-20000'), null)
})

test('a point is two numbers, a rectangle four, several points x, y; x, y', () => {
  assert.deepEqual(parseValue(point, '300, 400'), { value: [300, 400] })
  assert.deepEqual(parseValue(point, '300 400'), { value: [300, 400] })
  assert.deepEqual(parseValue(point, '-2.5, 7'), { value: [-2.5, 7] })
  assert.deepEqual(parseValue(point, '[72, 72]'), { value: [72, 72] })
  assert.deepEqual(parseValue(point, '300'), { error: { kind: 'count', min: 2, max: 2 } })
  assert.deepEqual(parseValue(point, '300, x'), { error: { kind: 'numbers', count: 2 } })
  assert.deepEqual(parseValue(rect, '10, 20, 110, 70'), { value: [10, 20, 110, 70] })
  assert.deepEqual(parseValue(points, '10, 20; 30, 40'), { value: [[10, 20], [30, 40]] })
  assert.deepEqual(parseValue(points, '10, 20; 30'), { error: { kind: 'points', count: 2 } })
  assert.deepEqual(parseValue(words, 'alpha, beta gamma'), { value: ['alpha', 'beta gamma'] })
  assert.deepEqual(parseValue({ type: 'array' }, 'x'), { error: { kind: 'json' } })
})

test('numbers and whole numbers are checked before the command runs', () => {
  assert.deepEqual(parseValue({ type: 'integer' }, '3'), { value: 3 })
  assert.deepEqual(parseValue({ type: 'integer' }, '3.5'), { error: { kind: 'whole' } })
  assert.deepEqual(parseValue({ type: 'number' }, 'abc'), { error: { kind: 'number' } })
  assert.deepEqual(parseValue({ type: 'string' }, ''), { value: undefined })
  assert.deepEqual(parseValue({}, '{"name":"Ada"}'), { value: { name: 'Ada' } })
})

test('values show without brackets and come back the same', () => {
  assert.equal(shownValue(pages, [3]), '3')
  assert.equal(shownValue(point, [72, 72]), '72, 72')
  assert.equal(shownValue(points, [[1, 2], [3, 4]]), '1, 2; 3, 4')
  assert.equal(shownValue({ type: 'object' }, { a: 1 }), '{"a":1}')
  assert.equal(initialText({ type: 'boolean' }, undefined), 'false')
  assert.equal(initialText({ type: 'string', enum: ['a', 'b'], required: true }, undefined), 'a')
  for (const [node, value] of [[pages, [2, 3]], [point, [1.5, 2]], [points, [[1, 2], [3, 4]]], [words, ['a', 'b']]]) {
    assert.deepEqual(parseValue(node, shownValue(node, value)), { value })
  }
})

test('an engine error names its field', () => {
  const keys = ['page', 'pages', 'to', 'at', 'contents']
  assert.equal(errorField('pages must be an array of integers', keys), 'pages')
  assert.equal(errorField('at must be an array of 2 numbers', keys), 'at')
  assert.equal(errorField('page.move: unknown argument "to"', keys), 'to')
  assert.equal(errorField('page 99 does not exist', keys), 'page')
  assert.equal(errorField('the document is read-only', keys), null)
  assert.equal(humanize('fontSize'), 'Font size')
  assert.equal(humanize('out_dir'), 'Out dir')
})

test('a selection marks one run per page, each line one quad in the engine\'s corner order', () => {
  assert.deepEqual(rectQuad([10, 20, 110, 32]), [10, 20, 110, 20, 10, 32, 110, 32])
  assert.equal(rectQuad([10, 20, 10, 32]), null)
  assert.equal(rectQuad([0, 0, Number.NaN, 1]), null)
  const selected = { text: 'a\nb\nc', pages: [{ page: 2, text: 'a\nb', rects: [[72, 100, 300, 112], [72, 115, 200, 127]] }, { page: 3, text: 'c', rects: [[72, 50, 90, 62]] }] }
  assert.deepEqual(selectionRuns(selected), [
    { page: 2, quads: [[72, 100, 300, 100, 72, 112, 300, 112], [72, 115, 200, 115, 72, 127, 200, 127]] },
    { page: 3, quads: [[72, 50, 90, 50, 72, 62, 90, 62]] },
  ])
  assert.deepEqual(selectionRuns(null), [])
  assert.deepEqual(matchRuns({ page: 4, rects: [[1, 2, 3, 4]] }), [{ page: 4, quads: [[1, 2, 3, 2, 1, 4, 3, 4]] }])
  assert.deepEqual(selectionEnd(selected), { page: 3, rect: [72, 50, 90, 62] })
  assert.equal(selectionBounds(selected), null)
  assert.deepEqual(selectionBounds({ text: 'a', pages: [selected.pages[0]] }), { page: 2, rect: [72, 100, 300, 127] })
})

test('list bounds are read back from the description the editor\'s schema carries them in', async () => {
  const { withBounds } = await import('./params.ts')
  const node = withBounds({ type: 'array', items: { type: 'number' }, description: '[x, y] in points. (minItems 2, maxItems 2)' })
  assert.equal(node.minItems, 2)
  assert.equal(node.maxItems, 2)
  assert.deepEqual(parseValue(node, '300'), { error: { kind: 'count', min: 2, max: 2 } })
  const nested = withBounds({ type: 'array', items: { type: 'array', items: { type: 'number' }, description: '(minItems 8, maxItems 8)' } })
  assert.equal(nested.items.minItems, 8)
  assert.equal(withBounds({ type: 'string' }).minItems, undefined)
})

test('a dialog starts from what is in front of the person: the selection, the last press, the current page', async () => {
  const { commandDefaults } = await import('./pdf.ts')
  const add = { id: 'comment.add', schema: { page: {}, at: {}, rect: {}, find: {}, type: {}, contents: {} } }
  const move = { id: 'page.move', schema: { pages: {}, to: {} } }
  const blank = { id: 'page.insertBlank', schema: { at: {} } }
  const plain = commandDefaults({ current: 2, click: null, selection: null })
  assert.deepEqual(plain(add), { page: 3, at: [72, 72], type: 'note', contents: '' })
  assert.deepEqual(plain(move), { pages: [3] })
  assert.deepEqual(plain(blank), { at: 4 })
  const clicked = commandDefaults({ current: 2, click: { page: 3, point: [300.4, 410.6] }, selection: null })
  assert.deepEqual(clicked(add).at, [300, 411])
  assert.deepEqual(commandDefaults({ current: 0, click: { page: 3, point: [300, 400] }, selection: null })(add).at, [72, 72])
  const selected = commandDefaults({ current: 0, click: null, selection: { page: 5, rect: [72.2, 100, 300, 112.6], text: 'quiet  aurora\n' } })
  assert.deepEqual(selected(add), { page: 5, at: [72, 72], rect: [72, 100, 300, 113], find: 'quiet aurora', type: 'note', contents: '' })
})

test('a comment\'s time reads the forms the engine answers', async () => {
  const { pdfDate, shownDate, statusOf, sameComment, commentRef } = await import('./comments.ts')
  assert.equal(pdfDate('D:20261120100000Z').toISOString(), '2026-11-20T10:00:00.000Z')
  assert.equal(pdfDate("D:20261120100000+01'00'").toISOString(), '2026-11-20T09:00:00.000Z')
  assert.equal(pdfDate('2026-11-20 10:00').getHours(), 10)
  assert.equal(pdfDate('not a date'), null)
  assert.equal(shownDate(null, 'en-US'), '')
  assert.equal(shownDate('2026-11-20 10:00', 'en-US', new Date(2026, 10, 20, 10, 0, 20).getTime()), 'now')
  assert.equal(statusOf({ status: 'Accepted' }), 'accepted')
  assert.equal(statusOf({ status: 'Odd' }), 'none')
  assert.equal(sameComment({ id: 'a', page: 1, index: 1 }, { id: 'a', page: 2, index: 3 }), true)
  assert.equal(sameComment({ id: null, page: 1, index: 1 }, { id: null, page: 1, index: 1 }), true)
  assert.deepEqual(commentRef({ id: null, page: 2, index: 3 }), { page: 2, index: 3 })
})
