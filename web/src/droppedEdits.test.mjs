// The notice of dropped edits: only the growth of the engine's count since the editor last read it.
import test from 'node:test'
import assert from 'node:assert/strict'
import { newlyDropped } from './droppedEdits.ts'

test('the edits dropped since the last reading', () => {
  assert.equal(newlyDropped(0, 0), 0)
  assert.equal(newlyDropped(0, 3), 3)
  assert.equal(newlyDropped(3, 5), 2)
  assert.equal(newlyDropped(5, 5), 0)
})

test('a count that did not grow or is not a number tells nothing', () => {
  assert.equal(newlyDropped(5, 2), 0)
  assert.equal(newlyDropped(0, Number.NaN), 0)
  assert.equal(newlyDropped(Number.NaN, 4), 0)
})
