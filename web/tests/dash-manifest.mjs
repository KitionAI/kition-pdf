// Enrich only the native contributor-original dashed hairline matrix; never overwrite evidence.
import assert from 'node:assert/strict'
import { readFile, writeFile } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { dirname, resolve } from 'node:path'
import { validateDashManifest } from './dash-support.mjs'

const options = {}, seen = new Set()
for (let index = 2; index < process.argv.length; index++) {
  const name = process.argv[index]
  if (name === '--help') {
    console.log('node tests/dash-manifest.mjs --raw <fixture-dir>/manifest.raw.json --out <same-fixture-dir>/manifest-pinned.json')
    process.exit(0)
  }
  if (!['--raw', '--out'].includes(name) || seen.has(name)) throw new Error('Unknown or duplicate option')
  seen.add(name)
  const value = process.argv[++index]
  if (!value || value.startsWith('--')) throw new Error('Missing option value')
  options[name.slice(2)] = resolve(value)
}
if (!options.raw || !options.out || options.raw === options.out || dirname(options.raw) !== dirname(options.out)) throw new Error('Separate raw and output manifests in the same fixture directory are required')
const hash = bytes => createHash('sha256').update(bytes).digest('hex')
const rawBytes = await readFile(options.raw), manifest = JSON.parse(rawBytes)
if (!Array.isArray(manifest.cases) || manifest.cases.length > 96) throw new Error('Expected the native original-dash exporter')
manifest.rawManifestSha256 = hash(rawBytes)
manifest.enrichment = 'Pinned PDF hashes; retained discovery dot centroid tolerance 0.75 (new compound probes retain authored 0.9)'
manifest.dashRule = 'User-space dash arclength and phase; one device-pixel stroke after affine transformation'
for (const fixture of manifest.cases) {
  if (!/^[a-z0-9-]+$/.test(fixture.id) || fixture.file !== `${fixture.id}.pdf`) throw new Error('Unsafe original fixture filename')
  fixture.sha256 = hash(await readFile(resolve(dirname(options.raw), fixture.file)))
  // Preserve discovery's stricter dot centroid bound; new corner probes use 0.9.
  if (/^(round|square)-dots(?:-control)?$/.test(fixture.id)) {
    for (const point of fixture.pointInk ?? []) point.centroidTolerance = 0.75
  }
}
validateDashManifest(manifest)
const output = `${JSON.stringify(manifest, null, 2)}\n`
try {
  await writeFile(options.out, output, { flag: 'wx' })
} catch (error) {
  if (error.code !== 'EEXIST') throw error
  assert.equal(await readFile(options.out, 'utf8'), output, 'Refuse to replace a different pinned manifest')
}
console.log(JSON.stringify({ sha256: hash(output), cases: manifest.cases.length, rasters: manifest.cases.length * manifest.scales.length, regions: manifest.cases.reduce((sum, fixture) => sum + fixture.regions.reduce((count, region) => count + (region.scales ?? manifest.scales).length, 0), 0), points: manifest.cases.reduce((sum, fixture) => sum + (fixture.pointInk?.length ?? 0) * manifest.scales.length, 0), tolerancePairs: manifest.cases.filter(fixture => fixture.expectedMatch).length * manifest.scales.length }))
