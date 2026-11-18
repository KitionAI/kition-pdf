import test from 'node:test'
import assert from 'node:assert/strict'
import { mkdtemp, mkdir, writeFile, rm, readFile, symlink, realpath } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { resolve } from 'node:path'
import { createHash } from 'node:crypto'
import { createServer } from 'vite'
import { frozenSharedSource, sharedPackageNames } from './hairline-shared.mjs'

async function setup(t) {
  const directory = await realpath(await mkdtemp(resolve(tmpdir(), 'pdf-hairline-shared-')))
  t.after(() => rm(directory, { recursive: true, force: true }))
  const snapshot = resolve(directory, 'snapshot'), live = resolve(directory, 'live')
  await mkdir(live, { recursive: true })
  const fingerprints = {}
  async function file(path, content) {
    await mkdir(resolve(snapshot, path, '..'), { recursive: true })
    await writeFile(resolve(snapshot, path), content)
    fingerprints[`kition-sdk/${path}`] = createHash('sha256').update(content).digest('hex')
  }
  for (const name of sharedPackageNames) {
    const exports = { '.': './src/index.ts', ...(name === 'kit-editor' ? { './styles.css': './src/styles.css' } : {}) }
    await file(`packages/${name}/package.json`, JSON.stringify({ name: `@kition/${name}`, exports }))
    await file(`packages/${name}/src/index.ts`, 'export const original = true\n')
  }
  await file('packages/kit-editor/src/styles.css', ':root { color: black; }\n')
  for (const name of ['kitEngine.ts', 'kitEngine.worker.ts']) await file(`packages/kit-editor/src/engine/${name}`, 'export const worker = true\n')
  const shared = await frozenSharedSource(snapshot, live)
  shared.setExpectedFingerprints(fingerprints)
  let middleware
  shared.plugin.configureServer({ middlewares: { use(handler) { middleware = handler } } })
  const request = async path => {
    const response = { statusCode: 200, body: null, next: false, end(body) { this.body = body } }
    await middleware({ url: `/@fs${path}` }, response, () => { response.next = true })
    return response
  }
  return { directory, snapshot, live, shared, request }
}

test('exact package and subpath aliases resolve into frozen sources in Vite', async t => {
  const { snapshot, directory, shared } = await setup(t)
  const server = await createServer({ root: directory, configFile: false, plugins: [shared.plugin], resolve: { alias: shared.aliases }, optimizeDeps: { noDiscovery: true, exclude: shared.packages }, server: { middlewareMode: true }, logLevel: 'silent' })
  t.after(() => server.close())
  for (const name of sharedPackageNames) assert.equal((await server.pluginContainer.resolveId(`@kition/${name}`)).id, resolve(snapshot, 'packages', name, 'src/index.ts'))
  assert.equal((await server.pluginContainer.resolveId('@kition/kit-editor/styles.css')).id, resolve(snapshot, 'packages/kit-editor/src/styles.css'))
  await assert.rejects(server.pluginContainer.resolveId('@kition/kit-editor/unlisted'), /no exact frozen/)
})
test('server and Worker reject direct live package resolution and requests', async t => {
  const { live, shared, request } = await setup(t)
  const path = resolve(live, 'packages/kit-editor/src/editor.tsx')
  await mkdir(resolve(path, '..'), { recursive: true }); await writeFile(path, 'export const live = true')
  await assert.rejects(shared.plugin.load(path), /Live shared/)
  await assert.rejects(shared.workerPlugins()[0].resolveId(path), /Live shared/)
  const response = await request(path)
  assert.equal(response.statusCode, 500); assert.equal(response.next, false)
  assert.equal(shared.finish(), false)
  assert.ok(!JSON.stringify(shared.report).includes(live))
})
test('actual frozen module requests record portable hashes and satisfy required coverage', async t => {
  const { snapshot, shared, request } = await setup(t)
  assert.equal(shared.finish(), false)
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
  const path = resolve(snapshot, 'packages/kit-editor/src/index.ts')
  await writeFile(path, 'export const changed = true\n')
  assert.equal((await request(path)).statusCode, 500)
  assert.match(shared.report.violations[0], /initial fingerprint/)
})
test('external dependencies may be symlinked while source exports cannot escape', async t => {
  const { directory, snapshot, live, shared, request } = await setup(t)
  const external = resolve(directory, 'external')
  await mkdir(external); await writeFile(resolve(external, 'index.js'), 'export const external = true')
  await symlink(external, resolve(snapshot, 'packages/kit-editor/node_modules'))
  assert.equal((await request(resolve(snapshot, 'packages/kit-editor/node_modules/index.js'))).next, true)
  const source = resolve(snapshot, 'packages/kit-editor/src/index.ts')
  await rm(source); await symlink(resolve(external, 'index.js'), source)
  await assert.rejects(frozenSharedSource(snapshot, live), /escapes its source/)
  assert.equal(shared.report.violations.length, 0)
})
