// Focus shows the page and the Focus bar only, and names the tool: the bar says Select text, the
// find bar is hidden in Focus and back after it, and Escape cancels first (KitPointer 4.4): with
// text selected it clears the selection and stays in Focus; with nothing to cancel it leaves.
// node tests/focus.mjs [chromium|webkit]
import assert from 'node:assert/strict'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'
// Playwright's WebKit reports a Mac user agent and, from it, navigator.platform "MacIntel" on every
// host, so on Linux the editor expects Cmd while ControlOrMeta presses Ctrl. Keep the host's own
// platform, which is what ControlOrMeta follows.
process.env.PLAYWRIGHT_NO_UA_PLATFORM = '1'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1165, height: 900 } })
try {
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html?modes=classic,focus&locale=en-US`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 90_000 })
  const kit = page.frameLocator('#kit')
  const app = kit.locator('.kui-app')
  const mode = () => app.getAttribute('data-mode')
  const escape = async () => {
    await page.keyboard.press('Escape')
    await page.waitForTimeout(300)
  }
  await page.waitForTimeout(500)

  // The find bar is the kit's own bar: hidden in Focus, back after it.
  await app.focus()
  await page.keyboard.press('ControlOrMeta+f')
  await kit.getByTestId('pdf-find-bar').waitFor({ timeout: 5000 })
  await page.keyboard.press('ControlOrMeta+Backslash')
  assert.equal(await mode(), 'focus', 'Cmd+\\ enters Focus from the find field')
  assert.equal(await kit.getByTestId('pdf-find-bar').count(), 0, 'Focus shows no find bar')
  assert.match(await kit.locator('.kui-focus-bar').textContent(), /Select text/, 'the Focus bar names the tool')
  await page.keyboard.press('ControlOrMeta+Backslash')
  assert.equal(await mode(), 'classic', 'Cmd+\\ leaves Focus')
  await kit.getByTestId('pdf-find-bar').waitFor({ timeout: 5000 })
  await kit.getByTestId('pdf-find-text').focus()
  await escape()
  assert.equal(await kit.getByTestId('pdf-find-bar').count(), 0, 'Escape closes the find bar')

  // Text selected in Focus: the first Escape clears it, the next one leaves Focus.
  await app.focus()
  await page.keyboard.press('ControlOrMeta+Backslash')
  assert.equal(await mode(), 'focus')
  const surface = kit.getByTestId('kit-canvas-surface')
  const box = await surface.boundingBox()
  assert(box, 'the canvas is drawn')
  // The sample's heading, a third of the way down the first page's top.
  const y = box.y + box.height * 0.3955
  await page.mouse.move(box.x + box.width * 0.17, y)
  await page.mouse.down()
  await page.mouse.move(box.x + box.width * 0.7, y, { steps: 8 })
  await page.mouse.up()
  await page.waitForTimeout(500)
  await escape()
  assert.equal(await mode(), 'focus', 'an Escape that cleared the selection stays in Focus')
  await escape()
  assert.equal(await mode(), 'classic', 'an Escape with nothing to cancel leaves Focus')
  console.log(`PASS kit.pdf focus (${which})`)
} finally {
  await browser.close()
  await server.close()
}
