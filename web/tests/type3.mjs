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
import { validateType3Manifest, compareType3Rasters, type3Summary, plannedCases, expectedRejection } from './type3-support.mjs'
import { frozenSharedSource, installedSdk, sdkFingerprints } from './hairline-shared.mjs'

const options = { browser: 'chromium', 'timeout-ms': 20000 }, seen = new Set()
for (let index = 2; index < process.argv.length; index++) {
  const name = process.argv[index]
  if (name === '--help') {
    console.log('node tests/type3.mjs --manifest <pinned-manifest.json> --engine-dir <isolated-engine> --out <report.json> [--shared-dir <frozen-sdk>] [--browser chromium|webkit] [--timeout-ms 20000]')
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
  scope: 'Original Type3 recursion and finite nesting through the real editor Worker',
  configuration: { scales: [1, 2], passes: [1, 2], viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1, timeoutMs: options['timeout-ms'], cleanupTimeoutMs: 60000 },
  expected: null,
  engine: { source: safe(process.env.KITION_GOLDEN_ENGINE_REVISION ?? 'unspecified; binary fingerprints alone do not establish source'), profile: safe(process.env.KITION_GOLDEN_ENGINE_PROFILE ?? 'unspecified') },
  limitations: ['Original authored type3 controls only; no corpus fidelity or general performance claim.', 'Explicit control pairs compare complete RGBA within one backend; no cross-renderer pixel identity is assumed.', 'Expected truncation rejects through the existing SDK error; detailed native warning counts are not exposed by Worker.', 'Direct RGBA is returned by the live editor Worker; this does not prove physical screen presentation.'],
  cases: [], comparisons: [], repeats: [], cleanup: [], errors: [],
}
let server, browser, context, override, shared
const save = async () => { await mkdir(dirname(resolve(options.out)), { recursive: true }); await writeFile(options.out, `${JSON.stringify(report, null, 2)}\n`) }
async function bounded(promise, label, timeoutMs = options['timeout-ms']) {
  let timer
  try { return await Promise.race([promise, new Promise((_resolve, reject) => { timer = setTimeout(() => reject(new Error(`${label} timed out`)), timeoutMs) })]) }
  finally { clearTimeout(timer) }
}
async function close(name, resource) {
  if (!resource) return
  const started = performance.now()
  const entry = { resource: name, timeoutMs: report.configuration.cleanupTimeoutMs, status: 'running' }
  report.cleanup.push(entry)
  try { await bounded(resource.close(), `${name} cleanup`, entry.timeoutMs); entry.status = 'passed' }
  catch (error) { entry.status = 'failed'; entry.error = safe(error); report.errors.push(entry.error) }
  finally { entry.elapsedMs = performance.now() - started }
}
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
    rasters: type3Summary(report.cases.flatMap(item => item.samples.filter(sample => sample.expected === 'raster'))),
    rejections: type3Summary(report.cases.flatMap(item => item.samples.filter(sample => sample.expected === 'rejection'))),
    regions: type3Summary(report.cases.flatMap(item => item.samples.flatMap(sample => sample.regions))),
    controls: type3Summary(report.comparisons),
    repeats: type3Summary(report.repeats),
    resetPages: type3Summary(report.cases.map(item => item.resetPage)),
    errorUi: type3Summary(report.cases.map(item => item.errorUi)),
    recovery: type3Summary(report.cases.flatMap(item => item.recovery ? [item.recovery] : [])),
  }
}
async function captureEngine(page, filename) {
  await page.waitForFunction(name => document.getElementById('status')?.textContent?.includes(`${name} open in `) && window.devHost?.connection() !== null, filename)
  await page.frameLocator('#kit').getByTestId('kit-canvas-surface').waitFor()
  const frame = await page.locator('#kit').elementHandle().then(handle => handle.contentFrame())
  assert.ok(frame, 'Real editor frame must exist')
  await bounded(frame.evaluate(async () => {
    const { KitEngine } = await import('/tests/sdk-editor.mjs')
    const execute = KitEngine.prototype.execute
    KitEngine.prototype.execute = function (...args) {
      window.__type3Engine = this
      KitEngine.prototype.execute = execute
      return execute.apply(this, args)
    }
  }), 'Capture existing engine')
  const command = await bounded(page.evaluate(() => window.devHost.connection().ai.run([{ command: 'bookmark_list', params: { limit: 1 } }], false)), 'Read-only engine capture')
  assert.equal(command.completed.length, 1)
  await frame.waitForFunction(() => window.__type3Engine && window.__kitionCacheQuiet.pending === 0 && performance.now() - window.__kitionCacheQuiet.lastActivity >= 250)
  return frame
}
async function renderSample(frame, fixture, manifest, scale, pageIndex = 0) {
  return bounded(frame.evaluate(async ({ fixture, manifest, scale, pageIndex }) => {
    const { analyzeType3, expectedRejection } = await import('/tests/type3-support.mjs')
    const engine = window.__type3Engine, area = engine.info.inspected.pages[pageIndex].area
    const width = manifest.pageWidth * scale, height = manifest.pageHeight * scale
    const before = window.__kitionCacheQuiet.dispatched, started = performance.now()
    let pixels, rejection
    try { pixels = await engine.render(area.x, area.y, scale, width, height) }
    catch (error) { rejection = String(error?.message ?? error) }
    const evidence = { renderMs: performance.now() - started, workerRasterRequests: window.__kitionCacheQuiet.dispatched - before }
    if (!evidence.workerRasterRequests) throw new Error('No real Worker raster request was observed')
    if (fixture.expectedTruncation) return { ...evidence, status: expectedRejection(rejection) ? 'passed' : 'failed', rejected: rejection !== undefined, rejection, failures: expectedRejection(rejection) ? [] : ['expected-render-budget-rejection-missing-or-different'], regions: [] }
    if (rejection !== undefined) throw new Error(rejection)
    const analysis = analyzeType3(pixels, width, height, manifest, fixture, scale)
    const digest = await crypto.subtle.digest('SHA-256', pixels)
    return { ...analysis, ...evidence, rgbaSha256: [...new Uint8Array(digest)].map(byte => byte.toString(16).padStart(2, '0')).join('') }
  }, { fixture, manifest, scale, pageIndex }), 'Type3 Worker render')
}
try {
  const manifestBytes = await readFile(options.manifest), manifest = JSON.parse(manifestBytes)
  report.manifestSha256 = hash(manifestBytes)
  validateType3Manifest(manifest)
  report.harness = {}
  for (const name of ['type3.mjs', 'type3-support.mjs', 'mesh-support.mjs', 'hairline-shared.mjs', 'sdk-editor.mjs', 'engine-override.mjs', 'cache-probe.mjs']) report.harness[name] = hash(await readFile(resolve(root, 'tests', name)))
  report.cases = plannedCases(manifest)
  report.comparisons = manifest.cases.filter(fixture => fixture.expectedMatch && !fixture.expectedTruncation).flatMap(fixture => [1, 2].flatMap(pass => manifest.scales.map(scale => ({ case: fixture.id, control: fixture.expectedMatch, scale, pass, status: 'untested' }))))
  report.repeats = manifest.cases.filter(fixture => !fixture.expectedTruncation).flatMap(fixture => manifest.scales.map(scale => ({ case: fixture.id, scale, status: 'untested' })))
  report.expected = { repeatComparisons: report.repeats.length, resetPageChecks: report.cases.length, cases: report.cases.length, rasterSamples: report.cases.reduce((n, item) => n + item.samples.filter(sample => sample.expected === 'raster').length, 0), expectedRejections: report.cases.reduce((n, item) => n + item.samples.filter(sample => sample.expected === 'rejection').length, 0), regionChecks: report.cases.reduce((n, item) => n + item.samples.reduce((s, sample) => s + sample.regions.length, 0), 0), controlComparisons: report.comparisons.length, errorUiChecks: report.cases.length, cleanRecoveryChecks: report.cases.filter(item => item.expectedTruncation).length }
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
      const frame = await captureEngine(page, fixture.file)
      const info = await frame.evaluate(() => ({ pages: window.__type3Engine.info.inspected.pages.map(item => item.area), dirty: window.__type3Engine.info.dirty, dpr: devicePixelRatio }))
      assert.equal(info.pages.length, manifest.pageCount); assert.equal(info.pages[0].width, manifest.pageWidth); assert.equal(info.pages[0].height, manifest.pageHeight)
      assert.equal(info.dirty, false); assert.equal(info.dpr, 1)
      result.connection = { status: 'passed', openAndCaptureMs: performance.now() - openedAt, freshContext: true, initialConnectionNull: true, openedFilename: fixture.file, captureCommand: 'bookmark_list', ...info }
      const failureUi = frame.getByTestId('kit-render-failure')
      if (fixture.expectedTruncation) {
        await failureUi.waitFor({ state: 'visible' })
        const text = await failureUi.innerText()
        assert.ok(expectedRejection(text), 'Editor must expose the expected rendering-budget rejection')
        assert.ok(await failureUi.getByRole('button').isVisible(), 'Expected error must have a visible retry control')
        result.errorUi = { status: 'passed', expected: 'visible-budget-error-and-retry', text: safe(text), retryVisible: true }
      } else {
        assert.equal(await failureUi.count(), 0, 'Finite Type3 fixture must not show a render failure')
        result.errorUi = { status: 'passed', expected: 'no-render-failure' }
      }
      for (const sample of result.samples) {
        if (sample.pass === 2 && sample.scale === manifest.scales[0]) {
          try {
            result.resetPage = { ...result.resetPage, ...await renderSample(frame, { expectedTruncation: false, regions: manifest.resetPage.regions }, manifest, 1, manifest.resetPage.index), pageIndex: manifest.resetPage.index, sameWorker: true }
            if (result.resetPage.status !== 'passed') throw new Error('Clean page after first Type3 pass failed')
          } catch (error) {
            result.resetPage.error = safe(error)
            result.resetPage.status = /timeout|timed out/i.test(result.resetPage.error) ? 'timeout' : 'failed'
            break
          }
        }
        try {
          Object.assign(sample, await renderSample(frame, fixture, manifest, sample.scale))
          if (sample.rejection) sample.rejection = safe(sample.rejection)
          console.log(JSON.stringify({ case: fixture.id, scale: sample.scale, pass: sample.pass, expected: sample.expected, status: sample.status, regions: sample.regions.length, failures: sample.failures ?? sample.regions.filter(region => region.status !== 'passed').map(region => ({ index: region.index, failures: region.failures })) }))
        } catch (error) {
          sample.error = safe(error); sample.status = /timeout|timed out/i.test(sample.error) ? 'timeout' : 'failed'
          break
        }
      }
      if (result.recovery && result.samples.every(sample => sample.status === 'passed')) {
        result.recovery.status = 'running'
        // Reopen a known clean document in this same editor session after rejection.
        const control = manifest.cases.find(item => item.id === fixture.expectedMatch)
        const filename = `${fixture.id}-recovery-${control.file}`
        await frame.evaluate(() => { window.__type3PreviousEngine = window.__type3Engine })
        await page.locator('#file').setInputFiles({ name: filename, mimeType: 'application/pdf', buffer: fixtures.get(control.id) })
        const recoveryFrame = await captureEngine(page, filename)
        const identity = await recoveryFrame.evaluate(() => ({ dirty: window.__type3Engine.info.dirty, engineReplaced: window.__type3Engine !== window.__type3PreviousEngine }))
        assert.equal(identity.dirty, false); assert.equal(identity.engineReplaced, true)
        assert.equal(await recoveryFrame.getByTestId('kit-render-failure').count(), 0)
        const sample = await renderSample(recoveryFrame, control, manifest, 1)
        result.recovery = { ...result.recovery, ...sample, ...identity, filename, sameBrowserContext: true }
      }
      assert.deepEqual(result.pageErrors, [], 'Editor must not report runtime errors')
      result.status = result.samples.every(sample => sample.status === 'passed') && result.errorUi.status === 'passed' && result.resetPage.status === 'passed' && (!result.recovery || result.recovery.status === 'passed') ? 'passed' : 'failed'
    } catch (error) {
      result.errors.push(safe(error)); result.status = 'failed'
      if (result.connection.status === 'passed' && result.errorUi.status === 'untested') result.errorUi.status = 'failed'
      if (result.recovery?.status === 'running') result.recovery.status = 'failed'
      if (result.connection.status === 'untested') result.connection.status = /timeout|timed out/i.test(safe(error)) ? 'timeout' : 'failed'
    } finally {
      await close('context', context); context = null
      result.engineAssetRequests = Object.fromEntries(Object.entries(override.served).map(([name, count]) => [name, count - servedBefore[name]]))
      if (Object.values(result.engineAssetRequests).some(count => count < 1)) { result.errors.push('Isolated engine JS and WASM were not both requested for this document'); result.status = 'failed' }
      summarize(); await save()
    }
  }
  for (const comparison of report.comparisons) {
    const sample = id => report.cases.find(item => item.id === id)?.samples.find(item => item.scale === comparison.scale && item.pass === comparison.pass)
    Object.assign(comparison, compareType3Rasters(sample(comparison.case), sample(comparison.control)))
  }
  for (const comparison of report.repeats) {
    const samples = report.cases.find(item => item.id === comparison.case).samples
    Object.assign(comparison, compareType3Rasters(samples.find(item => item.scale === comparison.scale && item.pass === 1), samples.find(item => item.scale === comparison.scale && item.pass === 2)))
  }
  for (const result of report.cases) if (result.recovery?.rgbaSha256) {
    const control = report.cases.find(item => item.id === result.recovery.control)?.samples.find(item => item.scale === 1 && item.pass === 1)
    result.recovery.comparison = compareType3Rasters(result.recovery, control)
    if (result.recovery.comparison.status !== 'passed') { result.recovery.status = 'failed'; result.status = 'failed' }
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
  report.status = report.errors.length === 0 && report.cases.length > 0 && report.cases.every(item => item.status === 'passed') && report.comparisons.every(item => item.status === 'passed') && report.repeats.every(item => item.status === 'passed') ? 'passed' : 'failed'
  report.finished = new Date().toISOString()
  await save()
  process.exitCode = report.status === 'passed' ? 0 : 1
}
