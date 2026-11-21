// Installs the published @kition/sdk and @kition/tessera. KITION_LINK=1 links checkouts instead for
// development: kition-sdk's packages/sdk (KITION_SDK, default ../kition-sdk) and kition-tessera's
// packages/tessera (KITION_TESSERA, default ../kition-tessera). Both packages' exports point at
// dist/, so build them first (`pnpm --filter @kition/sdk build` in kition-sdk, `pnpm build` in
// kition-tessera's packages/tessera). Do not commit the lockfile a linked install writes.
const fs = require('node:fs')
const path = require('node:path')

const repo = path.resolve(__dirname, '..')
const sdk = path.resolve(process.env.KITION_SDK ?? path.join(repo, '..', 'kition-sdk'))
const tessera = path.resolve(process.env.KITION_TESSERA ?? path.join(repo, '..', 'kition-tessera'))
const local = {
  '@kition/sdk': path.join(sdk, 'packages', 'sdk'),
  '@kition/tessera': path.join(tessera, 'packages', 'tessera'),
}

function readPackage(pkg) {
  if (pkg.name !== 'kition-pdf-ui' || process.env.KITION_LINK !== '1') return pkg
  for (const field of ['dependencies', 'devDependencies']) {
    for (const name of Object.keys(pkg[field] ?? {})) {
      const dir = local[name]
      if (dir && fs.existsSync(path.join(dir, 'package.json'))) {
        pkg[field][name] = `link:${path.relative(__dirname, dir).split(path.sep).join('/')}`
      }
    }
  }
  return pkg
}

module.exports = { hooks: { readPackage } }
