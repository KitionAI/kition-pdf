import test from 'node:test'
import assert from 'node:assert/strict'
import { analyzeHairline, compareHairlineIdentity, validateEndpoints, hairlineSummary } from './hairline-support.mjs'

const manifest = { pageWidth: 128, pageHeight: 128, scales: [1, 2, 4] }
function fixture(points = [16, 64.5, 112, 64.5], matrix = [1, 0, 0, 1]) {
  const [a, b, c, d] = matrix, determinant = a * d - b * c
  const dx = points[2] - points[0], dy = points[3] - points[1]
  return { matrix, translation: points.slice(0, 2), localStart: [0, 0], localEnd: [(d * dx - c * dy) / determinant, (-b * dx + a * dy) / determinant], pagePoints: points, normativeHairline: true }
}
function raster(fixture, { scale = 1, lineWidth = 1, displacement = 0, gap = false, alpha = 255 } = {}) {
  const width = manifest.pageWidth * scale, height = manifest.pageHeight * scale
  const pixels = new Uint8Array(width * height * 4).fill(255)
  const [x0, y0, x1, y1] = fixture.pagePoints
  const start = [x0 * scale, (128 - y0) * scale], end = [x1 * scale, (128 - y1) * scale]
  const length = Math.hypot(end[0] - start[0], end[1] - start[1]), tx = (end[0] - start[0]) / length, ty = (end[1] - start[1]) / length
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    let covered = 0
    // Original area-sampled rectangle, independent of the renderer and analysis bins.
    for (let sy = 0; sy < 8; sy++) for (let sx = 0; sx < 8; sx++) {
      const dx = x + (sx + 0.5) / 8 - start[0], dy = y + (sy + 0.5) / 8 - start[1]
      const along = dx * tx + dy * ty, normal = -dx * ty + dy * tx
      if (along >= 0 && along < length && Math.abs(normal - displacement) < lineWidth / 2 && (!gap || along < length / 2 - 3 || along > length / 2 + 3)) covered++
    }
    const offset = (y * width + x) * 4, channel = Math.round(255 * (1 - covered / 64))
    pixels[offset] = channel; pixels[offset + 1] = channel; pixels[offset + 2] = channel; pixels[offset + 3] = alpha
  }
  return analyzeHairline(pixels, width, height, manifest, fixture, scale)
}

test('one-device-pixel horizontal, vertical, and diagonal lines pass', () => {
  for (const points of [[16, 64.5, 112, 64.5], [64.5, 16, 64.5, 112], [24.5, 24.5, 103.5, 103.5]]) {
    const measured = raster(fixture(points))
    assert.equal(measured.status, 'passed', JSON.stringify(measured.failures))
    assert.equal(measured.continuity.covered, measured.continuity.samples)
  }
})
test('hairline device width stays one at scales two and four', () => {
  for (const scale of [2, 4]) assert.equal(raster(fixture(), { scale }).status, 'passed')
})
test('blank and faint rasters fail coverage instead of passing empty bounds', () => {
  for (const lineWidth of [0, 0.125]) assert.ok(raster(fixture(), { lineWidth }).failures.includes('insufficient-device-line-mass'))
  assert.equal(raster(fixture(), { alpha: 0 }).status, 'failed')
})
test('missing middle intervals fail continuity even with substantial total mass', () => {
  const measured = raster(fixture(), { gap: true })
  assert.ok(measured.equivalentWidth > 0.65)
  assert.ok(measured.failures.includes('discontinuous-or-faint-device-line'))
})
test('displaced and thick strokes report distinct geometric failures', () => {
  assert.ok(raster(fixture(), { displacement: 4 }).failures.includes('displaced-device-line'))
  assert.ok(raster(fixture(), { lineWidth: 10 }).failures.includes('excessive-device-line-width'))
  assert.ok(raster(fixture(), { lineWidth: 10 }).failures.includes('ink-outside-device-line-envelope'))
})
test('affine endpoints validate shear and anisotropy independently', () => {
  for (const matrix of [[100, 0, 0, 0.01], [1, 0, 100, 1], [0, 100, -0.01, 0]]) assert.ok(validateEndpoints(fixture([24.5, 24.5, 103.5, 103.5], matrix)).maximumError < 1e-6)
  const wrong = fixture(); wrong.translation[0]++
  assert.throws(() => validateEndpoints(wrong), /endpoints/)
  assert.throws(() => validateEndpoints({ ...fixture(), matrix: [1, 0, 1, 0] }), /Singular/)
})
test('nonzero width is explicitly unassessed, including absent raster ink', () => {
  const measured = raster({ ...fixture(), normativeHairline: false }, { lineWidth: 0 })
  assert.equal(measured.status, 'observed')
  assert.equal(measured.fidelityAssessment, 'unassessed-nonzero-width')
})
test('identity comparison checks mass and position without requiring equal RGBA', () => {
  const identity = { status: 'passed', mass: 96, perpendicularCentroid: 0, rgbaSha256: 'first' }
  assert.equal(compareHairlineIdentity({ ...identity, mass: 98, rgbaSha256: 'second' }, identity).status, 'passed')
  assert.equal(compareHairlineIdentity({ ...identity, mass: 120 }, identity).status, 'failed')
  assert.equal(compareHairlineIdentity({ ...identity, perpendicularCentroid: 0.4 }, identity).status, 'failed')
  assert.equal(compareHairlineIdentity({ status: 'untested' }, identity).status, 'untested')
  assert.equal(compareHairlineIdentity({ status: 'failed' }, identity).status, 'failed')
})
test('summary keeps descriptive, failed, and untested samples separate', () => {
  assert.deepEqual(hairlineSummary(['passed', 'failed', 'timeout', 'untested', 'observed'].map(status => ({ status }))), { planned: 5, passed: 1, failed: 1, timeout: 1, untested: 1, observed: 1 })
  assert.throws(() => hairlineSummary([{ status: 'unknown' }]), /Unknown/)
})
