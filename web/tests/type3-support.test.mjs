import test from 'node:test'
import assert from 'node:assert/strict'
import { expectedRejection, plannedCases, validateType3Manifest, compareType3Rasters, analyzeType3 } from './type3-support.mjs'

const region = { rect: [1, 1, 2, 2], rgb: [255, 0, 0], tolerance: 8 }
const control = { id: 'clean', file: 'clean.pdf', sha256: 'a'.repeat(64), glyphKind: 'd0', isStress: false, expectedTruncation: false, regions: [region] }
const cyclic = { ...control, id: 'cycle', file: 'cycle.pdf', isStress: true, expectedTruncation: true, expectedMatch: 'clean' }
const manifest = { schema: 1, coordinateSpace: 'pdf-user-space-y-up', pageWidth: 40, pageHeight: 40, pageCount: 2, resetPage: { index: 1, regions: [region] }, scales: [1, 2], cases: [cyclic, control] }

test('cyclic fixtures plan explicit errors while clean fixtures plan pixels, with two passes and reset', () => {
  validateType3Manifest(manifest)
  const [cycle, clean] = plannedCases(manifest)
  assert.equal(cycle.samples.length, 4)
  assert.equal(clean.samples.length, 4)
  assert.ok(cycle.samples.every(sample => sample.expected === 'rejection' && sample.regions.length === 0 && sample.status === 'untested'))
  assert.ok(clean.samples.every(sample => sample.expected === 'raster' && sample.regions.length === 1 && sample.status === 'untested'))
  assert.deepEqual(cycle.samples.map(sample => [sample.pass, sample.scale]), [[1, 1], [1, 2], [2, 1], [2, 2]])
  assert.equal(cycle.resetPage.status, 'untested')
  assert.equal(cycle.recovery.control, 'clean')
  assert.equal(clean.recovery, null)
})

test('only the actionable SDK budget rejection satisfies expected truncation', () => {
  const reason = 'Could not render PDF page 1 at scale 1 (tile None): page content exceeded the rendering budget. Retry the page or reopen the document.'
  assert.equal(expectedRejection(reason), true)
  for (const wrong of [undefined, '', 'render timed out', 'page content exceeded the rendering budget', reason.replace('page content exceeded the rendering budget', 'unknown PDF font')]) assert.equal(expectedRejection(wrong), false)
})

test('missing warning expectations or a truncated recovery target are rejected', () => {
  assert.throws(() => validateType3Manifest({ ...manifest, cases: [{ ...cyclic, expectedTruncation: undefined }, control] }), /classification/)
  assert.throws(() => validateType3Manifest({ ...manifest, cases: [{ ...cyclic, expectedMatch: undefined }, control] }), /recovery control/)
  assert.throws(() => validateType3Manifest({ ...manifest, cases: [cyclic, { ...control, expectedTruncation: true }] }), /comparison target/)
  assert.throws(() => validateType3Manifest({ ...manifest, pageCount: 1 }), /reset page/)
})

test('portable pinned fixtures and reset-page geometry are enforced', () => {
  assert.throws(() => validateType3Manifest({ ...manifest, cases: [{ ...cyclic, file: '../cycle.pdf' }, control] }), /filename/)
  assert.throws(() => validateType3Manifest({ ...manifest, cases: [{ ...cyclic, sha256: '' }, control] }), /SHA256/)
  assert.throws(() => validateType3Manifest({ ...manifest, resetPage: { index: 1, regions: [{ ...region, rect: [39, 0, 2, 2] }] } }), /rectangle/)
})

test('one missing later sentinel fails and repeat comparison requires complete RGBA equality', () => {
  const pixels = new Uint8Array(40 * 40 * 4).fill(255)
  for (let y = 37; y < 39; y++) for (let x = 1; x < 3; x++) { const offset = (y * 40 + x) * 4; pixels[offset + 1] = 0; pixels[offset + 2] = 0 }
  assert.equal(analyzeType3(pixels, 40, 40, manifest, control, 1).status, 'passed')
  pixels[(37 * 40 + 1) * 4] = 0
  assert.equal(analyzeType3(pixels, 40, 40, manifest, control, 1).regions[0].mismatchedPixels, 1)
  assert.equal(compareType3Rasters({ status: 'passed', rgbaSha256: 'a' }, { status: 'passed', rgbaSha256: 'b' }).status, 'failed')
  assert.equal(compareType3Rasters({ status: 'passed', rgbaSha256: 'a' }, { status: 'untested' }).status, 'untested')
})
