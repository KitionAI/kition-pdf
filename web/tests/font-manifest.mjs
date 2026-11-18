// Enrich only the native contributor-original font matrix; never overwrite evidence.
import assert from 'node:assert/strict'
import { readFile, writeFile } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { dirname, resolve } from 'node:path'
import { fontGlyphShape, validateFontManifest } from './font-support.mjs'

const options = {}, seen = new Set()
for (let index = 2; index < process.argv.length; index++) {
  const name = process.argv[index]
  if (name === '--help') {
    console.log('node tests/font-manifest.mjs --raw <fixture-dir>/manifest.raw.json --out <same-fixture-dir>/manifest-pinned.json')
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
if (manifest.origin !== 'Contributor-original geometric font programs, Apache-2.0' || !Array.isArray(manifest.cases) || manifest.cases.length > 32) throw new Error('Expected the native original-font exporter')
manifest.rawManifestSha256 = hash(rawBytes)
manifest.mappingRule = 'PDF 32000-1:2008 section 9.7.4.2; malformed cases remain separately classified'
const pairs = { 'truetype-unicode-swapped': 'truetype-identity', 'opentype-cid-unicode-swapped': 'opentype-cid-swapped-charset' }
for (const fixture of manifest.cases) {
  if (!/^[a-z0-9-]+$/.test(fixture.id) || fixture.file !== `${fixture.id}.pdf` || fixture.fontFile !== `${fixture.id}.font`) throw new Error('Unsafe original fixture filename')
  fixture.sha256 = hash(await readFile(resolve(dirname(options.raw), fixture.file)))
  fixture.fontSha256 = hash(await readFile(resolve(dirname(options.raw), fixture.fontFile)))
  fixture.mappingBasis = fixture.classification === 'format-control' ? 'normative-format-control' : fixture.assertMapping ? 'retained-compatibility-control' : 'descriptive-malformed-wrapper-probe'
  const white = [255, 255, 255], black = [0, 0, 0], shape = fontGlyphShape(fixture)
  fixture.regions = [
    ['left', [13, 19, 2, 2], shape === 'left-bar' ? black : white, fixture.assertMapping],
    ['right', [21, 19, 2, 2], shape === 'right-l' ? black : white, fixture.assertMapping],
    ['foot', [25, 11, 2, 2], shape === 'right-l' ? black : white, fixture.assertMapping],
    ['counter', [25, 16, 2, 6], white, true],
    ['marker', [1, 1, 2, 2], [255, 0, 0], true],
    ['background', [32, 30, 2, 2], white, true],
  ].map(([name, rect, rgb, assertColor]) => ({ name, rect, rgb, assertColor, tolerance: 8 }))
  if (pairs[fixture.id]) fixture.expectedMatch = pairs[fixture.id]
}
validateFontManifest(manifest)
const output = `${JSON.stringify(manifest, null, 2)}\n`
try {
  await writeFile(options.out, output, { flag: 'wx' })
} catch (error) {
  if (error.code !== 'EEXIST') throw error
  assert.equal(await readFile(options.out, 'utf8'), output, 'Refuse to replace a different pinned manifest')
}
console.log(JSON.stringify({ sha256: hash(output), cases: manifest.cases.length, rasters: manifest.cases.length * manifest.scales.length, regions: manifest.cases.length * manifest.scales.length * 6, mappingAssertions: manifest.cases.filter(fixture => fixture.assertMapping).length * manifest.scales.length, mappingObservations: manifest.cases.filter(fixture => !fixture.assertMapping).length * manifest.scales.length, exactPairs: manifest.cases.filter(fixture => fixture.expectedMatch).length * manifest.scales.length }))
