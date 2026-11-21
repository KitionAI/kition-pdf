// Functional whole-page cache evidence on the actual editor worker, not a latency benchmark.
import assert from 'node:assert/strict'
import { readFile, writeFile, mkdir } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createServer } from 'vite'
import { chromium, webkit } from 'playwright'
import { installPerformanceProbe } from './performance-probe.mjs'
import { installCacheQuietProbe } from './cache-probe.mjs'
import { PAGE_COUNT, PAGE_SIDE, PAGE_BYTES, CACHE_BUDGET, cacheFixture, pageColor, cachePlan, validateSnapshot, counterDelta, assertLookup, summarizeCache, parseCacheOptions } from './cache-support.mjs'

const options = parseCacheOptions(process.argv.slice(2))
if (options.help) {
  console.log('node tests/cache.mjs --out <report.json> [--browser chromium|webkit] [--engine-dir <diagnostics-engine-directory>] [--timeout-ms 120000]')
  process.exit(0)
}
const root = fileURLToPath(new URL('..', import.meta.url))
const engineDirectory = resolve(options.engineDir ?? resolve(root, 'public/engine'))
const hash = bytes => createHash('sha256').update(bytes).digest('hex')
const safeError = error => String(error?.message ?? error).replace(/\/Users\/[^\s:)'"<>]+|\/home\/[^\s:)'"<>]+/g, '<local-path>')
const fixture = cacheFixture()
const report = {
  schema: 1, started: new Date().toISOString(), status: 'running', browser: options.browser,
  scope: 'Deterministic real-editor 1:1 whole-page cache hit, pressure eviction, reconstruction, and edit invalidation evidence',
  configuration: { viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1, pageCount: PAGE_COUNT, pageSide: PAGE_SIDE, pageRgbaBytes: PAGE_BYTES, expectedBudgetBytes: CACHE_BUDGET, renderQuietMs: 250, timeoutMs: options.timeoutMs, engineOverride: Boolean(options.engineDir), engineProfile: process.env.KITION_GOLDEN_ENGINE_PROFILE ?? 'unspecified; no release-performance claim' },
  fixture: { sha256: hash(fixture), origin: 'Contributor-original ASCII rectangle geometry generated in memory; no fonts, images, or external documents.' },
  engine: { provenance: process.env.KITION_GOLDEN_ENGINE_REVISION ?? 'unknown: binary hashes alone do not establish source revision' },
  limitations: ['Functional counter evidence only; timings include quiet waits and are not performance acceptance.', 'Snapshots aggregate one WASM instance/thread and must show exactly one live renderer cache.', 'Cold open includes thumbnails; they are unmounted and pending rendering drained before the measured 1:1 baseline.', 'Only the fixed 48-page synthetic document is covered; no corpus, screenshot, or total-memory-plateau claim.', 'Canvas completion reuses the exact-buffer presentation probe; two RAF opportunities do not prove physical display presentation.'],
  steps: cachePlan(), assertions: [], errors: [], pageErrors: [],
}
let server, browser, context, frame, activeStep
const save = async () => { await mkdir(dirname(resolve(options.out)), { recursive: true }); await writeFile(options.out, `${JSON.stringify(report, null, 2)}\n`) }
async function bounded(promise, label) {
  let timer
  try { return await Promise.race([promise, new Promise((_resolve, reject) => { timer = setTimeout(() => reject(new Error(`${label} timed out`)), options.timeoutMs) })]) }
  finally { clearTimeout(timer) }
}
const close = async (name, resource) => { try { await bounded(resource?.close(), `${name} cleanup`) } catch (error) { report.errors.push(safeError(error)) } }

try {
  report.harness = {}
  for (const name of ['cache.mjs', 'cache-support.mjs', 'cache-probe.mjs', 'performance-probe.mjs']) report.harness[name] = hash(await readFile(resolve(root, 'tests', name)))
  const engineAssets = new Map()
  for (const name of ['kition_pdf_kit.js', 'kition_pdf_kit_bg.wasm']) {
    const bytes = await readFile(resolve(engineDirectory, name))
    report.engine[name] = hash(bytes)
    engineAssets.set(`/engine/${name}`, bytes)
  }
  // Installed before Vite's built-ins; serve only these two immutable, fingerprinted assets.
  const enginePlugin = {
    name: 'pdf-cache-diagnostics-engine',
    configureServer(vite) {
      vite.middlewares.use((request, response, next) => {
        const path = new URL(request.url ?? '/', 'http://localhost').pathname
        const bytes = engineAssets.get(path)
        if (!bytes) return next()
        response.setHeader('Content-Type', path.endsWith('.wasm') ? 'application/wasm' : 'application/javascript')
        response.setHeader('Cache-Control', 'no-store')
        response.end(bytes)
      })
    },
  }
  server = await createServer({ root, configFile: resolve(root, 'vite.config.ts'), plugins: [enginePlugin], server: { host: '127.0.0.1', port: 0 }, logLevel: 'error' })
  await server.listen()
  const origin = `http://127.0.0.1:${server.httpServer.address().port}`
  browser = await ({ chromium, webkit })[options.browser].launch()
  report.browserVersion = browser.version()
  context = await browser.newContext({ viewport: report.configuration.viewport, deviceScaleFactor: 1 })
  // A single init script guarantees installation order for the two Worker observers.
  await context.addInitScript({ content: `(${installPerformanceProbe.toString()})();\n(${installCacheQuietProbe.toString()})();` })
  const page = await context.newPage()
  page.setDefaultTimeout(options.timeoutMs)
  page.on('pageerror', error => report.pageErrors.push(safeError(error)))
  await page.route('**/sample/Sample.pdf', route => route.fulfill({ status: 404, body: '' }))
  await page.goto(`${origin}/dev.html?locale=en-US&modes=classic`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent === 'choose a file')
  activeStep = report.steps[0]
  await page.locator('#file').setInputFiles({ name: 'CachePressure.pdf', mimeType: 'application/pdf', buffer: fixture })
  const kit = page.frameLocator('#kit')
  await kit.getByTestId('pdf-status').waitFor()
  frame = page.frames().find(candidate => candidate !== page.mainFrame())
  const finish = async step => {
    await frame.waitForFunction(id => window.__kitionPerf?.goal.id === id && window.__kitionPerf.goal.status !== 'pending', step.id, { timeout: options.timeoutMs })
    const observed = await frame.evaluate(() => window.__kitionPerf.goal)
    assert.equal(observed.status, 'passed', observed.error ?? 'No matching completed canvas render')
    step.render = observed
  }
  const quiet = async () => {
    await frame.waitForFunction(() => window.__kitionCacheQuiet.pending === 0 && performance.now() - window.__kitionCacheQuiet.lastActivity >= 250, null, { timeout: options.timeoutMs })
    return frame.evaluate(() => ({ ...window.__kitionCacheQuiet }))
  }
  const workers = page.workers().filter(worker => /kitEngine\.worker/.test(worker.url()))
  assert.equal(workers.length, 1, 'Use the editor existing worker, with no parallel document instance')
  const worker = workers[0]
  const engineURL = await frame.evaluate(() => window.__kitionPerf.engineModuleUrl)
  const snapshot = async () => validateSnapshot(await bounded(worker.evaluate(async url => {
    const module = await import(url)
    if (typeof module.pdfCacheDiagnostics !== 'function') throw new Error('Opt-in pdfCacheDiagnostics export is unavailable. Build the PDF WASM with cache-diagnostics enabled.')
    return JSON.parse(module.pdfCacheDiagnostics())
  }, engineURL), 'Cache diagnostics'))
  const centerPixel = async step => {
    const pixel = await frame.evaluate(() => {
      const canvas = document.querySelector('[data-testid="kit-canvas-surface"]')
      return Array.from(canvas.getContext('2d').getImageData(Math.floor(canvas.width / 2), Math.floor(canvas.height / 2), 1, 1).data)
    })
    const expected = pageColor(step.page)
    assert.ok(pixel.every((value, channel) => Math.abs(value - expected[channel]) <= 1), 'Visible page-center color does not match the requested original fixture page')
    step.pixel = { actual: pixel, expected, tolerance: 1 }
  }
  const confirmOneToOne = async step => {
    const request = step.render.request
    assert.equal(request.method, 'render'); assert.equal(request.scale, 1)
    assert.equal(await frame.evaluate(() => window.devicePixelRatio), 1)
    const intersections = await frame.evaluate(area => window.__kitionPerf.pages.flatMap((page, index) => area.x1 > page.area.x && area.x0 < page.area.x + page.area.width && area.y1 > page.area.y && area.y0 < page.area.y + page.area.height ? [index + 1] : []), request.area)
    assert.deepEqual(intersections, [step.page], 'Cache deltas require an isolated single-page request')
  }
  await finish(activeStep)
  assert.equal(await frame.evaluate(() => window.__kitionPerf.pages.length), PAGE_COUNT)
  await kit.getByTestId('pdf-panel-bookmarks').click()
  activeStep.quiet = await quiet()
  activeStep.after = await snapshot()
  activeStep.status = 'passed'
  report.assertions.push('Opt-in diagnostics are available on the same real editor WASM instance with one live cache')
  let pressureBefore
  for (const step of report.steps.slice(1)) {
    activeStep = step
    step.quietBefore = await quiet()
    step.before = await snapshot()
    await frame.evaluate(sample => window.__kitionPerf.arm(sample), step)
    if (step.action === 'zoom') await kit.getByTestId('pdf-actual-size').click()
    else if (step.action === 'edit') {
      await frame.evaluate(() => window.__kitionPerf.start())
      const response = await bounded(page.evaluate(() => window.devHost.connection().ai.run([{ command: 'comment_add', params: { page: 1, type: 'note', contents: 'Cache invalidation probe', at: [80, 80] } }], false)), 'Comment edit')
      assert.equal(response.completed?.length, 1, 'Comment command must complete')
    } else {
      await kit.getByTestId('pdf-page-field').fill(String(step.page))
      await kit.getByTestId('pdf-page-field').press('Enter')
    }
    await finish(step)
    step.quietAfter = await quiet()
    step.after = await snapshot()
    step.delta = counterDelta(step.before, step.after)
    await confirmOneToOne(step)
    await centerPixel(step)
    assert.equal(step.quietAfter.dispatched - step.quietBefore.dispatched, 1, 'Background or duplicate render requests contaminated the counter interval')
    assertLookup(step.expectation, step.delta)
    if (step.phase === 'pressure') {
      pressureBefore ??= step.before
      if (step.page === PAGE_COUNT) {
        const delta = counterDelta(pressureBefore, step.after)
        assert.ok(delta.evictions > 0, 'Pressure did not evict any retained page')
        assert.ok(delta.admissions >= 46 - pressureBefore.residentEntries, 'The distinct-page sweep did not populate the expected whole-page cache')
        assert.equal(step.after.residentEntries, CACHE_BUDGET / PAGE_BYTES)
        assert.equal(step.after.residentBytes, CACHE_BUDGET)
        report.pressure = { before: pressureBefore, after: step.after, delta }
        report.assertions.push('The 46-page pressure sweep reaches the real 64 MiB limit and incrementally evicts pages')
      }
    }
    if (step.action === 'edit') {
      assert.ok(step.render.revisionAfter > step.render.revisionBefore, 'Edit did not advance the document revision')
      assert.ok(step.after.lastRendererGeneration > step.before.lastRendererGeneration, 'Edit did not replace the displayed renderer generation')
      assert.ok(step.after.lastDisplayRevision > step.before.lastDisplayRevision, 'New renderer did not observe the edited display revision')
      assert.equal(step.delta.rendererCreations, 1)
      assert.equal(step.after.residentEntries, 1)
      assert.equal(step.after.residentBytes, PAGE_BYTES)
      report.assertions.push('Editing advances revision and renderer generation, invalidates the old cache, and admits one new whole-page image')
    }
    step.status = 'passed'
    await save()
    console.log(JSON.stringify({ phase: step.phase, page: step.page, status: step.status, hits: step.delta.hits, misses: step.delta.misses, evictions: step.delta.evictions, residentBytes: step.after.residentBytes }))
  }
  assert.deepEqual(report.pageErrors, [])
  report.assertions.push('Warm and reconstructed pages use exact hit deltas with unchanged synthetic center pixels; evicted pages require a new full render', 'After edit invalidation the second page misses, then the edited first page hits in its new revision')
} catch (error) {
  const message = safeError(error)
  report.errors.push(message)
  if (activeStep?.status === 'untested') Object.assign(activeStep, { status: /timeout|timed out/i.test(message) ? 'timeout' : 'failed', error: message })
  if (frame) report.diagnostics = await bounded(frame.evaluate(() => ({ goal: window.__kitionPerf?.goal, mapping: window.__kitionPerf?.counters, queue: window.__kitionCacheQuiet, banner: document.querySelector('[data-testid="kit-render-failure"]')?.textContent ?? null })), 'Failure diagnostics').catch(() => ({ unavailable: true }))
} finally {
  await close('context', context)
  await close('browser', browser)
  await close('server', server)
  report.summary = summarizeCache(report.steps)
  report.status = report.errors.length === 0 && report.summary.passed === report.summary.planned ? 'passed' : 'failed'
  report.finished = new Date().toISOString()
  await save()
  process.exitCode = report.status === 'passed' ? 0 : 1
}
