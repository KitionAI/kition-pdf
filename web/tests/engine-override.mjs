import { readFile } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { resolve } from 'node:path'

/** Serve a test engine before Vite's static middleware, including requests made by Workers. */
export async function engineOverride(directory, ordinaryDirectory) {
  const assets = new Map(), hashes = {}, served = {}
  for (const name of ['kition_pdf_kit.js', 'kition_pdf_sdk_bg.wasm']) {
    const data = await readFile(resolve(directory ?? ordinaryDirectory, name))
    assets.set(`/engine/${name}`, data)
    hashes[name] = createHash('sha256').update(data).digest('hex')
    served[name] = 0
  }
  const plugin = {
    name: 'pdf-test-isolated-engine',
    configureServer(server) {
      server.middlewares.use((request, response, next) => {
        const path = new URL(request.url, 'http://localhost').pathname
        if (!directory || !path.startsWith('/engine/')) return next()
        const data = assets.get(path)
        if (!data) { response.statusCode = 404; response.end(); return }
        const name = path.slice('/engine/'.length)
        served[name]++
        response.setHeader('Content-Type', name.endsWith('.wasm') ? 'application/wasm' : 'text/javascript')
        response.setHeader('Cache-Control', 'no-store')
        response.end(data)
      })
    },
  }
  return { hashes, served, plugin }
}
