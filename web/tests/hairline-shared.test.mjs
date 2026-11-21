import test from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, mkdir, writeFile, rm, readFile, symlink, realpath } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { resolve } from 'node:path'
import { createHash } from 'node:crypto'
import { createServer } from 'vite'
import { frozenSharedSource, sdkFingerprints, SDK_PACKAGE } from './hairline-shared.mjs'

const subpaths = ['editor', 'host', 'protocol', 'dev-host']
const folders = { editor: 'kit-editor', host: 'kit-host', protocol: 'kit-ui-protocol', 'dev-host': 'kit-dev-host' }
const manifest = {
  name: '@kitionstage/sdk', version: '0.5.2-stage.2',
  exports: {
    ...Object.fromEntries(subpaths.map(name => [`./${name}`, { types: `./dist/types/${folders[name]}/src/index.d.ts`, import: `./dist/lib/${folders[name]}/index.js` }])),
    './editor/styles.css': './dist/lib/kit-editor/styles.css',
    './embed': { types: './dist/types/embed/src/index.d.ts', import: './dist/standalone/index.js' },
  },
}

async function setup(t) {
  const directory = await realpath(await mkdtemp(resolve(tmpdir(), 'pdf-hairline-shared-')))
  t.after(() => rm(directory, { recursive: true, force: true }))
  const snapshot = resolve(directory, 'snapshot'), live = resolve(directory, 'live')
  async function file(base, path, content) {
    await mkdir(resolve(base, path, '..'), { recursive: true })
    await writeFile(resolve(base, path), content)
  }
  for (const base of [snapshot, live]) {
    await file(base, 'package.json', JSON.stringify(manifest))
    for (const name of subpaths) await file(base, `dist/lib/${folders[name]}/index.js`, 'export const original = true\n')
    await file(base, 'dist/lib/kit-editor/styles.css', ':root { color: black; }\n')
    await file(base, 'dist/lib/assets/kitEngine.worker-abc123.js', 'export const worker = true\n')
    await file(base, 'dist/standalone/index.js', 'export const standalone = true\n')
  }
  const fingerprints = await sdkFingerprints(snapshot, { rejectSymlinks: true })
  const shared = await frozenSharedSource(snapshot, live)
  shared.setExpectedFingerprints(fingerprints)
  let middleware
  shared.plugin.configureServer({ middlewares: { use(handler) { middleware = handler } } })
  const request = async path => {
    const response = { statusCode: 200, body: null, next: false, end(body) { this.body = body } }
    await middleware({ url: `/@fs${path}` }, response, () => { response.next = true })
    return response
  }
  return { directory, snapshot, live, shared, request, fingerprints }
}

test('fingerprints cover the package manifest and its browser code only', async t => {
  const { fingerprints, snapshot } = await setup(t)
  assert.deepEqual(Object.keys(fingerprints).sort(), [`${SDK_PACKAGE}/package.json`, ...['assets/kitEngine.worker-abc123.js', ...subpaths.map(name => `${folders[name]}/index.js`), 'kit-editor/styles.css'].map(path => `${SDK_PACKAGE}/dist/lib/${path}`)].sort())
  await symlink(resolve(snapshot, 'dist/standalone'), resolve(snapshot, 'dist/lib/linked'))
  await assert.rejects(sdkFingerprints(snapshot, { rejectSymlinks: true }), /symlink/)
})
test('exact package subpath aliases resolve into frozen browser code in Vite', async t => {
  const { snapshot, directory, shared } = await setup(t)
  const server = await createServer({ root: directory, configFile: false, plugins: [shared.plugin], resolve: { alias: shared.aliases }, optimizeDeps: { noDiscovery: true, exclude: shared.packages }, server: { middlewareMode: true }, logLevel: 'silent' })
  t.after(() => server.close())
  for (const name of subpaths) assert.equal((await server.pluginContainer.resolveId(`${SDK_PACKAGE}/${name}`)).id, resolve(snapshot, 'dist/lib', folders[name], 'index.js'))
  assert.equal((await server.pluginContainer.resolveId(`${SDK_PACKAGE}/editor/styles.css`)).id, resolve(snapshot, 'dist/lib/kit-editor/styles.css'))
  await assert.rejects(server.pluginContainer.resolveId(`${SDK_PACKAGE}/editor/unlisted`), /no exact frozen/)
  await assert.rejects(server.pluginContainer.resolveId(`${SDK_PACKAGE}/embed`), /no exact frozen/)
})
test('server and Worker reject installed package resolution and requests', async t => {
  const { live, shared, request } = await setup(t)
  const path = resolve(live, 'dist/lib/kit-editor/index.js')
  await assert.rejects(shared.plugin.load(path), /Installed shared/)
  await assert.rejects(shared.workerPlugins()[0].resolveId(path), /Installed shared/)
  const response = await request(path)
  assert.equal(response.statusCode, 500); assert.equal(response.next, false)
  assert.equal(shared.finish(), false)
  assert.ok(!JSON.stringify(shared.report).includes(live))
})
test('actual frozen module requests record portable hashes and satisfy required coverage', async t => {
  const { snapshot, shared, request } = await setup(t)
  assert.equal(shared.finish(), false)
  assert.ok(shared.report.requiredModules.includes('dist/lib/assets/kitEngine.worker-abc123.js'))
  for (const name of shared.report.requiredModules) {
    const response = await request(`${resolve(snapshot, name)}?worker_file&type=module`)
    assert.equal(response.next, true)
    assert.equal(shared.report.requests[name].sha256, createHash('sha256').update(await readFile(resolve(snapshot, name))).digest('hex'))
  }
  assert.equal(shared.finish(), true)
  assert.deepEqual(shared.report.missingRequiredModules, [])
  assert.ok(!JSON.stringify(shared.report).includes(snapshot))
})
test('request-time source edits fail initial fingerprint verification', async t => {
  const { snapshot, shared, request } = await setup(t)
  const path = resolve(snapshot, 'dist/lib/kit-editor/index.js')
  await writeFile(path, 'export const changed = true\n')
  assert.equal((await request(path)).statusCode, 500)
  assert.match(shared.report.violations[0], /initial fingerprint/)
})
test('external dependencies may be symlinked while package exports cannot escape', async t => {
  const { directory, snapshot, live, shared, request } = await setup(t)
  const external = resolve(directory, 'external')
  await mkdir(external); await writeFile(resolve(external, 'index.js'), 'export const external = true')
  await symlink(external, resolve(snapshot, 'node_modules'))
  assert.equal((await request(resolve(snapshot, 'node_modules/index.js'))).next, true)
  const source = resolve(snapshot, 'dist/lib/kit-editor/index.js')
  await rm(source); await symlink(resolve(external, 'index.js'), source)
  await assert.rejects(frozenSharedSource(snapshot, live), /escapes its source/)
  assert.equal(shared.report.violations.length, 0)
})
test('a frozen copy must be the installed package, separate from it', async t => {
  const { snapshot, live } = await setup(t)
  await assert.rejects(frozenSharedSource(live, live), /separate/)
  await writeFile(resolve(snapshot, 'package.json'), JSON.stringify({ ...manifest, name: '@kition/other' }))
  await assert.rejects(frozenSharedSource(snapshot, live), /Invalid frozen package/)
})
