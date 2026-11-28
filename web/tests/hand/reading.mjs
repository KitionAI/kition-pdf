// Reading by hand (plan 2026-11-26 "PDF usable by hand", stage 8):
// node tests/hand/reading.mjs [chromium|webkit]
// Real pointer and keys only. A double click selects a word and a triple click its line (C6); a
// Shift+click runs the selection on from its anchor, Shift with the arrows moves its end a character
// (Alt: a word) and back (C7); a drag held at the bottom of the view scrolls the pages and the
// selection follows (C10). With the find bar open every match on the pages in view is marked and
// only those (D5); a click off the text keeps the bar (D7); Match case and Whole words only search
// again, and an earlier query comes back with Up in the empty field (D8). Full screen (Cmd+L) shows
// the page whole, the arrows turn a page, Escape goes back to the zoom from before (A15). The
// selected text is read back with a copy event, the view through the canvas's handle. Prints PASS or FAIL
// per step.
import { AURORA_PAGES, AURORA_PER_PAGE } from './fixture.mjs'
import { openHand } from './harness.mjs'
import { dispatchCopy } from '../selection-probe.mjs'

const which = process.argv[2] ?? 'chromium'
const hand = await openHand('hand-reading', which)
const { page, kit, frame, step, until, part, finish, mod, status, openFixture, ask, pages, view, toScreen, settle, press } = hand

/** The middle of line `i` (1-based) of a fixture page, page-local points from the top-left. */
const lineMiddle = (i) => 792 - (690 - 15 * (i - 1)) - 4
const LINE_5 = '1.5 A quiet aurora grows over the hills before the rays and the curtain.'
const highlights = () => kit.getByTestId('pdf-selection-highlight').count()
const count = () => kit.getByTestId('pdf-find-count').textContent()

/**
 * The selected text, as the editor puts it on the clipboard: a copy event sent to where the keyboard
 * is (the pages), read as it is handled (WebKit on Linux sends no copy event for Ctrl+C on the pages
 * without a DOM selection, so the key is not used to read).
 */
async function copied() {
  return (await frame().evaluate(dispatchCopy)).text
}
/** Waits for a selection whose copied text passes `check`; answers the last text copied. */
async function selectionWhere(check) {
  let text = null
  await until(async () => {
    if ((await highlights()) === 0) return false
    text = await copied()
    return check(text ?? '')
  }, 40)
  return text ?? ''
}

try {
  await openFixture()
  // The whole of page 1 in view.
  await press(`${mod}+0`)
  // Where "aurora" is on page 1 (line 5): read before anything is selected.
  const found = await ask('text.find', { query: 'aurora' })
  const word = found.matches?.find((match) => match.page === 1)?.rects?.[0]
  if (!word) throw new Error(`text.find found no aurora on page 1: ${JSON.stringify(found).slice(0, 200)}`)
  const onWord = async () => toScreen(0, (word[0] + word[2]) / 2, (word[1] + word[3]) / 2)

  await part('C6', async () => {
    const at = await onWord()
    await page.mouse.dblclick(at.x, at.y)
    const text = await selectionWhere((value) => value === 'aurora')
    await step('C6 a double click selects the word', text === 'aurora', JSON.stringify(text))
    // Apart from the double click (quick presses at one place count on: two, three, then one again).
    await page.waitForTimeout(700)
    await page.mouse.click(at.x, at.y, { clickCount: 3 })
    const line = await selectionWhere((value) => value === LINE_5)
    await step('C6 a triple click selects the line', line === LINE_5, JSON.stringify(line))
    await press('Escape')
    await step('C6 Escape clears it', (await highlights()) === 0)
  })

  await part('C7', async () => {
    const at = await onWord()
    await page.mouse.dblclick(at.x, at.y)
    await selectionWhere((value) => value === 'aurora')
    await press('Shift+ArrowRight')
    await press('Shift+ArrowRight')
    const longer = await selectionWhere((value) => value.length > 6)
    await step('C7 Shift+Right moves the end on', longer.startsWith('aurora') && longer.length > 'aurora'.length && longer.length <= 'aurora gr'.length, JSON.stringify(longer))
    await press('Shift+ArrowLeft')
    await press('Shift+ArrowLeft')
    const back = await selectionWhere((value) => value === 'aurora')
    await step('C7 Shift+Left moves it back', back === 'aurora', JSON.stringify(back))
    await press('Alt+Shift+ArrowRight')
    const words = await selectionWhere((value) => value.includes('grows'))
    await step('C7 Alt+Shift+Right moves it a word', words.startsWith('aurora') && words.includes('grows') && !words.includes('over'), JSON.stringify(words))
    // Shift+click on line 7: the selection runs from the word's start to there.
    const to = await toScreen(0, 200, lineMiddle(7))
    await page.keyboard.down('Shift')
    await page.mouse.click(to.x, to.y)
    await page.keyboard.up('Shift')
    const run = await selectionWhere((value) => value.includes('1.7'))
    await step('C7 Shift+click runs the selection on from its anchor', run.startsWith('aurora grows') && run.includes('1.6 A quiet') && run.includes('1.7 A quiet'), JSON.stringify(run.slice(0, 120)))
    await press('Escape')
  })

  await part('C10', async () => {
    // Fit Width: a page is taller than the view.
    await press(`${mod}+2`)
    await press('Home')
    const before = await view()
    const box = await hand.surface()
    const from = await toScreen(0, 70, lineMiddle(2))
    await page.mouse.move(from.x, from.y)
    await page.mouse.down()
    await page.mouse.move(from.x + 200, box.y + box.height - 30, { steps: 6 })
    await page.mouse.move(from.x + 210, box.y + box.height - 6, { steps: 4 })
    // Held still near the bottom edge.
    await page.waitForTimeout(1500)
    const during = await view()
    await page.mouse.up()
    await settle()
    const moved = (during.y - before.y) * during.zoom
    await step('C10 a drag held at the bottom edge scrolls the pages', moved > 300, `${moved.toFixed(0)} px`)
    const text = await selectionWhere((value) => value.length > 0)
    const lines = text.split('\n').length
    await step('C10 the selection follows the scroll past what was in view', text.startsWith('1.2 A quiet') && lines > 20, `${lines} lines`)
    await press('Escape')
  })

  await part('D5 D7', async () => {
    await press(`${mod}+0`)
    await press('Home')
    await page.keyboard.press(`${mod}+f`)
    await kit.getByTestId('pdf-find-text').waitFor({ timeout: 5000 })
    await page.keyboard.type('aurora')
    const total = AURORA_PAGES.length * AURORA_PER_PAGE
    await until(async () => (await count()) === `1/${total}`, 80)
    await settle(400)
    const marks = () => frame().evaluate(() => [...document.querySelectorAll('[data-testid="pdf-search-match"]')].map((item) => Number(item.getAttribute('data-page'))))
    let shown = await marks()
    await step('D5 the other matches on the page in view are marked', shown.filter((p) => p === 1).length === AURORA_PER_PAGE - 1, JSON.stringify(shown))
    await step('D5 the current match is marked on its own', (await kit.getByTestId('pdf-search-highlight').count()) > 0)
    // Only pages in view are marked.
    const v = await view()
    const list = await pages()
    const inView = (p) => {
      const area = list[p - 1].area
      return area.y + area.height >= v.y && area.y <= v.y + v.height / v.zoom
    }
    await step('D5 no page out of view is marked', shown.length > 0 && shown.every(inView), JSON.stringify(shown))
    // A click in the left margin of page 1, off the text: the find bar stays.
    const margin = await toScreen(0, 24, lineMiddle(10))
    // Resting there first: the page layer reads the page's lines for the cursor.
    await page.mouse.move(margin.x, margin.y)
    await page.waitForTimeout(400)
    await page.mouse.click(margin.x, margin.y)
    await settle()
    await step('D7 a click off the text keeps the find bar', (await kit.getByTestId('pdf-find-bar').count()) === 1 && (await count()) === `1/${total}`, await count())
    shown = await marks()
    await step('D7 and its marks', shown.length === AURORA_PER_PAGE - 1, JSON.stringify(shown))
    await press(`${mod}+g`)
    await step('D7 Cmd+G still steps after the click', (await count()) === `2/${total}`, await count())
  })

  await part('D8', async () => {
    const input = kit.getByTestId('pdf-find-text')
    await input.click()
    await input.fill('PAGE 3 HEADING')
    await until(async () => (await count()) === '1/1', 60)
    await step('D8 (setup) the search ignores case', (await count()) === '1/1', await count())
    await kit.getByTestId('pdf-find-case').click()
    await until(async () => (await count()) === 'No matches', 60)
    await step('D8 Match case searches again', (await count()) === 'No matches', await count())
    await step('D8 Match case shows it is on', (await kit.getByTestId('pdf-find-case').getAttribute('aria-pressed')) === 'true')
    await kit.getByTestId('pdf-find-case').click()
    await input.click()
    await input.fill('hill')
    await until(async () => /^1\/\d+$/.test((await count()) ?? ''), 60)
    const parts = await count()
    await kit.getByTestId('pdf-find-whole').click()
    await until(async () => (await count()) === 'No matches', 60)
    await step('D8 Whole words only leaves out parts of words', /^1\/\d+$/.test(parts ?? '') && (await count()) === 'No matches', `${parts} -> ${await count()}`)
    await kit.getByTestId('pdf-find-whole').click()
    await until(async () => (await count()) === parts, 60)
    await input.click()
    await input.fill('aurora')
    await until(async () => /^1\//.test((await count()) ?? ''), 60)
    await input.press('Escape')
    await until(async () => (await kit.getByTestId('pdf-find-bar').count()) === 0, 30)
    await page.keyboard.press(`${mod}+f`)
    await input.waitFor({ timeout: 5000 })
    await step('D8 the find bar opens empty', (await input.inputValue()) === '')
    await page.keyboard.press('ArrowUp')
    await until(async () => (await input.inputValue()) === 'aurora', 20)
    await step('D8 Up brings back the last query', (await input.inputValue()) === 'aurora', await input.inputValue())
    await until(async () => /^1\//.test((await count()) ?? ''), 60)
    await step('D8 and finds it again', /^1\//.test((await count()) ?? ''), await count())
    await press('Escape')
  })

  await part('A15 full screen', async () => {
    await press(`${mod}+2`)
    await press('Home')
    const before = await view()
    const mode = () => kit.locator('.kui-app').getAttribute('data-mode')
    await press(`${mod}+l`)
    await until(async () => (await mode()) === 'focus', 30)
    await settle(600)
    const full = await frame().evaluate(() => !!document.fullscreenElement)
    // Focus shows no status bar: the page shown is the one whole in view, nearest its middle (-1: none).
    const whole = async () => {
      const v = await view()
      const list = (await pages()).map((item) => item.area)
      const middle = v.y + v.height / v.zoom / 2
      const index = list.findIndex((area) => area.y <= middle && area.y + area.height >= middle)
      const area = list[index]
      return area && area.y >= v.y - 1 && area.y + area.height <= v.y + v.height / v.zoom + 1 ? index : -1
    }
    await step('A15 Cmd+L enters full screen reading (Focus)', (await mode()) === 'focus', `browser full screen: ${full}`)
    await step('A15 the page is whole in view', (await whole()) === 0, JSON.stringify(await view()))
    await press('ArrowRight')
    await settle(400)
    await step('A15 Right turns to the next page, whole', (await whole()) === 1, JSON.stringify(await view()))
    await press('PageDown')
    await settle(400)
    await step('A15 Page Down turns another', (await whole()) === 2, JSON.stringify(await view()))
    await press('ArrowLeft')
    await settle(400)
    await step('A15 Left turns back', (await whole()) === 1, JSON.stringify(await view()))
    await press('Escape')
    await until(async () => (await mode()) !== 'focus', 30)
    await settle(600)
    const after = await view()
    await step('A15 Escape leaves full screen', (await mode()) !== 'focus' && !(await frame().evaluate(() => !!document.fullscreenElement)), await mode())
    await step('A15 back at the zoom from before', Math.abs(after.zoom - before.zoom) < 0.01, `${before.zoom.toFixed(3)} -> ${after.zoom.toFixed(3)}`)
  })
} catch (error) {
  await step('ran to the end', false, error instanceof Error ? error.message.split('\n').slice(0, 6).join(' / ') : String(error))
}
await finish()
