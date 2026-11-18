export function validateMeshManifest(manifest) {
  if (manifest.schema !== 1 || manifest.coordinateSpace !== 'pdf-user-space-y-up' || manifest.pageWidth !== 40 || manifest.pageHeight !== 40 || JSON.stringify(manifest.scales) !== '[1,2]') throw new Error('Unsupported original mesh fixture geometry')
  if (!Array.isArray(manifest.cases) || manifest.cases.length < 1 || manifest.cases.length > 16) throw new Error('Expected a bounded focused mesh fixture set')
  const ids = new Set()
  for (const fixture of manifest.cases) {
    if (!/^[a-z0-9-]+$/.test(fixture.id) || ids.has(fixture.id)) throw new Error('Invalid or duplicate mesh fixture ID')
    ids.add(fixture.id)
    if (fixture.file !== `${fixture.id}.pdf` || !/^[a-f0-9]{64}$/.test(fixture.sha256 ?? '')) throw new Error('Original mesh fixture requires portable filename and pinned SHA256')
    if (![6, 7].includes(fixture.shadingType) || typeof fixture.isStress !== 'boolean') throw new Error('Missing mesh shading classification')
    if (!Array.isArray(fixture.regions) || fixture.regions.length === 0) throw new Error('Each fixture needs authored color-region assertions')
    for (const region of fixture.regions) {
      const rect = region.rect
      if (!Array.isArray(rect) || rect.length !== 4 || !rect.every(Number.isFinite) || rect[0] < 0 || rect[1] < 0 || rect[2] <= 0 || rect[3] <= 0 || rect[0] + rect[2] > manifest.pageWidth || rect[1] + rect[3] > manifest.pageHeight) throw new Error('Invalid authored PDF rectangle')
      if (!Array.isArray(region.rgb) || region.rgb.length !== 3 || !region.rgb.every(value => Number.isInteger(value) && value >= 0 && value <= 255) || !Number.isInteger(region.tolerance) || region.tolerance < 0 || region.tolerance > 16) throw new Error('Invalid region color or tolerance')
    }
  }
  for (const fixture of manifest.cases) if (fixture.expectedMatch !== undefined && (!ids.has(fixture.expectedMatch) || fixture.expectedMatch === fixture.id)) throw new Error('Invalid explicit mesh comparison target')
}

export function analyzeMesh(pixels, width, height, manifest, fixture, scale) {
  if (!manifest.scales.includes(scale) || width !== manifest.pageWidth * scale || height !== manifest.pageHeight * scale || pixels.length !== width * height * 4) throw new Error('Unexpected mesh Worker raster dimensions')
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
    if (mismatchedPixels) failures.push('authored-region-color-mismatch')
    if (alphaMismatches) failures.push('authored-region-not-opaque')
    return { index, ...region, deviceRect, status: failures.length ? 'failed' : 'passed', failures, samples, mismatchedPixels, alphaMismatches, maximumChannelDifference, channelMinimum, channelMaximum }
  })
  return { status: regions.every(region => region.status === 'passed') ? 'passed' : 'failed', width, height, regions }
}

export function compareMeshRasters(sample, control) {
  if (!sample || !control || sample.status === 'untested' || control.status === 'untested') return { status: 'untested', failures: ['missing-raster-sample'] }
  const failures = []
  if (sample.status !== 'passed' || control.status !== 'passed') failures.push('authored-region-or-execution-check-failed')
  if (!sample.rgbaSha256 || !control.rgbaSha256) failures.push('missing-raster-hash')
  else if (sample.rgbaSha256 !== control.rgbaSha256) failures.push('full-rgba-differs-from-explicit-control')
  return { status: failures.length ? 'failed' : 'passed', failures, rgbaEqual: Boolean(sample.rgbaSha256 && sample.rgbaSha256 === control.rgbaSha256) }
}

export function meshSummary(samples) {
  const counts = Object.fromEntries(['passed', 'failed', 'timeout', 'untested'].map(status => [status, samples.filter(sample => sample.status === status).length]))
  if (Object.values(counts).reduce((sum, count) => sum + count, 0) !== samples.length) throw new Error('Unknown mesh sample status')
  return { planned: samples.length, ...counts }
}
