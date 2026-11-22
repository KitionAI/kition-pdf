// Original SHA-pinned Type3 search fixture; real editor, no private corpus assets.
import assert from 'node:assert/strict'
import { readFile, writeFile, mkdir } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createServer } from 'vite'
import { engineOverride } from './engine-override.mjs'
import { chromium, webkit } from 'playwright'
import { installSearchProbe, searchSnapshot, searchInkSamples } from './search-probe.mjs'

const options = { browser: 'chromium', timeoutMs: 60000 }
for (let i = 2; i < process.argv.length; i++) {
  const arg = process.argv[i]
  if (arg === '--help') { options.help = true; continue }
  if (!['--manifest', '--out', '--browser', '--screenshot-dir', '--engine-dir'].includes(arg)) throw new Error('Unknown option')
  const value = process.argv[++i]
  if (!value || value.startsWith('--')) throw new Error('Missing option value')
  options[arg.slice(2)] = value
}
if (options.help) {
  console.log('node tests/search.mjs --manifest <pinned-search-manifest.json> --out <report.json> --browser chromium|webkit [--screenshot-dir <ignored-original-fixture-output>] [--engine-dir <isolated-engine-package>]')
  process.exit(0)
}
if (!options.manifest || !options.out || !['chromium', 'webkit'].includes(options.browser)) throw new Error('Manifest, report, and supported browser are required')
const lifecycleNames = [
  'repeated Find focuses the existing query; all 8 pages retain both rectangles at 50/100/200 percent',
  'previous/next and Enter/Shift+Enter wrap across the entire document',
  'repeated query reuses current-revision cache',
  'clearing input clears count and highlights immediately',
  'rapid typing issues one debounced request; no-results clears highlights',
  'IME suppresses intermediate queries and Enter navigation until composition ends',
  'late worker reply cannot replace a newer completed search',
  'document edit invalidates cached query and reissues search for the new revision',
  'close ignores an in-flight reply, removes highlights, and restores editor focus',
  'keyboard reopens search; Escape restores focus; replacement remounts without old highlights/cache',
]
const root = fileURLToPath(new URL('..', import.meta.url))
const hash = bytes => createHash('sha256').update(bytes).digest('hex')
const safe = error => String(error?.message ?? error).replace(/\/Users\/[^\s:)'"<>]+|\/home\/[^\s:)'"<>]+/g, '<local-path>')
const report = {
  schema: 1, status: 'running', browser: options.browser,
  scope: 'Real editor search lifecycle and independently authored multiline match geometry, eight crop/rotation pages',
  engine: { isolatedOverride: Boolean(options['engine-dir']), provenance: process.env.KITION_ENGINE_REVISION ?? 'unknown; binary hashes do not establish source revision', profile: process.env.KITION_ENGINE_PROFILE ?? 'unspecified' },
  configuration: { zooms: [0.5, 1, 2], deviceScaleFactors: [1, 2], cssTolerance: 0.75 },
  limitations: ['Exact scale changes call the real editor viewport from the harness; actual-size capture and search navigation use UI controls.', 'Synthetic Type3 geometry only; no general font, private corpus, physical-screen presentation, or performance claim.', 'Delayed replies are actual worker results held by a test-only page wrapper after completion; the probe does not alter engine output.', 'DPR scales raster backing pixels; expected search geometry is measured in CSS pixels.', 'Canvas checks sample only visible authored rectangle centers for opaque dark ink, not full-page or whole-glyph fidelity.', 'Screenshots, if requested, contain only the original fixture and attributed Inter UI text.'],
  cases: [], errors: [],
}
let server, browser
const save = async () => { await mkdir(dirname(resolve(options.out)), { recursive: true }); await writeFile(options.out, JSON.stringify(report, null, 2) + '\n') }
const near = (actual, expected, label, tolerance = 0.75) => assert.ok(Number.isFinite(actual) && Math.abs(actual - expected) <= tolerance, `${label}: ${actual} versus ${expected}`)

try {
  const manifestBytes = await readFile(options.manifest), manifest = JSON.parse(manifestBytes)
  report.manifestSha256 = hash(manifestBytes)
  assert.equal(manifest.schema, 1)
  assert.equal(manifest.coordinateSpace, 'page-local-top-left-points')
  assert.equal(manifest.pages.length, 8)
  assert.match(manifest.sha256 ?? '', /^[a-f0-9]{64}$/, 'Coordinator must pin fixture SHA256')
  const bytes = await readFile(resolve(dirname(resolve(options.manifest)), manifest.file))
  assert.equal(hash(bytes), manifest.sha256)
  report.fixtureSha256 = manifest.sha256
  report.harness = {}
  for (const name of ['search.mjs', 'search-probe.mjs', 'engine-override.mjs']) report.harness[name] = hash(await readFile(resolve(root, 'tests', name)))
  const engine = await engineOverride(options['engine-dir'], resolve(root, 'public/engine'))
  Object.assign(report.engine, engine.hashes, { served: engine.served })
  server = await createServer({ root, configFile: resolve(root, 'vite.config.ts'), plugins: [engine.plugin], server: { host: '127.0.0.1', port: 0 }, logLevel: 'error' })
  await server.listen()
  const origin = `http://127.0.0.1:${server.httpServer.address().port}`
  browser = await ({ chromium, webkit })[options.browser].launch()
  report.browserVersion = browser.version()
  for (const dpr of report.configuration.deviceScaleFactors) {
    const geometry = manifest.pages[0].queries.flatMap(({ query }) => report.configuration.zooms.flatMap(zoom => manifest.pages.map(page => ({ page: page.page, rotation: page.rotation, cropBox: page.cropBox, query, zoom, status: 'untested' }))))
    const result = { dpr, status: 'running', geometry, checks: lifecycleNames.map(name => ({ name, status: 'untested' })), pageErrors: [], errors: [] }
    report.cases.push(result)
    const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: dpr })
    try {
      const page = await context.newPage()
      page.setDefaultTimeout(options.timeoutMs)
      page.on('pageerror', error => result.pageErrors.push(safe(error)))
      await page.route('**/sample/Sample.pdf', route => route.fulfill({ status: 404, body: '' }))
      await page.goto(`${origin}/dev.html?locale=en-US&modes=classic`)
      await page.waitForFunction(() => document.getElementById('status')?.textContent === 'choose a file')
      const upload = name => page.locator('#file').setInputFiles({ name, mimeType: 'application/pdf', buffer: bytes })
      await upload('search-geometry.pdf')
      const kit = page.frameLocator('#kit')
      await kit.getByTestId('kit-canvas-surface').waitFor()
      const frame = page.frames().find(candidate => candidate !== page.mainFrame())
      await frame.evaluate(installSearchProbe)
      await kit.getByTestId('pdf-actual-size').click()
      await frame.waitForFunction(() => window.__pdfSearch?.viewport?.view.zoom === 1)
      await kit.getByTestId('pdf-actual-size').press('ControlOrMeta+f')
      const input = kit.getByTestId('pdf-find-text')
      const count = text => frame.waitForFunction(expected => document.querySelector('[data-testid="pdf-find-count"]')?.textContent === expected, text)
      const settle = () => frame.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))))
      const snapshot = () => frame.evaluate(searchSnapshot)
      const calls = () => frame.evaluate(() => window.__pdfSearch.calls.length)
      const none = () => frame.waitForFunction(() => document.querySelectorAll('[data-testid="pdf-search-highlight"]').length === 0)
      const begin = index => { result.checks[index].status = 'running' }
      const check = name => { const item = result.checks.find(item => item.name === name); assert.ok(item); item.status = 'passed' }
      begin(0)
      const queries = manifest.pages[0].queries.map(item => item.query)
      await input.fill(queries[0]); await count('1/8')
      await kit.getByTestId('pdf-actual-size').focus()
      await page.keyboard.press('ControlOrMeta+f')
      assert.equal(await frame.evaluate(() => document.activeElement?.getAttribute('data-testid')), 'pdf-find-text')
      assert.equal(await input.inputValue(), queries[0])
      await input.evaluate(element => {
        const observe = event => {
          if (!(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== 'f') return
          element.removeEventListener('keydown', observe, true)
          window.__pdfSearch.inputFindEvent = event
        }
        element.addEventListener('keydown', observe, true)
      })
      await page.keyboard.press('ControlOrMeta+f')
      assert.equal(await frame.evaluate(() => window.__pdfSearch.inputFindEvent?.defaultPrevented), true, 'Find input must prevent the browser Find shortcut')
      assert.equal(await frame.evaluate(() => document.activeElement?.getAttribute('data-testid')), 'pdf-find-text')
      assert.equal(await input.inputValue(), queries[0])
      await count('1/8')
      for (const query of queries) {
        await input.fill(query)
        await count('1/8')
        for (const zoom of report.configuration.zooms) {
          await frame.evaluate(scale => window.__pdfSearch.viewport.zoomTo(scale), zoom)
          await settle()
          for (let i = 0; i < manifest.pages.length; i++) {
            const expectedPage = manifest.pages[i]
            const measured = result.geometry.find(item => item.page === expectedPage.page && item.zoom === zoom && item.query === query)
            measured.status = 'running'
            await count(`${i + 1}/8`)
            await settle()
            const authored = expectedPage.queries.find(item => item.query === query).rects
            const inkHandle = await frame.waitForFunction(searchInkSamples, { page: expectedPage.page, rects: authored, zoom, dpr })
            const ink = await inkHandle.jsonValue()
            await inkHandle.dispose()
            const actual = await snapshot(), pageArea = actual.pages[i]
            assert.equal(actual.dpr, dpr)
            near(actual.view.zoom, zoom, 'actual zoom', 1e-8)
            near(pageArea.width, expectedPage.width, 'page width', 1e-4)
            near(pageArea.height, expectedPage.height, 'page height', 1e-4)
            assert.equal(actual.page, expectedPage.page)
            assert.equal(actual.overlay.pointerEvents, 'none')
            assert.equal(actual.overlay.ariaHidden, 'true')
            assert.equal(actual.overlay.overflow, 'hidden')
            near(actual.overlay.left, 0, 'overlay left'); near(actual.overlay.top, 0, 'overlay top')
            near(actual.overlay.width, actual.canvas.width, 'overlay width'); near(actual.overlay.height, actual.canvas.height, 'overlay height')
            const expected = expectedPage.queries.find(item => item.query === query).rects
            assert.equal(actual.rects.length, expected.length, 'All multiline rectangles must remain present')
            const rectangles = expected.map(([x0, y0, x1, y1], index) => {
              const target = { left: (pageArea.x + x0 - actual.view.x) * zoom, top: (pageArea.y + y0 - actual.view.y) * zoom, width: (x1 - x0) * zoom, height: (y1 - y0) * zoom }
              for (const key of Object.keys(target)) near(actual.rects[index][key], target[key], `page ${i + 1} rectangle ${index} ${key}`)
              return { expected: target, actual: actual.rects[index] }
            })
            assert.ok(rectangles.some(({ actual: rect }) => rect.left < actual.canvas.width && rect.left + rect.width > 0 && rect.top < actual.canvas.height && rect.top + rect.height > 0), 'Selected match must intersect the viewport')
            Object.assign(measured, { status: 'passed', view: actual.view, rectangles, visibleCanvasInk: ink })
            await kit.getByTestId('pdf-find-next').click()
          }
          await count('1/8')
        }
      }
      check('repeated Find focuses the existing query; all 8 pages retain both rectangles at 50/100/200 percent')
      begin(1)
      await kit.getByTestId('pdf-find-previous').click(); await count('8/8')
      await input.press('Enter'); await count('1/8')
      await input.press('Shift+Enter'); await count('8/8')
      check('previous/next and Enter/Shift+Enter wrap across the entire document')
      begin(2)
      const beforeCache = await calls()
      await input.fill(queries[0]); await count('1/8')
      assert.equal(await calls(), beforeCache)
      check('repeated query reuses current-revision cache')
      begin(3)
      await input.fill(''); await none()
      assert.equal(await kit.getByTestId('pdf-find-count').textContent(), '')
      check('clearing input clears count and highlights immediately')
      begin(4)
      const beforeDebounce = await calls()
      await input.fill('Z'); await page.waitForTimeout(40)
      await input.fill('ZZ'); await page.waitForTimeout(40)
      await input.fill('ZZZ')
      await count('No matches'); await none()
      assert.equal(await calls(), beforeDebounce + 1)
      check('rapid typing issues one debounced request; no-results clears highlights')
      begin(5)
      const beforeIme = await calls()
      await input.dispatchEvent('compositionstart', { data: '' })
      await input.fill('C'); await input.fill('CD')
      await input.press('Enter'); await page.waitForTimeout(300)
      assert.equal(await calls(), beforeIme)
      await none()
      await input.dispatchEvent('compositionend', { data: 'CD' })
      await count('1/8')
      assert.equal(await calls(), beforeIme + 1)
      assert.equal((await snapshot()).calls.at(-1).query, 'CD')
      check('IME suppresses intermediate queries and Enter navigation until composition ends')
      begin(6)
      await frame.evaluate(() => { window.__pdfSearch.holdQuery = 'AB' })
      await input.fill('AB')
      await frame.waitForFunction(() => window.__pdfSearch.held.length === 1)
      await input.fill('D'); await count('1/8')
      const latest = await snapshot()
      await frame.evaluate(() => window.__pdfSearch.release())
      await settle()
      assert.deepEqual((await snapshot()).rects, latest.rects)
      assert.equal((await snapshot()).count, latest.count)
      check('late worker reply cannot replace a newer completed search')
      begin(7)
      await input.fill(queries[0]); await count('1/8')
      const beforeEdit = await calls()
      await page.evaluate(() => window.devHost.connection().ai.run([{ command: 'bookmark_add', params: { page: 1, title: 'Original search revision check' } }], false))
      await frame.waitForFunction(previous => window.__pdfSearch.calls.length > previous, beforeEdit)
      await count('1/8')
      check('document edit invalidates cached query and reissues search for the new revision')
      if (dpr === 1 && options['screenshot-dir']) {
        const font = (await readFile(resolve(root, '../assets/fonts/Inter-Regular.ttf'))).toString('base64')
        await frame.addStyleTag({ content: `@font-face { font-family: SearchReviewInter; src: url(data:font/ttf;base64,${font}); } body, button, input, select { font-family: SearchReviewInter !important; }` })
        await frame.evaluate(() => document.fonts.load('400 12px SearchReviewInter'))
        await frame.evaluate(() => window.__pdfSearch.viewport.zoomTo(2))
        await settle()
        await mkdir(resolve(options['screenshot-dir']), { recursive: true })
        await page.locator('#kit').screenshot({ path: resolve(options['screenshot-dir'], `search-${options.browser}.png`) })
        result.screenshot = `search-${options.browser}.png`
      }
      begin(8)
      await frame.evaluate(() => { window.__pdfSearch.holdQuery = 'AB' })
      await input.fill('AB')
      await frame.waitForFunction(() => window.__pdfSearch.held.length === 1)
      await kit.getByTestId('pdf-find-close').click()
      await frame.evaluate(() => window.__pdfSearch.release())
      await settle(); await none()
      assert.equal(await kit.getByTestId('pdf-find-bar').count(), 0)
      result.restoredFocus = await frame.evaluate(() => {
        const active = document.activeElement
        return { insideEditor: active !== document.body && active !== document.documentElement && document.contains(active), testid: active?.getAttribute('data-testid') ?? null }
      })
      assert.equal(result.restoredFocus.insideEditor, true, 'Close must restore focus inside the editor')
      assert.equal(await page.evaluate(() => document.activeElement?.id), 'kit')
      check('close ignores an in-flight reply, removes highlights, and restores editor focus')
      begin(9)
      await page.keyboard.press('ControlOrMeta+f')
      await input.fill(queries[0]); await count('1/8')
      await input.press('Escape')
      await settle(); await none()
      assert.equal(await kit.getByTestId('pdf-find-bar').count(), 0)
      assert.equal(await frame.evaluate(() => document.activeElement !== document.body && document.activeElement !== document.documentElement), true)
      assert.equal(await page.evaluate(() => document.activeElement?.id), 'kit')
      await page.keyboard.press('ControlOrMeta+f')
      assert.equal(await input.inputValue(), '')
      await input.fill(queries[0]); await count('1/8')
      // Save the synthetic edit before replacing its document, preserving the actual host flow.
      await kit.getByTestId('pdf-save').click()
      await page.waitForFunction(() => window.devHost.files.get('search-geometry.pdf')?.version > 1)
      await upload('search-geometry-replacement.pdf')
      await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes('search-geometry-replacement.pdf open in'))
      await kit.getByTestId('kit-canvas-surface').waitFor()
      // The development host replaces the iframe; this checks remount cleanup, not an in-place swap.
      const replacement = page.frames().find(candidate => candidate !== page.mainFrame())
      await replacement.evaluate(installSearchProbe)
      await kit.getByTestId('pdf-actual-size').click()
      assert.equal(await kit.getByTestId('pdf-search-highlight').count(), 0)
      await kit.getByTestId('pdf-actual-size').press('ControlOrMeta+f')
      assert.equal(await input.inputValue(), '')
      await input.fill(queries[0])
      await replacement.waitForFunction(() => document.querySelector('[data-testid="pdf-find-count"]')?.textContent === '1/8')
      assert.equal(await replacement.evaluate(() => window.__pdfSearch.calls.length), 1)
      check('keyboard reopens search; Escape restores focus; replacement remounts without old highlights/cache')
      assert.deepEqual(result.pageErrors, [])
      result.status = 'passed'
    } catch (error) {
      result.status = 'failed'; result.errors.push(safe(error))
      for (const item of [...result.geometry, ...result.checks]) if (item.status === 'running') { item.status = 'failed'; item.error = safe(error) }
    }
    finally { await context.close(); await save() }
  }
  if (options['engine-dir']) for (const count of Object.values(report.engine.served)) assert.ok(count > 0, 'Isolated JS and WASM must both be served to the editor')
} catch (error) { report.errors.push(safe(error)) }
finally {
  try { await browser?.close() } catch (error) { report.errors.push(safe(error)) }
  try { await server?.close() } catch (error) { report.errors.push(safe(error)) }
  report.status = report.cases.length === 2 && report.cases.every(item => item.status === 'passed') && !report.errors.length ? 'passed' : 'failed'
  const geometry = report.cases.flatMap(item => item.geometry)
  report.completedGeometry = geometry.filter(item => item.status === 'passed').length
  report.plannedGeometry = 2 * 2 * 3 * 8
  report.geometry = { planned: report.plannedGeometry, passed: report.completedGeometry, failed: geometry.filter(item => item.status === 'failed').length }
  report.geometry.untested = report.geometry.planned - report.geometry.passed - report.geometry.failed
  const lifecycle = report.cases.flatMap(item => item.checks)
  report.lifecycle = { planned: 2 * lifecycleNames.length, passed: lifecycle.filter(item => item.status === 'passed').length, failed: lifecycle.filter(item => item.status === 'failed').length }
  report.lifecycle.untested = report.lifecycle.planned - report.lifecycle.passed - report.lifecycle.failed
  await save()
  console.log(JSON.stringify({ status: report.status, browser: options.browser, geometry: `${report.completedGeometry}/${report.plannedGeometry}`, checks: report.cases.map(item => ({ dpr: item.dpr, status: item.status, passed: item.checks.filter(check => check.status === 'passed').length, failed: item.checks.filter(check => check.status === 'failed').length, untested: item.checks.filter(check => check.status === 'untested').length, errors: item.errors })) }))
  process.exitCode = report.status === 'passed' ? 0 : 1
}
