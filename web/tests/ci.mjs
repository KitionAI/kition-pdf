// One browser harness on its own, as the kit's test scripts run it (CI included):
// exports the native contributor-original fixtures into the ignored target/kit-ci/<suite>,
// pins their SHA-256, then runs the harness in Chromium and then WebKit against the built
// engine in public/engine. Reports go to test-results/<suite>/, which CI keeps on failure.
// node tests/ci.mjs <suite>
import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

// Playwright's WebKit derives navigator.platform "MacIntel" from its Mac user agent on every host,
// so on Linux the editor would expect Cmd where ControlOrMeta presses Ctrl: keep the host's own.
process.env.PLAYWRIGHT_NO_UA_PLATFORM = '1'
const web = fileURLToPath(new URL('..', import.meta.url))
const repo = resolve(web, '..')
const engine = resolve(web, 'public/engine')
const hash = bytes => createHash('sha256').update(bytes).digest('hex')

// The 8-page original Type3 fixture behind search, selection and save; its exporter writes
// only to this directory.
const searchFixture = { crate: 'kition-pdf-automation', test: 'search_geometry', env: 'KITION_PDF_SEARCH_GEOMETRY_OUT', dir: resolve(repo, 'target/search-geometry-2026-10-22'), manifest: 'manifest.json' }
const render = (test, env, manifest, pin) => ({ crate: 'kition-pdf-render', test, env, manifest, pin })
const suites = {
  geometry: { fixture: render('controlled_geometry', 'KITION_PDF_GEOMETRY_OUT', 'manifest.json'), harness: 'raster-geometry.mjs' },
  hairline: { fixture: render('hairline_transform', 'KITION_PDF_HAIRLINE_OUT', 'manifest.json'), harness: 'hairline.mjs', engineDir: true },
  type3: { fixture: render('type3_recursion', 'KITION_PDF_TYPE3_OUT', 'manifest.raw.json'), harness: 'type3.mjs', engineDir: true },
  mesh: { fixture: render('mesh_geometry', 'KITION_PDF_MESH_OUT', 'manifest.raw.json'), harness: 'mesh.mjs', engineDir: true },
  font: { fixture: render('font_mapping', 'KITION_PDF_FONT_OUT', 'manifest.raw.json', 'font-manifest.mjs'), harness: 'font.mjs', engineDir: true },
  dash: { fixture: render('dashed_hairline', 'KITION_PDF_DASH_OUT', 'manifest.raw.json', 'dash-manifest.mjs'), harness: 'dash.mjs', engineDir: true },
  search: { fixture: searchFixture, harness: 'search.mjs', engineDir: true },
  selection: { fixture: searchFixture, harness: 'selection.mjs', engineDir: true },
  save: { fixture: searchFixture, harness: 'save.mjs', engineDir: true, artifacts: true },
  'save-sdk': { fixture: searchFixture, harness: 'save-sdk.mjs', engineDir: true, browsers: [null] },
  cache: { harness: 'cache.mjs', diagnosticsEngine: 'cache-diagnostics' },
}

const name = process.argv[2]
const suite = suites[name]
if (!suite || process.argv.length !== 3) throw new Error(`Usage: node tests/ci.mjs <${Object.keys(suites).join('|')}>`)

function run(command, args, options = {}) {
  console.log(`$ ${command} ${args.join(' ')}`)
  const result = spawnSync(command, args, { stdio: 'inherit', ...options, env: { ...process.env, ...options.env } })
  if (result.error) throw result.error
  return result.status === 0
}
function must(command, args, options) {
  if (!run(command, args, options)) throw new Error(`${command} ${args.join(' ')} failed`)
}

/** Exports the suite's fixtures afresh (the exporters refuse to replace different bytes) and pins them. */
function fixtures({ crate, test, env, dir, manifest, pin }) {
  dir ??= resolve(repo, 'target/kit-ci', name, 'fixtures')
  rmSync(dir, { recursive: true, force: true })
  must('cargo', ['test', '-p', crate, '--test', test], { cwd: repo, env: { [env]: dir } })
  const raw = resolve(dir, manifest), pinned = resolve(dir, 'manifest-pinned.json')
  if (!existsSync(raw)) throw new Error(`${crate} ${test} exported no ${manifest}`)
  if (pin) {
    must(process.execPath, [resolve(web, 'tests', pin), '--raw', raw, '--out', pinned], { cwd: web })
    return pinned
  }
  const data = JSON.parse(readFileSync(raw, 'utf8'))
  const sha = file => hash(readFileSync(resolve(dir, file)))
  if (typeof data.file === 'string') data.sha256 = sha(data.file)
  for (const item of [...(data.cases ?? []), ...(data.fixtures ?? [])]) item.sha256 = sha(item.file)
  writeFileSync(pinned, `${JSON.stringify(data, null, 2)}\n`)
  return pinned
}

/** Builds the engine with a diagnostics feature; under CI, installs what the build needs first. */
function diagnosticsEngine(feature) {
  const out = resolve(repo, 'target/kit-ci', name, 'engine')
  rmSync(out, { recursive: true, force: true })
  if (process.env.CI) {
    must('rustup', ['target', 'add', 'wasm32-unknown-unknown'])
    if (!run('wasm-pack', ['--version'])) must('cargo', ['install', 'wasm-pack', '--version', '0.13.1', '--locked'])
  }
  must('sh', [resolve(web, 'scripts/build-engine.sh')], { env: { KITION_PDF_ENGINE_OUT: out, KITION_PDF_ENGINE_FEATURES: feature } })
  return out
}

const manifest = suite.fixture && fixtures(suite.fixture)
const engineDir = suite.diagnosticsEngine ? diagnosticsEngine(suite.diagnosticsEngine) : suite.engineDir && engine
const reports = resolve(web, 'test-results', name)
rmSync(reports, { recursive: true, force: true })
mkdirSync(reports, { recursive: true })
const failed = []
for (const browser of suite.browsers ?? ['chromium', 'webkit']) {
  const label = browser ?? 'node'
  const out = resolve(reports, `${label}.json`)
  const args = [resolve(web, 'tests', suite.harness), '--out', out]
  if (manifest) args.push('--manifest', manifest)
  if (engineDir) args.push('--engine-dir', engineDir)
  if (browser) args.push('--browser', browser)
  if (suite.artifacts) args.push('--artifact-dir', resolve(reports, `${label}-artifacts`))
  const passed = run(process.execPath, args, { cwd: web })
  if (!passed) {
    failed.push(label)
    // The report says what failed; print its errors and the failing entries so the log does too.
    if (existsSync(out)) {
      const report = JSON.parse(readFileSync(out, 'utf8'))
      console.log(`${label}: status ${report.status}; errors: ${JSON.stringify(report.errors ?? [])}`)
      for (const line of collectFailures(report).slice(0, 40)) console.log(`  ${line}`)
    }
  }
}
if (failed.length) {
  console.error(`FAILED ${name}: ${failed.join(', ')} (reports in test-results/${name})`)
  process.exit(1)
}
console.log(`PASS ${name}`)

/** Paths of the report's entries whose status is neither passed nor observed. */
function collectFailures(value, path = '', found = []) {
  if (Array.isArray(value)) value.forEach((item, index) => collectFailures(item, `${path}[${item?.id ?? item?.name ?? index}]`, found))
  else if (value && typeof value === 'object') {
    if (typeof value.status === 'string' && !['passed', 'observed'].includes(value.status) && path) {
      const detail = value.error ?? value.errors ?? value.failures
      found.push(`${path}: ${value.status}${detail && (!Array.isArray(detail) || detail.length) ? ` ${JSON.stringify(detail).slice(0, 300)}` : ''}`)
    }
    for (const [key, item] of Object.entries(value)) if (item && typeof item === 'object') collectFailures(item, path ? `${path}.${key}` : key, found)
  }
  return found
}
