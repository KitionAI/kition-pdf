// Zoom the way a reader expects (plan 2026-11-26 "PDF usable by hand", stage 1):
// node tests/hand/zoom.mjs [chromium|webkit]
// Real keys, wheel and pointer only. Cmd+0 and the status bar's Fit fit the current page, not every
// page at once (A10: 7 %); Cmd+1 is 100 %, Cmd+2 fits the width; Cmd+= and Cmd+- step the ladder
// from 10 % to 3200 %; a mouse wheel's notch with Cmd zooms 1.2 times around the pointer and a
// trackpad's small steps keep their feel (A3: 2.72 times a notch); the percentage takes a typed
// value and the list its steps (A9); Actual Size is in the Home tab's row, not under More (A7);
// Right scrolls sideways while the page is wider than the view (B6).
// Prints PASS or FAIL per step.
import { openHand } from './harness.mjs'

const which = process.argv[2] ?? 'chromium'
const hand = await openHand('hand-zoom', which)
const { page, kit, step, part, finish, mod, status, openFixture, pages, view, focused, surface, settle, press } = hand

const near = (a, b, within) => Math.abs(a - b) <= within
/** The current page (from the status bar) and whether all of it is in view. */
const pageShown = async () => {
  const number = Number(/^Page (\d+)/.exec(await status())?.[1] ?? 1)
  const area = (await pages())[number - 1].area
  const v = await view()
  const inside = area.y >= v.y - 0.5 && area.y + area.height <= v.y + v.height / v.zoom + 0.5 && area.x >= v.x - 0.5 && area.x + area.width <= v.x + v.width / v.zoom + 0.5
  // The zoom a whole page fits at (the page and a 16 pt rim, 32 px from the view's edges).
  const fit = Math.min((v.width - 64) / (area.width + 32), (v.height - 64) / (area.height + 32))
  return { inside, fit, zoom: v.zoom }
}
const zoom = async () => (await view()).zoom

try {
  await openFixture()

  await part('A10', async () => {
    await press('PageDown')
    await press(`${mod}+0`)
    let shown = await pageShown()
    await step('A10 Cmd+0 fits the current page', shown.inside && near(shown.zoom, shown.fit, shown.fit * 0.02), `zoom ${shown.zoom.toFixed(3)}, a page fits at ${shown.fit.toFixed(3)}`)
    await press(`${mod}+1`)
    await step('A10 Cmd+1 is 100 %', near(await zoom(), 1, 1e-6), String(await zoom()))
    await press(`${mod}+2`)
    const v = await view()
    const area = (await pages())[0].area
    await step('A10 Cmd+2 fits the page width', near(v.width / v.zoom, area.width + 48, (area.width + 48) * 0.06), `${(v.width / v.zoom).toFixed(1)} pt across for a ${area.width} pt page`)
    await kit.getByTestId('pdf-zoom-fit').click()
    await settle()
    shown = await pageShown()
    await step('A10 the status bar\'s Fit fits the current page', shown.inside && near(shown.zoom, shown.fit, shown.fit * 0.02), `zoom ${shown.zoom.toFixed(3)}`)
    await step('A10 the status bar shows the percentage', (await kit.getByTestId('pdf-zoom-field').inputValue()) === `${Math.round(shown.zoom * 100)}%`, await kit.getByTestId('pdf-zoom-field').inputValue())
  })

  await part('A2 ladder', async () => {
    await press(`${mod}+1`)
    await press(`${mod}+=`)
    await step('Cmd+= steps up the ladder', near(await zoom(), 1.25, 1e-6), String(await zoom()))
    await press(`${mod}+-`)
    await press(`${mod}+-`)
    await step('Cmd+- steps down the ladder', near(await zoom(), 0.75, 1e-6), String(await zoom()))
    for (let i = 0; i < 12; i += 1) await page.keyboard.press(`${mod}+-`)
    await settle()
    await step('Cmd+- stops at 10 %', near(await zoom(), 0.1, 1e-6), String(await zoom()))
    for (let i = 0; i < 20; i += 1) await page.keyboard.press(`${mod}+=`)
    await settle()
    await step('Cmd+= stops at 3200 %', near(await zoom(), 32, 1e-6), String(await zoom()))
    await press(`${mod}+1`)
  })

  await part('A3', async () => {
    // Fit the width first: zoomed in from there, the page is wider than the view (narrower, it stays centred).
    await press(`${mod}+2`)
    const box = await surface()
    const at = { x: box.x + box.width * 0.4, y: box.y + box.height * 0.45 }
    const docAt = (v) => ({ x: v.x + (at.x - box.x) / v.zoom, y: v.y + (at.y - box.y) / v.zoom })
    await page.mouse.move(at.x, at.y)
    const before = await view()
    await page.keyboard.down(mod)
    await page.mouse.wheel(0, -100)
    await page.keyboard.up(mod)
    await settle()
    const after = await view()
    const ratio = after.zoom / before.zoom
    const [p, q] = [docAt(before), docAt(after)]
    await step('A3 one mouse wheel notch with Cmd zooms 1.2 times', near(ratio, 1.2, 0.01), ratio.toFixed(3))
    await step('A3 the point under the pointer stays there', near(p.x, q.x, 1) && near(p.y, q.y, 1), `${p.x.toFixed(1)},${p.y.toFixed(1)} -> ${q.x.toFixed(1)},${q.y.toFixed(1)}`)
    await page.keyboard.down(mod)
    await page.mouse.wheel(0, 100)
    await page.keyboard.up(mod)
    await settle()
    await step('A3 a notch the other way zooms back', near(await zoom(), before.zoom, 0.01), String(await zoom()))
    // A trackpad (or a pinch) sends small steps: the canvas's own smooth zoom.
    const small = await zoom()
    await page.keyboard.down(mod)
    await page.mouse.wheel(0, -10)
    await page.keyboard.up(mod)
    await settle()
    const smooth = (await zoom()) / small
    await step('A3 a trackpad\'s small step zooms a little', smooth > 1.05 && smooth < 1.15, smooth.toFixed(3))
  })

  await part('A9', async () => {
    await kit.getByTestId('pdf-zoom-field').click()
    await page.keyboard.type('150')
    await press('Enter')
    await step('A9 a typed percentage sets the zoom', near(await zoom(), 1.5, 1e-6), String(await zoom()))
    await step('A9 after Enter in the zoom field, the pages have the keyboard', (await focused()) === 'pages', await focused())
    await kit.getByTestId('pdf-zoom-field').click()
    await page.keyboard.type('abc')
    await press('Enter')
    await step('A9 text that is no zoom changes nothing', near(await zoom(), 1.5, 1e-6) && (await kit.getByTestId('pdf-zoom-field').inputValue()) === '150%', await kit.getByTestId('pdf-zoom-field').inputValue())
    await kit.getByTestId('pdf-zoom-menu').click()
    await kit.getByTestId('pdf-zoom-200').click()
    await settle()
    await step('A9 the list sets a step', near(await zoom(), 2, 1e-6), String(await zoom()))
    await kit.getByTestId('pdf-zoom-menu').click()
    await kit.getByTestId('pdf-zoom-fit-page').click()
    await settle()
    const shown = await pageShown()
    await step('A9 the list fits the page', shown.inside && near(shown.zoom, shown.fit, shown.fit * 0.02), String(shown.zoom))
  })

  await part('A7', async () => {
    const actual = kit.getByTestId('pdf-actual-size')
    await step('A7 Actual Size is in the Home tab\'s row', await actual.isVisible(), 'not under More')
    await actual.click()
    await settle()
    await step('A7 Actual Size is 100 %', near(await zoom(), 1, 1e-6), String(await zoom()))
  })

  await part('B6 sideways', async () => {
    await kit.getByTestId('pdf-zoom-menu').click()
    await kit.getByTestId('pdf-zoom-300').click()
    await settle()
    // The keyboard back on the pages, the way a person does: a click on the page.
    const box = await surface()
    await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2)
    await press('Home')
    const before = await view()
    const at = await status()
    await press('ArrowRight')
    const after = await view()
    await step('B6 Right scrolls sideways 40 px while the page is wider than the view', near((after.x - before.x) * after.zoom, 40, 1) && (await status()) === at, `${((after.x - before.x) * after.zoom).toFixed(1)} px, ${await status()}`)
    await press('ArrowLeft')
    await step('B6 Left scrolls back', near((await view()).x, before.x, 0.5))
  })
} catch (error) {
  await step('ran to the end', false, error instanceof Error ? error.message.split('\n').slice(0, 6).join(' / ') : String(error))
}
await finish()
