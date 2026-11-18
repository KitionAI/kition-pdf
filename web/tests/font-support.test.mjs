import test from 'node:test'
import assert from 'node:assert/strict'
import { analyzeFont, compareFontRasters, fontGlyphShape, fontSummary, validateFontManifest } from './font-support.mjs'

function fixture(gid = 2, asserted = true) {
  const white = [255, 255, 255], black = [0, 0, 0]
  return {
    id: 'original', file: 'original.pdf', fontFile: 'original.font', sha256: 'a'.repeat(64), fontSha256: 'b'.repeat(64),
    classification: asserted ? 'format-control' : 'malformed-wrapper-mapping-probe', assertMapping: asserted, candidateGlyphId: gid,
    regions: [
      ['left', [13, 19, 2, 2], gid === 1 ? black : white, asserted],
      ['right', [21, 19, 2, 2], gid === 2 ? black : white, asserted],
      ['foot', [25, 11, 2, 2], gid === 2 ? black : white, asserted],
      ['counter', [25, 16, 2, 6], white, true],
      ['marker', [1, 1, 2, 2], [255, 0, 0], true],
      ['background', [32, 30, 2, 2], white, true],
    ].map(([name, rect, rgb, assertColor]) => ({ name, rect, rgb, assertColor, tolerance: 8 })),
  }
}
const manifest = cases => ({ schema: 1, coordinateSpace: 'pdf-user-space-y-up', pageWidth: 40, pageHeight: 40, scales: [1, 2], cases })
function raster(gid, scale = 1) {
  const pixels = new Uint8Array(40 * scale * 40 * scale * 4).fill(255)
  const paint = ([x, y, width, height], rgb) => {
    for (let py = y * scale; py < (y + height) * scale; py++) for (let px = x * scale; px < (x + width) * scale; px++) {
      pixels.set([...rgb, 255], ((40 * scale - 1 - py) * 40 * scale + px) * 4)
    }
  }
  if (gid === 1) paint([12, 10, 4, 14], [0, 0, 0])
  if (gid === 2) { paint([20, 10, 4, 14], [0, 0, 0]); paint([24, 10, 4, 4], [0, 0, 0]) }
  paint([0, 0, 4, 4], [255, 0, 0])
  return pixels
}
const analyze = (pixels, f, scale = 1) => analyzeFont(pixels, 40 * scale, 40 * scale, manifest([f]), f, scale)

test('disjoint interiors identify both original glyphs and empty outlines at both scales', () => {
  for (const gid of [0, 1, 2, 3]) for (const scale of [1, 2]) {
    const f = fixture(gid)
    validateFontManifest(manifest([f]))
    const measured = analyze(raster(gid, scale), f, scale)
    assert.equal(measured.status, 'passed')
    assert.equal(measured.mapping.status, 'passed')
    assert.equal(measured.mapping.inferredShape, gid === 1 ? 'left-bar' : gid === 2 ? 'right-l' : 'empty')
    assert.equal(measured.regions[0].samples, 4 * scale * scale)
  }
})
test('prepatch direct GID rendering fails a normative charset assertion', () => {
  const measured = analyze(raster(1), fixture(2))
  assert.equal(measured.status, 'failed')
  assert.equal(measured.mapping.status, 'failed')
  assert.equal(measured.mapping.inferredShape, 'left-bar')
  assert.equal(measured.regions.filter(region => region.status === 'failed').length, 3)
})
test('malformed mapping observations retain disagreement without claiming an asserted pass', () => {
  const measured = analyze(raster(1), fixture(2, false))
  assert.equal(measured.status, 'passed')
  assert.equal(measured.mapping.status, 'observed')
  assert.equal(measured.mapping.asserted, false)
  assert.equal(measured.mapping.candidateMatches, false)
  assert.equal(measured.regions.filter(region => !region.assertColor && region.mismatchedPixels > 0).length, 3)
})
test('one wrong sentinel pixel, transparent glyph or filled counter fails even an observational case', () => {
  const at = (x, y) => ((39 - y) * 40 + x) * 4
  const f = fixture(2, false)
  for (const [x, y, channel, value] of [[1, 1, 0, 0], [13, 19, 3, 0], [25, 16, 0, 0]]) {
    const pixels = raster(1); pixels[at(x, y) + channel] = value
    assert.equal(analyze(pixels, f).status, 'failed')
  }
})
test('mixed glyph probes remain unknown and cannot pass a normative assertion', () => {
  const pixels = raster(2); pixels[((39 - 19) * 40 + 13) * 4] = 128
  assert.equal(analyze(pixels, fixture(2)).mapping.inferredShape, 'unknown')
  assert.equal(analyze(pixels, fixture(2)).status, 'failed')
  const observed = analyze(pixels, fixture(2, false))
  assert.equal(observed.mapping.inferredShape, 'unknown')
  assert.equal(observed.mapping.status, 'observed')
  assert.equal(observed.status, 'failed')
  assert.ok(observed.failures.includes('unrecognized-authored-glyph-geometry'))
})
test('bad geometry, missing program provenance, missing roles and disabled normative checks are rejected', () => {
  const f = fixture()
  assert.throws(() => validateFontManifest(manifest([{ ...f, fontSha256: undefined }])), /SHA256/)
  assert.throws(() => validateFontManifest(manifest([{ ...f, regions: f.regions.slice(1) }])), /regions/)
  assert.throws(() => validateFontManifest(manifest([{ ...f, assertMapping: false }])), /must assert/)
  assert.throws(() => validateFontManifest(manifest([{ ...f, expectedMatch: 'missing' }])), /comparison/)
  const changed = structuredClone(f); changed.regions[3].assertColor = false
  assert.throws(() => validateFontManifest(manifest([changed])), /Only descriptive/)
  changed.regions[3].assertColor = true; changed.regions[0].rect = [39, 0, 2, 2]
  assert.throws(() => validateFontManifest(manifest([changed])), /rectangle/)
  assert.throws(() => analyzeFont(raster(2), 39, 40, manifest([f]), f, 1), /dimensions/)
  const tiny = structuredClone(f); tiny.regions[0].rect = [0.1, 0.1, 0.1, 0.1]
  assert.ok(analyze(raster(2), tiny).regions[0].failures.includes('no-pixel-centers-in-authored-region'))
})
test('exact pairs need complete hashes and asserted mapping, separate from region measurements', () => {
  const sample = { ...analyze(raster(2), fixture(2)), rgbaSha256: 'a'.repeat(64) }
  assert.equal(compareFontRasters(sample, sample).status, 'passed')
  assert.equal(compareFontRasters(sample, { ...sample, rgbaSha256: 'b'.repeat(64) }).status, 'failed')
  assert.equal(compareFontRasters(sample, { ...sample, mapping: { asserted: false } }).status, 'failed')
  assert.equal(compareFontRasters(sample, { status: 'untested' }).status, 'untested')
  assert.equal(compareFontRasters(sample, { status: 'timeout' }).status, 'failed')
})
test('summary keeps failures, timeouts and untested samples explicit', () => {
  assert.deepEqual(fontSummary(['passed', 'failed', 'timeout', 'untested'].map(status => ({ status }))), { planned: 4, passed: 1, failed: 1, timeout: 1, untested: 1 })
  assert.throws(() => fontSummary([{ status: 'observed' }]), /Unknown/)
})

test('visible .notdef keeps semantic GID zero while requiring its authored shape', () => {
  for (const [shape, outline] of [['left-bar', 1], ['right-l', 2]]) for (const scale of [1, 2]) {
    const f = { ...fixture(outline), candidateGlyphId: 0, notdefShape: shape }
    validateFontManifest(manifest([f]))
    const measured = analyze(raster(outline, scale), f, scale)
    assert.equal(measured.status, 'passed')
    assert.equal(measured.mapping.candidateGlyphId, 0)
    assert.equal(measured.mapping.expectedShape, shape)
    assert.equal(analyze(raster(0, scale), f, scale).status, 'failed')
    assert.equal(analyze(raster(outline === 1 ? 2 : 1, scale), f, scale).status, 'failed')
  }
})

test('invalid direct GID three remains empty despite a visible .notdef declaration', () => {
  const f = { ...fixture(3), classification: 'malformed-explicit-map-compatibility', notdefShape: 'right-l' }
  validateFontManifest(manifest([f]))
  assert.equal(fontGlyphShape(f), 'empty')
  const measured = analyze(raster(3), f)
  assert.equal(measured.status, 'passed')
  assert.equal(measured.mapping.candidateGlyphId, 3)
  assert.equal(analyze(raster(2), f).status, 'failed')
})

test('missing or explicit empty .notdef preserves prior geometry for every supported GID', () => {
  for (const gid of [0, 1, 2, 3]) for (const scale of [1, 2]) {
    const original = fixture(gid), explicit = { ...original, notdefShape: 'empty' }
    validateFontManifest(manifest([explicit]))
    assert.deepEqual(analyze(raster(gid, scale), explicit, scale), analyze(raster(gid, scale), original, scale))
  }
})

test('unknown .notdef metadata cannot silently choose a fallback shape', () => {
  for (const value of [null, '', 'right', 'unknown', 2, {}, []]) {
    const f = { ...fixture(0), notdefShape: value }
    assert.throws(() => validateFontManifest(manifest([f])), /notdef shape metadata/)
    assert.throws(() => fontGlyphShape(f), /notdef shape metadata/)
  }
})
