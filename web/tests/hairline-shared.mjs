import { readdir, readFile, realpath } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { isAbsolute, relative, resolve, sep } from 'node:path'

/**
 * The editor runs on one SDK package, @kition/sdk. Its browser code is the built `dist/lib/`
 * (every subpath the editor and the development host import points there) and `package.json`
 * maps the subpaths; both are what the harnesses fingerprint and what a frozen copy freezes.
 */
export const SDK_PACKAGE = '@kition/sdk'
export const sdkFrozenPaths = ['package.json', 'dist/lib']
const hash = bytes => createHash('sha256').update(bytes).digest('hex')
const contains = (directory, file) => file === directory || file.startsWith(`${directory}${sep}`)
const cleanId = id => id.split('?')[0].split('#')[0]
const portable = path => path.split(sep).join('/')
const sdkLabel = path => `${SDK_PACKAGE}/${path}`

/** The installed @kition/sdk the editor in `web` runs on (pnpm links it into node_modules). */
export const installedSdk = web => realpath(resolve(web, 'node_modules', SDK_PACKAGE))

/** SHA-256 of the package's browser code and manifest, labelled `@kition/sdk/<path>`. */
export async function sdkFingerprints(packageRoot, { rejectSymlinks = false } = {}) {
  const fingerprints = {}
  async function walk(directory, path) {
    for (const entry of (await readdir(directory, { withFileTypes: true })).sort((a, b) => a.name.localeCompare(b.name))) {
      const child = `${path}/${entry.name}`
      if (rejectSymlinks && entry.isSymbolicLink()) throw new Error(`Source snapshot contains a symlink: ${sdkLabel(child)}`)
      if (entry.isDirectory()) await walk(resolve(directory, entry.name), child)
      else if (entry.isFile()) fingerprints[sdkLabel(child)] = hash(await readFile(resolve(directory, entry.name)))
    }
  }
  fingerprints[sdkLabel('package.json')] = hash(await readFile(resolve(packageRoot, 'package.json')))
  await walk(resolve(packageRoot, 'dist/lib'), 'dist/lib')
  return fingerprints
}

/** The browser file an export target names: a path, or the `import` (else `default`) condition. */
function browserTarget(target) {
  if (typeof target === 'string') return target
  if (target && typeof target === 'object') return browserTarget(target.import ?? target.default)
  return null
}

/**
 * Serves the editor's @kition/sdk from a frozen copy of the package (its `package.json` and
 * `dist/`; a `node_modules` symlink beside them may provide its external dependencies, such as
 * React and @kition/tessera) instead of the installed one, and proves the browser loaded only
 * frozen bytes: each subpath resolves by an exact alias into the copy's `dist/lib`, a request
 * into the installed package fails, and every frozen module the browser requests must match the
 * fingerprint taken before the run. External installed dependencies remain external.
 */
export async function frozenSharedSource(directory, livePackage) {
  const root = await realpath(directory), live = await realpath(livePackage)
  if (root === live) throw new Error('Frozen shared source must be separate from the installed SDK')
  const manifest = JSON.parse(await readFile(resolve(root, 'package.json')))
  const liveManifest = JSON.parse(await readFile(resolve(live, 'package.json')))
  if (manifest.name !== liveManifest.name || !manifest.exports || typeof manifest.exports !== 'object') throw new Error(`Invalid frozen package: it must be ${liveManifest.name} with exports`)
  const code = resolve(root, 'dist/lib')
  const aliases = [], exports = {}, requests = {}, violations = []
  let expectedFingerprints = null
  for (const [key, entry] of Object.entries(manifest.exports)) {
    if (key !== '.' && !/^\.\/[A-Za-z0-9./_-]+$/.test(key)) throw new Error(`Unsupported frozen package export: ${key}`)
    const target = browserTarget(entry)
    if (typeof target !== 'string' || !target.startsWith('./dist/')) throw new Error(`Unsupported frozen package export: ${key}`)
    // Subpaths for Node or other bundles (dist/standalone) stay unaliased, so importing one fails.
    if (!target.startsWith('./dist/lib/')) continue
    const path = await realpath(resolve(root, target))
    if (!contains(code, path)) throw new Error(`Frozen package export escapes its source: ${key}`)
    const specifier = SDK_PACKAGE + (key === '.' ? '' : key.slice(1))
    aliases.push({ find: new RegExp(`^${specifier.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`), replacement: path })
    exports[specifier] = target.slice(2)
  }
  function reject(message) { violations.push(message); throw new Error(message) }
  async function inspectPath(path) {
    const canonical = await realpath(path).catch(() => path)
    if (contains(live, canonical) || contains(live, path)) reject('Installed shared SDK package request or resolution rejected')
    if (!contains(root, path) && !contains(root, canonical)) return null
    const frozen = /^(package\.json$|dist\/lib\/)/
    if (frozen.test(portable(relative(root, path))) && !contains(root, canonical)) reject('Frozen shared source resolves outside the snapshot')
    const label = portable(relative(root, canonical))
    // Dependency symlinks are allowed, but the package's own code must remain frozen.
    if (!frozen.test(label)) return null
    return { path: canonical, label }
  }
  function guardPlugin(target) {
    return {
      name: `pdf-hairline-frozen-shared-${target}`,
      enforce: 'pre',
      async resolveId(id) {
        if (id === SDK_PACKAGE || id.startsWith(`${SDK_PACKAGE}/`)) reject('Shared package import has no exact frozen export alias')
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
              if (!expectedFingerprints || expectedFingerprints[sdkLabel(entry.label)] !== digest) reject(`Frozen requested module differs from its initial fingerprint: ${entry.label}`)
              const previous = requests[entry.label]
              if (previous && previous.sha256 !== digest) reject(`Frozen module changed between requests: ${entry.label}`)
              requests[entry.label] = { sha256: digest, count: (previous?.count ?? 0) + 1 }
            }
            next()
          } catch (error) {
            const message = error.message.startsWith('Installed shared') || error.message.startsWith('Frozen ') ? error.message : 'Frozen shared request could not be verified'
            if (!violations.includes(message)) violations.push(message)
            response.statusCode = 500; response.end(message)
          }
        })
      },
    }
  }
  // The editor, its engine Worker (a hashed asset kit-editor starts by URL), and the host side.
  const workers = (await readdir(resolve(code, 'assets')).catch(() => [])).filter(name => /^kitEngine\.worker-[A-Za-z0-9_-]+\.js$/.test(name))
  if (workers.length !== 1) throw new Error('Frozen package must hold exactly one engine Worker asset')
  const required = ['dist/lib/kit-editor/index.js', `dist/lib/assets/${workers[0]}`, 'dist/lib/kit-host/index.js', 'dist/lib/kit-ui-protocol/index.js', 'dist/lib/kit-dev-host/index.js']
  return {
    root, aliases, packages: [SDK_PACKAGE],
    report: { mode: 'frozen', package: manifest.name, version: manifest.version, exports, requests, violations, requiredModules: required, missingRequiredModules: [...required] },
    plugin: guardPlugin('server'), workerPlugins: () => [guardPlugin('worker')],
    setExpectedFingerprints(fingerprints) { expectedFingerprints = fingerprints },
    finish() {
      this.report.missingRequiredModules = required.filter(name => !requests[name]?.count)
      return violations.length === 0 && this.report.missingRequiredModules.length === 0
    },
  }
}
