// Real editor Worker verification; generated original PDFs stay outside tracked files.
import assert from 'node:assert/strict'
import { readFile, writeFile, mkdir, readdir } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createServer } from 'vite'
import { chromium, webkit } from 'playwright'
import { engineOverride } from './engine-override.mjs'
import { installCacheQuietProbe } from './cache-probe.mjs'
import { validateFontManifest, compareFontRasters, fontSummary } from './font-support.mjs'
import { frozenSharedSource, sharedPackageNames } from './hairline-shared.mjs'

const options = { browser: 'chromium', 'timeout-ms': 20000 }, seen = new Set()
for (let index = 2; index < process.argv.length; index++) {
  const name = process.argv[index]
  if (name === '--help') {
    console.log('node tests/font.mjs --manifest <pinned-manifest.json> --engine-dir <isolated-engine> --out <report.json> [--shared-dir <frozen-sdk>] [--browser chromium|webkit] [--timeout-ms 20000]')
    process.exit(0)
  }
  if (!['--manifest', '--engine-dir', '--shared-dir', '--out', '--browser', '--timeout-ms'].includes(name) || seen.has(name)) throw new Error('Unknown or duplicate option')
  seen.add(name)
  const value = process.argv[++index]
  if (!value || value.startsWith('--')) throw new Error('Missing option value')
  options[name.slice(2)] = name === '--timeout-ms' ? Number(value) : value
}
if (!options.manifest || !options.out || !options['engine-dir'] || !['chromium', 'webkit'].includes(options.browser) || !Number.isSafeInteger(options['timeout-ms']) || options['timeout-ms'] <= 0) throw new Error('Pinned manifest, isolated engine, output, browser, and positive timeout are required')
const root = fileURLToPath(new URL('..', import.meta.url))
const hash = bytes => createHash('sha256').update(bytes).digest('hex')
const safe = error => String(error?.message ?? error).replace(/\/Users\/[^\s:)'"<>]+|\/home\/[^\s:)'"<>]+/g, '<local-path>')
const report = {
  schema: 1, started: new Date().toISOString(), status: 'running', browser: options.browser,
  scope: 'Original embedded-font mapping controls through the real editor Worker',
  configuration: { scales: [1, 2], viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1, timeoutMs: options['timeout-ms'], cleanupTimeoutMs: 60000 },
  expected: null,
  engine: { source: safe(process.env.KITION_GOLDEN_ENGINE_REVISION ?? 'unspecified; binary fingerprints alone do not establish source'), profile: safe(process.env.KITION_GOLDEN_ENGINE_PROFILE ?? 'unspecified') },
  limitations: ['Contributor-original geometric fonts only; no corpus font extraction or corpus fidelity claim.', 'Explicit control pairs compare complete RGBA within one backend; no cross-renderer pixel identity is assumed.', 'Normative mapping assertions, accepted compatibility assertions, and descriptive malformed-wrapper observations are classified separately.', 'An observed-only mapping sample can pass measurement integrity without accepting its candidate mapping.', 'Empty pixels cannot distinguish empty .notdef from an absent glyph; native traced GID and no-fallback checks are separate.', 'Direct RGBA is returned by the live editor Worker; this does not prove physical screen presentation.'],
  cases: [], comparisons: [], errors: [],
}
let server, browser, context, override, shared
const save = async () => { await mkdir(dirname(resolve(options.out)), { recursive: true }); await writeFile(options.out, `${JSON.stringify(report, null, 2)}\n`) }
async function bounded(promise, label, timeout = options['timeout-ms']) {
  let timer
  try { return await Promise.race([promise, new Promise((_resolve, reject) => { timer = setTimeout(() => reject(new Error(`${label} timed out`)), timeout) })]) }
  finally { clearTimeout(timer) }
}
async function close(name, resource) { try { if (resource) await bounded(resource.close(), `${name} cleanup`, report.configuration.cleanupTimeoutMs) } catch (error) { report.errors.push(safe(error)) } }
async function sourceFingerprints() {
  const fingerprints = {}
  async function walk(directory, label) {
    for (const entry of (await readdir(directory, { withFileTypes: true })).sort((a, b) => a.name.localeCompare(b.name))) {
      if (shared && entry.isSymbolicLink()) throw new Error(`Source snapshot contains a symlink: ${label}/${entry.name}`)
      if (entry.isDirectory()) await walk(resolve(directory, entry.name), `${label}/${entry.name}`)
      else if (entry.isFile()) fingerprints[`${label}/${entry.name}`] = hash(await readFile(resolve(directory, entry.name)))
    }
  }
  await walk(resolve(root, 'src'), 'web/src')
  const sharedRoot = shared?.root ?? resolve(root, '../../kition-sdk')
  for (const name of shared ? sharedPackageNames : ['kit-editor', 'kit-host', 'kit-ui-protocol', 'kit-dev-host']) {
    await walk(resolve(sharedRoot, 'packages', name, 'src'), `kition-sdk/packages/${name}/src`)
    if (shared) fingerprints[`kition-sdk/packages/${name}/package.json`] = hash(await readFile(resolve(sharedRoot, 'packages', name, 'package.json')))
  }
  for (const name of ['package.json', 'vite.config.ts']) fingerprints[`web/${name}`] = hash(await readFile(resolve(root, name)))
  return fingerprints
}
function summarize() {
  report.summary = {
    rasters: fontSummary(report.cases.flatMap(item => item.samples)),
    regions: fontSummary(report.cases.flatMap(item => item.samples.flatMap(sample => sample.regions))),
    controls: fontSummary(report.comparisons),
    mappingAssertions: fontSummary(report.cases.filter(item => item.assertMapping).flatMap(item => item.samples.map(sample => ({ status: sample.mapping?.status ?? sample.status })))),
    observedMappings: report.cases.filter(item => !item.assertMapping).flatMap(item => item.samples.map(sample => ({ case: item.id, scale: sample.scale, measurementStatus: sample.status, mapping: sample.mapping ?? null }))),
  }
}
try {
  const manifestBytes = await readFile(options.manifest), manifest = JSON.parse(manifestBytes)
  report.manifestSha256 = hash(manifestBytes)
  validateFontManifest(manifest)
  report.harness = {}
  for (const name of ['font.mjs', 'font-support.mjs', 'hairline-shared.mjs', 'engine-override.mjs', 'cache-probe.mjs']) report.harness[name] = hash(await readFile(resolve(root, 'tests', name)))
  report.cases = manifest.cases.map(fixture => ({ ...fixture, status: 'untested', connection: { status: 'untested' }, samples: manifest.scales.map(scale => ({ scale, dpi: scale * 72, status: 'untested', regions: fixture.regions.map((region, index) => ({ index, ...region, status: 'untested' })) })), errors: [], pageErrors: [] }))
  report.comparisons = manifest.cases.filter(fixture => fixture.expectedMatch).flatMap(fixture => manifest.scales.map(scale => ({ case: fixture.id, control: fixture.expectedMatch, scale, status: 'untested' })))
  report.expected = { cases: report.cases.length, rasterSamples: report.cases.length * manifest.scales.length, regionChecks: manifest.cases.reduce((sum, fixture) => sum + fixture.regions.length, 0) * manifest.scales.length, controlComparisons: report.comparisons.length, mappingAssertions: manifest.cases.filter(fixture => fixture.assertMapping).length * manifest.scales.length, mappingObservations: manifest.cases.filter(fixture => !fixture.assertMapping).length * manifest.scales.length }
  // Verify every fixture before starting the browser, leaving complete denominators on failure.
  const fixtures = new Map()
  for (const fixture of manifest.cases) {
    const bytes = await readFile(resolve(dirname(resolve(options.manifest)), fixture.file))
    assert.equal(hash(bytes), fixture.sha256, `SHA256 differs for ${fixture.id}`)
    const fontBytes = await readFile(resolve(dirname(resolve(options.manifest)), fixture.fontFile))
    assert.equal(hash(fontBytes), fixture.fontSha256, `Original program SHA256 differs for ${fixture.id}`)
    fixtures.set(fixture.id, bytes)
  }
  override = await engineOverride(resolve(options['engine-dir']), resolve(root, 'public/engine'))
  report.engine.hashes = override.hashes
  report.engine.served = override.served
  if (options['shared-dir']) shared = await frozenSharedSource(resolve(options['shared-dir']), resolve(root, '../../kition-sdk'))
  report.sharedSource = shared?.report ?? { mode: 'live' }
  report.sourceFiles = { before: await sourceFingerprints(), after: null, changed: [] }
  shared?.setExpectedFingerprints(report.sourceFiles.before)
  summarize(); await save()
  server = await createServer({
    root, configFile: resolve(root, 'vite.config.ts'), plugins: [override.plugin, ...(shared ? [shared.plugin] : [])],
    ...(shared ? { resolve: { alias: shared.aliases }, optimizeDeps: { exclude: shared.packages }, worker: { plugins: shared.workerPlugins } } : {}),
    server: { host: '127.0.0.1', port: 0, ...(shared ? { fs: { allow: [resolve(root, '..'), shared.root] } } : {}) }, logLevel: 'error',
  })
  await server.listen()
  const origin = `http://127.0.0.1:${server.httpServer.address().port}`
  browser = await ({ chromium, webkit })[options.browser].launch()
  report.browserVersion = browser.version()
  for (const result of report.cases) {
    const fixture = manifest.cases.find(item => item.id === result.id)
    result.status = 'running'
    const openedAt = performance.now()
    const servedBefore = { ...override.served }
    try {
      // Fresh context and one unique filename prevent stale document/connection readiness.
      context = await browser.newContext({ viewport: report.configuration.viewport, deviceScaleFactor: 1 })
      await context.addInitScript(installCacheQuietProbe)
      const page = await context.newPage()
      page.setDefaultTimeout(options['timeout-ms'])
      page.on('pageerror', error => result.pageErrors.push(safe(error)))
      await page.route('**/sample/Sample.pdf', route => route.fulfill({ status: 404, body: '' }))
      await page.goto(`${origin}/dev.html?locale=en-US`)
      await page.waitForFunction(() => document.getElementById('status')?.textContent === 'choose a file' && window.devHost?.connection() === null)
      await page.locator('#file').setInputFiles({ name: fixture.file, mimeType: 'application/pdf', buffer: fixtures.get(fixture.id) })
      await page.waitForFunction(name => document.getElementById('status')?.textContent?.includes(`${name} open in `) && window.devHost?.connection() !== null, fixture.file)
      await page.frameLocator('#kit').getByTestId('kit-canvas-surface').waitFor()
      const frame = await page.locator('#kit').elementHandle().then(handle => handle.contentFrame())
      assert.ok(frame, 'Real editor frame must exist')
      await bounded(frame.evaluate(async () => {
        const url = performance.getEntriesByType('resource').map(item => item.name).find(name => /\/engine\/kitEngine\.ts(?:\?|$)/.test(name))
        if (!url) throw new Error('Live editor engine module is unavailable')
        const { KitEngine } = await import(url)
        const execute = KitEngine.prototype.execute
        KitEngine.prototype.execute = function (...args) {
          window.__fontEngine = this
          KitEngine.prototype.execute = execute
          return execute.apply(this, args)
        }
      }), 'Capture existing engine')
      const command = await bounded(page.evaluate(() => window.devHost.connection().ai.run([{ command: 'bookmark_list', params: { limit: 1 } }], false)), 'Read-only engine capture')
      assert.equal(command.completed.length, 1)
      await frame.waitForFunction(() => window.__fontEngine && window.__kitionCacheQuiet.pending === 0 && performance.now() - window.__kitionCacheQuiet.lastActivity >= 250)
      const info = await frame.evaluate(() => ({ pages: window.__fontEngine.info.inspected.pages.map(item => item.area), dirty: window.__fontEngine.info.dirty, dpr: devicePixelRatio }))
      assert.equal(info.pages.length, 1); assert.equal(info.pages[0].width, manifest.pageWidth); assert.equal(info.pages[0].height, manifest.pageHeight)
      assert.equal(info.dirty, false); assert.equal(info.dpr, 1)
      result.connection = { status: 'passed', openAndCaptureMs: performance.now() - openedAt, freshContext: true, initialConnectionNull: true, openedFilename: fixture.file, captureCommand: 'bookmark_list', ...info }
      for (const sample of result.samples) {
        try {
          Object.assign(sample, await bounded(frame.evaluate(async ({ fixture, manifest, scale }) => {
            const { analyzeFont } = await import('/tests/font-support.mjs')
            const engine = window.__fontEngine, area = engine.info.inspected.pages[0].area
            const width = manifest.pageWidth * scale, height = manifest.pageHeight * scale
            const before = window.__kitionCacheQuiet.dispatched, started = performance.now()
            const pixels = await engine.render(area.x, area.y, scale, width, height)
            const renderMs = performance.now() - started
            if (window.__kitionCacheQuiet.dispatched <= before) throw new Error('No real Worker raster request was observed')
            const analysis = analyzeFont(pixels, width, height, manifest, fixture, scale)
            const digest = await crypto.subtle.digest('SHA-256', pixels)
            return { ...analysis, renderMs, workerRasterRequests: window.__kitionCacheQuiet.dispatched - before, rgbaSha256: [...new Uint8Array(digest)].map(byte => byte.toString(16).padStart(2, '0')).join('') }
          }, { fixture, manifest, scale: sample.scale }), 'Font Worker render'))
          console.log(JSON.stringify({ case: fixture.id, scale: sample.scale, status: sample.status, mapping: sample.mapping, regions: sample.regions.length, failures: sample.regions.filter(region => region.status !== 'passed').map(region => ({ index: region.index, failures: region.failures })) }))
        } catch (error) {
          sample.error = safe(error); sample.status = /timeout|timed out/i.test(sample.error) ? 'timeout' : 'failed'
          break
        }
      }
      assert.deepEqual(result.pageErrors, [], 'Editor must not report runtime errors')
      result.status = result.samples.every(sample => sample.status === 'passed') ? 'passed' : 'failed'
    } catch (error) {
      result.errors.push(safe(error)); result.status = 'failed'
      if (result.connection.status === 'untested') result.connection.status = 'failed'
    } finally {
      await close('context', context); context = null
      result.engineAssetRequests = Object.fromEntries(Object.entries(override.served).map(([name, count]) => [name, count - servedBefore[name]]))
      if (Object.values(result.engineAssetRequests).some(count => count < 1)) { result.errors.push('Isolated engine JS and WASM were not both requested for this document'); result.status = 'failed' }
      summarize(); await save()
    }
  }
  for (const comparison of report.comparisons) {
    const sample = id => report.cases.find(item => item.id === id)?.samples.find(item => item.scale === comparison.scale)
    Object.assign(comparison, compareFontRasters(sample(comparison.case), sample(comparison.control)))
  }
} catch (error) { report.errors.push(safe(error)) }
finally {
  await close('context', context); await close('browser', browser); await close('server', server)
  if (shared && !shared.finish()) report.errors.push('Frozen shared source verification failed; see sharedSource violations and required modules')
  if (report.sourceFiles) {
    try {
      report.sourceFiles.after = await sourceFingerprints()
      report.sourceFiles.changed = [...new Set([...Object.keys(report.sourceFiles.before), ...Object.keys(report.sourceFiles.after)])].filter(name => report.sourceFiles.before[name] !== report.sourceFiles.after[name])
      if (report.sourceFiles.changed.length) report.errors.push('Editor or shared SDK source changed during browser execution; see sourceFiles.changed')
    } catch (error) { report.errors.push(safe(error)) }
  }
  summarize()
  report.status = report.errors.length === 0 && report.cases.length > 0 && report.cases.every(item => item.status === 'passed') && report.comparisons.every(item => item.status === 'passed') ? 'passed' : 'failed'
  report.finished = new Date().toISOString()
  await save()
  process.exitCode = report.status === 'passed' ? 0 : 1
}
