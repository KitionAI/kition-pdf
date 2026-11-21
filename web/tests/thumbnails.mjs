// Contributor-original geometric PDFs are generated in a temporary directory; no corpus assets.
import assert from 'node:assert/strict'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createHash } from 'node:crypto'
import { createServer } from 'vite'
import { chromium, webkit } from 'playwright'

const root = fileURLToPath(new URL('..', import.meta.url))
const out = process.argv[2]
if (!out) throw new Error('Usage: node tests/thumbnails.mjs <report.json>')
const temporary = await mkdtemp(resolve(tmpdir(), 'kition-pdf-thumbnails-'))
const report = { status: 'failed', scope: 'Virtual thumbnails, synthetic PDFs, injected failure/retry', engineProvenance: process.env.KITION_GOLDEN_ENGINE_REVISION ?? 'unknown: binary fingerprint does not prove source revision', cases: [] }

function syntheticPdf(count) {
  const objects = ['', '<< /Type /Catalog /Pages 2 0 R >>', `<< /Type /Pages /Count ${count} /Kids [${Array.from({ length: count }, (_, i) => `${3 + i * 2} 0 R`).join(' ')}] >>`]
  for (let i = 0; i < count; i++) {
    objects.push(`<< /Type /Page /Parent 2 0 R /MediaBox [0 0 400 600] /Resources << >> /Contents ${4 + i * 2} 0 R >>`)
    const marks = `q ${((i % 7) + 1) / 10} 0.3 0.6 rg 20 20 300 500 re f Q\n`
    objects.push(`<< /Length ${marks.length} >>\nstream\n${marks}endstream`)
  }
  let pdf = '%PDF-1.4\n'
  const offsets = [0]
  for (let i = 1; i < objects.length; i++) { offsets.push(pdf.length); pdf += `${i} 0 obj\n${objects[i]}\nendobj\n` }
  const start = pdf.length
  pdf += `xref\n0 ${objects.length}\n0000000000 65535 f \n${offsets.slice(1).map(n => `${String(n).padStart(10, '0')} 00000 n \n`).join('')}trailer\n<< /Size ${objects.length} /Root 1 0 R >>\nstartxref\n${start}\n%%EOF\n`
  return pdf
}

const long = resolve(temporary, 'long.pdf')
const short = resolve(temporary, 'short.pdf')
await writeFile(long, syntheticPdf(300))
await writeFile(short, syntheticPdf(2))
report.engineSha256 = createHash('sha256').update(await readFile(resolve(root, 'public/engine/kition_pdf_kit_bg.wasm'))).digest('hex')
const server = await createServer({ root, configFile: resolve(root, 'vite.config.ts'), server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
try {
  await server.listen()
  const origin = `http://127.0.0.1:${server.httpServer.address().port}`
  for (const [name, engine] of [['chromium', chromium], ['webkit', webkit]]) {
    const browser = await engine.launch()
    const result = { browser: name, status: 'failed', checks: [], errors: [] }
    report.cases.push(result)
    try {
      const page = await browser.newPage({ viewport: { width: 1440, height: 900 } })
      page.setDefaultTimeout(30000)
      page.on('pageerror', error => result.errors.push(error.message))
      await page.route('**/sample/Sample.pdf', route => route.fulfill({ status: 404, body: '' }))
      await page.goto(`${origin}/dev.html?locale=en-US`)
      await page.waitForFunction(() => document.getElementById('status')?.textContent === 'choose a file')
      await page.locator('#file').setInputFiles(long)
      const kit = page.frameLocator('#kit')
      await kit.getByTestId('pdf-thumb-0').locator('canvas').waitFor()
      const frame = page.frames().find(item => item !== page.mainFrame())
      await frame.evaluate(async () => {
        const { KitEngine } = await import('/tests/sdk-editor.mjs')
        const previous = KitEngine.prototype.render
        window.thumbnailProbe = { calls: [], fail: false, active: 0, maxActive: 0 }
        KitEngine.prototype.render = async function (...args) {
          if (args[3] > 132 || args[4] > 180) return previous.apply(this, args)
          const probe = window.thumbnailProbe
          probe.calls.push({ y: args[1], width: args[3], height: args[4] })
          probe.active++
          probe.maxActive = Math.max(probe.maxActive, probe.active)
          try {
            if (probe.fail) throw new Error('Synthetic thumbnail render failure')
            return await previous.apply(this, args)
          } finally { probe.active-- }
        }
      })
      await frame.waitForFunction(() => document.querySelector('[data-testid="pdf-thumb-0"] canvas')?.width > 1)
      const bounds = async () => frame.evaluate(() => {
        const list = document.querySelector('[data-testid="pdf-thumbs"]')
        return { canvases: list.querySelectorAll('canvas').length, canvasBytes: [...list.querySelectorAll('canvas')].reduce((sum, canvas) => sum + canvas.width * canvas.height * 4, 0), bytes: Number(list.dataset.cacheBytes), entries: Number(list.dataset.cacheCount), height: list.clientHeight, calls: window.thumbnailProbe?.calls.length ?? null, maxActive: window.thumbnailProbe?.maxActive ?? null }
      })
      await page.waitForTimeout(1200)
      result.initial = await bounds()
      assert.ok(result.initial.canvases <= 20 && result.initial.calls < 25)
      assert.ok(result.initial.bytes <= 2 * 1024 * 1024 && result.initial.entries <= 21)
      result.checks.push('300-page open retains bounded canvases, pixels, and requests')
      await kit.getByTestId('pdf-thumbs').evaluate(element => { element.scrollTop = 150 * 240 })
      await kit.getByTestId('pdf-thumb-150').locator('canvas').waitFor()
      await frame.waitForFunction(() => document.querySelector('[data-testid="pdf-thumb-150"] canvas')?.width > 1)
      assert.equal(await kit.getByTestId('pdf-thumb-1').count(), 0)
      result.scrolled = await bounds()
      assert.ok(result.scrolled.canvases <= 20 && result.scrolled.bytes <= 2 * 1024 * 1024)
      result.checks.push('scroll replaces virtual rows and evicts offscreen images')
      await kit.getByTestId('pdf-thumb-150').press('End')
      await frame.waitForFunction(() => document.querySelector('[data-testid="pdf-status"]')?.textContent === 'Page 300 of 300')
      await frame.waitForFunction(() => document.querySelector('[data-testid="pdf-thumb-299"] canvas')?.width > 1)
      result.checks.push('keyboard End navigates to and paints the last page')
      const before = await frame.evaluate(() => window.thumbnailProbe.calls.length)
      await page.evaluate(() => window.devHost.connection().ai.run([{ command: 'bookmark_add', params: { page: 300, title: 'Synthetic revision' } }], false))
      await frame.waitForFunction(count => window.thumbnailProbe.calls.length > count, before)
      result.checks.push('document revision invalidates thumbnails and renders again')
      await frame.evaluate(() => { window.thumbnailProbe.fail = true })
      await kit.getByTestId('pdf-thumbs').evaluate(element => { element.scrollTop = 80 * 240 })
      await kit.getByTestId('pdf-thumb-error-80').waitFor()
      result.errorRow = await kit.getByTestId('pdf-thumb-error-80').evaluate(element => {
        const row = element.parentElement.getBoundingClientRect()
        return { height: row.height, contentBottom: element.getBoundingClientRect().bottom - row.top }
      })
      assert.ok(result.errorRow.contentBottom <= result.errorRow.height)
      if (name === 'chromium') {
        const regular = (await readFile(resolve(root, '../assets/fonts/Inter-Regular.ttf'))).toString('base64')
        await frame.addStyleTag({ content: `@font-face { font-family: ThumbnailReviewInter; src: url(data:font/ttf;base64,${regular}); } .pdf-side, .pdf-side button { font-family: ThumbnailReviewInter; }` })
        await frame.evaluate(() => document.fonts.load('400 12px ThumbnailReviewInter'))
        await kit.getByTestId('pdf-thumb-error-80').locator('..').screenshot({ path: resolve(dirname(out), 'thumbnails-error-row.png') })
      }
      await frame.evaluate(() => { window.thumbnailProbe.fail = false })
      await kit.getByTestId('pdf-thumb-error-80').getByRole('button').click()
      await kit.getByTestId('pdf-thumb-error-80').waitFor({ state: 'hidden' })
      await frame.waitForFunction(() => document.querySelector('[data-testid="pdf-thumb-80"] canvas')?.width > 1)
      result.checks.push('failed thumbnail is visible and explicit Retry recovers')
      assert.equal((await bounds()).maxActive, 1)
      await page.locator('#file').setInputFiles(short)
      await frame.waitForFunction(() => document.querySelector('[data-testid="pdf-status"]')?.textContent === 'Page 1 of 2')
      await frame.waitForFunction(() => document.querySelector('[data-testid="pdf-thumb-0"] canvas')?.width > 1)
      result.replaced = await bounds()
      assert.ok(result.replaced.entries <= 2 && result.replaced.canvases <= 2)
      result.checks.push('new document drops prior rows/cache; thumbnail requests remain serialized')
      assert.deepEqual(result.errors, [])
      result.status = 'passed'
    } catch (error) { result.error = String(error) }
    finally { await browser.close() }
    console.log(JSON.stringify(result))
  }
} finally {
  await server.close()
  await rm(temporary, { recursive: true, force: true })
  report.status = report.cases.length === 2 && report.cases.every(item => item.status === 'passed') ? 'passed' : 'failed'
  await writeFile(out, JSON.stringify(report, null, 2) + '\n')
  process.exitCode = report.status === 'passed' ? 0 : 1
}
