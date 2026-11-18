import test from 'node:test'
import assert from 'node:assert/strict'
import { analyzeMesh, compareMeshRasters, meshSummary, validateMeshManifest } from './mesh-support.mjs'

const fixture = { id: 'original', file: 'original.pdf', sha256: 'a'.repeat(64), shadingType: 6, isStress: false, regions: [{ rect: [1, 1, 2, 2], rgb: [255, 0, 0], tolerance: 8 }] }
const manifest = { schema: 1, coordinateSpace: 'pdf-user-space-y-up', pageWidth: 40, pageHeight: 40, scales: [1, 2], cases: [fixture] }
function pixels(scale = 1) {
  const data = new Uint8Array(40 * scale * 40 * scale * 4).fill(255)
  for (let y = 37 * scale; y < 39 * scale; y++) for (let x = scale; x < 3 * scale; x++) {
    const offset = (y * 40 * scale + x) * 4
    data[offset + 1] = 0; data[offset + 2] = 0
  }
  return data
}
test('PDF Y-up authored regions sample the correct pixels at both scales', () => {
  validateMeshManifest(manifest)
  for (const scale of manifest.scales) {
    const measured = analyzeMesh(pixels(scale), 40 * scale, 40 * scale, manifest, fixture, scale)
    assert.equal(measured.status, 'passed')
    assert.equal(measured.regions[0].samples, 4 * scale * scale)
  }
})
test('one wrong sentinel pixel fails rather than averaging into a pass', () => {
  const data = pixels(); data[(37 * 40 + 1) * 4] = 0
  const measured = analyzeMesh(data, 40, 40, manifest, fixture, 1)
  assert.equal(measured.status, 'failed')
  assert.equal(measured.regions[0].mismatchedPixels, 1)
})
test('RGB tolerance is inclusive while transparent expected colors fail', () => {
  const data = pixels(), offset = (37 * 40 + 1) * 4
  data[offset + 1] = 8
  assert.equal(analyzeMesh(data, 40, 40, manifest, fixture, 1).status, 'passed')
  data[offset + 1] = 9
  assert.equal(analyzeMesh(data, 40, 40, manifest, fixture, 1).status, 'failed')
  data[offset + 1] = 0; data[offset + 3] = 0
  assert.equal(analyzeMesh(data, 40, 40, manifest, fixture, 1).regions[0].alphaMismatches, 1)
})
test('empty pixel-center regions and wrong raster dimensions cannot pass', () => {
  const tiny = { ...fixture, regions: [{ rect: [0.1, 0.1, 0.1, 0.1], rgb: [255, 255, 255], tolerance: 8 }] }
  assert.deepEqual(analyzeMesh(pixels(), 40, 40, manifest, tiny, 1).regions[0].failures, ['no-pixel-centers-in-authored-region'])
  assert.throws(() => analyzeMesh(pixels(), 39, 40, manifest, fixture, 1), /dimensions/)
})
test('invalid geometry, missing hashes and dangling pair targets are rejected', () => {
  assert.throws(() => validateMeshManifest({ ...manifest, coordinateSpace: 'unknown' }), /geometry/)
  assert.throws(() => validateMeshManifest({ ...manifest, cases: [{ ...fixture, sha256: undefined }] }), /SHA256/)
  assert.throws(() => validateMeshManifest({ ...manifest, cases: [{ ...fixture, expectedMatch: 'missing' }] }), /comparison/)
  assert.throws(() => validateMeshManifest({ ...manifest, cases: [{ ...fixture, regions: [{ ...fixture.regions[0], rect: [39, 0, 2, 2] }] }] }), /rectangle/)
})
test('complete RGBA controls remain separate from authored region assertions', () => {
  const sample = { status: 'passed', rgbaSha256: 'a'.repeat(64) }
  assert.equal(compareMeshRasters(sample, sample).status, 'passed')
  assert.equal(compareMeshRasters(sample, { ...sample, rgbaSha256: 'b'.repeat(64) }).status, 'failed')
  assert.equal(compareMeshRasters(sample, { ...sample, status: 'failed' }).status, 'failed')
  assert.equal(compareMeshRasters(sample, { status: 'untested' }).status, 'untested')
  assert.equal(compareMeshRasters(sample, { status: 'timeout' }).status, 'failed')
})
test('failed, timed out and untested populations remain explicit', () => {
  assert.deepEqual(meshSummary(['passed', 'failed', 'timeout', 'untested'].map(status => ({ status }))), { planned: 4, passed: 1, failed: 1, timeout: 1, untested: 1 })
  assert.throws(() => meshSummary([{ status: 'running' }]), /Unknown/)
})
