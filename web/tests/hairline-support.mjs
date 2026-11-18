// Independent analytic checks for contributor-original affine stroke fixtures.
export const hairlineLimits = Object.freeze({ minimumWidth: 0.65, maximumWidth: 1.5, centroid: 0.65, endpointExclusion: 2, continuityInk: 0.1, continuityTangent: 0.8, continuityNormal: 1.5, envelope: 2, outsideAbsolute: 0.5, outsideFraction: 0.005, identityMassFraction: 0.1, identityCentroid: 0.25 })

export function validateHairlineManifest(manifest) {
  if (manifest.schema !== 1 || manifest.coordinateSpace !== 'pdf-user-space-y-up' || manifest.pageWidth !== 128 || manifest.pageHeight !== 128 || JSON.stringify(manifest.scales) !== '[1,2,4]') throw new Error('Unsupported hairline manifest geometry')
  if (manifest.cases?.length !== 42) throw new Error('Expected 42 authored affine stroke cases')
  const ids = new Set()
  for (const fixture of manifest.cases) {
    if (!/^(hairline|thin)-(identity|uniform|uniform-large|wide|tall|shear|rotated-wide)-(horizontal|vertical|diagonal)$/.test(fixture.id) || ids.has(fixture.id)) throw new Error('Invalid or duplicate fixture ID')
    ids.add(fixture.id)
    if (fixture.file !== `${fixture.id}.pdf` || !/^[a-f0-9]{64}$/.test(fixture.sha256 ?? '')) throw new Error('Each original PDF requires a portable filename and pinned SHA256')
    if (fixture.normativeHairline !== fixture.id.startsWith('hairline-') || fixture.lineWidth !== (fixture.normativeHairline ? 0 : 0.001)) throw new Error('Incorrect normative hairline classification')
    validateEndpoints(fixture)
  }
}

export function validateEndpoints(fixture) {
  for (const [name, length] of [['matrix', 4], ['translation', 2], ['localStart', 2], ['localEnd', 2], ['pagePoints', 4]]) {
    if (fixture[name]?.length !== length || !fixture[name].every(Number.isFinite)) throw new Error(`Invalid ${name}`)
  }
  const [a, b, c, d] = fixture.matrix, [e, f] = fixture.translation
  if (Math.abs(a * d - b * c) < 1e-12) throw new Error('Singular fixture matrix')
  const points = [fixture.localStart, fixture.localEnd].flatMap(([x, y]) => [a * x + c * y + e, b * x + d * y + f])
  const maximumError = Math.max(...points.map((value, index) => Math.abs(value - fixture.pagePoints[index])))
  if (maximumError > 1e-6) throw new Error('Authored endpoints do not match the affine transform')
  if (Math.hypot(points[2] - points[0], points[3] - points[1]) <= 4) throw new Error('Fixture segment is too short')
  return { maximumError, transformedPoints: points }
}

export function analyzeHairline(pixels, width, height, manifest, fixture, scale) {
  const endpoints = validateEndpoints(fixture)
  if (!manifest.scales.includes(scale) || width !== manifest.pageWidth * scale || height !== manifest.pageHeight * scale || pixels.length !== width * height * 4) throw new Error('Unexpected Worker raster dimensions')
  const [x0, y0, x1, y1] = fixture.pagePoints
  const start = [x0 * scale, (manifest.pageHeight - y0) * scale], end = [x1 * scale, (manifest.pageHeight - y1) * scale]
  const length = Math.hypot(end[0] - start[0], end[1] - start[1])
  const tangent = [(end[0] - start[0]) / length, (end[1] - start[1]) / length], normal = [-tangent[1], tangent[0]]
  const limit = hairlineLimits, bins = []
  for (let along = limit.endpointExclusion; along < length - limit.endpointExclusion; along++) bins.push({ along, ink: 0 })
  let mass = 0, interiorMass = 0, normalMoment = 0, outsideMass = 0, maximumInk = 0
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    const offset = (y * width + x) * 4
    // Black appearance composited over white, including transparent output.
    const ink = (1 - (pixels[offset] + pixels[offset + 1] + pixels[offset + 2]) / 765) * pixels[offset + 3] / 255
    if (ink <= 0) continue
    const dx = x + 0.5 - start[0], dy = y + 0.5 - start[1]
    const along = dx * tangent[0] + dy * tangent[1], perpendicular = dx * normal[0] + dy * normal[1]
    mass += ink; maximumInk = Math.max(maximumInk, ink)
    if (along >= limit.endpointExclusion && along < length - limit.endpointExclusion) { interiorMass += ink; normalMoment += perpendicular * ink }
    const beyond = along < 0 ? -along : Math.max(0, along - length)
    if (Math.hypot(beyond, perpendicular) > limit.envelope) outsideMass += ink
    if (Math.abs(perpendicular) <= limit.continuityNormal) {
      const first = Math.max(0, Math.ceil(along - limit.continuityTangent - limit.endpointExclusion))
      const last = Math.min(bins.length - 1, Math.floor(along + limit.continuityTangent - limit.endpointExclusion))
      for (let index = first; index <= last; index++) bins[index].ink += ink
    }
  }
  const equivalentWidth = mass / length, interiorEquivalentWidth = interiorMass / (length - 2 * limit.endpointExclusion)
  const perpendicularCentroid = interiorMass ? normalMoment / interiorMass : null
  const missing = bins.filter(bin => bin.ink < limit.continuityInk).map(bin => bin.along)
  const failures = []
  if (fixture.normativeHairline) {
    if (equivalentWidth < limit.minimumWidth || interiorEquivalentWidth < limit.minimumWidth) failures.push('insufficient-device-line-mass')
    if (equivalentWidth > limit.maximumWidth || interiorEquivalentWidth > limit.maximumWidth) failures.push('excessive-device-line-width')
    if (perpendicularCentroid === null || Math.abs(perpendicularCentroid) > limit.centroid) failures.push('displaced-device-line')
    if (missing.length) failures.push('discontinuous-or-faint-device-line')
    if (outsideMass > Math.max(limit.outsideAbsolute, limit.outsideFraction * mass)) failures.push('ink-outside-device-line-envelope')
  }
  return { status: fixture.normativeHairline ? (failures.length ? 'failed' : 'passed') : 'observed', fidelityAssessment: fixture.normativeHairline ? 'bounded-device-hairline' : 'unassessed-nonzero-width', failures, width, height, endpoints, deviceStart: start, deviceEnd: end, length, mass, interiorMass, equivalentWidth, interiorEquivalentWidth, perpendicularCentroid, outsideMass, maximumInk, continuity: { samples: bins.length, covered: bins.length - missing.length, minimumInk: Math.min(...bins.map(bin => bin.ink)), missingPositions: missing } }
}

export function compareHairlineIdentity(sample, identity) {
  if (!sample || !identity || sample.status === 'untested' || identity.status === 'untested') return { status: 'untested', failures: ['missing-raster-sample'] }
  if (!Number.isFinite(sample.mass) || !Number.isFinite(identity.mass)) return { status: 'failed', failures: ['raster-execution-failed'] }
  const relativeMassDelta = identity.mass ? Math.abs(sample.mass - identity.mass) / identity.mass : null
  const centroidDelta = sample.perpendicularCentroid !== null && identity.perpendicularCentroid !== null ? Math.abs(sample.perpendicularCentroid - identity.perpendicularCentroid) : null
  const failures = []
  if (sample.status !== 'passed' || identity.status !== 'passed') failures.push('analytic-hairline-check-failed')
  if (relativeMassDelta === null || relativeMassDelta > hairlineLimits.identityMassFraction) failures.push('identity-mass-difference')
  if (centroidDelta === null || centroidDelta > hairlineLimits.identityCentroid) failures.push('identity-position-difference')
  return { status: failures.length ? 'failed' : 'passed', failures, relativeMassDelta, centroidDelta, rgbaEqual: sample.rgbaSha256 && identity.rgbaSha256 ? sample.rgbaSha256 === identity.rgbaSha256 : null }
}

export function hairlineSummary(samples) {
  const counts = Object.fromEntries(['passed', 'failed', 'timeout', 'untested', 'observed'].map(status => [status, samples.filter(sample => sample.status === status).length]))
  if (Object.values(counts).reduce((sum, count) => sum + count, 0) !== samples.length) throw new Error('Unknown hairline sample status')
  return { planned: samples.length, ...counts }
}
