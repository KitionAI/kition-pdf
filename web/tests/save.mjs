// Original fixture only. Real Worker, Kit UI protocol, and development-host memory storage.
import assert from 'node:assert/strict'
import { readFile, writeFile, mkdir } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createServer } from 'vite'
import { chromium, webkit } from 'playwright'
import { engineOverride } from './engine-override.mjs'
import { installedSdk, sdkFingerprints } from './hairline-shared.mjs'
import { installSaveHostProbe } from './save-probe.mjs'
import { saveCheckNames, saveScenarios } from './save-scenarios.mjs'

const options = { browser: 'chromium' }, seen = new Set()
for (let index = 2; index < process.argv.length; index++) {
  const arg = process.argv[index]
  if (arg === '--help') { options.help = true; continue }
  if (!['--manifest', '--out', '--browser', '--engine-dir', '--artifact-dir'].includes(arg) || seen.has(arg)) throw new Error('Unknown or duplicate option')
  seen.add(arg)
  const value = process.argv[++index]
  if (!value || value.startsWith('--')) throw new Error('Missing option value')
  options[arg.slice(2)] = value
}
if (options.help) {
  console.log('node tests/save.mjs --manifest <pinned-original-manifest.json> --engine-dir <isolated-engine-package> --browser chromium|webkit --out <report.json> --artifact-dir <new-ignored-output-directory>')
  process.exit(0)
}
if (!options.manifest || !options.out || !options['engine-dir'] || !options['artifact-dir'] || !['chromium', 'webkit'].includes(options.browser)) throw new Error('Manifest, engine directory, output, artifact directory, and supported browser are required')
const root = fileURLToPath(new URL('..', import.meta.url))
const hash = bytes => createHash('sha256').update(bytes).digest('hex')
const safe = error => String(error?.message ?? error).replace(/\/Users\/[^\s:)'"<>]+|\/home\/[^\s:)'"<>]+/g, '<local-path>')
const report = {
  schema: 1, status: 'running', browser: options.browser,
  scope: 'Real PDF Worker serialization, Kit UI document/save and host/write, development-host memory storage, reopen, and undo/redo',
  engine: { isolatedOverride: true, provenance: process.env.KITION_ENGINE_REVISION ?? 'unknown; hashes alone do not prove source revision', profile: process.env.KITION_ENGINE_PROFILE ?? 'unspecified' },
  limitations: ['The host stores files in memory; this does not test Kition PUT /v1/kits/files, atomic disk replacement, permissions, full disks, or crash recovery.', 'Failure and delayed acknowledgement are injected around the existing dev-host protocol handler in this browser page; no shared SDK or host source is modified.', 'Reopening uses the existing file picker and remounts the iframe; in-place replacement is not covered.', 'No replica capability is advertised by this dev host; replica persistence and collaboration merge are not tested.', 'Only the original eight-page Type3 fixture is used. No corpus fidelity, encrypted PDF, signature, or DocChecksum claim is made.', 'External PDF text/raster comparisons are separate; exported snapshots and expectations support them but do not count as passing those comparisons.'],
  checks: saveCheckNames.map(name => ({ name, status: 'untested' })), pageErrors: [], errors: [], snapshots: [],
}
const artifacts = { schema: 1, documents: [] }
let server, browser, context, page
const saveReport = async () => { await mkdir(dirname(resolve(options.out)), { recursive: true }); await writeFile(options.out, JSON.stringify(report, null, 2) + '\n') }
try {
  const manifestBytes = await readFile(options.manifest), manifest = JSON.parse(manifestBytes)
  assert.equal(manifest.schema, 1); assert.equal(manifest.pages.length, 8)
  assert.equal(manifest.coordinateSpace, 'page-local-top-left-points')
  assert.match(manifest.sha256 ?? '', /^[a-f0-9]{64}$/)
  // This is the already attributed, contributor-original geometry fixture, not an arbitrary PDF.
  assert.equal(manifest.sha256, '3a40455a0f4f9e0de1f66d3e3456b1d663c59ba3aa7ff51295b7f35b1666c2b1')
  const bytes = await readFile(resolve(dirname(resolve(options.manifest)), manifest.file))
  assert.equal(hash(bytes), manifest.sha256)
  Object.assign(report, { manifestSha256: hash(manifestBytes), fixtureSha256: hash(bytes), harness: {}, sharedSources: {} })
  for (const name of ['save.mjs', 'save-probe.mjs', 'save-scenarios.mjs', 'engine-override.mjs', 'hairline-shared.mjs', 'sdk-editor.mjs', 'sdk-protocol.mjs']) report.harness[name] = hash(await readFile(resolve(root, 'tests', name)))
  // The installed @kition/sdk the host and editor run on (its manifest and browser code).
  Object.assign(report.sharedSources, await sdkFingerprints(await installedSdk(root)))
  await mkdir(resolve(options['artifact-dir']), { recursive: true })
  await writeFile(resolve(options['artifact-dir'], 'original.pdf'), bytes, { flag: 'wx' })
  const document = { id: `original-save-${options.browser}`, original: 'original.pdf', sha256: hash(bytes), outputs: [] }
  artifacts.documents.push(document)
  const writeManifest = () => writeFile(resolve(options['artifact-dir'], 'manifest.json'), JSON.stringify(artifacts, null, 2) + '\n')
  await writeManifest()
  const snapshot = async ({ id, bytes: data, version, expectedBookmarks, prefix, annotation }) => {
    const file = `${id}.pdf`
    // Record actual host-stored bytes before checking, retaining evidence even on failure.
    await writeFile(resolve(options['artifact-dir'], file), data, { flag: 'wx' })
    const preserved = data.length >= prefix.data.length && data.subarray(0, prefix.data.length).equals(prefix.data)
    const output = { id, file, kind: annotation ? 'annotation' : 'unchanged', sha256: hash(data), size: data.length, prefixOf: prefix.file, prefixPreserved: preserved, hostVersion: version, expectedBookmarks, ...(annotation ? { annotation } : {}) }
    document.outputs.push(output); report.snapshots.push(output); await writeManifest(); await saveReport()
    assert.equal(preserved, true, 'Incremental save must preserve the acknowledged input prefix')
    assert.ok(data.length > prefix.data.length, 'An edited save must append an incremental section')
    return { file }
  }
  const engine = await engineOverride(options['engine-dir'], resolve(root, 'public/engine'))
  Object.assign(report.engine, engine.hashes, { served: engine.served })
  server = await createServer({ root, configFile: resolve(root, 'vite.config.ts'), plugins: [engine.plugin], server: { host: '127.0.0.1', port: 0 }, logLevel: 'error' })
  await server.listen()
  browser = await ({ chromium, webkit })[options.browser].launch(); report.browserVersion = browser.version()
  context = await browser.newContext({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 })
  page = await context.newPage(); page.setDefaultTimeout(60000)
  page.on('pageerror', error => report.pageErrors.push(safe(error)))
  await page.route('**/sample/Sample.pdf', route => route.fulfill({ status: 404, body: '' }))
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html?locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent === 'choose a file')
  await page.evaluate(installSaveHostProbe)
  await saveScenarios({ page, kit: page.frameLocator('#kit'), bytes, manifest, result: report, snapshot, safe })
  for (const count of Object.values(report.engine.served)) assert.ok(count > 0, 'Isolated JS and WASM must both be served')
  assert.deepEqual(report.pageErrors, [])
} catch (error) { report.errors.push(safe(error)) }
finally {
  if (page && !page.isClosed()) {
    try {
      const observed = await page.evaluate(() => { const probe = window.__pdfSaveHost; if (!probe) return {}; return { events: probe.events, writes: probe.writes } })
      Object.assign(report, observed)
    } catch (error) { report.errors.push(safe(error)) }
  }
  try { await context?.close(); await browser?.close() } catch (error) { report.errors.push(safe(error)) }
  try { await server?.close() } catch (error) { report.errors.push(safe(error)) }
  report.counts = { planned: saveCheckNames.length, passed: report.checks.filter(item => item.status === 'passed').length, failed: report.checks.filter(item => item.status === 'failed').length, untested: report.checks.filter(item => item.status === 'untested').length }
  report.status = report.counts.passed === report.counts.planned && !report.errors.length && !report.pageErrors.length ? 'passed' : 'failed'
  await saveReport()
  console.log(JSON.stringify({ status: report.status, counts: report.counts, errors: report.errors, pageErrors: report.pageErrors, snapshots: report.snapshots.map(item => ({ id: item.id, sha256: item.sha256, prefixPreserved: item.prefixPreserved })) }))
  process.exitCode = report.status === 'passed' ? 0 : 1
}
