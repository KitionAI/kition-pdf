// The hand tests' shared start (plan 2026-11-26, "PDF usable by hand"): starts Vite, opens dev.html
// in `which` browser, opens the synthetic PDF (fixture.mjs) through the host's file input as a
// person does, and gives the steps their helpers. The steps use real `page.mouse` and
// `page.keyboard` input only, and never focus anything before a key; the helpers read the editor
// back: the host's `inspect` and read commands, the canvas's view, where the keyboard is.
import { mkdirSync } from 'node:fs'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'
import { handPdf } from './fixture.mjs'

export const FILE = 'hand.pdf'

export async function openHand(script, which, { query = 'locale=en-US' } = {}) {
  const server = await createServer({ configFile: new URL('../../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
  await server.listen()
  const origin = `http://127.0.0.1:${server.httpServer.address().port}`
  const browser = await (which === 'webkit' ? webkit : chromium).launch()
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 } })
  const page = await context.newPage()
  const errors = []
  let failures = 0
  let shots = 0
  page.on('pageerror', (error) => errors.push(error.message))
  const evidence = new URL('../../test-results/hand/', import.meta.url).pathname
  const step = async (name, ok, detail = '') => {
    if (!ok) {
      failures += 1
      mkdirSync(evidence, { recursive: true })
      shots += 1
      await page.screenshot({ path: `${evidence}${script}-${which}-${shots}.png` }).catch(() => undefined)
    }
    console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` - ${detail}` : ''}`)
  }
  const until = async (check, tries = 80) => {
    for (let i = 0; i < tries; i += 1) {
      if (await check().catch(() => false)) return true
      await page.waitForTimeout(100)
    }
    return false
  }
  /** Runs one part; a part that throws fails and the next one still runs. */
  const part = async (name, body) => {
    try {
      await body()
    } catch (error) {
      await step(`${name}: ran to the end`, false, error instanceof Error ? error.message.split('\n').slice(0, 4).join(' / ') : String(error))
    }
  }
  const finish = async () => {
    if (errors.length) console.log(`page errors:\n${errors.slice(0, 8).join('\n')}`)
    await browser.close()
    await server.close()
    console.log(`${failures ? 'FAIL' : 'PASS'} ${script} (${which}): ${failures} failed`)
    process.exit(failures ? 1 : 0)
  }

  await page.goto(`${origin}/dev.html?${query}`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 90_000 })
  const kit = page.frameLocator('#kit')
  const frame = () => page.frames().find((candidate) => candidate !== page.mainFrame())
  const mod = (await page.evaluate(() => /Mac|iPhone|iPad/i.test(navigator.platform))) ? 'Meta' : 'Control'
  const status = () => kit.getByTestId('pdf-status').textContent({ timeout: 2000 }).catch(() => '')

  /** Opens the synthetic PDF through the host's file input; nothing in the editor is clicked. */
  const openFixture = async () => {
    await page.evaluate(() => { document.getElementById('status').textContent = 'opening…' })
    await page.setInputFiles('#file', { name: FILE, mimeType: 'application/pdf', buffer: handPdf() })
    await page.waitForFunction((name) => document.getElementById('status')?.textContent?.includes(`${name} open in`), FILE, { timeout: 90_000 })
    await until(async () => /^Page 1 of \d+$/.test(await status()), 200)
    await until(async () => (await view()) != null, 100)
    await page.waitForTimeout(600)
  }

  // Reads, not a person's actions: the host's agent connection and the editor's own state.
  const inspect = () => page.evaluate(async () => (await window.devHost.connection().ai.inspect()).details)
  const ask = (command, params = {}) => page.evaluate(async ([command, params]) => {
    const answer = await window.devHost.connection().ai.run([{ command, params }], false)
    return answer.completed?.[0]?.result ?? answer
  }, [command, params])
  const pages = async () => (await inspect()).pages
  /** The canvas's view (zoom, the document point at its top-left, its size in CSS pixels), read through the canvas's handle. */
  const view = () => frame().evaluate(() => {
    const element = document.querySelector('[data-testid="kit-canvas"]')
    if (!element) return null
    const key = Object.keys(element).find((name) => name.startsWith('__reactFiber'))
    for (let fiber = element[key], i = 0; fiber && i < 80; fiber = fiber.return, i += 1) {
      const handle = fiber.memoizedProps?.ref?.current ?? fiber.memoizedProps?.canvas?.current
      if (handle && typeof handle.view === 'function') return handle.view()
    }
    return null
  })
  /** Where the keyboard is in the editor's page: `body`, `pages`, or a test id or tag. */
  const focused = () => frame().evaluate(() => {
    const active = document.activeElement
    if (!active || active === document.body) return 'body'
    if (active.classList.contains('pdf-pages')) return 'pages'
    return active.getAttribute('data-testid') ?? active.className ?? active.tagName.toLowerCase()
  })
  const surface = async () => kit.getByTestId('kit-canvas-surface').boundingBox()
  /** A page-local point (PDF points from the top-left) of page `index` (0-based) on the browser page. */
  const toScreen = async (index, px, py) => {
    const [v, list, box] = [await view(), await pages(), await surface()]
    const area = list[index].area
    return { x: box.x + (area.x + px - v.x) * v.zoom, y: box.y + (area.y + py - v.y) * v.zoom }
  }
  /** Waits until the view stops moving (a scroll past the painted frame waits for its render). */
  const settle = async (min = 250) => {
    await page.waitForTimeout(min)
    let last = ''
    for (let i = 0; i < 30; i += 1) {
      const now = JSON.stringify(await view().catch(() => null)) + (await status())
      if (now === last) return
      last = now
      await page.waitForTimeout(120)
    }
  }
  const press = async (key) => {
    await page.keyboard.press(key)
    await settle()
  }
  return { page, kit, frame, step, until, part, finish, mod, status, openFixture, inspect, ask, pages, view, focused, surface, toScreen, settle, press }
}
