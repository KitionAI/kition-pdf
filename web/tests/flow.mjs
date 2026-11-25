// The editor's flow in a real browser, through the development host: node tests/flow.mjs [chromium|webkit]
// Starts the Vite dev server itself, opens dev.html in Chinese, and prints PASS or FAIL per step.
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const context = await browser.newContext({ viewport: { width: 1440, height: 900 } })
if (which === 'chromium') await context.grantPermissions(['clipboard-read', 'clipboard-write'])
const page = await context.newPage()
const errors = []
let failures = 0
page.on('pageerror', (error) => errors.push(error.message))
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` - ${detail}` : ''}`)
}
const until = async (check, tries = 60) => {
  for (let i = 0; i < tries; i += 1) {
    if (await check().catch(() => false)) return true
    await page.waitForTimeout(100)
  }
  return false
}
const inspect = () => page.evaluate(async () => window.devHost.connection().ai.inspect())
const details = async () => (await inspect()).details

try {
  await page.goto(`${origin}/dev.html?modes=classic&locale=zh-CN`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  const kit = page.frameLocator('#kit')
  await page.waitForTimeout(1000)
  const status = () => kit.getByTestId('pdf-status').textContent()
  const ask = (command, params = {}) => page.evaluate(async ([command, params]) => (await window.devHost.connection().ai.run([{ command, params }], false)).completed[0].result, [command, params])
  step('Chinese tab names', (await kit.getByTestId('ribbon-tab-home').textContent()) === '主页')
  step('status shows the pages', (await status()) === '第 1 页，共 2 页', await status())
  await until(async () => (await kit.getByTestId('pdf-thumb-1').locator('canvas').evaluate((canvas) => {
    const data = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data
    let dark = 0
    for (let i = 0; i < data.length; i += 4) if (data[i] < 160) dark++
    return dark
  })) > 20)
  step('draws page thumbnails', true)
  await kit.getByTestId('pdf-thumb-1').click()
  await until(async () => (await status()) === '第 2 页，共 2 页')
  step('goes to a page from its thumbnail', (await status()) === '第 2 页，共 2 页')
  await kit.getByTestId('pdf-previous').click()
  await until(async () => (await status()) === '第 1 页，共 2 页')
  step('previous page', (await status()) === '第 1 页，共 2 页')

  // Scrolling far past the first or last page stops at it: the pages never leave the view.
  const pageShare = () => kit.locator('.kui-canvas-region canvas').first().evaluate((canvas) => {
    const data = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data
    let white = 0
    for (let i = 0; i < data.length; i += 4) if (data[i] > 250 && data[i + 1] > 250 && data[i + 2] > 250) white++
    return white / (data.length / 4)
  })
  const canvasBox = await kit.locator('.kui-canvas-region canvas').first().boundingBox()
  for (const delta of [-600, 600]) {
    // Each direction starts on the first page, so the second does not just scroll back.
    await kit.getByTestId('pdf-thumb-0').click()
    await until(async () => (await status()) === '第 1 页，共 2 页')
    await page.mouse.move(canvasBox.x + canvasBox.width / 2, canvasBox.y + canvasBox.height / 2)
    for (let i = 0; i < 30; i += 1) await page.mouse.wheel(delta, delta * 2)
    await page.waitForTimeout(500)
    const share = await pageShare()
    step(`scrolling ${delta < 0 ? 'before the first' : 'past the last'} page keeps the pages in view`, share > 0.3, share.toFixed(2))
  }
  await kit.getByTestId('pdf-thumb-0').click()
  await until(async () => (await status()) === '第 1 页，共 2 页')
  // The vertical scrollbar shows where the pages are and drags through them.
  const scrollbar = kit.locator('.kui-scrollbar[aria-orientation="vertical"]')
  const scrollMax = Number(await scrollbar.getAttribute('aria-valuemax'))
  await until(async () => Number(await scrollbar.getAttribute('aria-valuenow')) < scrollMax / 2)
  step('a vertical scrollbar spans the pages', scrollMax > 0 && Number(await scrollbar.getAttribute('aria-valuenow')) < scrollMax / 2, `${await scrollbar.getAttribute('aria-valuenow')} / ${scrollMax}`)
  const scrollThumb = await scrollbar.locator('.kui-scrollbar-thumb').boundingBox()
  const scrollTrack = await scrollbar.boundingBox()
  await page.mouse.move(scrollThumb.x + scrollThumb.width / 2, scrollThumb.y + 4)
  await page.mouse.down()
  await page.mouse.move(scrollThumb.x + scrollThumb.width / 2, scrollTrack.y + scrollTrack.height, { steps: 8 })
  await page.mouse.up()
  await until(async () => (await status()) === '第 2 页，共 2 页')
  step('dragging the scrollbar goes through the pages', (await status()) === '第 2 页，共 2 页', await status())
  await kit.getByTestId('pdf-thumb-0').click()
  await until(async () => (await status()) === '第 1 页，共 2 页')

  await kit.getByTestId('pdf-insert-page').click()
  await until(async () => (await status())?.endsWith('共 3 页'))
  step('inserts a page after the current one', (await status())?.endsWith('共 3 页'))
  await kit.getByTestId('pdf-thumb-1').click()
  await until(async () => (await status())?.startsWith('第 2 页'))
  await kit.getByTestId('pdf-delete-page').click()
  await until(async () => (await status())?.endsWith('共 2 页'))
  step('deletes the current page', (await status())?.endsWith('共 2 页'))
  await kit.getByTestId('pdf-thumb-0').click()
  await until(async () => (await status())?.startsWith('第 1 页'))
  await kit.getByTestId('pdf-rotate-right').click()
  await until(async () => { const area = (await details()).pages[0].area; return area.width > area.height })
  const area = (await details()).pages[0].area
  step('rotates the current page', area.width > area.height, JSON.stringify(area))
  // The rotation is the person's step: undo it in the person's history (an agent's undo takes back only its own steps, KitModel M12a).
  await page.evaluate(() => window.devHost.connection().ai.run([{ command: 'edit.undo', params: { scope: 'person' } }], false))

  await kit.getByTestId('pdf-sticky-note').click()
  step('the comment dialog starts on the current page', (await kit.locator('#param-page').inputValue()) === '1')
  await kit.locator('#param-contents').fill('Looks good')
  await kit.getByTestId('command-dialog-run').click()
  await kit.getByTestId('pdf-panel-comments').click()
  await until(async () => (await kit.getByTestId('pdf-comment').count()) === 3)
  step('the comment shows in the comments panel', (await kit.getByTestId('pdf-comment').count()) === 3)

  await kit.getByTestId('pdf-panel-bookmarks').click()
  await kit.getByTestId('pdf-bookmark').nth(1).click()
  await until(async () => (await status())?.startsWith('第 2 页'))
  step('a bookmark goes to its page', (await status())?.startsWith('第 2 页'))

  await kit.getByTestId('pdf-panel-pages').click()
  await kit.getByTestId('pdf-thumb-0').click()
  await until(async () => (await status())?.startsWith('第 1 页'))
  await kit.getByTestId('pdf-thumb-0').press('ControlOrMeta+f')
  await kit.getByTestId('pdf-find-text').fill('oxygen')
  await until(async () => (await kit.getByTestId('pdf-find-count').textContent()) === '1/2')
  await until(async () => (await status())?.startsWith('第 2 页'))
  step('find goes to the match', (await status())?.startsWith('第 2 页'))

  await kit.getByTestId('pdf-panel-fields').click()
  await kit.getByTestId('pdf-field-observer').fill('Ada')
  await kit.getByTestId('pdf-field-observer').press('Enter')
  await until(async () => (await ask('form.listFields')).fields[0].value === 'Ada')
  step('fills a form field', (await ask('form.listFields')).fields[0].value === 'Ada')

  const run = await page.evaluate(async () => window.devHost.connection().ai.run([{ command: 'bookmark.add', params: { page: 1, title: 'Agent' } }], false))
  await page.waitForTimeout(300)
  step('the agent edits the live document', run.completed[0].changed && (await page.evaluate(() => window.devHost.changes.at(-1).by)) === 'agent')
  await kit.getByTestId('pdf-save').click()
  await until(async () => (await page.evaluate(() => window.devHost.files.get('Sample.pdf').version)) > 1)
  const header = await page.evaluate(() => new TextDecoder().decode(new Uint8Array(window.devHost.files.get('Sample.pdf').bytes).slice(0, 5)))
  step('saves a PDF through the host', header === '%PDF-', header)
} catch (error) {
  failures += 1
  console.log(`FAIL ${error instanceof Error ? error.message.split('\n')[0] : String(error)}`)
} finally {
  if (errors.length) console.log(`page errors:\n${errors.slice(0, 8).join('\n')}`)
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
