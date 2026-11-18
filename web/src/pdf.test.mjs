import test from 'node:test'
import assert from 'node:assert/strict'
import { pagesPanBounds } from './pdf.ts'

test('panning stops at every page plus the opening margin', () => {
  const pages = [{ x0: 0, y0: 0, x1: 612, y1: 792 }, { x0: 0, y0: 808, x1: 842, y1: 1403 }]
  assert.deepEqual(pagesPanBounds(pages), { x0: -24, y0: -24, x1: 866, y1: 1427 })
})

test('no pages, or only empty ones, leave the canvas unbounded', () => {
  assert.equal(pagesPanBounds([]), null)
  assert.equal(pagesPanBounds([{ x0: 0, y0: 0, x1: 0, y1: 0 }]), null)
})
