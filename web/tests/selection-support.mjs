import assert from 'node:assert/strict'

const center = ([x0, y0, x1, y1]) => [(x0 + x1) / 2, (y0 + y1) / 2]
function halves(rect, rotation) {
  const [x0, y0, x1, y1] = rect, x = (x0 + x1) / 2, y = (y0 + y1) / 2
  if (rotation === 0) return [[x0, y0, x, y1], [x, y0, x1, y1]]
  if (rotation === 90) return [[x0, y0, x1, y], [x0, y, x1, y1]]
  if (rotation === 180) return [[x, y0, x1, y1], [x0, y0, x, y1]]
  if (rotation === 270) return [[x0, y, x1, y1], [x0, y0, x1, y]]
  throw new Error('Fixture rotation must be a right angle')
}

/** Derive authored glyph rectangles from full lines, checking against the partial-query oracle. */
export function fixtureGlyphs(page) {
  const full = page.queries.find(item => item.query === 'AB CD').rects
  const partial = page.queries.find(item => item.query === 'B C').rects
  assert.equal(full.length, 2)
  const [a, b] = halves(full[0], page.rotation), [c, d] = halves(full[1], page.rotation)
  assert.deepEqual([b, c], partial, 'Authored glyph split must agree with independent partial rectangles')
  return { a, b, c, d, full, partial }
}

export function selectionCases(page) {
  const { a, b, c, d, full, partial } = fixtureGlyphs(page)
  const expected = (text, rects) => ({ text, pages: [{ page: page.page, text, rects }] })
  const union = [Math.min(a[0], c[0]), Math.min(a[1], c[1]), Math.max(a[2], c[2]), Math.max(a[3], c[3])]
  return [
    { id: 'range-forward', mode: 'range', anchor: center(a), focus: center(d), expected: expected('AB\nCD', full) },
    { id: 'range-reverse', mode: 'range', anchor: center(d), focus: center(a), expected: expected('AB\nCD', full) },
    { id: 'range-partial', mode: 'range', anchor: center(b), focus: center(c), expected: expected('B\nC', partial) },
    { id: 'column-first-glyphs', mode: 'column', anchor: union.slice(0, 2), focus: union.slice(2), expected: expected(page.rotation === 180 ? 'C\nA' : 'A\nC', [a, c]) },
  ]
}

export function crossPageCase(first, second, reverse = false) {
  const left = fixtureGlyphs(first), right = fixtureGlyphs(second)
  const endpoints = { page: first.page, anchor: center(left.c), end_page: second.page, focus: center(right.b) }
  const expected = { text: 'CD\nAB', pages: [{ page: first.page, text: 'CD', rects: [left.full[1]] }, { page: second.page, text: 'AB', rects: [right.full[0]] }] }
  return { id: reverse ? 'cross-page-reverse' : 'cross-page-forward', mode: 'range', ...(reverse ? { page: endpoints.end_page, anchor: endpoints.focus, end_page: endpoints.page, focus: endpoints.anchor } : endpoints), expected }
}

export function assertSelected(actual, expected, tolerance = 0.02) {
  assert.ok(actual && actual.text === expected.text, 'Selected plain text differs from authored text')
  assert.equal(actual.pages.length, expected.pages.length, 'Selected page count')
  expected.pages.forEach((page, index) => {
    const selected = actual.pages[index]
    assert.equal(selected.page, page.page)
    assert.ok(selected.text === page.text, 'Selected per-page text differs from authored text')
    assert.equal(selected.rects.length, page.rects.length, 'Selected rectangle count')
    page.rects.forEach((rect, r) => rect.forEach((value, axis) => assert.ok(Number.isFinite(selected.rects[r][axis]) && Math.abs(selected.rects[r][axis] - value) <= tolerance, `Selected rectangle ${r} coordinate ${axis} differs`)))
  })
}
