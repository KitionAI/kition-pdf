// Independent scalar dash intervals and device-space measurements for original PDFs.
export const dashLimits = Object.freeze({ minimumWidth: 0.65, maximumWidth: 1.5, centroid: 0.65, endpointGuard: 2, minimumInterior: 2, continuityInk: 0.1, continuityTangent: 0.8, continuityNormal: 1.5, gapWidth: 0.1, pairTolerance: 2, maximumIntervals: 4096 })
const finiteArray = (value, length) => Array.isArray(value) && value.length === length && value.every(Number.isFinite)
const transformPoint = ([a, b, c, d, e, f], [x, y]) => [a * x + c * y + e, b * x + d * y + f]

export function dashIntervals(line) {
  if (!finiteArray(line.localStart, 2) || !finiteArray(line.localEnd, 2) || !finiteArray(line.matrix, 6) || !Array.isArray(line.dashArray) || line.dashArray.length > 128 || !line.dashArray.every(value => Number.isFinite(value) && value >= 0) || !Number.isFinite(line.dashOffset) || !['butt', 'round', 'square'].includes(line.cap) || !Number.isFinite(line.opacity) || line.opacity <= 0 || line.opacity > 1) throw new Error('Invalid analytic dash line')
  const length = Math.hypot(line.localEnd[0] - line.localStart[0], line.localEnd[1] - line.localStart[1])
  const start = transformPoint(line.matrix, line.localStart), end = transformPoint(line.matrix, line.localEnd)
  const pageLength = Math.hypot(end[0] - start[0], end[1] - start[1])
  if (!Number.isFinite(length) || !Number.isFinite(pageLength) || length <= 0 || pageLength <= 0) throw new Error('Degenerate analytic dash line')
  if (!line.dashArray.length) return { localLength: length, pageLength, pageStart: start, pageEnd: end, normalizedPhase: 0, intervals: [{ start: 0, end: length, on: true }] }
  const pattern = line.dashArray.length % 2 ? [...line.dashArray, ...line.dashArray] : line.dashArray
  const period = pattern.reduce((sum, value) => sum + value, 0)
  if (!Number.isFinite(period) || period <= 0) throw new Error('Analytic dash pattern must have a positive finite period; sanitized solid controls use an empty array')
  const normalizedPhase = ((line.dashOffset % period) + period) % period
  let phase = normalizedPhase, index = 0
  // At an exact boundary, advance into the next nonempty interval. Dot cases
  // use independent point probes, not this positive-length interval analyzer.
  for (let visited = 0; visited <= pattern.length; visited++) {
    if (pattern[index] > 0 && phase < pattern[index]) break
    phase -= pattern[index]
    index = (index + 1) % pattern.length
    if (visited === pattern.length) throw new Error('Dash phase made no progress')
  }
  let remaining = pattern[index] - phase, position = 0
  const intervals = []
  for (let steps = 0; position < length; steps++) {
    if (steps >= dashLimits.maximumIntervals * (pattern.length + 1)) throw new Error('Analytic dash interval budget exceeded')
    if (remaining > 0) {
      const end = Math.min(length, position + remaining)
      if (!(end > position)) throw new Error('Analytic dash interval made no progress')
      intervals.push({ start: position, end, on: index % 2 === 0 })
      if (intervals.length > dashLimits.maximumIntervals) throw new Error('Analytic dash interval budget exceeded')
      position = end
    }
    index = (index + 1) % pattern.length
    remaining = pattern[index]
  }
  return { localLength: length, pageLength, pageStart: start, pageEnd: end, normalizedPhase, intervals }
}

export function validateDashManifest(manifest) {
  if (manifest.schema !== 1 || manifest.coordinateSpace !== 'pdf-user-space-y-up' || manifest.pageWidth !== 128 || manifest.pageHeight !== 128 || JSON.stringify(manifest.scales) !== '[1,2,4]') throw new Error('Unsupported original dash fixture geometry')
  if (!Array.isArray(manifest.cases) || !manifest.cases.length || manifest.cases.length > 96) throw new Error('Expected a bounded dash fixture matrix')
  const ids = new Set()
  for (const fixture of manifest.cases) {
    if (!/^[a-z0-9-]+$/.test(fixture.id) || ids.has(fixture.id) || fixture.file !== `${fixture.id}.pdf` || !/^[a-f0-9]{64}$/.test(fixture.sha256 ?? '')) throw new Error('Dash cases require unique portable IDs and pinned PDFs')
    ids.add(fixture.id)
    if (!['sample', 'control'].includes(fixture.role) || !['normative-hairline', 'sanitized-fallback'].includes(fixture.classification)) throw new Error('Missing dash role or classification')
    if (!Array.isArray(fixture.regions) || !fixture.regions.length) throw new Error('Dash fixture needs continuation and background regions')
    for (const region of fixture.regions) {
      if (region.scales !== undefined && (!Array.isArray(region.scales) || !region.scales.length || new Set(region.scales).size !== region.scales.length || !region.scales.every(scale => manifest.scales.includes(scale)))) throw new Error('Invalid authored dash region scales')
      if (typeof region.id !== 'string' || !finiteArray(region.rect, 4) || region.rect[0] < 0 || region.rect[1] < 0 || region.rect[2] <= 0 || region.rect[3] <= 0 || region.rect[0] + region.rect[2] > 128 || region.rect[1] + region.rect[3] > 128 || !finiteArray(region.rgba, 4) || !region.rgba.every(value => Number.isInteger(value) && value >= 0 && value <= 255) || !Number.isInteger(region.tolerance) || region.tolerance < 0 || region.tolerance > 16) throw new Error('Invalid authored dash color region')
    }
    if (fixture.analyticLine) dashIntervals(fixture.analyticLine)
    if (fixture.pointInk !== undefined && (!Array.isArray(fixture.pointInk) || fixture.pointInk.length > 256)) throw new Error('Invalid dash point probes')
    if (!fixture.analyticLine && !(fixture.pointInk ?? []).some(point => point.minMass > 0)) throw new Error('Dash fixture requires an independent positive-content check')
    for (const point of fixture.pointInk ?? []) {
      if (typeof point.id !== 'string' || !finiteArray(point.point, 2) || !Number.isFinite(point.radius) || point.radius <= 0 || point.radius > 8 || !Number.isFinite(point.minMass) || point.minMass < 0 || !Number.isFinite(point.maxMass) || point.maxMass < point.minMass || !Number.isFinite(point.centroidTolerance) || point.centroidTolerance < 0 || !Number.isFinite(point.opacity ?? 1) || (point.opacity ?? 1) <= 0 || (point.opacity ?? 1) > 1) throw new Error('Invalid authored dash point measurement')
    }
  }
  for (const fixture of manifest.cases) {
    if (fixture.role === 'control' && fixture.expectedMatch !== undefined) throw new Error('Dash controls cannot reference another control')
    if (fixture.role === 'sample' && (!ids.has(fixture.expectedMatch) || manifest.cases.find(item => item.id === fixture.expectedMatch).role !== 'control')) throw new Error('Every dash sample requires an explicit control case')
  }
}

function regionMeasurements(pixels, width, height, manifest, fixture, scale) {
  return fixture.regions.filter(region => !region.scales || region.scales.includes(scale)).map(region => {
    const [x, y, w, h] = region.rect
    let samples = 0, mismatchedPixels = 0, maximumChannelDifference = 0
    for (let py = Math.ceil((manifest.pageHeight - y - h) * scale - 0.5); py < Math.ceil((manifest.pageHeight - y) * scale - 0.5); py++) {
      for (let px = Math.ceil(x * scale - 0.5); px < Math.ceil((x + w) * scale - 0.5); px++) {
        if (px < 0 || py < 0 || px >= width || py >= height) continue
        let differs = false
        for (let channel = 0; channel < 4; channel++) {
          const difference = Math.abs(pixels[(py * width + px) * 4 + channel] - region.rgba[channel])
          maximumChannelDifference = Math.max(maximumChannelDifference, difference)
          if (difference > region.tolerance) differs = true
        }
        samples++; if (differs) mismatchedPixels++
      }
    }
    return { ...region, status: samples && !mismatchedPixels ? 'passed' : 'failed', samples, mismatchedPixels, maximumChannelDifference }
  })
}

function pixelInk(pixels, offset, opacity) {
  return (1 - (pixels[offset] + pixels[offset + 1] + pixels[offset + 2]) / 765) * pixels[offset + 3] / 255 / opacity
}

export function analyzeDash(pixels, width, height, manifest, fixture, scale) {
  if (!manifest.scales.includes(scale) || width !== manifest.pageWidth * scale || height !== manifest.pageHeight * scale || pixels.length !== width * height * 4) throw new Error('Unexpected dash raster dimensions')
  const regions = regionMeasurements(pixels, width, height, manifest, fixture, scale)
  const excludedRegions = fixture.regions.filter(region => region.scales && !region.scales.includes(scale)).map(region => ({ id: region.id, scales: region.scales, reason: 'outside-authored-region-scales' }))
  const pointChecks = (fixture.pointInk ?? []).map(point => {
    const center = [point.point[0] * scale, (manifest.pageHeight - point.point[1]) * scale]
    let mass = 0, xMoment = 0, yMoment = 0, samples = 0
    for (let y = Math.max(0, Math.floor(center[1] - point.radius)); y < Math.min(height, Math.ceil(center[1] + point.radius)); y++) for (let x = Math.max(0, Math.floor(center[0] - point.radius)); x < Math.min(width, Math.ceil(center[0] + point.radius)); x++) {
      const dx = x + 0.5 - center[0], dy = y + 0.5 - center[1]
      if (Math.hypot(dx, dy) > point.radius) continue
      const ink = pixelInk(pixels, (y * width + x) * 4, point.opacity ?? 1)
      mass += ink; xMoment += dx * ink; yMoment += dy * ink; samples++
    }
    const centroidDistance = mass > 0 ? Math.hypot(xMoment, yMoment) / mass : null
    const failures = []
    if (!samples || mass < point.minMass || mass > point.maxMass) failures.push('dot-mass-out-of-bounds')
    if (centroidDistance === null || centroidDistance > point.centroidTolerance) failures.push('dot-centroid-out-of-bounds')
    return { ...point, status: failures.length ? 'failed' : 'passed', failures, center, mass, centroidDistance, samples }
  })
  const lineChecks = [], excludedIntervals = []
  let lineGeometry = null
  if (fixture.analyticLine) {
    const line = fixture.analyticLine, geometry = dashIntervals(line)
    const start = [geometry.pageStart[0] * scale, (manifest.pageHeight - geometry.pageStart[1]) * scale]
    const end = [geometry.pageEnd[0] * scale, (manifest.pageHeight - geometry.pageEnd[1]) * scale]
    const length = geometry.pageLength * scale, ratio = length / geometry.localLength
    const tangent = [(end[0] - start[0]) / length, (end[1] - start[1]) / length], normal = [-tangent[1], tangent[0]]
    const intervals = geometry.intervals.map((interval, index) => {
      const begin = interval.start * ratio + dashLimits.endpointGuard, finish = interval.end * ratio - dashLimits.endpointGuard
      const eligible = finish - begin >= dashLimits.minimumInterior
      const item = { index, kind: interval.on ? 'on' : 'gap', localStart: interval.start, localEnd: interval.end, begin, finish, eligible, mass: 0, normalMoment: 0, unexpectedColorPixels: 0, continuity: [] }
      if (eligible && interval.on) for (let along = begin + 0.5; along < finish; along++) item.continuity.push({ along, ink: 0 })
      return item
    })
    for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
      const offset = (y * width + x) * 4, ink = pixelInk(pixels, offset, line.opacity)
      if (ink <= 0) continue
      const dx = x + 0.5 - start[0], dy = y + 0.5 - start[1]
      const along = dx * tangent[0] + dy * tangent[1], perpendicular = dx * normal[0] + dy * normal[1]
      let low = 0, high = intervals.length
      while (low < high) { const mid = (low + high) >>> 1; if (intervals[mid].finish <= along) low = mid + 1; else high = mid }
      const interval = intervals[low]
      if (!interval?.eligible || along < interval.begin || along >= interval.finish) continue
      const spread = Math.max(pixels[offset], pixels[offset + 1], pixels[offset + 2]) - Math.min(pixels[offset], pixels[offset + 1], pixels[offset + 2])
      // Colored continuation markers are outside the black-line assessment.
      if (spread > 2) { if (Math.abs(perpendicular) <= dashLimits.continuityNormal) interval.unexpectedColorPixels++; continue }
      interval.mass += ink; interval.normalMoment += ink * perpendicular
      if (interval.kind === 'on' && Math.abs(perpendicular) <= dashLimits.continuityNormal) {
        const first = Math.max(0, Math.ceil(along - dashLimits.continuityTangent - interval.begin - 0.5))
        const last = Math.min(interval.continuity.length - 1, Math.floor(along + dashLimits.continuityTangent - interval.begin - 0.5))
        for (let bin = first; bin <= last; bin++) interval.continuity[bin].ink += ink
      }
    }
    for (const interval of intervals) {
      if (!interval.eligible) { excludedIntervals.push({ index: interval.index, kind: interval.kind, reason: 'shorter-than-fixed-interior-guard', deviceLength: (interval.localEnd - interval.localStart) * ratio }); continue }
      const equivalentWidth = interval.mass / (interval.finish - interval.begin)
      const centroid = interval.mass ? interval.normalMoment / interval.mass : null
      const missingPositions = interval.continuity.filter(bin => bin.ink < dashLimits.continuityInk).map(bin => bin.along)
      const failures = []
      if (interval.unexpectedColorPixels) failures.push('unexpected-line-color')
      if (interval.kind === 'on') {
        if (equivalentWidth < dashLimits.minimumWidth || equivalentWidth > dashLimits.maximumWidth) failures.push('dash-device-width-out-of-bounds')
        if (centroid === null || Math.abs(centroid) > dashLimits.centroid) failures.push('dash-device-centroid-out-of-bounds')
        if (missingPositions.length) failures.push('dash-interior-discontinuous')
      } else if (equivalentWidth > dashLimits.gapWidth) failures.push('ink-in-authored-dash-gap')
      lineChecks.push({ index: interval.index, kind: interval.kind, status: failures.length ? 'failed' : 'passed', failures, deviceInterior: [interval.begin, interval.finish], equivalentWidth, centroid, mass: interval.mass, continuitySamples: interval.continuity.length, missingPositions, unexpectedColorPixels: interval.unexpectedColorPixels })
    }
    lineGeometry = { ...geometry, deviceStart: start, deviceEnd: end, deviceLength: length }
  }
  const failures = []
  if (regions.some(region => region.status !== 'passed')) failures.push('authored-color-region-failed')
  if (pointChecks.some(point => point.status !== 'passed')) failures.push('authored-dot-measurement-failed')
  if (lineChecks.some(check => check.status !== 'passed')) failures.push('analytic-dash-geometry-failed')
  if (fixture.analyticLine && !lineChecks.some(check => check.kind === 'on')) failures.push('no-eligible-dash-interior-check')
  return { status: failures.length ? 'failed' : 'passed', failures, width, height, assessment: fixture.analyticLine ? 'analytic-dash' : pointChecks.length ? 'authored-dots-and-regions' : 'authored-regions-only', regions, excludedRegions, pointChecks, lineChecks, excludedIntervals, lineGeometry, measurements: { regions: regions.length, excludedRegions: excludedRegions.length, points: pointChecks.length, onIntervals: lineChecks.filter(check => check.kind === 'on').length, gapIntervals: lineChecks.filter(check => check.kind === 'gap').length, excludedIntervals: excludedIntervals.length } }
}

export function compareDashRasters(sample, control, sampleRgba, controlRgba) {
  if (!sample || !control || sample.status === 'untested' || control.status === 'untested' || !sampleRgba || !controlRgba) return { status: 'untested', failures: ['missing-dash-raster'] }
  const failures = [], tolerance = dashLimits.pairTolerance
  if (sample.status !== 'passed' || control.status !== 'passed') failures.push('dash-measurement-or-execution-failed')
  if (sample.width !== control.width || sample.height !== control.height || sampleRgba.length !== sample.width * sample.height * 4 || controlRgba.length !== sampleRgba.length) return { status: 'failed', failures: [...failures, 'dash-pair-dimensions-differ'], tolerance }
  let maximumChannelDifference = 0, pixelsOverTolerance = 0, alphaMismatches = 0
  for (let offset = 0; offset < sampleRgba.length; offset += 4) {
    let differs = false
    for (let channel = 0; channel < 4; channel++) {
      const delta = Math.abs(sampleRgba[offset + channel] - controlRgba[offset + channel])
      maximumChannelDifference = Math.max(maximumChannelDifference, delta)
      if (delta > tolerance) differs = true
    }
    if (differs) pixelsOverTolerance++
    if (sampleRgba[offset + 3] !== 255 || controlRgba[offset + 3] !== 255) alphaMismatches++
  }
  if (pixelsOverTolerance) failures.push('dash-control-pixel-tolerance-exceeded')
  if (alphaMismatches) failures.push('dash-control-not-opaque')
  return { status: failures.length ? 'failed' : 'passed', failures, tolerance, rgbaEqual: maximumChannelDifference === 0, maximumChannelDifference, pixelsOverTolerance, alphaMismatches, comparedPixels: sampleRgba.length / 4 }
}

export function dashSummary(samples) {
  const result = Object.fromEntries(['passed', 'failed', 'timeout', 'untested'].map(status => [status, samples.filter(sample => sample.status === status).length]))
  if (Object.values(result).reduce((sum, count) => sum + count, 0) !== samples.length) throw new Error('Unknown dash result status')
  return { planned: samples.length, ...result }
}
