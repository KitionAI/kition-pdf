// Original fixture only. The optional engine override is served in memory, never copied into public/.
import assert from 'node:assert/strict'
import { readFile, writeFile, mkdir } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createServer } from 'vite'
import { engineOverride } from './engine-override.mjs'
import { chromium, webkit } from 'playwright'
import { installSearchProbe, searchInkSamples } from './search-probe.mjs'
import { installSelectionProbe, selectionSnapshot } from './selection-probe.mjs'
import { selectionCases, assertSelected } from './selection-support.mjs'
import { lifecycleNames, selectionLifecycle } from './selection-lifecycle.mjs'

const options = { browser: 'chromium', timeoutMs: 60000 }
const seen = new Set()
for (let i = 2; i < process.argv.length; i++) {
  const arg = process.argv[i]
  if (arg === '--help') { options.help = true; continue }
  if (!['--manifest', '--out', '--browser', '--engine-dir', '--screenshot-dir'].includes(arg) || seen.has(arg)) throw new Error('Unknown or duplicate option')
  seen.add(arg)
  const value = process.argv[++i]
  if (!value || value.startsWith('--')) throw new Error('Missing option value')
  options[arg.slice(2)] = value
}
if (options.help) {
  console.log('node tests/selection.mjs --manifest <pinned-original-manifest.json> --out <report.json> --browser chromium|webkit [--engine-dir <isolated-engine-package>] [--screenshot-dir <ignored-original-fixture-output>]')
  process.exit(0)
}
if (!options.manifest || !options.out || !['chromium', 'webkit'].includes(options.browser)) throw new Error('Manifest, output, and supported browser are required')
const root = fileURLToPath(new URL('..', import.meta.url))
const hash = bytes => createHash('sha256').update(bytes).digest('hex')
const safe = error => String(error?.message ?? error).replace(/\/Users\/[^\s:)'"<>]+|\/home\/[^\s:)'"<>]+/g, '<local-path>')
const near = (a, b, label, tolerance = 0.75) => assert.ok(Number.isFinite(a) && Math.abs(a - b) <= tolerance, `${label} differs from authored geometry`)
const report = {
  schema: 1, status: 'running', browser: options.browser,
  scope: 'Original fixture pointer selection, native selected text, overlay geometry, and copy lifecycle',
  engine: { isolatedOverride: Boolean(options['engine-dir']), provenance: process.env.KITION_GOLDEN_ENGINE_REVISION ?? 'unknown; hashes alone do not prove source revision', profile: process.env.KITION_GOLDEN_ENGINE_PROFILE ?? 'unspecified', served: {} },
  configuration: { zooms: [0.5, 1, 2], deviceScaleFactors: [1, 2], cssTolerance: 0.75 },
  limitations: ['All pointer drags use real browser mouse events; exact view positioning calls the real editor viewport.', 'Original Type3 fixture only; optional screenshots show this original fixture with attributed Inter UI text, never private corpus content.', 'Canvas samples cover only visible authored rectangle centers, not whole-page fidelity.', 'Native clipboard event payload is checked in both browsers; actual system clipboard reads are separately classified when unavailable.', 'Document replacement through the development host remounts the iframe; in-place replacement is not claimed.', 'Delayed replies retain real worker output in a page-only probe; the probe does not alter engine output or shared SDK source.'],
  cases: [], errors: [],
}
let server, browser
const save = async () => { await mkdir(dirname(resolve(options.out)), { recursive: true }); await writeFile(options.out, JSON.stringify(report, null, 2) + '\n') }
try {
  const manifestBytes = await readFile(options.manifest), manifest = JSON.parse(manifestBytes)
  report.manifestSha256 = hash(manifestBytes)
  assert.equal(manifest.schema, 1); assert.equal(manifest.coordinateSpace, 'page-local-top-left-points'); assert.equal(manifest.pages.length, 8)
  assert.match(manifest.sha256 ?? '', /^[a-f0-9]{64}$/)
  const bytes = await readFile(resolve(dirname(resolve(options.manifest)), manifest.file))
  assert.equal(hash(bytes), manifest.sha256)
  report.fixtureSha256 = manifest.sha256
  report.harness = {}
  for (const name of ['selection.mjs', 'selection-probe.mjs', 'selection-support.mjs', 'selection-lifecycle.mjs', 'search-probe.mjs', 'engine-override.mjs']) report.harness[name] = hash(await readFile(resolve(root, 'tests', name)))
  const engine = await engineOverride(options['engine-dir'], resolve(root, 'public/engine'))
  Object.assign(report.engine, engine.hashes, { served: engine.served })
  server = await createServer({ root, configFile: resolve(root, 'vite.config.ts'), plugins: [engine.plugin], server: { host: '127.0.0.1', port: 0 }, logLevel: 'error' })
  await server.listen()
  const origin = `http://127.0.0.1:${server.httpServer.address().port}`
  browser = await ({ chromium, webkit })[options.browser].launch()
  report.browserVersion = browser.version()
  for (const dpr of report.configuration.deviceScaleFactors) {
    const geometry = manifest.pages.flatMap(fixture => report.configuration.zooms.flatMap(zoom => selectionCases(fixture).map(spec => ({ page: fixture.page, rotation: fixture.rotation, cropBox: fixture.cropBox, zoom, gesture: spec.id, status: 'untested' }))))
    const result = { dpr, status: 'running', geometry, checks: lifecycleNames.map(name => ({ name, status: 'untested' })), clipboard: { status: 'untested' }, pageErrors: [], errors: [] }
    report.cases.push(result)
    const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: dpr })
    if (options.browser === 'chromium') await context.grantPermissions(['clipboard-read', 'clipboard-write'])
    try {
      const page = await context.newPage(); page.setDefaultTimeout(options.timeoutMs)
      page.on('pageerror', error => result.pageErrors.push(safe(error)))
      await page.route('**/sample/Sample.pdf', route => route.fulfill({ status: 404, body: '' }))
      await page.goto(`${origin}/dev.html?locale=en-US&modes=classic`)
      await page.waitForFunction(() => document.getElementById('status')?.textContent === 'choose a file')
      const upload = name => page.locator('#file').setInputFiles({ name, mimeType: 'application/pdf', buffer: bytes })
      await upload('selection-geometry.pdf')
      const kit = page.frameLocator('#kit'), canvas = kit.getByTestId('kit-canvas-surface')
      await canvas.waitFor()
      const frame = page.frames().find(candidate => candidate !== page.mainFrame())
      await frame.evaluate(installSearchProbe); await frame.evaluate(installSelectionProbe)
      await kit.getByTestId('pdf-actual-size').click()
      await frame.waitForFunction(() => window.__pdfSearch?.viewport && window.__pdfSearch.engine?.info.inspected.pages.length === 8)
      const settle = () => frame.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))
      const snap = () => frame.evaluate(selectionSnapshot)
      const none = () => frame.waitForFunction(() => document.querySelectorAll('[data-testid="pdf-selection-highlight"]').length === 0)
      const place = async (first = 1, last = first, zoom = 1) => {
        await frame.evaluate(({ first, last, zoom }) => {
          const probe = window.__pdfSearch, pages = probe.engine.info.inspected.pages.slice(first - 1, last).map(item => item.area)
          const bounds = document.querySelector('[data-testid="kit-canvas-surface"]').getBoundingClientRect()
          const x0 = Math.min(...pages.map(a => a.x)), y0 = Math.min(...pages.map(a => a.y))
          const x1 = Math.max(...pages.map(a => a.x + a.width)), y1 = Math.max(...pages.map(a => a.y + a.height))
          probe.viewport.zoomTo(zoom)
          probe.viewport.panTo((x0 + x1 - bounds.width / zoom) / 2, (y0 + y1 - bounds.height / zoom) / 2)
        }, { first, last, zoom })
        await settle()
      }
      const point = async (pageNumber, local) => {
        const actual = await snap(), area = actual.pages[pageNumber - 1], bounds = await canvas.boundingBox()
        return { x: bounds.x + (area.x + local[0] - actual.view.x) * actual.view.zoom, y: bounds.y + (area.y + local[1] - actual.view.y) * actual.view.zoom }
      }
      const drag = async spec => {
        const start = await point(spec.page, spec.anchor), end = await point(spec.end_page ?? spec.page, spec.focus)
        if (spec.mode === 'column') await page.keyboard.down('Alt')
        try { await page.mouse.move(start.x, start.y); await page.mouse.down(); await page.mouse.move(end.x, end.y, { steps: 4 }); await page.mouse.up() }
        finally { if (spec.mode === 'column') await page.keyboard.up('Alt') }
      }
      const verify = async (spec, zoom) => {
        await frame.waitForFunction(({ spec, zoom }) => {
          const probe = window.__pdfSelection, call = probe.calls.at(-1), params = call?.params
          return probe.active === 0 && call?.status === 'returned' && params.page === spec.page && (params.end_page ?? params.page) === (spec.end_page ?? spec.page) && params.mode === spec.mode && ['anchor', 'focus'].every(key => params[key]?.every((value, axis) => Math.abs(value - spec[key][axis]) <= 1.1 / zoom))
        }, { spec, zoom })
        await settle()
        const actual = await snap()
        assertSelected(actual.latest.result, spec.expected)
        assert.equal(actual.rects.length, spec.expected.pages.reduce((sum, item) => sum + item.rects.length, 0))
        assert.equal(actual.overlay?.pointerEvents, 'none'); assert.equal(actual.overlay?.ariaHidden, 'true')
        const expected = spec.expected.pages.flatMap(item => item.rects.map(rect => ({ page: item.page, rect })))
        const rectangles = expected.map(({ page: number, rect: [x0, y0, x1, y1] }, index) => {
          const area = actual.pages[number - 1], shown = actual.rects[index]
          assert.equal(shown.page, number)
          const target = { left: (area.x + x0 - actual.view.x) * zoom, top: (area.y + y0 - actual.view.y) * zoom, width: (x1 - x0) * zoom, height: (y1 - y0) * zoom }
          for (const axis of Object.keys(target)) near(shown[axis], target[axis], `Overlay ${axis}`)
          return { page: number, expected: target, actual: shown }
        })
        const ink = []
        for (const selected of spec.expected.pages) {
          const handle = await frame.waitForFunction(searchInkSamples, { page: selected.page, rects: selected.rects, zoom, dpr })
          ink.push({ page: selected.page, ...(await handle.jsonValue()) }); await handle.dispose()
        }
        return { rectangles, visibleCanvasInk: ink, text: spec.expected.text, request: actual.latest.params, maxActive: actual.maxActive }
      }
      for (const fixture of manifest.pages) for (const zoom of report.configuration.zooms) {
        await place(fixture.page, fixture.page, zoom)
        for (const item of selectionCases(fixture)) {
          const measured = result.geometry.find(entry => entry.page === fixture.page && entry.zoom === zoom && entry.gesture === item.id)
          measured.status = 'running'
          const spec = { ...item, page: fixture.page }
          await drag(spec)
          Object.assign(measured, await verify(spec, zoom), { status: 'passed' })
          if (options['screenshot-dir'] && dpr === 1 && fixture.page === 1 && zoom === 2 && item.id === 'range-forward') {
            const font = (await readFile(resolve(root, '../assets/fonts/Inter-Regular.ttf'))).toString('base64')
            await frame.addStyleTag({ content: `@font-face { font-family: SelectionReviewInter; src: url(data:font/ttf;base64,${font}); } body, body * { font-family: SelectionReviewInter !important; }` })
            await frame.evaluate(() => document.fonts.load('400 12px SelectionReviewInter'))
            await settle(); await verify(spec, zoom)
            await mkdir(resolve(options['screenshot-dir']), { recursive: true })
            await page.locator('#kit').screenshot({ path: resolve(options['screenshot-dir'], `selection-${options.browser}.png`) })
            result.screenshot = `selection-${options.browser}.png`
          }
        }
      }
      await selectionLifecycle({ page, frame, kit, canvas, result, manifest, dpr, upload, place, point, drag, verify, settle, snap, none })
      assert.deepEqual(result.pageErrors, [])
      result.status = 'passed'
    } catch (error) {
      result.status = 'failed'; result.errors.push(safe(error))
      for (const item of [...result.geometry, ...result.checks]) if (item.status === 'running') { item.status = 'failed'; item.error = safe(error) }
    } finally { await context.close(); await save() }
  }
  if (options['engine-dir']) for (const count of Object.values(report.engine.served)) assert.ok(count > 0, 'Isolated JS and WASM must both be served to the editor')
} catch (error) { report.errors.push(safe(error)) }
finally {
  try { await browser?.close() } catch (error) { report.errors.push(safe(error)) }
  try { await server?.close() } catch (error) { report.errors.push(safe(error)) }
  for (const [name, planned] of [['geometry', 192], ['checks', 2 * lifecycleNames.length]]) {
    const items = report.cases.flatMap(item => item[name]), passed = items.filter(item => item.status === 'passed').length, failed = items.filter(item => item.status === 'failed').length
    report[name === 'checks' ? 'lifecycle' : 'geometry'] = { planned, passed, failed, untested: planned - passed - failed }
  }
  report.status = report.cases.length === 2 && report.cases.every(item => item.status === 'passed') && !report.errors.length ? 'passed' : 'failed'
  await save(); console.log(JSON.stringify({ status: report.status, geometry: report.geometry, lifecycle: report.lifecycle, errors: report.errors, cases: report.cases.map(item => ({ dpr: item.dpr, status: item.status, errors: item.errors, clipboard: item.clipboard })) }))
  process.exitCode = report.status === 'passed' ? 0 : 1
}
