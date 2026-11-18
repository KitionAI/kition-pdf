const roles = ['left', 'right', 'foot', 'counter', 'marker', 'background']
const geometryRoles = new Set(['left', 'right', 'foot'])
const sha256 = /^[a-f0-9]{64}$/
export function fontGlyphShape(fixture) {
  if (fixture.notdefShape !== undefined && !['empty', 'left-bar', 'right-l'].includes(fixture.notdefShape)) throw new Error('Invalid original .notdef shape metadata')
  const gid = fixture.candidateGlyphId
  return gid === 0 ? fixture.notdefShape ?? 'empty' : gid === 1 ? 'left-bar' : gid === 2 ? 'right-l' : 'empty'
}

export function validateFontManifest(manifest) {
  if (manifest.schema !== 1 || manifest.coordinateSpace !== 'pdf-user-space-y-up' || manifest.pageWidth !== 40 || manifest.pageHeight !== 40 || JSON.stringify(manifest.scales) !== '[1,2]') throw new Error('Unsupported original font fixture geometry')
  if (!Array.isArray(manifest.cases) || manifest.cases.length < 1 || manifest.cases.length > 32) throw new Error('Expected a bounded focused font fixture set')
  const ids = new Set()
  for (const fixture of manifest.cases) {
    if (!/^[a-z0-9-]+$/.test(fixture.id) || ids.has(fixture.id)) throw new Error('Invalid or duplicate font fixture ID')
    ids.add(fixture.id)
    if (fixture.file !== `${fixture.id}.pdf` || fixture.fontFile !== `${fixture.id}.font` || !sha256.test(fixture.sha256 ?? '') || !sha256.test(fixture.fontSha256 ?? '')) throw new Error('Original font fixture requires portable filenames and pinned SHA256')
    if (!['format-control', 'malformed-wrapper-compatibility', 'malformed-wrapper-mapping-probe', 'malformed-multiple-programs', 'malformed-explicit-map-compatibility'].includes(fixture.classification) || typeof fixture.assertMapping !== 'boolean' || ![0, 1, 2, 3].includes(fixture.candidateGlyphId)) throw new Error('Missing font mapping classification')
    fontGlyphShape(fixture)
    if (fixture.classification === 'format-control' && !fixture.assertMapping) throw new Error('Normative format control must assert mapping')
    if (!Array.isArray(fixture.regions) || fixture.regions.length !== roles.length || fixture.regions.some((region, index) => region.name !== roles[index])) throw new Error('Each fixture needs ordered authored glyph, continuation and background regions')
    for (const region of fixture.regions) {
      const rect = region.rect
      if (!Array.isArray(rect) || rect.length !== 4 || !rect.every(Number.isFinite) || rect[0] < 0 || rect[1] < 0 || rect[2] <= 0 || rect[3] <= 0 || rect[0] + rect[2] > manifest.pageWidth || rect[1] + rect[3] > manifest.pageHeight) throw new Error('Invalid authored PDF rectangle')
      if (!Array.isArray(region.rgb) || region.rgb.length !== 3 || !region.rgb.every(value => Number.isInteger(value) && value >= 0 && value <= 255) || !Number.isInteger(region.tolerance) || region.tolerance < 0 || region.tolerance > 16) throw new Error('Invalid region color or tolerance')
      if (typeof region.assertColor !== 'boolean' || region.assertColor !== (fixture.assertMapping || !geometryRoles.has(region.name))) throw new Error('Only descriptive glyph regions may omit a candidate color assertion')
    }
  }
  for (const fixture of manifest.cases) if (fixture.expectedMatch !== undefined && (!ids.has(fixture.expectedMatch) || fixture.expectedMatch === fixture.id || !fixture.assertMapping || !manifest.cases.find(item => item.id === fixture.expectedMatch).assertMapping)) throw new Error('Invalid explicit font comparison target')
}

export function analyzeFont(pixels, width, height, manifest, fixture, scale) {
  if (!manifest.scales.includes(scale) || width !== manifest.pageWidth * scale || height !== manifest.pageHeight * scale || pixels.length !== width * height * 4) throw new Error('Unexpected font Worker raster dimensions')
  const regions = fixture.regions.map((region, index) => {
    const [x, y, w, h] = region.rect
    const deviceRect = [x * scale, (manifest.pageHeight - y - h) * scale, (x + w) * scale, (manifest.pageHeight - y) * scale]
    let samples = 0, mismatchedPixels = 0, alphaMismatches = 0, maximumChannelDifference = 0
    const channelMinimum = [255, 255, 255], channelMaximum = [0, 0, 0]
    for (let py = Math.max(0, Math.ceil(deviceRect[1] - 0.5)); py < Math.min(height, Math.ceil(deviceRect[3] - 0.5)); py++) {
      for (let px = Math.max(0, Math.ceil(deviceRect[0] - 0.5)); px < Math.min(width, Math.ceil(deviceRect[2] - 0.5)); px++) {
        const offset = (py * width + px) * 4
        let differs = false
        for (let channel = 0; channel < 3; channel++) {
          const value = pixels[offset + channel], difference = Math.abs(value - region.rgb[channel])
          channelMinimum[channel] = Math.min(channelMinimum[channel], value)
          channelMaximum[channel] = Math.max(channelMaximum[channel], value)
          maximumChannelDifference = Math.max(maximumChannelDifference, difference)
          if (difference > region.tolerance) differs = true
        }
        if (differs) mismatchedPixels++
        if (pixels[offset + 3] !== 255) alphaMismatches++
        samples++
      }
    }
    const failures = []
    if (!samples) failures.push('no-pixel-centers-in-authored-region')
    if (region.assertColor && mismatchedPixels) failures.push('authored-region-color-mismatch')
    if (alphaMismatches) failures.push('authored-region-not-opaque')
    const observedColor = !samples || alphaMismatches ? 'unknown' : channelMaximum.every(value => value <= region.tolerance) ? 'black' : channelMinimum.every(value => value >= 255 - region.tolerance) ? 'white' : 'mixed'
    return { index, ...region, deviceRect, status: failures.length ? 'failed' : 'passed', failures, samples, mismatchedPixels, alphaMismatches, maximumChannelDifference, channelMinimum, channelMaximum, observedColor }
  })
  const color = name => regions.find(region => region.name === name)?.observedColor
  const shape = [color('left'), color('right'), color('foot')].join(',')
  const inferredShape = ({ 'black,white,white': 'left-bar', 'white,black,black': 'right-l', 'white,white,white': 'empty' })[shape] ?? 'unknown'
  const expectedShape = fontGlyphShape(fixture)
  const candidateMatches = inferredShape === expectedShape
  const mapping = { asserted: fixture.assertMapping, status: fixture.assertMapping ? candidateMatches ? 'passed' : 'failed' : 'observed', classification: fixture.classification, candidateGlyphId: fixture.candidateGlyphId, expectedShape, inferredShape, candidateMatches }
  const failures = []
  if (regions.some(region => region.status !== 'passed')) failures.push('authored-region-measurement-failed')
  if (inferredShape === 'unknown') failures.push('unrecognized-authored-glyph-geometry')
  if (fixture.assertMapping && !candidateMatches) failures.push('asserted-glyph-mapping-differs')
  return { status: failures.length ? 'failed' : 'passed', failures, width, height, mapping, regions }
}

export function compareFontRasters(sample, control) {
  if (!sample || !control || sample.status === 'untested' || control.status === 'untested') return { status: 'untested', failures: ['missing-raster-sample'] }
  const failures = []
  if (sample.status !== 'passed' || control.status !== 'passed') failures.push('authored-region-or-execution-check-failed')
  if (!sample.mapping?.asserted || !control.mapping?.asserted) failures.push('comparison-requires-asserted-mapping')
  if (!sample.rgbaSha256 || !control.rgbaSha256) failures.push('missing-raster-hash')
  else if (sample.rgbaSha256 !== control.rgbaSha256) failures.push('full-rgba-differs-from-explicit-control')
  return { status: failures.length ? 'failed' : 'passed', failures, rgbaEqual: Boolean(sample.rgbaSha256 && sample.rgbaSha256 === control.rgbaSha256) }
}

export function fontSummary(samples) {
  const counts = Object.fromEntries(['passed', 'failed', 'timeout', 'untested'].map(status => [status, samples.filter(sample => sample.status === status).length]))
  if (Object.values(counts).reduce((sum, count) => sum + count, 0) !== samples.length) throw new Error('Unknown font sample status')
  return { planned: samples.length, ...counts }
}
