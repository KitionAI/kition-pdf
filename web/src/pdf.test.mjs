import test from 'node:test'
import assert from 'node:assert/strict'
import { pageFromField, pagesPanBounds } from './pdf.ts'

test('panning stops at every page plus the opening margin', () => {
  const pages = [{ x0: 0, y0: 0, x1: 612, y1: 792 }, { x0: 0, y0: 808, x1: 842, y1: 1403 }]
  assert.deepEqual(pagesPanBounds(pages), { x0: -24, y0: -24, x1: 866, y1: 1427 })
})

test('no pages, or only empty ones, leave the canvas unbounded', () => {
  assert.equal(pagesPanBounds([]), null)
  assert.equal(pagesPanBounds([{ x0: 0, y0: 0, x1: 0, y1: 0 }]), null)
})

test('the page field takes a label or a number kept to the document, and nothing else', () => {
  const pages = [{ label: 'i' }, { label: 'ii' }, { label: '1' }, { label: '2' }]
  assert.equal(pageFromField('ii', pages, 4), 1)
  assert.equal(pageFromField(' 2 ', pages, 4), 3)
  assert.equal(pageFromField('3', [{ label: '1' }, { label: '2' }, { label: '3' }], 3), 2)
  assert.equal(pageFromField('99', pages, 4), 3)
  assert.equal(pageFromField('0', [{ label: '1' }, { label: '2' }], 2), 0)
  for (const text of ['abc', '', '  ', '2.5', '-1', '1e3']) assert.equal(pageFromField(text, pages, 4), null, text)
})
