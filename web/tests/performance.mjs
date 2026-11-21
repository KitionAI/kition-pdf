// Controlled measurement harness; corpus PDFs stay outside the repository.
import { readFile, writeFile, mkdir, readdir } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { dirname, resolve, isAbsolute } from 'node:path'
import { fileURLToPath } from 'node:url'
import { platform, arch, cpus, totalmem, loadavg } from 'node:os'
import { createServer } from 'vite'
import * as playwright from 'playwright'
import { installPerformanceProbe } from './performance-probe.mjs'
import { parsePerformanceOptions, planSamples, summarize } from './performance-support.mjs'
import { createProcessSampler } from './performance-process.mjs'

const options = parsePerformanceOptions(process.argv.slice(2))
if (options.help) {
  console.log('node tests/performance.mjs --manifest <cases.json> --out <report.json> [--browser chromium|webkit|firefox] [--cycles 100] [--window-pages 40] [--timeout-ms 120000]')
  process.exit(0)
}
const root = fileURLToPath(new URL('..', import.meta.url))
const hash = bytes => createHash('sha256').update(bytes).digest('hex')
const portableError = error => String(error?.message ?? error).replaceAll(root, '<web-root>').replace(/\/Users\/[^\s:)'"<>]+|\/home\/[^\s:)'"<>]+/g, '<local-path>')
const report = {
  schema: 1, started: new Date().toISOString(), browser: options.browser,
  environment: { platform: platform(), architecture: arch(), logicalCpuCount: cpus().length, totalRamBytes: totalmem(), loadAverageStart: loadavg() },
  configuration: { viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1, cycles: options.cycles, windowPages: options.windowPages, timeoutMs: options.timeoutMs, sidebar: 'default thumbnails for cold open; bookmarks during navigation', engineProfile: process.env.KITION_GOLDEN_ENGINE_PROFILE ?? 'unspecified; no release-performance claim' },
  metric: 'Action event to matching completed worker render, bitmap upload, live canvas draw, and two requestAnimationFrame opportunities; not physical screen presentation.',
  engineLatency: 'Worker dispatch to reply includes queue wait, WASM work, and message transfer. Thumbnail work may share the queue.',
  decision: 'Measurement completion does not approve latency targets, certify a memory plateau, or establish regression against an earlier build.',
  limitations: ['Each case starts a fresh browser, context and worker; OS file cache and Vite module cache are not reset.', 'Repeated-pair is a warm workload, not proof of individual renderer cache hits.', 'The 40-page window exercises pressure but does not directly observe cache eviction counters.', 'DOM canvas RGBA is width*height*4 only: excludes offscreen frames, GPU copies and browser overhead.', 'WASM addressable linear-memory capacity is not live allocation or RSS.', 'Instrumentation retains at most 256 render metadata entries in the page; pixel buffers are weakly referenced only.', 'Edit refresh includes the host AI command transport; navigation and zoom clocks begin at captured DOM input events.', 'Continuous wheel scrolling, navigation while searching, and rapid overlapping cancellation are not covered by this sequential baseline.'],
  cases: [], errors: [],
}
let server, browserServer, browser, sampler
const close = async (name, resource) => { try { await resource?.close() } catch (error) { report.errors.push(`${name}: ${portableError(error)}`) } }
const save = async () => {
  await mkdir(dirname(resolve(options.out)), { recursive: true })
  await writeFile(options.out, `${JSON.stringify(report, null, 2)}\n`)
}
async function bounded(promise, label, timeoutMs = options.timeoutMs) {
  let timeout
  try { return await Promise.race([promise, new Promise((_resolve, reject) => { timeout = setTimeout(() => reject(new Error(`${label} timed out`)), timeoutMs) })]) }
  finally { clearTimeout(timeout) }
}

async function wasmSnapshot(page, frame) {
  const engineURL = await frame.evaluate(() => window.__kitionPerf.engineModuleUrl)
  const workers = page.workers().filter(worker => /kitEngine\.worker/.test(worker.url()))
  if (workers.length !== 1 || !engineURL) return { status: 'unsupported', reason: 'Exactly one active editor engine worker and its original module URL are required.' }
  let timeout
  try {
    return await Promise.race([
      workers[0].evaluate(async url => {
        const module = await import(url)
        const exports = await module.default()
        if (!(exports.memory instanceof WebAssembly.Memory)) return { status: 'unsupported', reason: 'The existing module does not export memory.' }
        return { status: 'measured', linearMemoryCapacityBytes: exports.memory.buffer.byteLength, wasmPages: exports.memory.buffer.byteLength / 65536 }
      }, engineURL),
      new Promise((_resolve, reject) => { timeout = setTimeout(() => reject(new Error('WASM checkpoint timed out')), 10000) }),
    ])
  } catch (error) { return { status: 'unsupported', reason: portableError(error) } }
  finally { clearTimeout(timeout) }
}

try {
  const manifestBytes = await readFile(options.manifest)
  report.manifestSha256 = hash(manifestBytes)
  const manifest = JSON.parse(manifestBytes)
  if (!Array.isArray(manifest.cases) || !manifest.cases.length) throw new Error('manifest.cases must be a nonempty array')
  for (const item of manifest.cases) if (!isAbsolute(item.path) || !Number.isInteger(item.pages) || item.pages < 2) throw new Error('Each case needs an absolute PDF path and at least two pages')
  report.harness = {}
  for (const name of ['performance.mjs', 'performance-probe.mjs', 'performance-support.mjs', 'performance-process.mjs']) report.harness[name] = hash(await readFile(resolve(root, 'tests', name)))
  report.engine = { provenance: process.env.KITION_GOLDEN_ENGINE_REVISION ?? 'unknown: hashes alone do not establish source revision' }
  for (const name of ['kition_pdf_kit.js', 'kition_pdf_kit_bg.wasm']) report.engine[name] = hash(await readFile(resolve(root, 'public/engine', name)))
  report.fontAssets = {}
  for (const name of await readdir(resolve(root, 'public/fonts'))) if (/\.(?:ttf|otf|woff2?)$/i.test(name)) report.fontAssets[name] = hash(await readFile(resolve(root, 'public/fonts', name)))
  server = await createServer({ root, configFile: resolve(root, 'vite.config.ts'), server: { host: '127.0.0.1', port: 0 }, logLevel: 'error' })
  await server.listen()
  const origin = `http://127.0.0.1:${server.httpServer.address().port}`
  for (const [index, item] of manifest.cases.entries()) {
    const result = { id: `case-${index + 1}`, group: /^[a-zA-Z0-9_-]+$/.test(item.group ?? '') ? item.group : 'unspecified', expectedPages: item.pages, status: 'running', samples: planSamples(item.pages, options.cycles, options.windowPages), resources: [], pageErrors: [] }
    report.cases.push(result)
    let context, frame, activeSample
    try {
      browserServer = await playwright[options.browser].launchServer()
      browser = await playwright[options.browser].connect(browserServer.wsEndpoint())
      result.browserVersion = browser.version()
      sampler = createProcessSampler(browserServer.process().pid, { intervalMs: 2000 })
      await sampler.start()
      result.sha256 = hash(await readFile(item.path))
      if (item.sha256 && item.sha256 !== result.sha256) throw new Error('PDF SHA256 mismatch')
      context = await browser.newContext({ viewport: report.configuration.viewport, deviceScaleFactor: 1 })
      await context.addInitScript(installPerformanceProbe)
      const page = await context.newPage()
      page.setDefaultTimeout(options.timeoutMs)
      page.on('pageerror', error => result.pageErrors.push(portableError(error)))
      await page.route('**/sample/Sample.pdf', route => route.fulfill({ status: 404, body: '' }))
      await page.goto(`${origin}/dev.html?locale=en-US`)
      await page.waitForFunction(() => document.getElementById('status')?.textContent === 'choose a file')
      activeSample = result.samples[0]
      await page.locator('#file').setInputFiles(item.path)
      const kit = page.frameLocator('#kit')
      await kit.getByTestId('pdf-status').waitFor()
      frame = page.frames().find(candidate => candidate !== page.mainFrame())
      const finish = async sample => {
        await frame.waitForFunction(id => window.__kitionPerf?.goal.id === id && window.__kitionPerf.goal.status !== 'pending', sample.id, { timeout: options.timeoutMs })
        const observed = await frame.evaluate(() => window.__kitionPerf.goal)
        Object.assign(sample, observed)
        if (sample.status !== 'passed') throw new Error(sample.error ?? 'Render failed')
      }
      const checkpoint = async label => {
        const dom = await frame.evaluate(() => window.__kitionPerf.resources())
        const wasm = await wasmSnapshot(page, frame)
        result.resources.push({ label, dom, wasm })
        await sampler.checkpoint(`${result.id}:${label}`)
      }
      await finish(activeSample)
      result.actualPages = await frame.evaluate(() => window.__kitionPerf.pages.length)
      if (result.actualPages !== item.pages) throw new Error('Inspected page count does not match manifest')
      await frame.waitForFunction(() => document.fonts.status === 'loaded', null, { timeout: options.timeoutMs })
      result.fonts = await frame.evaluate(() => [...document.fonts].map(font => ({ family: font.family, style: font.style, weight: font.weight, status: font.status })))
      await checkpoint('cold-open')
      await kit.getByTestId('pdf-panel-bookmarks').click()
      for (let position = 1; position < result.samples.length; position++) {
        activeSample = result.samples[position]
        await frame.evaluate(sample => window.__kitionPerf.arm(sample), activeSample)
        if (activeSample.action === 'navigate') {
          await kit.getByTestId('pdf-page-field').fill(String(activeSample.page))
          await kit.getByTestId('pdf-page-field').press('Enter')
        } else if (activeSample.action === 'zoom') {
          await kit.getByTestId(activeSample.mode).click()
        } else if (activeSample.action === 'edit') {
          await frame.evaluate(() => window.__kitionPerf.start())
          const response = await bounded(page.evaluate(async number => window.devHost.connection().ai.run([{ command: 'comment_add', params: { page: number, type: 'note', contents: 'Performance refresh probe', at: [80, 80] } }], false), activeSample.page), 'Edit command')
          if (response.completed?.length !== 1) throw new Error('Edit command did not complete')
        }
        await finish(activeSample)
        const next = result.samples[position + 1]
        if (!next || next.phase !== activeSample.phase || (activeSample.phase === 'repeated-pair' && activeSample.cycle % 10 === 0 && next.cycle !== activeSample.cycle)) {
          await checkpoint(`${activeSample.phase}${activeSample.cycle ? `-${activeSample.cycle}` : ''}`)
          await save()
          console.log(JSON.stringify({ case: result.id, phase: activeSample.phase, cycle: activeSample.cycle ?? null, completed: position + 1 }))
        }
      }
      result.status = result.pageErrors.length ? 'failed' : 'passed'
      result.instrumentation = await frame.evaluate(() => ({ requestCount: window.__kitionPerf.requestCount, counters: window.__kitionPerf.counters }))
    } catch (error) {
      result.error = portableError(error)
      result.status = 'failed'
      if (activeSample?.status === 'untested') Object.assign(activeSample, { status: /timeout|timed out/i.test(result.error) ? 'timeout' : 'failed', error: result.error })
      if (frame) result.diagnostics = await frame.evaluate(() => ({ goal: window.__kitionPerf?.goal, requestCount: window.__kitionPerf?.requestCount, counters: window.__kitionPerf?.counters, recentRequests: window.__kitionPerf?.requests.slice(-5), banner: document.querySelector('[data-testid="kit-render-failure"]')?.textContent ?? null })).catch(() => ({ unavailable: true }))
    } finally {
      result.summary = summarize(result.samples)
      result.byPhase = Object.fromEntries([...new Set(result.samples.map(sample => sample.phase))].map(phase => [phase, summarize(result.samples.filter(sample => sample.phase === phase))]))
      await close('case context', context)
      if (sampler) {
        await sampler.checkpoint(`${result.id}:context-closed`)
        result.processMemory = await sampler.stop()
        sampler = null
      }
      await close('browser connection', browser)
      await close('browser server', browserServer)
      browser = null; browserServer = null
      await save()
    }
  }
} catch (error) { report.errors.push(portableError(error)) }
finally {
  try { report.processMemory = await sampler?.stop() } catch (error) { report.errors.push(portableError(error)) }
  await close('browser connection', browser)
  await close('browser server', browserServer)
  await close('Vite server', server)
  report.finished = new Date().toISOString()
  report.environment.loadAverageEnd = loadavg()
  report.status = report.errors.length === 0 && report.cases.length > 0 && report.cases.every(result => result.status === 'passed') ? 'passed' : 'failed'
  await save()
  process.exitCode = report.status === 'passed' ? 0 : 1
}
