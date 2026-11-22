// Local corpus smoke through the real host. No PDFs or screenshots are copied into the repository.
// Usage: node tests/corpus.mjs --manifest /absolute/cases.json --out /tmp/results.json [--browser chromium|webkit|firefox]
import { readFile, writeFile, mkdir } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { dirname, resolve, isAbsolute } from 'node:path'
import { fileURLToPath } from 'node:url'
import { closeResource, parseOptions } from './corpus-support.mjs'

let options
try {
  options = parseOptions(process.argv.slice(2))
} catch (error) {
  console.error(String(error))
  process.exit(2)
}
if (options.help) {
  console.log('node tests/corpus.mjs --manifest /absolute/cases.json --out /tmp/results.json [--browser chromium|webkit|firefox]\nManifest: { cases: [{ group: "golden_ppt|golden_excel|golden_word", path: "/absolute/file.pdf", pages: 15, sha256: "optional", query: "optional expected text" }] }')
  process.exit(0)
}
const { manifest: manifestPath, out: outputPath, browser: browserName } = options
const root = fileURLToPath(new URL('..', import.meta.url))
const report = { schema: 1, browser: browserName, started: new Date().toISOString(), scope: 'host open, page count, first/middle/last navigation, zoom, optional search; not visual fidelity', engine: {}, cases: [], errors: [] }
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex')
let server
let browser
try {
  const { cases } = JSON.parse(await readFile(manifestPath, 'utf8'))
  if (!Array.isArray(cases) || !cases.length) throw new Error('manifest.cases must be a non-empty array')
  for (const group of ['golden_ppt', 'golden_excel', 'golden_word']) {
    if (!cases.some((item) => item.group === group)) throw new Error(`missing representative group: ${group}`)
  }
  for (const item of cases) {
    if (!isAbsolute(item.path) || !Number.isInteger(item.pages) || item.pages < 1) throw new Error('each case needs an absolute path and positive integer pages')
    if (item.query !== undefined && (typeof item.query !== 'string' || !item.query.trim())) throw new Error('query must be a non-empty string when supplied')
  }
  report.harnessSha256 = hash(await readFile(fileURLToPath(import.meta.url)))
  report.manifestSha256 = hash(await readFile(manifestPath))
  for (const name of ['kition_pdf_kit.js', 'kition_pdf_kit_bg.wasm']) {
    report.engine[name] = hash(await readFile(resolve(root, 'public/engine', name)))
  }
  report.engine.provenance = process.env.KITION_ENGINE_REVISION ?? 'unknown: binary fingerprints do not prove source revision'
  const playwright = await import('playwright')
  const { createServer } = await import('vite')
  server = await createServer({ root, configFile: resolve(root, 'vite.config.ts'), server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
  await server.listen()
  const origin = `http://127.0.0.1:${server.httpServer.address().port}`
  browser = await playwright[browserName].launch()
  report.browserVersion = browser.version()
  for (const item of cases) {
    const result = { group: item.group, path: item.path, expectedPages: item.pages, status: 'failed', checks: [], limitations: ['No text selection overlay exists in this editor.', 'Pixel visibility is not a fidelity comparison; save/edit roundtrip is covered separately.'] }
    report.cases.push(result)
    const context = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 })
    const page = await context.newPage()
    page.setDefaultTimeout(30000)
    const errors = []
    page.on('pageerror', (error) => errors.push(error.message))
    try {
      result.sha256 = hash(await readFile(item.path))
      if (item.sha256 && result.sha256 !== item.sha256) throw new Error('PDF sha256 mismatch')
      // Suppress automatic sample open, then use the host file picker; engine file access stays in the host protocol.
      await page.route('**/sample/Sample.pdf', (route) => route.fulfill({ status: 404, body: '' }))
      await page.goto(`${origin}/dev.html?locale=en-US&modes=classic`)
      await page.waitForFunction(() => document.getElementById('status')?.textContent === 'choose a file')
      const started = performance.now()
      await page.locator('#file').setInputFiles(item.path)
      await page.waitForFunction(() => / open in |could not open /.test(document.getElementById('status')?.textContent ?? ''), null, { timeout: 120000 })
      const hostStatus = await page.locator('#status').textContent()
      if (hostStatus.includes('could not open')) throw new Error(hostStatus)
      const kit = page.frameLocator('#kit')
      await kit.getByTestId('pdf-status').waitFor()
      const inspected = await page.evaluate(async () => window.devHost.connection().ai.inspect())
      result.actualPages = inspected.details.pages.length
      if (result.actualPages !== item.pages) throw new Error(`page count ${result.actualPages}, expected ${item.pages}`)
      result.openMs = Math.round(performance.now() - started)
      result.checks.push('open and page count')
      // Avoid queueing all document thumbnails while measuring navigation.
      await kit.getByTestId('pdf-panel-bookmarks').click()
      result.navigation = []
      for (const number of [...new Set([1, Math.ceil(item.pages / 2), item.pages])]) {
        const began = performance.now()
        await kit.getByTestId('pdf-page-field').fill(String(number))
        await kit.getByTestId('pdf-page-field').press('Enter')
        await page.waitForFunction(([wanted, total]) => {
          const doc = document.querySelector('#kit')?.contentDocument
          return doc?.querySelector('[data-testid="pdf-status"]')?.textContent === `Page ${wanted} of ${total}`
        }, [number, item.pages])
        result.navigation.push({ page: number, statusMs: Math.round(performance.now() - began) })
      }
      result.checks.push('first/middle/last page navigation (status latency, not render latency)')
      for (const mode of ['pdf-fit-page', 'pdf-actual-size', 'pdf-fit-width']) await kit.getByTestId(mode).click()
      result.checks.push('zoom controls')
      // Wait for an actual painted canvas; a blank/transparent canvas cannot pass.
      await page.waitForFunction(() => {
        const doc = document.querySelector('#kit')?.contentDocument
        return [...(doc?.querySelectorAll('.pdf-pages canvas') ?? [])].some((canvas) => {
          if (!canvas.width || !canvas.height) return false
          const data = canvas.getContext('2d')?.getImageData(0, 0, canvas.width, canvas.height).data
          if (!data) return false
          for (let i = 3; i < data.length; i += 4) if (data[i]) return true
          return false
        })
      })
      result.checks.push('canvas contains painted pixels (blank-page/render-error detection requires pixel oracle)')
      if (item.query) {
        // The shared shell deliberately leaves text-field shortcuts to the field.
        await kit.getByTestId('pdf-fit-width').press('ControlOrMeta+f')
        await kit.getByTestId('pdf-find-text').fill(item.query)
        await page.waitForFunction(() => {
          const text = document.querySelector('#kit')?.contentDocument?.querySelector('[data-testid="pdf-find-count"]')?.textContent
          return /^1\/[1-9][0-9]*$/.test(text ?? '')
        })
        result.search = { count: await kit.getByTestId('pdf-find-count').textContent() }
        result.checks.push('search finds configured expected text')
        const total = Number(result.search.count.split('/')[1])
        await kit.getByTestId('pdf-find-text').press('Shift+Enter')
        if (await kit.getByTestId('pdf-find-count').textContent() !== `${total}/${total}`) throw new Error('previous search match did not wrap to the last match')
        await kit.getByTestId('pdf-find-text').press('Enter')
        if (await kit.getByTestId('pdf-find-count').textContent() !== `1/${total}`) throw new Error('next search match did not wrap to the first match')
        result.checks.push('previous/next search match wraparound')
        await kit.getByTestId('pdf-find-text').fill(`kition-missing-query-${result.sha256}`)
        await page.waitForFunction(() => document.querySelector('#kit')?.contentDocument?.querySelector('[data-testid="pdf-find-count"]')?.textContent === 'No matches')
        result.checks.push('absent query reports no matches')
        await kit.getByTestId('pdf-find-text').fill('')
        if (await kit.getByTestId('pdf-find-count').textContent() !== '') throw new Error('clearing search retained stale matches')
        await kit.getByTestId('pdf-find-text').press('Escape')
        await kit.getByTestId('pdf-find-bar').waitFor({ state: 'hidden' })
        result.checks.push('clear query and close search')
      } else result.limitations.push('Search not tested: no expected query supplied.')
      if (errors.length) throw new Error(`browser page errors: ${errors.join('; ')}`)
      result.status = 'passed'
    } catch (error) {
      result.error = String(error)
    } finally {
      result.pageErrors = errors
      await closeResource('context', context, report.errors)
    }
    console.log(`${result.status.toUpperCase()} ${item.group}: ${item.path}${result.error ? ` — ${result.error}` : ''}`)
  }
} catch (error) {
  report.errors.push(String(error))
} finally {
  await closeResource('browser', browser, report.errors)
  await closeResource('server', server, report.errors)
  report.finished = new Date().toISOString()
  report.status = report.errors.length || report.cases.some((item) => item.status !== 'passed') || !report.cases.length ? 'failed' : 'passed'
  await mkdir(dirname(resolve(outputPath)), { recursive: true })
  await writeFile(outputPath, `${JSON.stringify(report, null, 2)}\n`)
  console.log(`${report.status.toUpperCase()} report: ${resolve(outputPath)}`)
  if (report.errors.length) console.error(report.errors.join('\n'))
  process.exitCode = report.status === 'passed' ? 0 : 1
}
