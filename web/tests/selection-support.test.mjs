import test from 'node:test'
import assert from 'node:assert/strict'
import { selectionCases, crossPageCase, assertSelected } from './selection-support.mjs'

const page = (number, rotation, full, partial) => ({ page: number, rotation, queries: [{ query: 'AB CD', rects: full }, { query: 'B C', rects: partial }] })
const pages = [
  page(1, 0, [[50, 44, 74, 64], [50, 84, 74, 104]], [[62, 44, 74, 64], [50, 84, 62, 104]]),
  page(2, 90, [[136, 50, 156, 74], [96, 50, 116, 74]], [[136, 62, 156, 74], [96, 50, 116, 62]]),
  page(3, 180, [[166, 136, 190, 156], [166, 96, 190, 116]], [[166, 136, 178, 156], [178, 96, 190, 116]]),
  page(4, 270, [[44, 166, 64, 190], [84, 166, 104, 190]], [[44, 166, 64, 178], [84, 178, 104, 190]]),
]
for (const fixture of pages) test(`selection oracle derives forward/reverse/column glyphs at rotation ${fixture.rotation}`, () => {
  const [forward, reverse, partial, column] = selectionCases(fixture)
  assert.deepEqual(forward.anchor, reverse.focus)
  assert.deepEqual(forward.focus, reverse.anchor)
  assert.deepEqual(forward.expected, reverse.expected)
  assert.equal(partial.expected.text, 'B\nC')
  assert.equal(column.expected.text, fixture.rotation === 180 ? 'C\nA' : 'A\nC')
  assert.equal(column.expected.pages[0].rects.length, 2)
  assertSelected(forward.expected, reverse.expected)
})
test('cross-page reverse preserves ascending page/text order', () => {
  const forward = crossPageCase(pages[0], pages[1]), reverse = crossPageCase(pages[0], pages[1], true)
  assert.deepEqual(forward.expected, reverse.expected)
  assert.equal(forward.expected.text, 'CD\nAB')
  assert.equal(reverse.page, 2)
  assert.equal(reverse.endPage, 1)
})
test('oracle rejects wrong text, missing rectangles, and displaced geometry', () => {
  const { expected } = selectionCases(pages[0])[0]
  assert.throws(() => assertSelected({ ...expected, text: 'wrong' }, expected))
  assert.throws(() => assertSelected({ ...expected, pages: [{ ...expected.pages[0], rects: [] }] }, expected))
  const moved = structuredClone(expected); moved.pages[0].rects[0][0] += 1
  assert.throws(() => assertSelected(moved, expected))
})
