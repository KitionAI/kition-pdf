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
import { validateDashManifest, compareDashRasters, dashSummary } from './dash-support.mjs'
import { frozenSharedSource, installedSdk, sdkFingerprints } from './hairline-shared.mjs'

const options = { browser: 'chromium', 'timeout-ms': 20000 }, seen = new Set()
for (let index = 2; index < process.argv.length; index++) {
  const name = process.argv[index]
  if (name === '--help') {
    console.log('node tests/dash.mjs --manifest <pinned-manifest.json> --engine-dir <isolated-engine> --out <report.json> [--shared-dir <frozen-sdk>] [--browser chromium|webkit] [--timeout-ms 20000]')
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
  scope: 'Original affine dashed hairline controls through the real editor Worker',
  configuration: { scales: [1, 2, 4], viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1, timeoutMs: options['timeout-ms'], cleanupTimeoutMs: 60000 },
  expected: null,
  engine: { source: safe(process.env.KITION_GOLDEN_ENGINE_REVISION ?? 'unspecified; binary fingerprints alone do not establish source'), profile: safe(process.env.KITION_GOLDEN_ENGINE_PROFILE ?? 'unspecified') },
  limitations: ['Contributor-original paths only; no full-corpus fidelity claim.', 'User-space scalar intervals precede affine transformation; curved and compound paths use authored probes plus independently segmented controls.', 'Same-backend complete RGBA comparisons allow channel differences up to 2; exact equality is reported separately.', 'Direct RGBA comes from the live editor Worker; physical screen presentation is not asserted.'],
  cases: [], comparisons: [], errors: [],
}
const rasters = new Map()
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
  // The SDK package the editor runs on: the frozen copy (--shared-dir), else the installed one.
  Object.assign(fingerprints, await sdkFingerprints(shared?.root ?? await installedSdk(root), { rejectSymlinks: Boolean(shared) }))
  for (const name of ['package.json', 'vite.config.ts']) fingerprints[`web/${name}`] = hash(await readFile(resolve(root, name)))
  return fingerprints
}
function summarize() {
  report.summary = {
    rasters: dashSummary(report.cases.flatMap(item => item.samples)),
    regions: dashSummary(report.cases.flatMap(item => item.samples.flatMap(sample => sample.regions))),
    controls: dashSummary(report.comparisons),
    points: dashSummary(report.cases.flatMap(item => item.samples.flatMap(sample => sample.pointChecks ?? []))),
    excludedRegions: report.cases.flatMap(item => item.samples.flatMap(sample => (sample.excludedRegions ?? []).map(region => ({ case: item.id, scale: sample.scale, ...region })))),
  }
}
try {
  const manifestBytes = await readFile(options.manifest), manifest = JSON.parse(manifestBytes)
  report.manifestSha256 = hash(manifestBytes)
  validateDashManifest(manifest)
  report.harness = {}
  for (const name of ['dash.mjs', 'dash-support.mjs', 'hairline-shared.mjs', 'sdk-editor.mjs', 'engine-override.mjs', 'cache-probe.mjs']) report.harness[name] = hash(await readFile(resolve(root, 'tests', name)))
  report.cases = manifest.cases.map(fixture => ({ ...fixture, status: 'untested', connection: { status: 'untested' }, samples: manifest.scales.map(scale => ({ scale, dpi: scale * 72, status: 'untested', regions: fixture.regions.filter(region => !region.scales || region.scales.includes(scale)).map((region, index) => ({ index, ...region, status: 'untested' })), pointChecks: (fixture.pointInk ?? []).map(point => ({ ...point, status: 'untested' })), excludedRegions: fixture.regions.filter(region => region.scales && !region.scales.includes(scale)).map(region => ({ id: region.id, scales: region.scales, reason: 'outside-authored-region-scales' })) })), errors: [], pageErrors: [] }))
  report.comparisons = manifest.cases.filter(fixture => fixture.expectedMatch).flatMap(fixture => manifest.scales.map(scale => ({ case: fixture.id, control: fixture.expectedMatch, scale, status: 'untested' })))
  report.expected = { cases: report.cases.length, rasterSamples: report.cases.length * manifest.scales.length, regionChecks: report.cases.reduce((sum, fixture) => sum + fixture.samples.reduce((count, sample) => count + sample.regions.length, 0), 0), pointChecks: report.cases.reduce((sum, fixture) => sum + fixture.samples.reduce((count, sample) => count + sample.pointChecks.length, 0), 0), controlComparisons: report.comparisons.length }
  // Verify every fixture before starting the browser, leaving complete denominators on failure.
  const fixtures = new Map()
  for (const fixture of manifest.cases) {
    const bytes = await readFile(resolve(dirname(resolve(options.manifest)), fixture.file))
    assert.equal(hash(bytes), fixture.sha256, `SHA256 differs for ${fixture.id}`)
    fixtures.set(fixture.id, bytes)
  }
  override = await engineOverride(resolve(options['engine-dir']), resolve(root, 'public/engine'))
  report.engine.hashes = override.hashes
  report.engine.served = override.served
  if (options['shared-dir']) shared = await frozenSharedSource(resolve(options['shared-dir']), await installedSdk(root))
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
        const { KitEngine } = await import('/tests/sdk-editor.mjs')
        const execute = KitEngine.prototype.execute
        KitEngine.prototype.execute = function (...args) {
          window.__dashEngine = this
          KitEngine.prototype.execute = execute
          return execute.apply(this, args)
        }
      }), 'Capture existing engine')
      const command = await bounded(page.evaluate(() => window.devHost.connection().ai.run([{ command: 'bookmark_list', params: { limit: 1 } }], false)), 'Read-only engine capture')
      assert.equal(command.completed.length, 1)
      await frame.waitForFunction(() => window.__dashEngine && window.__kitionCacheQuiet.pending === 0 && performance.now() - window.__kitionCacheQuiet.lastActivity >= 250)
      const info = await frame.evaluate(() => ({ pages: window.__dashEngine.info.inspected.pages.map(item => item.area), dirty: window.__dashEngine.info.dirty, dpr: devicePixelRatio }))
      assert.equal(info.pages.length, 1); assert.equal(info.pages[0].width, manifest.pageWidth); assert.equal(info.pages[0].height, manifest.pageHeight)
      assert.equal(info.dirty, false); assert.equal(info.dpr, 1)
      result.connection = { status: 'passed', openAndCaptureMs: performance.now() - openedAt, freshContext: true, initialConnectionNull: true, openedFilename: fixture.file, captureCommand: 'bookmark_list', ...info }
      for (const sample of result.samples) {
        try {
          const rendered = await bounded(frame.evaluate(async ({ fixture, manifest, scale }) => {
            const { analyzeDash } = await import('/tests/dash-support.mjs')
            const engine = window.__dashEngine, area = engine.info.inspected.pages[0].area
            const width = manifest.pageWidth * scale, height = manifest.pageHeight * scale
            const before = window.__kitionCacheQuiet.dispatched, started = performance.now()
            const pixels = await engine.render(area.x, area.y, scale, width, height)
            const renderMs = performance.now() - started
            if (window.__kitionCacheQuiet.dispatched <= before) throw new Error('No real Worker raster request was observed')
            const analysis = analyzeDash(pixels, width, height, manifest, fixture, scale)
            const digest = await crypto.subtle.digest('SHA-256', pixels)
            let binary = ''
            for (let start = 0; start < pixels.length; start += 16384) binary += String.fromCharCode(...pixels.subarray(start, start + 16384))
            return { ...analysis, rgbaBase64: btoa(binary), renderMs, workerRasterRequests: window.__kitionCacheQuiet.dispatched - before, rgbaSha256: [...new Uint8Array(digest)].map(byte => byte.toString(16).padStart(2, '0')).join('') }
          }, { fixture, manifest, scale: sample.scale }), 'Dash Worker render')
          const { rgbaBase64, ...measurement } = rendered
          rasters.set(`${fixture.id}:${sample.scale}`, Buffer.from(rgbaBase64, 'base64'))
          Object.assign(sample, measurement)
          console.log(JSON.stringify({ case: fixture.id, scale: sample.scale, status: sample.status, regions: sample.regions.length, failures: sample.regions.filter(region => region.status !== 'passed').map(region => ({ index: region.index, failures: region.failures })) }))
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
    Object.assign(comparison, compareDashRasters(sample(comparison.case), sample(comparison.control), rasters.get(`${comparison.case}:${comparison.scale}`), rasters.get(`${comparison.control}:${comparison.scale}`)))
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
