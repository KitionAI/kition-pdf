export const EDGE_TOLERANCE_PX = 1.5
const CONTINUITY_INK = 0.01
const INTERIOR_INK = 1 - 32 / 255
const BLANK_LIMIT = 5 / 255

export function devicePoint(x, y, pageHeight, scale) { return [x * scale, (pageHeight - y) * scale] }
export function transformPoint([a, b, c, d, e, f], x, y) { return [a * x + c * y + e, b * x + d * y + f] }
export function inversePoint([a, b, c, d, e, f], x, y) {
  const determinant = a * d - b * c
  if (!Number.isFinite(determinant) || Math.abs(determinant) < 1e-12) throw new Error('Singular glyph matrix')
  return [(d * (x - e) - c * (y - f)) / determinant, (-b * (x - e) + a * (y - f)) / determinant]
}
export function segmentDistance(x, y, [x0, y0], [x1, y1]) {
  const dx = x1 - x0, dy = y1 - y0
  const t = Math.max(0, Math.min(1, ((x - x0) * dx + (y - y0) * dy) / (dx * dx + dy * dy || 1)))
  return Math.hypot(x - x0 - t * dx, y - y0 - t * dy)
}
function darkness(pixels, width, x, y) {
  const offset = (y * width + x) * 4
  // Compare composited appearance on the declared white page, including alpha.
  return (1 - (pixels[offset] + pixels[offset + 1] + pixels[offset + 2]) / (3 * 255)) * pixels[offset + 3] / 255
}
function bounds(points, width, height, margin = 3) {
  return { x0: Math.max(0, Math.floor(Math.min(...points.map(p => p[0])) - margin)), y0: Math.max(0, Math.floor(Math.min(...points.map(p => p[1])) - margin)), x1: Math.min(width, Math.ceil(Math.max(...points.map(p => p[0])) + margin)), y1: Math.min(height, Math.ceil(Math.max(...points.map(p => p[1])) + margin)) }
}
function observedInk(pixels, width, roi) {
  let mass = 0, count = 0, xSum = 0, ySum = 0
  const box = { x0: Infinity, y0: Infinity, x1: -Infinity, y1: -Infinity }
  for (let y = roi.y0; y < roi.y1; y++) for (let x = roi.x0; x < roi.x1; x++) {
    const ink = darkness(pixels, width, x, y)
    mass += ink; xSum += (x + 0.5) * ink; ySum += (y + 0.5) * ink
    if (ink > CONTINUITY_INK) { count++; box.x0 = Math.min(box.x0, x); box.y0 = Math.min(box.y0, y); box.x1 = Math.max(box.x1, x + 1); box.y1 = Math.max(box.y1, y + 1) }
  }
  return { coverageMass: mass, inkPixels: count, centroid: mass ? [xSum / mass, ySum / mass] : null, bounds: count ? box : null }
}

function analyzeSegment(pixels, width, height, pageHeight, scale, segment) {
  const start = devicePoint(segment.x0, segment.y0, pageHeight, scale), end = devicePoint(segment.x1, segment.y1, pageHeight, scale)
  const length = Math.hypot(end[0] - start[0], end[1] - start[1])
  if (!length) throw new Error('A stroke fixture segment must have nonzero length')
  const tangent = [(end[0] - start[0]) / length, (end[1] - start[1]) / length]
  const normal = [-tangent[1], tangent[0]]
  const halfWidth = segment.width === 0 ? 0.5 : segment.width * scale / 2
  const tolerance = halfWidth + EDGE_TOLERANCE_PX
  const roi = bounds([start, end], width, height, tolerance + 2)
  const observed = observedInk(pixels, width, roi)
  let samples = 0, covered = 0, spillPixels = 0
  for (let along = 2; along < length - 2; along += 1) {
    samples++
    const px = start[0] + tangent[0] * along, py = start[1] + tangent[1] * along
    const radius = Math.ceil(tolerance + 1)
    let found = false
    for (let y = Math.max(0, Math.floor(py) - radius); y < Math.min(height, Math.ceil(py) + radius); y++) for (let x = Math.max(0, Math.floor(px) - radius); x < Math.min(width, Math.ceil(px) + radius); x++) {
      const dx = x + 0.5 - px, dy = y + 0.5 - py
      if (Math.abs(dx * tangent[0] + dy * tangent[1]) <= 0.8 && Math.abs(dx * normal[0] + dy * normal[1]) <= tolerance && darkness(pixels, width, x, y) > CONTINUITY_INK) found = true
    }
    if (found) covered++
  }
  for (let y = roi.y0; y < roi.y1; y++) for (let x = roi.x0; x < roi.x1; x++) if (segmentDistance(x + 0.5, y + 0.5, start, end) > tolerance && darkness(pixels, width, x, y) > BLANK_LIMIT) spillPixels++
  const perpendicularOffset = observed.centroid ? (observed.centroid[0] - start[0]) * normal[0] + (observed.centroid[1] - start[1]) * normal[1] : null
  const failures = []
  if (!observed.inkPixels) failures.push('missing-stroke')
  if (!samples || covered !== samples) failures.push('stroke-discontinuity-or-displacement')
  if (perpendicularOffset === null || Math.abs(perpendicularOffset) > EDGE_TOLERANCE_PX) failures.push('displaced-stroke-centroid')
  if (spillPixels) failures.push('unexpected-ink-outside-stroke-guard')
  return { id: segment.id, kind: 'stroke', status: failures.length ? 'failed' : 'passed', failures, roi, widthPoints: segment.width, ...observed, continuity: { covered, samples }, perpendicularOffset, spillPixels }
}

function analyzeGlyph(pixels, width, height, pageHeight, scale, glyph, rects) {
  const polygons = rects.map(([x0, y0, x1, y1]) => [[x0, y0], [x1, y0], [x1, y1], [x0, y1]].map(([x, y]) => devicePoint(...transformPoint(glyph.matrix, x, y), pageHeight, scale)))
  const roi = bounds(polygons.flat(), width, height)
  const observed = observedInk(pixels, width, roi)
  let interiorPixels = 0, missingInteriorPixels = 0, blankGuardPixels = 0, unexpectedGuardInkPixels = 0, edgePixels = 0, edgeCoverageMass = 0
  for (let y = roi.y0; y < roi.y1; y++) for (let x = roi.x0; x < roi.x1; x++) {
    const [localX, localY] = inversePoint(glyph.matrix, (x + 0.5) / scale, pageHeight - (y + 0.5) / scale)
    const inside = rects.some(([x0, y0, x1, y1]) => localX >= x0 && localX <= x1 && localY >= y0 && localY <= y1)
    // Shared rectangle edges are conservatively treated as edge zones too.
    const distance = Math.min(...polygons.flatMap(polygon => polygon.map((point, index) => segmentDistance(x + 0.5, y + 0.5, point, polygon[(index + 1) % polygon.length]))))
    const ink = darkness(pixels, width, x, y)
    if (distance <= EDGE_TOLERANCE_PX) { edgePixels++; edgeCoverageMass += ink }
    else if (inside) { interiorPixels++; if (ink < INTERIOR_INK) missingInteriorPixels++ }
    else { blankGuardPixels++; if (ink > BLANK_LIMIT) unexpectedGuardInkPixels++ }
  }
  const failures = []
  if (!observed.inkPixels) failures.push('missing-glyph')
  if (!interiorPixels || missingInteriorPixels) failures.push('missing-interior-or-wrong-glyph-transform')
  if (!blankGuardPixels || unexpectedGuardInkPixels) failures.push('displaced-or-unexpected-glyph-ink')
  return { id: glyph.id, kind: 'type3-glyph', status: failures.length ? 'failed' : 'passed', failures, matrix: glyph.matrix, roi, ...observed, interiorPixels, missingInteriorPixels, blankGuardPixels, unexpectedGuardInkPixels, antialiasObservation: { edgePixels, edgeCoverageMass } }
}

export function analyzeGeometry(pixels, width, height, manifest, fixture, scale) {
  if (manifest.coordinateSpace !== 'pdf-user-space-y-up' || manifest.rotation !== 0 || manifest.glyphShape.rectFormat !== 'x0,y0,x1,y1') throw new Error('Unsupported geometry coordinate or rectangle format')
  if (width !== manifest.pageWidth * scale || height !== manifest.pageHeight * scale || pixels.length !== width * height * 4) throw new Error('Raster dimensions do not match fixture metadata')
  const regions = [
    ...fixture.segments.map(segment => analyzeSegment(pixels, width, height, manifest.pageHeight, scale, segment)),
    ...fixture.glyphs.map(glyph => analyzeGlyph(pixels, width, height, manifest.pageHeight, scale, glyph, manifest.glyphShape.rects)),
  ]
  if (!regions.length) throw new Error('Fixture has no geometry assertions')
  return { status: regions.every(region => region.status === 'passed') ? 'passed' : 'failed', scale, dpi: scale * 72, width, height, tolerances: { edgePixels: EDGE_TOLERANCE_PX, continuityInk: CONTINUITY_INK, interiorInk: INTERIOR_INK, blankInkLimit: BLANK_LIMIT }, regions }
}

export function geometrySummary(samples) {
  const counts = Object.fromEntries(['passed', 'failed', 'timeout', 'untested'].map(status => [status, samples.filter(sample => sample.status === status).length]))
  if (Object.values(counts).reduce((a, b) => a + b, 0) !== samples.length) throw new Error('Unknown sample status')
  return { planned: samples.length, ...counts, completionRate: samples.length ? counts.passed / samples.length : null }
}
