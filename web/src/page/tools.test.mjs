import test from 'node:test'
import assert from 'node:assert/strict'
import { DEFAULT_STYLES, TOOLS, fitTextBox, lineEnd, lineLongEnough, rectQuadOf, shapeBigEnough, shapeRect, strokePoints, styleParams, toolDef } from './tools.ts'

test('the rail has Select and the hand, then the twelve comment tools and page text, V and H the only keys', () => {
  assert.deepEqual(TOOLS.map((tool) => tool.id), ['select', 'hand', 'note', 'textbox', 'addText', 'highlight', 'underline', 'strikeout', 'ink', 'rectangle', 'oval', 'line', 'arrow'])
  assert.deepEqual(TOOLS.filter((tool) => tool.shortcut).map((tool) => tool.shortcut), ['V', 'H'])
  // One-shot tools go back to Select; markup, shapes and drawing stay on.
  assert.deepEqual(TOOLS.filter((tool) => tool.oneShot).map((tool) => tool.id), ['note', 'textbox', 'addText'])
  assert.equal(toolDef('ink').selects, false)
  assert.equal(toolDef('rectangle').selects, true)
  assert.equal(toolDef('nothing').id, 'select')
})

test('a new comment carries only the style values that differ from the engine\'s defaults', () => {
  assert.deepEqual(styleParams('rectangle', DEFAULT_STYLES.rectangle), {})
  assert.deepEqual(styleParams('rectangle', { color: '#1a73e8', opacity: 0.5, width: 4 }), { color: '#1A73E8', opacity: 0.5, width: 4 })
  // A highlight has no line width; a note no width either.
  assert.deepEqual(styleParams('highlight', { ...DEFAULT_STYLES.highlight, width: 9 }), {})
  assert.deepEqual(styleParams('note', { ...DEFAULT_STYLES.note, width: 9, opacity: 0.4 }), { opacity: 0.4 })
})

test('a drag makes a rectangle from the press to the release; Shift a square of the larger side', () => {
  assert.deepEqual(shapeRect([100, 100], [40, 160]), [40, 100, 100, 160])
  assert.deepEqual(shapeRect([100, 100], [130, 180], true), [100, 100, 180, 180])
  assert.deepEqual(shapeRect([100, 100], [60, 90], true), [60, 60, 100, 100])
  assert.equal(shapeBigEnough([0, 0, 1.5, 40]), false)
  assert.equal(shapeBigEnough([0, 0, 2, 2]), true)
})

test('Shift keeps a line level or plumb, whichever is nearer', () => {
  assert.deepEqual(lineEnd([10, 10], [100, 30], true), [100, 10])
  assert.deepEqual(lineEnd([10, 10], [20, 90], true), [10, 90])
  assert.deepEqual(lineEnd([10, 10], [20, 90]), [20, 90])
  assert.equal(lineLongEnough([0, 0], [1, 1]), false)
  assert.equal(lineLongEnough([0, 0], [2, 0]), true)
})

test('a stroke drops repeats, keeps its end and turns a click into a dot', () => {
  assert.deepEqual(strokePoints([[0, 0], [0.1, 0], [1, 0], [1.2, 0]], 0.5), [[0, 0], [1, 0], [1.2, 0]])
  assert.deepEqual(strokePoints([[5, 5]]), [[5, 5], [5.1, 5]])
  assert.deepEqual(strokePoints([[0, 0], [Number.NaN, 1], [3, 3]]), [[0, 0], [3, 3]])
  const long = Array.from({ length: 5000 }, (_, i) => [i, 0])
  const thin = strokePoints(long, 0.5)
  assert.ok(thin.length <= 2000)
  assert.deepEqual(thin.at(-1), [4999, 0])
})

test('an area is one quad in comment.add\'s corner order', () => {
  assert.deepEqual(rectQuadOf([10, 20, 30, 40]), [10, 20, 30, 20, 10, 40, 30, 40])
})

test('a clicked text box fits its text: 40 to 300 points wide, wrapped beyond, kept on the page', () => {
  const measure = (line) => line.length * 6
  assert.deepEqual(fitTextBox('Hi', [100, 100], measure, 612, 792), [100, 100, 140, 119])
  const wide = fitTextBox('word '.repeat(40).trim(), [100, 100], measure, 612, 792)
  assert.equal(wide[2] - wide[0] <= 300, true)
  assert.ok(wide[3] - wide[1] > 30)
  const corner = fitTextBox('Near the edge of the page', [600, 780], measure, 612, 792)
  assert.ok(corner[2] <= 612 && corner[3] <= 792)
})
