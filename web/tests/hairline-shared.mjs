import { readFile, realpath } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { isAbsolute, relative, resolve, sep } from 'node:path'

export const sharedPackageNames = ['kit-editor', 'kit-host', 'kit-ui-protocol', 'kit-dev-host', 'kit-engine', 'kdm']
const hash = bytes => createHash('sha256').update(bytes).digest('hex')
const contains = (directory, file) => file === directory || file.startsWith(`${directory}${sep}`)
const cleanId = id => id.split('?')[0].split('#')[0]
const portable = path => path.split(sep).join('/')

/** Freeze only shared source resolution; external installed dependencies remain external. */
export async function frozenSharedSource(directory, liveDirectory) {
  const root = await realpath(directory), live = await realpath(liveDirectory)
  if (root === live) throw new Error('Frozen shared source must be separate from the live SDK')
  const livePackages = resolve(live, 'packages'), snapshotPackages = resolve(root, 'packages')
  const aliases = [], exports = {}, requests = {}, violations = []
  let expectedFingerprints = null
  for (const name of sharedPackageNames) {
    const packageRoot = resolve(snapshotPackages, name)
    const manifest = JSON.parse(await readFile(resolve(packageRoot, 'package.json')))
    if (manifest.name !== `@kition/${name}` || !manifest.exports || typeof manifest.exports !== 'object') throw new Error(`Invalid frozen package exports: ${name}`)
    exports[name] = {}
    for (const [key, target] of Object.entries(manifest.exports)) {
      if ((key !== '.' && !/^\.\/[A-Za-z0-9./_-]+$/.test(key)) || typeof target !== 'string' || !target.startsWith('./src/')) throw new Error(`Unsupported frozen package export: ${name}`)
      const path = await realpath(resolve(packageRoot, target))
      if (!contains(resolve(packageRoot, 'src'), path)) throw new Error(`Frozen package export escapes its source: ${name}`)
      const specifier = manifest.name + (key === '.' ? '' : key.slice(1))
      aliases.push({ find: new RegExp(`^${specifier.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`), replacement: path })
      exports[name][specifier] = `packages/${name}/${target.slice(2)}`
    }
  }
  function reject(message) { violations.push(message); throw new Error(message) }
  async function inspectPath(path) {
    const canonical = await realpath(path).catch(() => path)
    if (contains(livePackages, canonical)) reject('Live shared SDK package request or resolution rejected')
    if (!contains(snapshotPackages, path) && !contains(snapshotPackages, canonical)) return null
    const sourcePattern = /^packages\/(kit-editor|kit-host|kit-ui-protocol|kit-dev-host|kit-engine|kdm)\/(src\/|package\.json$)/
    if (sourcePattern.test(portable(relative(root, path))) && !contains(snapshotPackages, canonical)) reject('Frozen shared source resolves outside the snapshot')
    const label = portable(relative(root, canonical))
    // Dependency symlinks are allowed, but shared package source must remain frozen.
    if (!sourcePattern.test(label)) return null
    return { path: canonical, label }
  }
  function guardPlugin(target) {
    return {
      name: `pdf-hairline-frozen-shared-${target}`,
      enforce: 'pre',
      async resolveId(id) {
        if (id.startsWith('@kition/')) reject('Shared package import has no exact frozen export alias')
        const path = cleanId(id)
        if (isAbsolute(path)) await inspectPath(path)
        return null
      },
      async load(id) { const path = cleanId(id); if (isAbsolute(path)) await inspectPath(path); return null },
      configureServer(server) {
        server.middlewares.use(async (request, response, next) => {
          try {
            const pathname = decodeURIComponent(new URL(request.url, 'http://localhost').pathname)
            if (!pathname.startsWith('/@fs/')) return next()
            const entry = await inspectPath(pathname.slice('/@fs'.length))
            if (entry) {
              const digest = hash(await readFile(entry.path))
              if (!expectedFingerprints || expectedFingerprints[`kition-sdk/${entry.label}`] !== digest) reject(`Frozen requested module differs from its initial fingerprint: ${entry.label}`)
              const previous = requests[entry.label]
              if (previous && previous.sha256 !== digest) reject(`Frozen module changed between requests: ${entry.label}`)
              requests[entry.label] = { sha256: digest, count: (previous?.count ?? 0) + 1 }
            }
            next()
          } catch (error) {
            const message = error.message.startsWith('Live shared') || error.message.startsWith('Frozen ') ? error.message : 'Frozen shared request could not be verified'
            if (!violations.includes(message)) violations.push(message)
            response.statusCode = 500; response.end(message)
          }
        })
      },
    }
  }
  const required = ['packages/kit-editor/src/engine/kitEngine.ts', 'packages/kit-editor/src/engine/kitEngine.worker.ts', 'packages/kit-host/src/index.ts', 'packages/kit-ui-protocol/src/index.ts', 'packages/kit-dev-host/src/index.ts']
  return {
    root, aliases, packages: sharedPackageNames.map(name => `@kition/${name}`),
    report: { mode: 'frozen', exports, requests, violations, requiredModules: required, missingRequiredModules: [...required] },
    plugin: guardPlugin('server'), workerPlugins: () => [guardPlugin('worker')],
    setExpectedFingerprints(fingerprints) { expectedFingerprints = fingerprints },
    finish() {
      this.report.missingRequiredModules = required.filter(name => !requests[name]?.count)
      return violations.length === 0 && this.report.missingRequiredModules.length === 0
    },
  }
}
