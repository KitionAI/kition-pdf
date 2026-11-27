// The keyboard always works (plan 2026-11-26 "PDF usable by hand", stage 1):
// node tests/hand/keys.mjs [chromium|webkit]
// Real keys and pointer only, and no focus() before a key: right after a file opens PageDown and
// Cmd+F work (K1); after a parameters dialog ran, after Enter in the form fields panel and after
// Enter in the page field, Cmd+Z and PageDown work (K2, J2); a key that finds the body holding
// the keyboard still works; the arrows, Space, PageDown, Home, End, Cmd+arrows scroll as a
// reader's do (B6, B7); a page field that names no page shows the current one again (A12);
// Cmd+A selects every page's text and Cmd+C copies it with a ribbon button holding the keyboard
// (C8); with the find bar open an edit leaves the count and the view alone (D4), Cmd+G steps,
// Escape closes it wherever the keyboard is (D6), and Cmd+F takes the selected text (D8).
// Prints PASS or FAIL per step.
import { AURORA_PAGES, AURORA_PER_PAGE, PAGES } from './fixture.mjs'
import { openHand } from './harness.mjs'

const which = process.argv[2] ?? 'chromium'
const hand = await openHand('hand-keys', which)
const { page, kit, frame, step, until, part, finish, mod, status, openFixture, ask, pages, view, focused, settle, press } = hand

const near = (a, b, within = 1) => Math.abs(a - b) <= within
/** The pages' scroll bounds (every page, 24 pt round them), as the editor stops scrolling. */
const bounds = async () => {
  const list = (await pages()).map((item) => item.area)
  return { y0: Math.min(...list.map((a) => a.y)) - 24, y1: Math.max(...list.map((a) => a.y + a.height)) + 24 }
}
const comments = async () => (await ask('comment.list')).comments?.length ?? 0
/** The screen pixels the view moved down by since `before`. */
const moved = (before, after) => (after.y - before.y) * after.zoom

try {
  await openFixture()

  await part('K1', async () => {
    await step('K1 the pages have the keyboard as soon as the file opens', (await focused()) === 'pages', await focused())
    const before = await view()
    await press('PageDown')
    const after = await view()
    await step('K1 PageDown works before anything is clicked', moved(before, after) > 100, `${before.y.toFixed(1)} -> ${after.y.toFixed(1)}`)
    await page.keyboard.press(`${mod}+f`)
    const opened = await until(async () => (await kit.getByTestId('pdf-find-bar').count()) === 1, 40)
    await step('K1 Cmd+F opens the find bar before anything is clicked', opened && (await focused()) === 'pdf-find-text', await focused())
    await press('Escape')
    await step('K1 Escape in the find field closes it and the pages have the keyboard', (await kit.getByTestId('pdf-find-bar').count()) === 0 && (await focused()) === 'pages', await focused())
  })

  await part('B6', async () => {
    const { y0, y1 } = await bounds()
    await press('Home')
    let v = await view()
    await step('B6 Home goes to the top of the first page', near(v.y, y0, 0.5) && /^Page 1 of/.test(await status()), `${v.y.toFixed(1)} vs ${y0}`)
    let before = v
    await press('ArrowDown')
    v = await view()
    await step('B6 Down scrolls 40 px', near(moved(before, v), 40), moved(before, v).toFixed(1))
    before = v
    await press('ArrowUp')
    v = await view()
    await step('B6 Up scrolls 40 px back', near(moved(before, v), -40), moved(before, v).toFixed(1))
    before = v
    await press(' ')
    v = await view()
    await step('B6 Space scrolls a screen less a line', near(moved(before, v), v.height - 40), `${moved(before, v).toFixed(1)} of a ${v.height} px view`)
    before = v
    await press('Shift+ ')
    v = await view()
    await step('B6 Shift+Space scrolls a screen back', near(moved(before, v), -(v.height - 40)), moved(before, v).toFixed(1))
    await press('End')
    v = await view()
    await step('B6 End goes to the bottom of the last page', near(v.y + v.height / v.zoom, y1, 0.5) && (await status()) === `Page ${PAGES} of ${PAGES}`, `${(v.y + v.height / v.zoom).toFixed(1)} vs ${y1}, ${await status()}`)
    await press(`${mod}+ArrowUp`)
    v = await view()
    await step('B6 Cmd+Up goes to the top', near(v.y, y0, 0.5), v.y.toFixed(1))
    await press(`${mod}+ArrowDown`)
    v = await view()
    await step('B6 Cmd+Down goes to the bottom', near(v.y + v.height / v.zoom, y1, 0.5), v.y.toFixed(1))
    await press('Home')
    // The pages fit the view's width: Right and Left turn the page.
    await press('ArrowRight')
    await step('B6 Right turns to the next page when the page fits the width', (await status()) === `Page 2 of ${PAGES}`, await status())
    await press('ArrowLeft')
    await step('B6 Left turns back', (await status()) === `Page 1 of ${PAGES}`, await status())
  })

  await part('B7', async () => {
    await press('Home')
    const before = await view()
    await press('PageDown')
    const after = await view()
    await step('B7 PageDown scrolls a screen less a line, not to the next page\'s top', near(moved(before, after), after.height - 40), `${moved(before, after).toFixed(1)} of ${after.height}`)
    await press('PageUp')
    await step('B7 PageUp scrolls it back', near((await view()).y, before.y, 0.5))
  })

  await part('K2 dialog', async () => {
    await kit.getByTestId('pdf-sticky-note').click()
    await kit.locator('#param-contents').waitFor({ timeout: 10_000 })
    await kit.locator('#param-contents').click()
    await page.keyboard.type('Hand note')
    const count = await comments()
    await kit.getByTestId('command-dialog-run').click()
    await until(async () => (await comments()) === count + 1, 60)
    await until(async () => (await focused()) === 'pages', 20)
    await step('K2 after a parameters dialog ran, the pages have the keyboard', (await focused()) === 'pages', await focused())
    await page.keyboard.press(`${mod}+z`)
    const undone = await until(async () => (await comments()) === count, 60)
    await step('J2 Cmd+Z after the dialog takes the note back', undone, `${await comments()} comments`)
    const before = await view()
    await press('PageDown')
    await step('K2 PageDown after the dialog scrolls', moved(before, await view()) > 100)
    // Escape closes a dialog; the keyboard is the editor's after it.
    await kit.getByTestId('pdf-sticky-note').click()
    await kit.locator('#param-contents').waitFor({ timeout: 10_000 })
    await page.keyboard.press('Escape')
    await until(async () => (await kit.locator('#param-contents').count()) === 0, 30)
    await page.waitForTimeout(200)
    const where = await focused()
    await step('K2 after Escape closed a dialog, the editor has the keyboard', where !== 'body', where)
  })

  await part('K2 field', async () => {
    await kit.getByTestId('pdf-panel-fields').click()
    const field = kit.getByTestId('pdf-field-observer')
    await field.waitFor({ timeout: 10_000 })
    await field.click()
    await page.keyboard.type('Ada')
    await page.keyboard.press('Enter')
    const value = async () => (await ask('form.listFields')).fields?.find((item) => item.name === 'observer')?.value ?? null
    const filled = await until(async () => (await value()) === 'Ada', 60)
    await step('K2 Enter in the fields panel fills the field', filled, String(await value()))
    await until(async () => (await focused()) === 'pages', 20)
    await step('K2 after Enter in the fields panel, the pages have the keyboard', (await focused()) === 'pages', await focused())
    await page.keyboard.press(`${mod}+z`)
    const undone = await until(async () => (await value()) !== 'Ada', 60)
    await step('J2 Cmd+Z after the fields panel takes the value back', undone, String(await value()))
    await kit.getByTestId('pdf-panel-pages').click()
  })

  await part('K2 page field', async () => {
    await press('Home')
    const field = kit.getByTestId('pdf-page-field')
    await field.click()
    await page.keyboard.type('5')
    await press('Enter')
    await step('K2 the page field goes to page 5', (await status()) === `Page 5 of ${PAGES}`, await status())
    await step('K2 after Enter in the page field, the pages have the keyboard', (await focused()) === 'pages', await focused())
    const before = await view()
    await press('PageDown')
    await step('K2 PageDown after the page field scrolls', moved(before, await view()) > 100)
    await press('PageUp')
    // A12: text that names no page.
    await field.click()
    await page.keyboard.type('abc')
    await press('Enter')
    await step('A12 Enter on text that names no page shows the current page again', (await field.inputValue()) === '5' && (await status()) === `Page 5 of ${PAGES}`, `${await field.inputValue()}, ${await status()}`)
    await page.keyboard.type('xyz')
    await press('Escape')
    await step('A12 Escape in the page field shows the current page and gives the pages the keyboard', (await field.inputValue()) === '5' && (await focused()) === 'pages', `${await field.inputValue()}, ${await focused()}`)
  })

  await part('K2 body', async () => {
    // Setup: the keyboard lost to the body in a way nobody thought of.
    await frame().evaluate(() => { if (document.activeElement instanceof HTMLElement) document.activeElement.blur() })
    await step('K2 (setup) the body has the keyboard', (await focused()) === 'body')
    const before = await view()
    await press('PageDown')
    await step('K2 a key that finds the body still scrolls the pages', moved(before, await view()) > 100 && (await focused()) === 'pages', await focused())
  })

  await part('C8', async () => {
    await press('Home')
    await page.keyboard.press(`${mod}+a`)
    const shown = await until(async () => (await kit.getByTestId('pdf-selection-highlight').count()) > 0, 80)
    const onPages = await frame().evaluate(() => new Set([...document.querySelectorAll('[data-testid="pdf-selection-highlight"]')].map((item) => item.getAttribute('data-page'))).size)
    await step('C8 Cmd+A selects text', shown, `${onPages} pages highlighted in view`)
    // The keyboard on a ribbon button, then Cmd+C: the selection is what is copied.
    await frame().evaluate(() => {
      window.__handCopied = null
      window.addEventListener('copy', (event) => { window.__handCopied = { text: event.clipboardData?.getData('text/plain') ?? '', prevented: event.defaultPrevented } })
    })
    // A click on a side panel button takes the keyboard off the pages (to the button; in WebKit,
    // where a click focuses no button, to the editor's frame).
    await kit.getByTestId('pdf-panel-pages').click()
    const where = await focused()
    await step('C8 (setup) the keyboard is off the pages', where !== 'pages' && where !== 'body', where)
    await page.keyboard.press(`${mod}+c`)
    await until(async () => (await frame().evaluate(() => window.__handCopied)) != null, 30)
    const copied = await frame().evaluate(() => window.__handCopied)
    const text = copied?.text ?? ''
    await step('C8 Cmd+C with a button holding the keyboard copies the selection', copied?.prevented === true && text.includes('1.1 A quiet display'), JSON.stringify(text.slice(0, 60)))
    await step('C8 Select All took every page', text.includes(`${PAGES}.40 A quiet`) && text.includes('Page 1 heading'), `${text.length} characters`)
    await press('Escape')
    await step('C8 Escape clears the selection', (await kit.getByTestId('pdf-selection-highlight').count()) === 0)
  })

  await part('D4', async () => {
    await press('Home')
    await page.keyboard.press(`${mod}+f`)
    await kit.getByTestId('pdf-find-text').waitFor({ timeout: 5000 })
    await page.keyboard.type('aurora')
    const total = AURORA_PAGES.length * AURORA_PER_PAGE
    const count = () => kit.getByTestId('pdf-find-count').textContent()
    await until(async () => (await count()) === `1/${total}`, 80)
    for (let i = 0; i < 4; i += 1) await press('Enter')
    await until(async () => (await count()) === `5/${total}`, 40)
    await step('D4 (setup) the fifth match is current', (await count()) === `5/${total}`, await count())
    const before = await view()
    const at = await status()
    const notes = await comments()
    await kit.getByTestId('pdf-sticky-note').click()
    await kit.locator('#param-contents').waitFor({ timeout: 10_000 })
    await kit.locator('#param-contents').click()
    await page.keyboard.type('While finding')
    await kit.getByTestId('command-dialog-run').click()
    await until(async () => (await comments()) === notes + 1, 60)
    await page.waitForTimeout(1200)
    await settle()
    const after = await view()
    await step('D4 an edit while finding keeps the current match', (await count()) === `5/${total}`, await count())
    await step('D4 an edit while finding leaves the view where it was', near(after.y, before.y, 0.5) && (await status()) === at, `${before.y.toFixed(1)} -> ${after.y.toFixed(1)}, ${at} -> ${await status()}`)
    // D8: Cmd+G and Shift+Cmd+G with the pages holding the keyboard.
    await step('D6 (setup) the pages have the keyboard after the dialog', (await focused()) === 'pages', await focused())
    await press(`${mod}+g`)
    await step('D8 Cmd+G goes to the next match', (await count()) === `6/${total}`, await count())
    await press(`${mod}+Shift+g`)
    await step('D8 Shift+Cmd+G goes back', (await count()) === `5/${total}`, await count())
    await press('Escape')
    await step('D6 Escape with the pages holding the keyboard closes the find bar', (await kit.getByTestId('pdf-find-bar').count()) === 0)
  })

  await part('D8 seed', async () => {
    await press('Home')
    // Drag over the fifth line of page 1 (it says "aurora").
    const top = 792 - (690 - 15 * 4) - 4
    const from = await hand.toScreen(0, 70, top)
    const to = await hand.toScreen(0, 420, top)
    await page.mouse.move(from.x, from.y)
    await page.mouse.down()
    await page.mouse.move(to.x, to.y, { steps: 8 })
    await page.mouse.up()
    await until(async () => (await kit.getByTestId('pdf-selection-highlight').count()) > 0, 60)
    await page.keyboard.press(`${mod}+f`)
    await kit.getByTestId('pdf-find-text').waitFor({ timeout: 5000 })
    const seeded = await kit.getByTestId('pdf-find-text').inputValue()
    await step('D8 Cmd+F with text selected looks for that text', seeded.includes('1.5 A quiet aurora'), JSON.stringify(seeded))
    await until(async () => (await kit.getByTestId('pdf-find-count').textContent()) === '1/1', 60)
    await step('D8 the selected text is found', (await kit.getByTestId('pdf-find-count').textContent()) === '1/1', await kit.getByTestId('pdf-find-count').textContent())
    await press('Escape')
  })
} catch (error) {
  await step('ran to the end', false, error instanceof Error ? error.message.split('\n').slice(0, 6).join(' / ') : String(error))
}
await finish()
