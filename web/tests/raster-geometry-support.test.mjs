import test from 'node:test'
import assert from 'node:assert/strict'
import { devicePoint, transformPoint, inversePoint, segmentDistance, analyzeGeometry, geometrySummary } from './raster-geometry-support.mjs'

const manifest = { coordinateSpace: 'pdf-user-space-y-up', rotation: 0, pageWidth: 20, pageHeight: 20, glyphShape: { rectFormat: 'x0,y0,x1,y1', rects: [[0, 0, 6, 12], [6, 0, 10, 6]] } }
const white = () => new Uint8Array(20 * 20 * 4).fill(255)
const set = (pixels, x, y, value) => { for (let channel = 0; channel < 3; channel++) pixels[(y * 20 + x) * 4 + channel] = value }
const line = { segments: [{ id: 'line', x0: 2, y0: 10, x1: 18, y1: 10, width: 0 }], glyphs: [] }

test('matrix and PDF y-axis conversion use final glyph placement', () => {
  assert.deepEqual(devicePoint(3, 5, 20, 4), [12, 60])
  const matrix = [0, 1, -1, 0, 15, 2]
  assert.deepEqual(transformPoint(matrix, 4, 7), [8, 6])
  assert.deepEqual(inversePoint(matrix, 8, 6), [4, 7])
  assert.equal(segmentDistance(4, 3, [0, 0], [8, 0]), 3)
  assert.throws(() => inversePoint([1, 1, 1, 1, 0, 0], 0, 0))
})
test('stroke geometry tolerates AA mass differences but rejects missing and displaced ink', () => {
  const dark = white(), light = white(), displaced = white(), broken = white()
  for (let x = 2; x < 18; x++) { set(dark, x, 9, 0); set(light, x, 9, 200); set(displaced, x, 13, 0); if (x < 8 || x > 10) set(broken, x, 9, 0) }
  assert.equal(analyzeGeometry(dark, 20, 20, manifest, line, 1).status, 'passed')
  assert.equal(analyzeGeometry(light, 20, 20, manifest, line, 1).status, 'passed')
  assert.equal(analyzeGeometry(white(), 20, 20, manifest, line, 1).status, 'failed')
  assert.equal(analyzeGeometry(displaced, 20, 20, manifest, line, 1).status, 'failed')
  assert.equal(analyzeGeometry(broken, 20, 20, manifest, line, 1).status, 'failed')
})
test('a glyph needs both its expected deep interior and a blank surrounding guard', () => {
  const pixels = white()
  for (let y = 4; y < 16; y++) for (let x = 4; x < 10; x++) set(pixels, x, y, 0)
  for (let y = 10; y < 16; y++) for (let x = 10; x < 14; x++) set(pixels, x, y, 0)
  const fixture = { segments: [], glyphs: [{ id: 'L', matrix: [1, 0, 0, 1, 4, 4] }] }
  assert.equal(analyzeGeometry(pixels, 20, 20, manifest, fixture, 1).status, 'passed')
  set(pixels, 6, 8, 32)
  assert.equal(analyzeGeometry(pixels, 20, 20, manifest, fixture, 1).status, 'passed')
  set(pixels, 6, 8, 33)
  assert.equal(analyzeGeometry(pixels, 20, 20, manifest, fixture, 1).status, 'failed')
  set(pixels, 6, 8, 255)
  assert.equal(analyzeGeometry(pixels, 20, 20, manifest, fixture, 1).status, 'failed')
  set(pixels, 6, 8, 0); set(pixels, 12, 6, 0)
  assert.equal(analyzeGeometry(pixels, 20, 20, manifest, fixture, 1).status, 'failed')
})
test('failure and timeout remain in the planned render denominator', () => {
  assert.deepEqual(geometrySummary([{ status: 'passed' }, { status: 'failed' }, { status: 'timeout' }, { status: 'untested' }]), { planned: 4, passed: 1, failed: 1, timeout: 1, untested: 1, completionRate: 0.25 })
})
