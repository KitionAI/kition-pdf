import { analyzeMesh, compareMeshRasters, meshSummary } from './mesh-support.mjs'

export const analyzeType3 = analyzeMesh
export const compareType3Rasters = compareMeshRasters
export const type3Summary = meshSummary
export const truncationReason = 'page content exceeded the rendering budget'

export function expectedRejection(message) {
  const text = String(message ?? '')
  return text.includes(truncationReason) && text.includes('Could not render PDF page') && text.includes('Retry the page or reopen the document.')
}

export function validateType3Manifest(manifest) {
  if (manifest.schema !== 1 || manifest.coordinateSpace !== 'pdf-user-space-y-up' || manifest.pageWidth !== 40 || manifest.pageHeight !== 40 || JSON.stringify(manifest.scales) !== '[1,2]') throw new Error('Unsupported original Type3 fixture geometry')
  if (manifest.pageCount !== 2 || manifest.resetPage?.index !== 1 || !Array.isArray(manifest.resetPage.regions) || manifest.resetPage.regions.length === 0) throw new Error('A second clean reset page and authored regions are required')
  if (!Array.isArray(manifest.cases) || manifest.cases.length < 2 || manifest.cases.length > 32) throw new Error('Expected a bounded Type3 fixture set with controls')
  const ids = new Map()
  for (const fixture of manifest.cases) {
    if (!/^[a-z0-9-]+$/.test(fixture.id) || ids.has(fixture.id)) throw new Error('Invalid or duplicate Type3 fixture ID')
    ids.set(fixture.id, fixture)
    if (fixture.file !== `${fixture.id}.pdf` || !/^[a-f0-9]{64}$/.test(fixture.sha256 ?? '')) throw new Error('Type3 fixture requires portable filename and pinned SHA256')
    if (typeof fixture.isStress !== 'boolean' || typeof fixture.expectedTruncation !== 'boolean' || !['d0', 'd1', 'mixed'].includes(fixture.glyphKind)) throw new Error('Missing Type3 stress, truncation or glyph classification')
    if (!Array.isArray(fixture.regions) || !fixture.regions.length) throw new Error('Every fixture needs native authored regions')
    for (const region of [...fixture.regions, ...manifest.resetPage.regions]) {
      const rect = region.rect
      if (!Array.isArray(rect) || rect.length !== 4 || !rect.every(Number.isFinite) || rect[0] < 0 || rect[1] < 0 || rect[2] <= 0 || rect[3] <= 0 || rect[0] + rect[2] > 40 || rect[1] + rect[3] > 40) throw new Error('Invalid authored Type3 rectangle')
      if (!Array.isArray(region.rgb) || region.rgb.length !== 3 || !region.rgb.every(value => Number.isInteger(value) && value >= 0 && value <= 255) || !Number.isInteger(region.tolerance) || region.tolerance < 0 || region.tolerance > 16) throw new Error('Invalid authored Type3 color')
    }
  }
  for (const fixture of manifest.cases) {
    const control = ids.get(fixture.expectedMatch)
    if (fixture.expectedMatch !== undefined && (!control || control === fixture || control.expectedTruncation)) throw new Error('Invalid explicit Type3 comparison target')
    if (fixture.expectedTruncation && !control) throw new Error('A truncated case requires a clean recovery control')
  }
}

export function plannedCases(manifest) {
  return manifest.cases.map(fixture => ({
    ...fixture, status: 'untested', connection: { status: 'untested' },
    resetPage: { status: 'untested', pageIndex: manifest.resetPage.index, regions: manifest.resetPage.regions.map((region, index) => ({ index, ...region, status: 'untested' })) },
    errorUi: { status: 'untested' }, recovery: fixture.expectedTruncation ? { status: 'untested', control: fixture.expectedMatch } : null,
    samples: [1, 2].flatMap(pass => manifest.scales.map(scale => ({ pass, scale, dpi: scale * 72, status: 'untested', expected: fixture.expectedTruncation ? 'rejection' : 'raster', regions: fixture.expectedTruncation ? [] : fixture.regions.map((region, index) => ({ index, ...region, status: 'untested' })) }))),
    errors: [], pageErrors: [],
  }))
}
