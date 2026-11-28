// An edit repaints only what it changed (plan 2026-11-26 "PDF usable by hand", stage 3; case
// issue-hand-e10-thumbnails-blank): node tests/hand/repaint.mjs [chromium|webkit]
// Real pointer and keys only, on the hand fixture. Page 3 turned from its thumbnail's menu, a
// rectangle drawn on page 1 with the rectangle tool, and the city field of page 4 filled on the page
// each draw that page's thumbnail again and no other visible one (the engine's thumbnail renders
// are counted), every visible thumbnail shows a picture in every animation frame meanwhile (the old
// one stays until the new one arrives), and no screenshot of the page canvas taken during the edit
// shows the pages gone white. Results are read back with inspect, comment.list and form.listFields.
// Prints PASS or FAIL per step.
import { handPdf } from './fixture.mjs'
import { openHand } from './harness.mjs'

const which = process.argv[2] ?? 'chromium'
const hand = await openHand('hand-repaint', which)
const { page, kit, frame, step, until, part, finish, mod, status, openFixture, ask, pages, toScreen, settle, press } = hand

/**
 * Counts the engine renders the thumbnails ask for (no wider than 132, no taller than 180 pixels)
 * by the document point they start at, and watches every visible thumbnail in each animation
 * frame: one without a picture (a 1-pixel canvas, or no dark pixel) is a blank.
 */
async function watch() {
  await frame().evaluate(async () => {
    const { KitEngine } = await import('/tests/sdk-editor.mjs')
    const probe = (window.repaintProbe ??= { calls: [], blanks: 0, frames: 0, watching: false })
    if (!probe.installed) {
      probe.installed = true
      const render = KitEngine.prototype.render
      KitEngine.prototype.render = function (...args) {
        if (args[3] <= 132 && args[4] <= 180) probe.calls.push({ x: args[0], y: args[1] })
        return render.apply(this, args)
      }
      const tick = () => {
        if (probe.watching) {
          probe.frames += 1
          for (const canvas of document.querySelectorAll('.pdf-thumb canvas')) {
            const box = canvas.getBoundingClientRect()
            const list = document.querySelector('[data-testid="pdf-thumbs"]').getBoundingClientRect()
            if (box.bottom < list.top || box.top > list.bottom) continue
            let dark = 0
            if (canvas.width > 1) {
              const data = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data
              for (let i = 0; i < data.length && dark === 0; i += 4) if (data[i] < 128 && data[i + 3] > 0) dark += 1
            }
            if (!dark) probe.blanks += 1
          }
        }
        requestAnimationFrame(tick)
      }
      requestAnimationFrame(tick)
    }
    probe.calls.length = 0
    probe.blanks = 0
    probe.frames = 0
    probe.watching = true
  })
}
/** What the watch saw: the pages (0-based) whose thumbnails were drawn, blanks, frames watched. */
async function seen({ stop = true } = {}) {
  const probe = await frame().evaluate((stop) => {
    const probe = window.repaintProbe
    if (stop) probe.watching = false
    return { calls: [...probe.calls], blanks: probe.blanks, frames: probe.frames }
  }, stop)
  const list = await pages()
  const drawn = probe.calls.map((call) => list.findIndex((item) => Math.abs(item.area.x - call.x) < 0.5 && Math.abs(item.area.y - call.y) < 0.5))
  return { drawn: drawn.sort((a, b) => a - b), blanks: probe.blanks, frames: probe.frames }
}
/** The thumbnails in the list's view (0-based pages). */
const visibleThumbs = () => frame().evaluate(() => {
  const list = document.querySelector('[data-testid="pdf-thumbs"]').getBoundingClientRect()
  return [...document.querySelectorAll('.pdf-thumb')].filter((element) => {
    const box = element.getBoundingClientRect()
    return box.bottom > list.top && box.top < list.bottom
  }).map((element) => Number(element.dataset.testid.replace('pdf-thumb-', '')))
})
/** Whether every visible thumbnail has its picture. */
const thumbsDrawn = () => frame().evaluate(() => [...document.querySelectorAll('.pdf-thumb canvas')].every((canvas) => canvas.width > 1))

/** Screenshots of the page canvas's middle while `edit` runs: the share of dark (text) pixels in each. */
async function frames(edit, ms = 1500) {
  const box = await kit.getByTestId('kit-canvas-surface').boundingBox()
  const clip = { x: box.x + box.width * 0.2, y: box.y + box.height * 0.15, width: box.width * 0.6, height: box.height * 0.7 }
  const shots = [await page.screenshot({ clip })]
  const started = Date.now()
  const running = edit()
  while (Date.now() - started < ms) shots.push(await page.screenshot({ clip }))
  await running
  return frame().evaluate(async (list) => {
    const out = []
    for (const b64 of list) {
      const image = await createImageBitmap(await (await fetch(`data:image/png;base64,${b64}`)).blob())
      const canvas = new OffscreenCanvas(image.width, image.height)
      const context = canvas.getContext('2d')
      context.drawImage(image, 0, 0)
      const data = context.getImageData(0, 0, image.width, image.height).data
      let dark = 0
      for (let i = 0; i < data.length; i += 4) if (data[i] + data[i + 1] + data[i + 2] < 384) dark += 1
      out.push(dark / (data.length / 4))
    }
    return out
  }, shots.map((shot) => shot.toString('base64')))
}
/** A frame "went white" when it shows less than a third of the text the frame before the edit did. */
const whiteFrames = (shares) => shares.slice(1).filter((share) => share < shares[0] / 3).length

/** Goes to page `number` with the page field and waits for the thumbnails to be drawn. */
async function showPage(number) {
  await kit.getByTestId('pdf-page-field').click()
  await page.keyboard.press(`${mod}+a`)
  await page.keyboard.type(String(number))
  await press('Enter')
  await until(async () => (await status()) === `Page ${number} of 8`, 40)
  await until(thumbsDrawn, 60)
  await page.waitForTimeout(600)
}
const others = (drawn, page) => drawn.filter((index) => index !== page)

try {
  await openFixture(handPdf())
  if ((await kit.getByTestId('pdf-thumbs').count()) === 0) await kit.getByTestId('pdf-panel-pages').click()
  await kit.getByTestId('pdf-thumb-2').waitFor({ timeout: 10_000 })
  await until(thumbsDrawn, 80)
  await page.waitForTimeout(800)

  await part('E10 rotate', async () => {
    const visible = await visibleThumbs()
    await step('E10 thumbnail 3 is in view with others', visible.includes(2) && visible.length >= 3, JSON.stringify(visible))
    await watch()
    const shares = await frames(async () => {
      const box = await kit.getByTestId('pdf-thumb-2').boundingBox()
      await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2, { button: 'right' })
      await kit.getByTestId('pdf-thumb-menu-rotate-right').click()
      await until(async () => (await pages())[2].area.width > (await pages())[2].area.height, 40)
      await until(async () => (await seen({ stop: false })).drawn.includes(2), 60)
      await page.waitForTimeout(800)
    })
    const result = await seen()
    const list = await pages()
    await step('E10 page 3 turned', list[2].area.width > list[2].area.height, JSON.stringify(list[2].area))
    await step('E10 the turned page\'s thumbnail was drawn again', result.drawn.includes(2), JSON.stringify(result.drawn))
    await step('E10 no other visible thumbnail was drawn again', others(result.drawn, 2).length === 0, JSON.stringify(result.drawn))
    await step('E10 no thumbnail went blank while it was drawn', result.blanks === 0 && result.frames > 5, `${result.blanks} blanks in ${result.frames} frames`)
    await step('E10 the pages never went white on screen', whiteFrames(shares) === 0, shares.map((share) => share.toFixed(3)).join(' '))
    const canvas = await kit.getByTestId('pdf-thumb-2').locator('canvas').evaluate((element) => [element.width, element.height])
    await step('E10 the new thumbnail is the turned page', canvas[0] > canvas[1], JSON.stringify(canvas))
  })

  await part('E10 comment', async () => {
    const before = (await ask('comment.list')).comments?.length ?? 0
    await kit.getByTestId('pdf-tool-rectangle').click()
    await watch()
    const shares = await frames(async () => {
      const a = await toScreen(0, 120, 200)
      const b = await toScreen(0, 260, 280)
      await page.mouse.move(a.x, a.y)
      await page.mouse.down()
      await page.mouse.move((a.x + b.x) / 2, (a.y + b.y) / 2, { steps: 8 })
      await page.mouse.move(b.x, b.y, { steps: 8 })
      await page.mouse.up()
      await until(async () => ((await ask('comment.list')).comments?.length ?? 0) > before, 40)
      await page.waitForTimeout(1200)
    })
    const result = await seen()
    await press('Escape')
    await press('v')
    const comments = (await ask('comment.list')).comments ?? []
    await step('E10 the rectangle was made on page 1', comments.length === before + 1 && comments.some((item) => item.type === 'Square' && item.page === 1), JSON.stringify(comments.map((item) => [item.type, item.page])))
    await step('E10 page 1\'s thumbnail was drawn again', result.drawn.includes(0), JSON.stringify(result.drawn))
    await step('E10 no other visible thumbnail was drawn again for the comment', others(result.drawn, 0).length === 0, JSON.stringify(result.drawn))
    await step('E10 no thumbnail went blank for the comment', result.blanks === 0, `${result.blanks} blanks in ${result.frames} frames`)
    await step('E10 the pages never went white for the comment', whiteFrames(shares) === 0, shares.map((share) => share.toFixed(3)).join(' '))
  })

  await part('E10 field', async () => {
    await showPage(4)
    await press(`${mod}+0`)
    await until(thumbsDrawn, 60)
    await page.waitForTimeout(800)
    await watch()
    const p = await toScreen(3, 460, 792 - 723)
    await page.mouse.click(p.x, p.y)
    await settle(150)
    await page.keyboard.type('Oslo')
    const shares = await frames(async () => {
      await page.keyboard.press('Tab')
      await until(async () => ((await ask('form.listFields')).fields ?? []).find((item) => item.name === 'city')?.value === 'Oslo', 60)
      await page.waitForTimeout(1200)
    })
    const result = await seen()
    const city = ((await ask('form.listFields')).fields ?? []).find((item) => item.name === 'city')?.value
    await step('E10 the field holds what was typed', city === 'Oslo', String(city))
    await step('E10 page 4\'s thumbnail was drawn again', result.drawn.includes(3), JSON.stringify(result.drawn))
    await step('E10 no other visible thumbnail was drawn again for the field', others(result.drawn, 3).length === 0, JSON.stringify(result.drawn))
    await step('E10 no thumbnail went blank for the field', result.blanks === 0, `${result.blanks} blanks in ${result.frames} frames`)
    await step('E10 the pages never went white for the field', whiteFrames(shares) === 0, shares.map((share) => share.toFixed(3)).join(' '))
  })

  await part('E10 undo', async () => {
    await page.keyboard.press('Escape')
    await watch()
    await press(`${mod}+z`)
    await until(async () => ((await ask('form.listFields')).fields ?? []).find((item) => item.name === 'city')?.value !== 'Oslo', 40)
    await page.waitForTimeout(1200)
    const result = await seen()
    await step('E10 an undo of the field draws page 4 only', others(result.drawn, 3).length === 0 && result.drawn.includes(3), JSON.stringify(result.drawn))
    await step('E10 and nothing went blank', result.blanks === 0, `${result.blanks} blanks in ${result.frames} frames`)
  })
} catch (error) {
  await step('ran to the end', false, error instanceof Error ? error.message : String(error))
}
await finish()
