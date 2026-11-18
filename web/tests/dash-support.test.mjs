import test from 'node:test'
import assert from 'node:assert/strict'
import { analyzeDash, compareDashRasters, dashIntervals, dashSummary, validateDashManifest } from './dash-support.mjs'
const line = (overrides = {}) => ({ localStart: [0, 0], localEnd: [96, 0], matrix: [1, 0, 0, .01, 16, 64.5], dashArray: [12, 12], dashOffset: 0, cap: 'butt', opacity: 1, ...overrides })
const fixture = (overrides = {}) => ({ id: 'control', file: 'control.pdf', sha256: 'a'.repeat(64), role: 'control', classification: 'normative-hairline', regions: [{ id: 'white', rect: [120, 120, 2, 2], rgba: [255, 255, 255, 255], tolerance: 2 }], analyticLine: line(), ...overrides })
const manifest = cases => ({ schema: 1, pageWidth: 128, pageHeight: 128, coordinateSpace: 'pdf-user-space-y-up', scales: [1, 2, 4], cases })
function raster({ solid = false, thick = 1, phase = 0, gray = 0 } = {}) {
  const pixels = new Uint8Array(128 * 128 * 4).fill(255)
  for (let x = 16; x < 112; x++) for (let y = 63 - Math.floor(thick / 2); y < 63 - Math.floor(thick / 2) + thick; y++) if (solid || ((x - 16 + phase) % 24) < 12) pixels.set([gray, gray, gray, 255], (y * 128 + x) * 4)
  return pixels
}
const analyze = (pixels, f = fixture(), scale = 1) => analyzeDash(pixels, 128 * scale, 128 * scale, manifest([f]), f, scale)
test('positive scalar intervals preserve odd parity, negative phase and exact boundaries', () => {
  assert.deepEqual(dashIntervals(line()).intervals.slice(0, 3), [{ start: 0, end: 12, on: true }, { start: 12, end: 24, on: false }, { start: 24, end: 36, on: true }])
  assert.deepEqual(dashIntervals(line({ dashOffset: -6 })).intervals.slice(0, 2), [{ start: 0, end: 6, on: false }, { start: 6, end: 18, on: true }])
  assert.equal(dashIntervals(line({ dashOffset: 12 })).intervals[0].on, false)
  assert.deepEqual(dashIntervals(line({ dashArray: [12] })).intervals, dashIntervals(line()).intervals)
  assert.deepEqual(dashIntervals(line({ dashArray: [5, 3, 2] })).intervals.slice(0, 4), [{ start: 0, end: 5, on: true }, { start: 5, end: 8, on: false }, { start: 8, end: 10, on: true }, { start: 10, end: 15, on: false }])
  assert.deepEqual(dashIntervals(line({ dashOffset: 4806 })).intervals, dashIntervals(line({ dashOffset: 6 })).intervals)
})
test('individual zero entries progress, invalid or excessive intervals reject', () => {
  assert.ok(dashIntervals(line({ dashArray: [24, 0] })).intervals.every(item => item.on))
  assert.ok(dashIntervals(line({ dashArray: [0, 24] })).intervals.every(item => !item.on))
  for (const dashArray of [[0, 0], [-1, 3], [Infinity, 1], [1e-8, 1e-8]]) assert.throws(() => dashIntervals(line({ dashArray })))
  assert.equal(dashIntervals(line({ dashArray: [] })).intervals.length, 1)
})
test('equal page centerlines with different local lengths retain different user-space dashes', () => {
  const a = dashIntervals(line()), b = dashIntervals(line({ localEnd: [48, 0], matrix: [2, 0, 0, .01, 16, 64.5] }))
  assert.deepEqual(a.pageStart, b.pageStart); assert.deepEqual(a.pageEnd, b.pageEnd)
  assert.equal(a.intervals[0].end * a.pageLength / a.localLength, 12)
  assert.equal(b.intervals[0].end * b.pageLength / b.localLength, 24)
})
test('fixed geometry gates accept one-pixel dashes and reject filled gaps, wrong phase and excessive width', () => {
  assert.equal(analyze(raster()).status, 'passed')
  for (const options of [{ solid: true }, { phase: 12 }, { thick: 10 }]) assert.equal(analyze(raster(options)).status, 'failed')
  assert.equal(analyze(new Uint8Array(128 * 128 * 4).fill(255)).status, 'failed')
})
test('half-opacity strokes normalize mass but still require gaps and continuity', () => {
  assert.equal(analyze(raster({ gray: 128 }), fixture({ analyticLine: line({ opacity: .5 }) })).status, 'passed')
  assert.equal(analyze(raster({ gray: 128, solid: true }), fixture({ analyticLine: line({ opacity: .5 }) })).status, 'failed')
})
test('scale-restricted regions are explicitly excluded rather than passed or untested', () => {
  const f = fixture({ regions: [{ id: 'scale-one', rect: [0, 0, 1, 1], rgba: [0, 0, 0, 255], tolerance: 2, scales: [1] }] })
  validateDashManifest(manifest([f]))
  const measured = analyze(new Uint8Array(256 * 256 * 4).fill(255), f, 2)
  assert.deepEqual(measured.excludedRegions, [{ id: 'scale-one', scales: [1], reason: 'outside-authored-region-scales' }])
  assert.equal(measured.regions.length, 0)
  assert.equal(analyze(raster(), f).regions[0].status, 'failed')
  for (const scales of [[], [3], [1, 1], '1']) assert.throws(() => validateDashManifest(manifest([{ ...f, regions: [{ ...f.regions[0], scales }] }])), /scales/)
})
test('independent point probes reject empty controls and shifted dots', () => {
  const f = fixture({ analyticLine: undefined, pointInk: [{ id: 'dot', point: [16.5, 64.5], radius: 2, minMass: .85, maxMass: 1.15, centroidTolerance: .75 }] })
  const pixels = new Uint8Array(128 * 128 * 4).fill(255)
  pixels.set([0, 0, 0, 255], (63 * 128 + 16) * 4)
  assert.equal(analyze(pixels, f).status, 'passed')
  assert.equal(analyze(new Uint8Array(pixels.length).fill(255), f).status, 'failed')
  const shifted = new Uint8Array(pixels.length).fill(255); shifted.set([0, 0, 0, 255], (63 * 128 + 17) * 4)
  assert.equal(analyze(shifted, f).status, 'failed')
})
test('same-backend tolerance is inclusive at two, exact equality distinct, opacity required', () => {
  const sample = { width: 1, height: 1, status: 'passed' }, a = new Uint8Array([10, 10, 10, 255])
  assert.equal(compareDashRasters(sample, sample, a, a).rgbaEqual, true)
  const within = compareDashRasters(sample, sample, a, new Uint8Array([12, 10, 10, 255]))
  assert.equal(within.status, 'passed'); assert.equal(within.rgbaEqual, false)
  assert.equal(compareDashRasters(sample, sample, a, new Uint8Array([13, 10, 10, 255])).status, 'failed')
  assert.equal(compareDashRasters(sample, sample, a, new Uint8Array([10, 10, 10, 254])).status, 'failed')
  assert.equal(compareDashRasters(sample, sample, a, undefined).status, 'untested')
  assert.equal(compareDashRasters(sample, { ...sample, status: 'failed' }, a, a).status, 'failed')
})
test('manifest validates pinned portable pairs and finite geometry', () => {
  const control = fixture(), sample = fixture({ id: 'sample', file: 'sample.pdf', role: 'sample', expectedMatch: 'control' })
  validateDashManifest(manifest([sample, control]))
  assert.throws(() => validateDashManifest(manifest([fixture({ analyticLine: undefined })])), /positive-content/)
  for (const changed of [{ sha256: '' }, { file: '../bad.pdf' }, { expectedMatch: 'missing' }, { analyticLine: line({ matrix: [0, 0, 0, 0, 0, 0] }) }]) assert.throws(() => validateDashManifest(manifest([{ ...sample, ...changed }, control])))
  assert.throws(() => analyzeDash(raster(), 127, 128, manifest([control]), control, 1), /dimensions/)
})
test('summary keeps failures, timeouts and untested work distinct', () => {
  assert.deepEqual(dashSummary(['passed', 'failed', 'timeout', 'untested'].map(status => ({ status }))), { planned: 4, passed: 1, failed: 1, timeout: 1, untested: 1 })
  assert.throws(() => dashSummary([{ status: 'observed' }]), /Unknown/)
})
