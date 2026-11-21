// Same SHA-pinned original fixtures as the native geometry checks; no screenshots.
import assert from 'node:assert/strict'
import { readFile, writeFile, mkdir } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createServer } from 'vite'
import { chromium, webkit } from 'playwright'
import { installPerformanceProbe } from './performance-probe.mjs'
import { installCacheQuietProbe } from './cache-probe.mjs'
import { geometrySummary } from './raster-geometry-support.mjs'

const options = { browser: 'chromium', timeoutMs: 120000 }
const seen = new Set()
for (let index = 2; index < process.argv.length; index++) {
  const name = process.argv[index]
  if (name === '--help') { options.help = true; continue }
  if (!['--manifest', '--out', '--browser', '--timeout-ms'].includes(name) || seen.has(name)) throw new Error('Unknown or duplicate option')
  seen.add(name)
  const value = process.argv[++index]
  if (!value || value.startsWith('--')) throw new Error('Missing option value')
  if (name === '--timeout-ms') options.timeoutMs = Number(value)
  else options[name.slice(2)] = value
}
if (options.help) {
  console.log('node tests/raster-geometry.mjs --manifest <sha-pinned-manifest.json> --out <report.json> [--browser chromium|webkit] [--timeout-ms 120000]')
  process.exit(0)
}
if (!options.manifest || !options.out || !['chromium', 'webkit'].includes(options.browser) || !Number.isSafeInteger(options.timeoutMs) || options.timeoutMs <= 0) throw new Error('Manifest, output, supported browser, and positive timeout are required')
const root = fileURLToPath(new URL('..', import.meta.url))
const hash = bytes => createHash('sha256').update(bytes).digest('hex')
const safeError = error => String(error?.message ?? error).replace(/\/Users\/[^\s:)'"<>]+|\/home\/[^\s:)'"<>]+/g, '<local-path>')
const report = {
  schema: 1, started: new Date().toISOString(), status: 'running', browser: options.browser,
  scope: 'Original Type3 glyph and thin-stroke geometry on the real editor worker at 72/144/288 DPI',
  configuration: { viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1, scales: [1, 2, 4], timeoutMs: options.timeoutMs, engineProfile: process.env.KITION_GOLDEN_ENGINE_PROFILE ?? 'unspecified; no release-performance claim' },
  engine: { provenance: process.env.KITION_GOLDEN_ENGINE_REVISION ?? 'unknown: binary fingerprints do not prove source revision' },
  limitations: ['Synthetic geometry only; no private corpus, proprietary font, screenshot, or general font fidelity claim.', 'Region support and placement are tested independently of antialias coverage-mass observations.', 'Direct raster geometry and editor canvas sanity are separate checks; direct raster bytes are not assumed to have reached the visible canvas.', 'Two animation-frame opportunities establish canvas submission continuity, not physical screen presentation.', 'The existing editor engine instance is reused; no second document or page-thread WASM instance is created.'],
  cases: [], errors: [],
}
let server, browser, context
const save = async () => { await mkdir(dirname(resolve(options.out)), { recursive: true }); await writeFile(options.out, `${JSON.stringify(report, null, 2)}\n`) }
async function bounded(promise, label) {
  let timeout
  try { return await Promise.race([promise, new Promise((_resolve, reject) => { timeout = setTimeout(() => reject(new Error(`${label} timed out`)), options.timeoutMs) })]) }
  finally { clearTimeout(timeout) }
}
const close = async (name, resource) => { try { await bounded(resource?.close(), name) } catch (error) { report.errors.push(safeError(error)) } }

try {
  const manifestBytes = await readFile(options.manifest)
  const manifest = JSON.parse(manifestBytes)
  report.manifestSha256 = hash(manifestBytes)
  assert.equal(manifest.schema, 1)
  assert.deepEqual(manifest.dpis, [72, 144, 288])
  assert.equal(manifest.coordinateSpace, 'pdf-user-space-y-up')
  assert.equal(manifest.rotation, 0)
  assert.equal(manifest.glyphShape.rectFormat, 'x0,y0,x1,y1')
  assert.ok(manifest.fixtures?.length > 0)
  for (const fixture of manifest.fixtures) {
    assert.match(fixture.id, /^[a-z0-9-]+$/)
    assert.match(fixture.sha256 ?? '', /^[a-f0-9]{64}$/, 'Root must SHA-pin each native-exported fixture before browser execution')
  }
  report.harness = {}
  for (const name of ['raster-geometry.mjs', 'raster-geometry-support.mjs', 'performance-probe.mjs', 'cache-probe.mjs']) report.harness[name] = hash(await readFile(resolve(root, 'tests', name)))
  for (const name of ['kition_pdf_kit.js', 'kition_pdf_kit_bg.wasm']) report.engine[name] = hash(await readFile(resolve(root, 'public/engine', name)))
  server = await createServer({ root, configFile: resolve(root, 'vite.config.ts'), server: { host: '127.0.0.1', port: 0 }, logLevel: 'error' })
  await server.listen()
  const origin = `http://127.0.0.1:${server.httpServer.address().port}`
  for (const fixture of manifest.fixtures) {
    const result = { id: fixture.id, status: 'running', sha256: fixture.sha256, samples: [1, 2, 4].map(scale => ({ scale, dpi: scale * 72, status: 'untested' })), interface: { status: 'untested' }, errors: [], pageErrors: [] }
    report.cases.push(result)
    let frame
    try {
      const bytes = await readFile(resolve(dirname(resolve(options.manifest)), fixture.file))
      assert.equal(hash(bytes), fixture.sha256, 'Fixture SHA256 differs from the pinned native-exported bytes')
      browser = await ({ chromium, webkit })[options.browser].launch()
      result.browserVersion = browser.version()
      context = await browser.newContext({ viewport: report.configuration.viewport, deviceScaleFactor: 1 })
      await context.addInitScript({ content: `(${installPerformanceProbe.toString()})();\n(${installCacheQuietProbe.toString()})();` })
      const page = await context.newPage()
      page.setDefaultTimeout(options.timeoutMs)
      page.on('pageerror', error => result.pageErrors.push(safeError(error)))
      await page.route('**/sample/Sample.pdf', route => route.fulfill({ status: 404, body: '' }))
      await page.goto(`${origin}/dev.html?locale=en-US&modes=classic`)
      await page.waitForFunction(() => document.getElementById('status')?.textContent === 'choose a file')
      await page.locator('#file').setInputFiles({ name: `${fixture.id}.pdf`, mimeType: 'application/pdf', buffer: bytes })
      const kit = page.frameLocator('#kit')
      await kit.getByTestId('pdf-status').waitFor()
      frame = page.frames().find(candidate => candidate !== page.mainFrame())
      const finish = async id => {
        await frame.waitForFunction(wanted => window.__kitionPerf?.goal.id === wanted && window.__kitionPerf.goal.status !== 'pending', id, { timeout: options.timeoutMs })
        const goal = await frame.evaluate(() => window.__kitionPerf.goal)
        assert.equal(goal.status, 'passed', goal.error ?? 'No completed current-page canvas render')
        return goal
      }
      await finish('cold-open-0')
      await kit.getByTestId('pdf-panel-bookmarks').click()
      const quiet = () => frame.waitForFunction(() => window.__kitionCacheQuiet.pending === 0 && performance.now() - window.__kitionCacheQuiet.lastActivity >= 250, null, { timeout: options.timeoutMs })
      await quiet()
      // Capture the actual editor's KitEngine when its next visible render starts.
      await frame.evaluate(async () => {
        const url = performance.getEntriesByType('resource').map(entry => entry.name).find(name => /\/engine\/kitEngine\.ts(?:\?|$)/.test(name))
        if (!url) throw new Error('The live editor engine module is unavailable')
        const { KitEngine } = await import(url)
        const originals = Object.fromEntries(['render', 'renderArea'].map(method => [method, KitEngine.prototype[method]]))
        for (const method of Object.keys(originals)) KitEngine.prototype[method] = function (...args) {
          window.__rasterGeometryEngine = this
          for (const name of Object.keys(originals)) KitEngine.prototype[name] = originals[name]
          return originals[method].apply(this, args)
        }
        window.__kitionPerf.arm({ id: 'geometry-actual', page: 1, action: 'zoom' })
      })
      await kit.getByTestId('pdf-actual-size').click()
      const actual = await finish('geometry-actual')
      assert.equal(actual.request.method, 'render'); assert.equal(actual.request.scale, 1)
      await quiet()
      const engineInfo = await frame.evaluate(() => ({ pages: window.__rasterGeometryEngine.info.inspected.pages.map(page => page.area), workers: window.__kitionCacheQuiet.pending, dpr: window.devicePixelRatio }))
      assert.equal(engineInfo.pages.length, 1)
      assert.equal(engineInfo.pages[0].width, manifest.pageWidth); assert.equal(engineInfo.pages[0].height, manifest.pageHeight)
      assert.equal(engineInfo.dpr, 1)
      result.interface.initialActualSize = actual
      result.editorPageArea = engineInfo.pages[0]
      for (const sample of result.samples) {
        try {
          const measured = await bounded(frame.evaluate(async ({ fixture, manifest, scale }) => {
            const { analyzeGeometry } = await import('/tests/raster-geometry-support.mjs')
            const engine = window.__rasterGeometryEngine
            const area = engine.info.inspected.pages[0].area
            const width = manifest.pageWidth * scale, height = manifest.pageHeight * scale
            const started = performance.now()
            const pixels = await engine.render(area.x, area.y, scale, width, height)
            const renderMs = performance.now() - started
            const analysis = analyzeGeometry(pixels, width, height, manifest, fixture, scale)
            const digest = await crypto.subtle.digest('SHA-256', pixels)
            return { ...analysis, renderMs, rgbaSha256: [...new Uint8Array(digest)].map(byte => byte.toString(16).padStart(2, '0')).join('') }
          }, { fixture, manifest, scale: sample.scale }), 'Geometry raster request')
          Object.assign(sample, measured)
          console.log(JSON.stringify({ fixture: fixture.id, scale: sample.scale, status: sample.status, regions: sample.regions.length, failures: sample.regions.filter(region => region.status !== 'passed').map(region => ({ id: region.id, failures: region.failures })) }))
        } catch (error) {
          sample.error = safeError(error)
          sample.status = /timeout|timed out/i.test(sample.error) ? 'timeout' : 'failed'
          break
        }
      }
      await quiet()
      await frame.evaluate(() => window.__kitionPerf.arm({ id: 'geometry-canvas-refresh', page: 1, action: 'navigate' }))
      await kit.getByTestId('pdf-page-field').fill('1')
      await kit.getByTestId('pdf-page-field').press('Enter')
      result.interface.refreshed = await finish('geometry-canvas-refresh')
      result.interface.canvas = await frame.evaluate(() => {
        const canvas = document.querySelector('[data-testid="kit-canvas-surface"]')
        const pixels = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data
        let darkPixels = 0
        for (let offset = 0; offset < pixels.length; offset += 4) if (pixels[offset] < 80 && pixels[offset + 1] < 80 && pixels[offset + 2] < 80 && pixels[offset + 3] > 200) darkPixels++
        return { width: canvas.width, height: canvas.height, darkPixels }
      })
      assert.ok(result.interface.canvas.darkPixels > 20, 'Editor canvas contains no substantial dark fixture ink')
      result.interface.status = 'passed'
      assert.deepEqual(result.pageErrors, [])
      result.status = result.samples.every(sample => sample.status === 'passed') ? 'passed' : 'failed'
    } catch (error) {
      result.errors.push(safeError(error)); result.status = 'failed'
      if (result.interface.status === 'untested') result.interface.status = 'failed'
      if (frame) result.diagnostics = await bounded(frame.evaluate(() => ({ goal: window.__kitionPerf?.goal, mapping: window.__kitionPerf?.counters, queue: window.__kitionCacheQuiet })), 'Failure diagnostics').catch(() => ({ unavailable: true }))
    } finally {
      result.summary = geometrySummary(result.samples)
      await close('context', context); context = null
      await close('browser', browser); browser = null
      await save()
    }
  }
} catch (error) { report.errors.push(safeError(error)) }
finally {
  await close('context', context)
  await close('browser', browser)
  await close('server', server)
  report.summary = geometrySummary(report.cases.flatMap(result => result.samples))
  report.status = report.errors.length === 0 && report.cases.length > 0 && report.cases.every(result => result.status === 'passed') ? 'passed' : 'failed'
  report.finished = new Date().toISOString()
  await save()
  process.exitCode = report.status === 'passed' ? 0 : 1
}
