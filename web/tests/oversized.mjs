// Original geometric fixtures only. Exercise the real worker/WASM; never inject render errors.
// Usage: KITION_GOLDEN_ENGINE_REVISION=<source-digest> node tests/oversized.mjs <report.json>
import assert from 'node:assert/strict'
import { readFile, mkdir, writeFile } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createServer } from 'vite'
import { chromium, webkit } from 'playwright'
import { closeResource } from './golden-support.mjs'

const root = fileURLToPath(new URL('..', import.meta.url))
const [output] = process.argv.slice(2)
if (!output || process.argv.length !== 3) throw new Error('Usage: node tests/oversized.mjs <report.json>')
const hash = bytes => createHash('sha256').update(bytes).digest('hex')

/** No font, image, or outside asset: rectangle colors are defined by this test. */
function pdf(pages) {
  const objects = ['', '<< /Type /Catalog /Pages 2 0 R >>', `<< /Type /Pages /Count ${pages.length} /Kids [${pages.map((_p, i) => `${3 + i * 2} 0 R`).join(' ')}] >>`]
  for (const [i, page] of pages.entries()) {
    objects.push(`<< /Type /Page /Parent 2 0 R /MediaBox [0 0 ${page.width} ${page.height}] /Resources << >> /Contents ${4 + i * 2} 0 R >>`)
    objects.push(`<< /Length ${page.marks.length} >>\nstream\n${page.marks}endstream`)
  }
  let body = '%PDF-1.7\n'
  const offsets = [0]
  for (let i = 1; i < objects.length; i++) { offsets.push(body.length); body += `${i} 0 obj\n${objects[i]}\nendobj\n` }
  const xref = body.length
  body += `xref\n0 ${objects.length}\n0000000000 65535 f \n${offsets.slice(1).map(n => `${String(n).padStart(10, '0')} 00000 n \n`).join('')}`
  return Buffer.from(`${body}trailer\n<< /Size ${objects.length} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`)
}

const healthy = { width: 300, height: 300, marks: '0 1 0 rg 10 10 280 280 re f\n' }
const widePage = { width: 9000, height: 300, marks: '1 0 0 rg 8800 0 200 300 re f\n0 1 0 rg 0 120 9000 60 re f\n' }
const tallPage = { width: 300, height: 9000, marks: '0 0 1 rg 0 0 300 200 re f\n0 1 0 rg 120 0 60 9000 re f\n' }
const geometry = pdf([
  widePage,
  tallPage,
  { width: 9000, height: 9000, marks: '1 0 0 rg 8800 0 200 200 re f\n' },
  healthy,
])
// A fitted viewport of this page reliably exceeds the source-work cap even on a wide window.
const excessive = pdf([{ width: 20000, height: 20000, marks: '0 0 1 rg 10 10 80 80 re f\n' }, healthy])
const report = {
  scope: 'Real WASM oversized tile/reduction pixels, seams, work-budget failure, Retry, and unsaved document preservation',
  engine: { provenance: process.env.KITION_GOLDEN_ENGINE_REVISION ?? 'unknown: fingerprints do not prove source revision' },
  fixtures: { geometry: hash(geometry), excessive: hash(excessive) },
  cases: [], errors: [],
}
let server
try {
  report.harnessSha256 = hash(await readFile(fileURLToPath(import.meta.url)))
  for (const name of ['kition_pdf_kit.js', 'kition_pdf_sdk_bg.wasm']) report.engine[name] = hash(await readFile(resolve(root, 'public/engine', name)))
  server = await createServer({ root, configFile: resolve(root, 'vite.config.ts'), server: { host: '127.0.0.1', port: 0 }, logLevel: 'error' })
  await server.listen()
  const origin = `http://127.0.0.1:${server.httpServer.address().port}`
  for (const [name, browserType] of [['chromium', chromium], ['webkit', webkit]]) {
    const result = { browser: name, status: 'failed', checks: [], pageErrors: [] }
    report.cases.push(result)
    let browser
    try {
      browser = await browserType.launch()
      result.version = browser.version()
      result.fitPage = []
      for (const [shape, fixture, channel] of [['wide', widePage, 0], ['tall', tallPage, 2]]) {
        const fitted = { shape, status: 'failed', fixtureSha256: hash(pdf([fixture])) }
        result.fitPage.push(fitted)
        const host = await browser.newPage({ viewport: { width: 1280, height: 900 } })
        host.setDefaultTimeout(180000)
        host.on('pageerror', error => result.pageErrors.push(error.message))
        try {
          await host.route('**/sample/Sample.pdf', route => route.fulfill({ status: 404, body: '' }))
          await host.goto(`${origin}/dev.html?locale=en-US`)
          await host.waitForFunction(() => document.getElementById('status')?.textContent === 'choose a file')
          await host.locator('#file').setInputFiles({ name: `${shape}.pdf`, mimeType: 'application/pdf', buffer: pdf([fixture]) })
          const editor = host.frameLocator('#kit')
          await editor.getByTestId('pdf-status').waitFor()
          await editor.getByTestId('pdf-panel-bookmarks').click()
          await editor.getByTestId('pdf-actual-size').click()
          const live = host.frames().find(item => item !== host.mainFrame())
          // Observe actual engine calls without changing their arguments or results.
          await live.evaluate(async () => {
            const url = performance.getEntriesByType('resource').map(item => item.name).find(name => /\/engine\/kitEngine\.ts(?:\?|$)/.test(name))
            const { KitEngine } = await import(url)
            window.fitRenders = []
            for (const method of ['render', 'renderArea']) {
              const previous = KitEngine.prototype[method]
              KitEngine.prototype[method] = async function (...args) {
                const call = { method, args, status: 'pending' }
                window.fitRenders.push(call)
                try { const pixels = await previous.apply(this, args); call.status = 'passed'; return pixels }
                catch (error) { call.status = 'failed'; call.error = error.message; throw error }
              }
            }
          })
          await editor.getByTestId('pdf-fit-page').click()
          await live.waitForFunction(() => window.fitRenders.some(call => call.status !== 'pending' && (call.args.at(-2) > 200 || call.args.at(-1) > 200)))
          await live.waitForFunction(expectedChannel => {
            const canvas = document.querySelector('[data-testid="kit-canvas-surface"]')
            if (document.querySelector('[data-testid="kit-render-failure"]')) return true
            if (!canvas?.width || !canvas.height) return false
            const pixels = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data
            let edge = 0, stripe = 0
            for (let i = 0; i < pixels.length; i += 4) {
              if (pixels[i + expectedChannel] > 250 && pixels[i + 1] < 5 && pixels[i + (expectedChannel === 0 ? 2 : 0)] < 5) edge++
              if (pixels[i + 1] > 250 && pixels[i] < 5 && pixels[i + 2] < 5) stripe++
            }
            window.fitPixels = { width: canvas.width, height: canvas.height, edge, stripe }
            return edge > 20 && stripe > 20
          }, channel)
          fitted.renders = await live.evaluate(() => window.fitRenders)
          fitted.pixels = await live.evaluate(() => window.fitPixels)
          fitted.banner = await editor.getByTestId('kit-render-failure').allTextContents()
          assert.deepEqual(fitted.banner, [], `${shape} fit-page failed`)
          assert.ok(fitted.pixels?.edge > 20 && fitted.pixels?.stripe > 20, `${shape} fitted content is missing`)
          fitted.status = 'passed'
        } catch (error) {
          fitted.error = String(error)
          const live = host.frames().find(item => item !== host.mainFrame())
          if (live) {
            fitted.renders ??= await live.evaluate(() => window.fitRenders).catch(() => null)
            fitted.banner ??= await live.locator('[data-testid="kit-render-failure"]').allTextContents().catch(() => [])
          }
        }
        finally { await host.close() }
        console.log(JSON.stringify({ browser: name, fitPage: fitted }))
      }
      const page = await browser.newPage({ viewport: { width: 1280, height: 900 } })
      page.setDefaultTimeout(180000)
      page.on('pageerror', error => result.pageErrors.push(error.message))
      await page.route('**/sample/Sample.pdf', route => route.fulfill({ status: 404, body: '' }))
      await page.goto(`${origin}/dev.html?locale=en-US`)
      await page.waitForFunction(() => document.getElementById('status')?.textContent === 'choose a file')
      await page.locator('#file').setInputFiles({ name: 'ExcessiveSourceWork.pdf', mimeType: 'application/pdf', buffer: excessive })
      const kit = page.frameLocator('#kit')
      await kit.getByTestId('pdf-status').waitFor()
      await kit.getByTestId('pdf-panel-bookmarks').click()
      await kit.getByTestId('kit-render-failure').waitFor()
      result.banner = await kit.getByTestId('kit-render-failure').textContent()
      assert.match(result.banner, /PDF view exceeds rendering work budget/)
      assert.match(result.banner, /smaller area|zoom/i)
      result.checks.push('real excessive-source-work failure is visible with actionable guidance')
      const frame = page.frames().find(item => item !== page.mainFrame())
      await frame.evaluate(async () => {
        const url = performance.getEntriesByType('resource').map(item => item.name).find(name => /\/engine\/kitEngine\.ts(?:\?|$)/.test(name))
        if (!url) throw new Error('Live worker engine module is unavailable')
        window.oversizedEngineModule = url
        window.realRenderFailures = 0
        const { KitEngine } = await import(url)
        for (const method of ['render', 'renderArea']) {
          const previous = KitEngine.prototype[method]
          KitEngine.prototype[method] = async function (...args) {
            try { return await previous.apply(this, args) }
            catch (error) { window.realRenderFailures++; throw error }
          }
        }
      })
      await kit.getByTestId('kit-render-failure').getByRole('button', { name: 'Retry' }).click()
      await frame.waitForFunction(() => window.realRenderFailures > 0)
      await kit.getByTestId('kit-render-failure').waitFor()
      result.checks.push('Retry issues another real failing worker request and retains the error')

      result.worker = await frame.evaluate(async fixture => {
        let engine
        let timeout
        let finished = false
        const run = async () => {
          const { KitEngine } = await import(window.oversizedEngineModule)
          engine = await KitEngine.open({ kitId: 'kit.pdf', base: new URL('./engine/', document.baseURI).href, module: 'kition_pdf_kit', documentClass: 'PdfDocument' }, 'OversizedGeometry.pdf', new Uint8Array(fixture).buffer)
          if (finished) {
            engine.dispose()
            throw new Error('The worker opened after its test deadline')
          }
          const checks = []
          const areas = engine.info.inspected.pages.map(page => page.area)
          const [wide, tall, square, good] = areas
          const red = [255, 0, 0, 255], green = [0, 255, 0, 255], blue = [0, 0, 255, 255]
          const sample = (pixels, width, x, y, expected, label) => {
            const actual = Array.from(pixels.slice((y * width + x) * 4, (y * width + x) * 4 + 4))
            if (actual.length !== 4 || actual.some((value, channel) => Math.abs(value - expected[channel]) > 1)) throw new Error(`${label}: expected ${expected}, got ${actual}`)
          }
          const solid = (pixels, expected, label) => {
            for (let index = 0; index < pixels.length; index++) {
              if (Math.abs(pixels[index] - expected[index % 4]) > 1) throw new Error(`${label}: wrong pixel channel at byte ${index}: ${pixels[index]}`)
            }
          }
          const image = async (label, width, height, draw, inspect) => {
            const started = performance.now()
            const pixels = await draw()
            if (pixels.length !== width * height * 4) throw new Error(`${label}: unexpected pixel length ${pixels.length}`)
            inspect(pixels)
            checks.push({ label, status: 'passed', width, height, elapsedMs: Math.round(performance.now() - started) })
          }
          await engine.execute('comment_add', { page: 4, type: 'note', contents: 'Unsaved edit survives rendering', at: [80, 80] })
          const before = await engine.save()
          const revision = engine.info.revision
          await image('9000-square far corner at 1:1', 16, 16,
            () => engine.render(square.x + 8850, square.y + 8850, 1, 16, 16), pixels => sample(pixels, 16, 8, 8, red, 'far corner'))
          await image('9000-square far corner reduced', 16, 16,
            () => engine.renderArea(square.x + 8840, square.y + 8840, square.x + 8904, square.y + 8904, 16, 16), pixels => sample(pixels, 16, 8, 8, red, 'reduced far corner'))
          await image('wide source span >8192', 900, 30,
            () => engine.renderArea(wide.x, wide.y, wide.x + 9000, wide.y + 300, 900, 30), pixels => {
              sample(pixels, 900, 890, 4, red, 'far right')
              for (const x of [1, 101, 409, 410, 819, 820, 899]) sample(pixels, 900, x, 15, green, 'wide reduction seam')
            })
          await image('tall source span >8192', 30, 900,
            () => engine.renderArea(tall.x, tall.y, tall.x + 300, tall.y + 9000, 30, 900), pixels => {
              sample(pixels, 30, 4, 890, blue, 'far bottom')
              for (const y of [1, 101, 409, 410, 819, 820, 899]) sample(pixels, 30, 15, y, green, 'tall reduction seam')
            })
          await image('render at half scale retains far-right pixels', 4500, 150,
            () => engine.render(wide.x, wide.y, 0.5, 4500, 150), pixels => sample(pixels, 4500, 4450, 20, red, 'half-scale far right'))
          for (const scale of [1, 2]) await image(`5000-pixel horizontal output seams at scale ${scale}`, 5000, 40,
            () => engine.render(wide.x + 2000, wide.y + 130, scale, 5000, 40), pixels => {
              solid(pixels, green, 'complete horizontal strip')
              for (const x of [0, 1023, 1024, 2047, 2048, 4095, 4096, 4999]) sample(pixels, 5000, x, 10, green, 'horizontal tile seam')
            })
          await image('5000-pixel vertical output seams', 40, 5000,
            () => engine.render(tall.x + 130, tall.y + 2000, 1, 40, 5000), pixels => {
              solid(pixels, green, 'complete vertical strip')
              for (const y of [0, 1023, 1024, 2047, 2048, 4095, 4096, 4999]) sample(pixels, 40, 10, y, green, 'vertical tile seam')
            })
          let rejected = ''
          try { await engine.renderArea(square.x, square.y, square.x + 9000, square.y + 9000, 90, 90) }
          catch (error) { rejected = error.message }
          if (!rejected.includes('PDF view exceeds rendering work budget') || !/smaller area|zoom/i.test(rejected)) throw new Error(`Expected actionable work limit, got: ${rejected}`)
          checks.push({ label: '81-million-source-pixel reduction rejects before excessive work', status: 'passed', message: rejected })
          await image('healthy page still renders after rejection', 16, 16,
            () => engine.render(good.x + 20, good.y + 20, 1, 16, 16), pixels => sample(pixels, 16, 8, 8, green, 'healthy page'))
          const after = await engine.save()
          if (!engine.info.dirty || engine.info.revision !== revision || before.length !== after.length || before.some((value, index) => value !== after[index])) throw new Error('Rendering changed the dirty state, revision, or save bytes')
          checks.push({ label: 'successful and rejected renders preserve dirty revision and serialized bytes', status: 'passed' })
          return checks
        }
        try { return await Promise.race([run(), new Promise((_resolve, reject) => { timeout = setTimeout(() => reject(new Error('Worker suite exceeded 300 seconds')), 300000) })]) }
        finally { finished = true; clearTimeout(timeout); engine?.dispose() }
      }, [...geometry])
      await kit.getByTestId('pdf-page-field').fill('2')
      await kit.getByTestId('pdf-page-field').press('Enter')
      await kit.getByTestId('pdf-actual-size').click()
      await kit.getByTestId('kit-render-failure').waitFor({ state: 'hidden' })
      result.checks.push('navigating to a healthy page clears the real failure without reopening')
      assert.ok(result.fitPage.every(item => item.status === 'passed'), 'Real editor fit-page checks failed')
      assert.deepEqual(result.pageErrors, [])
      result.status = 'passed'
    } catch (error) { result.error = String(error) }
    finally { await closeResource(`${name} browser`, browser, report.errors) }
    console.log(JSON.stringify(result))
  }
} catch (error) { report.errors.push(String(error)) }
finally {
  await closeResource('server', server, report.errors)
  report.status = report.errors.length === 0 && report.cases.length === 2 && report.cases.every(result => result.status === 'passed') ? 'passed' : 'failed'
  await mkdir(dirname(resolve(output)), { recursive: true })
  await writeFile(output, JSON.stringify(report, null, 2) + '\n')
  process.exitCode = report.status === 'passed' ? 0 : 1
}
