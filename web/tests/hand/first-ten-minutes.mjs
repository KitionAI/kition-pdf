// The first ten minutes (plan 2026-11-26 "PDF usable by hand", stage 9): what a person used to PDF
// readers and markup tools does first, thirty things and six more, each done the way a person does
// it, with real pointer and keys only (node tests/hand/first-ten-minutes.mjs [chromium|webkit]).
// Each thing is one part: its actions are counted (one press, click, drag or typed value is one
// step; getting to a known place first is not counted) and set against the steps the reference
// takes, and what it should do is read back with inspect and read-only commands. The plan cut four
// of them (two-page and single-page layouts, turning the view, editing text that is already on a
// page, moving and scaling an image): they are listed as skipped-by-plan, not run. Saving and
// closing are the host's: Cmd+S writes through the host, Cmd+W goes to the host. The end prints a
// table: each thing, passed / failed / skipped-by-plan, its steps against the reference's.
// HAND_SHOTS=<folder> keeps a screenshot after each thing. Prints PASS or FAIL per step.
import { mkdirSync } from 'node:fs'
import { AURORA_PAGES, AURORA_PER_PAGE, MARKS, OUTLINE, PAGES, handPdf } from './fixture.mjs'
import { FILE, openHand } from './harness.mjs'
import { dispatchCopy } from '../selection-probe.mjs'

const which = process.argv[2] ?? 'chromium'
const hand = await openHand('hand-first-ten-minutes', which)
const { page, kit, frame, until, finish, mod, status, inspect, ask, pages, view, focused, surface, toScreen, settle, press } = hand

const LADDER = [0.1, 0.25, 0.33, 0.5, 0.66, 0.75, 1, 1.25, 1.5, 2, 3, 4, 6, 8, 12, 16, 32]
const near = (a, b, within = 1) => Math.abs(a - b) <= within
/** The middle of line `i` (1-based) of a fixture page, page-local points from the top-left. */
const lineMiddle = (i) => 792 - (690 - 15 * (i - 1)) - 4
/** A user-space rectangle of an unturned Letter page as the page-local one (y down). */
const local = ([x0, y0, x1, y1]) => [x0, 792 - y1, x1, 792 - y0]
const middle = (rect) => [(rect[0] + rect[2]) / 2, (rect[1] + rect[3]) / 2]
const comments = async () => (await ask('comment.list')).comments ?? []
const added = async (before) => (await comments()).filter((item) => !before.some((old) => old.id === item.id))
const fields = async () => (await ask('form.listFields')).fields ?? []
const value = async (name) => (await fields()).find((item) => item.name === name)?.value ?? null
const pressed = async (id) => (await kit.getByTestId(`pdf-tool-${id}`).getAttribute('aria-pressed')) === 'true'
const outline = () => kit.getByTestId('pdf-comment-outline')
const findCount = () => kit.getByTestId('pdf-find-count').textContent()
const highlights = () => kit.getByTestId('pdf-selection-highlight').count()
/** The pages' scroll bounds (every page, 24 pt round them). */
const bounds = async () => {
  const list = (await pages()).map((item) => item.area)
  return { y0: Math.min(...list.map((a) => a.y)) - 24, y1: Math.max(...list.map((a) => a.y + a.height)) + 24 }
}
/** The document point in the middle of the view. */
const centre = (v) => ({ x: v.x + v.width / v.zoom / 2, y: v.y + v.height / v.zoom / 2 })
/** The heading of each page, in order ("Page N heading" names the page it was made as). */
const order = async () => {
  const result = []
  for (let n = 1; n <= (await pages()).length; n += 1) result.push(Number(JSON.stringify(await ask('text.extract', { pages: [n] })).match(/Page (\d+) heading/)?.[1] ?? 0))
  return result
}
/** The text the pages would copy: a copy event sent to them, read as it is handled. */
const copiedNow = async () => (await frame().evaluate(dispatchCopy)).text ?? ''
const listenCopy = () => frame().evaluate(() => {
  window.__firstCopy = null
  if (!window.__firstCopyListening) {
    window.__firstCopyListening = true
    // Read while the event is handled (the editor's own listener, on the document, has written it).
    window.addEventListener('copy', (event) => { window.__firstCopy = { text: event.clipboardData?.getData('text/plain') ?? '' } })
  }
})
const lastCopy = () => frame().evaluate(() => window.__firstCopy)
/** Whether the host's document has unsaved changes. */
const dirty = () => page.evaluate(async () => { const answer = await window.devHost.connection().ai.inspect(); return answer.dirty ?? answer.details?.dirty })
/** The page (0-based) in the middle of the view, whole in view, or -1. */
const wholeInView = async () => {
  const v = await view()
  const list = (await pages()).map((item) => item.area)
  const mid = v.y + v.height / v.zoom / 2
  const index = list.findIndex((area) => area.y <= mid && area.y + area.height >= mid)
  const area = list[index]
  return area && area.y >= v.y - 1 && area.y + area.height <= v.y + v.height / v.zoom + 1 ? index : -1
}

// --- Places to start from (not counted). ---
async function toPage(number, fit = true) {
  await kit.getByTestId('pdf-page-field').click()
  await page.keyboard.press(`${mod}+a`)
  await page.keyboard.type(String(number))
  await press('Enter')
  if (fit) await press(`${mod}+0`)
  await until(async () => (await status()) === `Page ${number} of ${PAGES}`, 40)
}
/** Nothing open, nothing selected, Select on, the pages holding the keyboard. */
async function calm() {
  for (let i = 0; i < 3; i += 1) await page.keyboard.press('Escape')
  if (!(await pressed('select'))) await kit.getByTestId('pdf-tool-select').click()
  const box = await surface()
  // A click on the grey round the page gives the pages the keyboard and selects nothing.
  await page.mouse.click(box.x + 6, box.y + box.height - 6)
  await settle(200)
}

// --- The table. ---
const results = []
let current = null
const check = async (name, ok, detail = '') => {
  if (current && !ok) current.failed += 1
  if (current && !ok) current.why.push(name)
  await hand.step(`${current ? `N${current.n} ` : ''}${name}`, ok, detail)
}
/**
 * A known difference from the reference that the plan leaves (or has not reached): printed PASS
 * or GAP, never a failure; the table lists the gaps it saw.
 */
const known = async (name, ok, detail = '') => {
  if (current && !ok) current.gaps.push(name)
  console.log(`${ok ? 'PASS' : 'GAP'} ${current ? `N${current.n} ` : ''}${name}${detail ? ` - ${detail}` : ''}`)
}
/** One person's action: counted for the thing being done. */
const act = async (body, steps = 1) => {
  if (current) current.steps += steps
  return body()
}
const shotDir = process.env.HAND_SHOTS ? `${process.env.HAND_SHOTS.replace(/\/$/, '')}/` : null
if (shotDir) mkdirSync(shotDir, { recursive: true })

async function thing(n, title, reference, body, { note = '' } = {}) {
  current = { n, title, reference, steps: 0, failed: 0, why: [], gaps: [], note }
  try {
    await body()
  } catch (error) {
    await check('ran to the end', false, error instanceof Error ? error.message.split('\n').slice(0, 4).join(' / ') : String(error))
  }
  if (shotDir) await page.screenshot({ path: `${shotDir}n${n}-${which}.png` }).catch(() => undefined)
  results.push({ ...current, result: current.failed ? 'failed' : current.gaps.length ? 'passed, with gaps' : 'passed' })
  current = null
}
function cut(n, title, reference, why) {
  console.log(`SKIPPED-BY-PLAN N${n} ${title} - ${why}`)
  results.push({ n, title, reference, steps: null, result: 'skipped-by-plan', why: [why], gaps: [], note: '' })
}

// --- Gestures, each one step. ---
const dragPoints = async (a, b, { steps = 10, shift = false } = {}) => {
  await page.mouse.move(a.x, a.y)
  if (shift) await page.keyboard.down('Shift')
  await page.mouse.down()
  await page.mouse.move((a.x + b.x) / 2, (a.y + b.y) / 2 + 3, { steps })
  await page.mouse.move(b.x, b.y, { steps })
  await page.mouse.up()
  if (shift) await page.keyboard.up('Shift')
}
const dragOn = async (index, from, to, options) => dragPoints(await toScreen(index, ...from), await toScreen(index, ...to), options)
const clickOn = async (index, at, options) => {
  const p = await toScreen(index, ...at)
  await page.mouse.click(p.x, p.y, options)
}
const selectLine = async (index, line, to = 400) => {
  await dragOn(index, [70, lineMiddle(line)], [to, lineMiddle(line)], { steps: 6 })
  await until(async () => (await highlights()) > 0, 60)
}

try {
  // N1: open a PDF.
  await thing(1, 'Open a PDF', '1–2', async () => {
    await page.evaluate(() => { document.getElementById('status').textContent = 'opening…' })
    await act(() => page.setInputFiles('#file', { name: FILE, mimeType: 'application/pdf', buffer: handPdf({ marks: true, outline: true }) }))
    await page.waitForFunction((name) => document.getElementById('status')?.textContent?.includes(`${name} open in`), FILE, { timeout: 90_000 })
    await until(async () => /^Page 1 of \d+$/.test(await status()), 200)
    await until(async () => (await view()) != null, 100)
    await settle(800)
    const v = await view()
    const area = (await pages())[0].area
    await check('the file opens at its first page', (await status()) === `Page 1 of ${PAGES}`, await status())
    await check('the page fits the view\'s width', near(v.width / v.zoom, area.width + 48, (area.width + 48) * 0.06), `${(v.width / v.zoom).toFixed(0)} pt across a ${area.width} pt page`)
    await check('the first page\'s top is at the view\'s top', near(v.y, area.y - 24, 2), `${v.y.toFixed(1)} vs ${area.y - 24}`)
    await check('Select is the tool', await pressed('select'))
    await check('the pages have the keyboard', (await focused()) === 'pages', await focused())
    await check('a file with bookmarks opens with them shown (R-F7)', (await kit.getByTestId('pdf-bookmarks').count()) === 1)
  })

  // N2: scroll with the wheel.
  await thing(2, 'Scroll with the wheel', '1', async () => {
    const box = await surface()
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
    const before = await view()
    await act(() => page.mouse.wheel(0, 100))
    await settle()
    const after = await view()
    await check('a wheel notch scrolls the pages down', after.y > before.y && near((after.y - before.y) * after.zoom, 100, 2), `${((after.y - before.y) * after.zoom).toFixed(1)} px`)
    await act(() => page.mouse.wheel(0, -6000))
    await settle()
    await check('wheeling back stops at the top', near((await view()).y, (await bounds()).y0, 0.5), `${(await view()).y.toFixed(1)}`)
  }, { note: 'a notch is the browser\'s wheel delta (100 px here), not 40 pt' })

  // N3: turn with the keyboard.
  await thing(3, 'Page with the keyboard', '1', async () => {
    let before = await view()
    await act(() => press('ArrowDown'))
    let after = await view()
    await check('Down scrolls 40 px', near((after.y - before.y) * after.zoom, 40), ((after.y - before.y) * after.zoom).toFixed(1))
    before = after
    await act(() => press('PageDown'))
    after = await view()
    await check('Page Down scrolls a screen less a line', near((after.y - before.y) * after.zoom, after.height - 40), `${((after.y - before.y) * after.zoom).toFixed(1)} of ${after.height}`)
    before = after
    await act(() => press('PageUp'))
    after = await view()
    await check('Page Up scrolls it back', near((after.y - before.y) * after.zoom, -(after.height - 40)), ((after.y - before.y) * after.zoom).toFixed(1))
  })

  // N4: go to page N.
  await thing(4, 'Go to page N', '3', async () => {
    const field = kit.getByTestId('pdf-page-field')
    await act(() => field.click())
    await act(() => page.keyboard.type('5'))
    await act(() => press('Enter'))
    const v = await view()
    const area = (await pages())[4].area
    await check('the page field goes to page 5', (await status()) === `Page 5 of ${PAGES}`, await status())
    await check('page 5\'s top is at the view\'s top', area.y >= v.y && area.y - v.y < 40, `${(area.y - v.y).toFixed(1)} pt below`)
    await check('the field shows the page, the status bar the count', (await field.inputValue()) === '5' && (await status()).endsWith(`of ${PAGES}`), await field.inputValue())
  }, { note: 'no Shift+Cmd+N; the page field is a click away' })

  // N5: Home and End.
  await thing(5, 'Go to the start and the end', '1', async () => {
    const { y0, y1 } = await bounds()
    await act(() => press('End'))
    let v = await view()
    await check('End goes to the last page\'s bottom', near(v.y + v.height / v.zoom, y1, 0.5) && (await status()) === `Page ${PAGES} of ${PAGES}`, await status())
    await act(() => press('Home'))
    v = await view()
    await check('Home goes to the first page\'s top', near(v.y, y0, 0.5) && (await status()) === `Page 1 of ${PAGES}`, await status())
  })

  // N6: zoom in and out with the keys.
  await thing(6, 'Zoom in and out', '1', async () => {
    const before = await view()
    await act(() => press(`${mod}+=`))
    const after = await view()
    const next = LADDER.find((step) => step > before.zoom + 1e-6)
    await check('Cmd+= steps up to the next step of the ladder', near(after.zoom, next, 1e-6), `${before.zoom.toFixed(3)} -> ${after.zoom}`)
    const [a, b] = [centre(before), centre(after)]
    await check('the view\'s middle stays where it was', near(a.x, b.x, 2) && near(a.y, b.y, 2), `${a.x.toFixed(1)},${a.y.toFixed(1)} -> ${b.x.toFixed(1)},${b.y.toFixed(1)}`)
    await act(() => press(`${mod}+-`))
    const back = await view()
    await check('Cmd+- steps down the ladder', near(back.zoom, LADDER.filter((step) => step < after.zoom - 1e-6).at(-1), 1e-6), String(back.zoom))
  })

  // N7: zoom about a point with Cmd and the wheel.
  await thing(7, 'Zoom about a point', '1', async () => {
    const box = await surface()
    const at = { x: box.x + box.width * 0.35, y: box.y + box.height * 0.4 }
    const docAt = (v) => ({ x: v.x + (at.x - box.x) / v.zoom, y: v.y + (at.y - box.y) / v.zoom })
    await page.mouse.move(at.x, at.y)
    const before = await view()
    await act(async () => {
      await page.keyboard.down(mod)
      await page.mouse.wheel(0, -100)
      await page.keyboard.up(mod)
    })
    await settle()
    const after = await view()
    const [p, q] = [docAt(before), docAt(after)]
    await check('Cmd and a wheel notch zoom in', after.zoom > before.zoom * 1.1, (after.zoom / before.zoom).toFixed(3))
    await check('the point under the pointer stays under it', near(p.x, q.x, 1) && near(p.y, q.y, 1), `${p.x.toFixed(1)},${p.y.toFixed(1)} -> ${q.x.toFixed(1)},${q.y.toFixed(1)}`)
  })

  // N8: fit width, fit page, 100 %; a fit stays while the window changes.
  await thing(8, 'Fit width, fit page, 100 %', '1', async () => {
    const area = (await pages())[0].area
    await act(() => press(`${mod}+1`))
    await check('Cmd+1 is 100 %', near((await view()).zoom, 1, 1e-6), String((await view()).zoom))
    await act(() => press(`${mod}+0`))
    let v = await view()
    const fit = Math.min((v.width - 64) / (area.width + 32), (v.height - 64) / (area.height + 32))
    await check('Cmd+0 fits the page', near(v.zoom, fit, fit * 0.03), `${v.zoom.toFixed(3)} vs ${fit.toFixed(3)}`)
    await act(() => press(`${mod}+2`))
    v = await view()
    await check('Cmd+2 fits the width', near(v.width / v.zoom, area.width + 48, (area.width + 48) * 0.06), `${(v.width / v.zoom).toFixed(1)} pt across`)
    const size = page.viewportSize()
    await page.setViewportSize({ width: size.width - 300, height: size.height })
    await settle(600)
    v = await view()
    await check('a narrower window keeps the width fitted', near(v.width / v.zoom, area.width + 48, (area.width + 48) * 0.06), `${(v.width / v.zoom).toFixed(1)} pt across a ${v.width} px view`)
    await page.setViewportSize(size)
    await settle(600)
    await act(() => press(`${mod}+0`))
    await page.setViewportSize({ width: size.width, height: size.height - 200 })
    await settle(600)
    v = await view()
    const shown = (await pages())[Number(/^Page (\d+)/.exec(await status())?.[1] ?? 1) - 1].area
    const fitNow = Math.min((v.width - 64) / (shown.width + 32), (v.height - 64) / (shown.height + 32))
    await check('a shorter window keeps the page fitted', near(v.zoom, fitNow, fitNow * 0.03), `${v.zoom.toFixed(3)} vs ${fitNow.toFixed(3)}`)
    await page.setViewportSize(size)
    await settle(600)
    await press(`${mod}+1`)
    await page.setViewportSize({ width: size.width - 300, height: size.height })
    await settle(600)
    await check('100 % stays 100 % when the window changes', near((await view()).zoom, 1, 1e-6), String((await view()).zoom))
    await page.setViewportSize(size)
    await settle(600)
  })

  // N9: pan with Space held, or the hand.
  await thing(9, 'Pan the page', '1–2', async () => {
    await press(`${mod}+2`)
    await press('Home')
    const box = await surface()
    const from = { x: box.x + box.width / 2, y: box.y + box.height * 0.7 }
    let before = await view()
    await act(async () => {
      await page.keyboard.down(' ')
      await dragPoints(from, { x: from.x, y: from.y - 200 })
      await page.keyboard.up(' ')
    })
    await settle()
    let after = await view()
    await check('a drag with Space held moves the pages with the pointer', near((after.y - before.y) * after.zoom, 200, 12), `${((after.y - before.y) * after.zoom).toFixed(1)} px`)
    await check('Select is still the tool after Space', await pressed('select'))
    await act(() => press('h'))
    before = await view()
    await act(() => dragPoints(from, { x: from.x, y: from.y - 150 }))
    await settle()
    after = await view()
    await check('H, then a drag pans', (await pressed('hand')) && near((after.y - before.y) * after.zoom, 150, 12), `${((after.y - before.y) * after.zoom).toFixed(1)} px`)
    await act(() => press('v'))
    await check('V goes back to Select', await pressed('select'))
  })

  cut(10, 'Two pages, or one page at a time', '2', 'plan section 13, stage 8: single page and two-page layouts cut')
  cut(11, 'Turn the view a quarter', '1', 'plan section 13, stage 8: turning the view cut')

  // N12: select some text and copy it.
  await thing(12, 'Select text and copy it', '2', async () => {
    await calm()
    await toPage(1)
    await listenCopy()
    await act(() => selectLine(0, 7, 520))
    await until(async () => (await kit.getByTestId('pdf-selection-bar').count()) === 1, 40)
    const bar = []
    for (const name of ['comment', 'highlight', 'underline', 'strikeout', 'copy']) bar.push(await kit.getByTestId(`pdf-bar-${name}`).count())
    await check('the selection bar shows: Add comment, Highlight, Underline, Strikethrough, Copy', bar.every((count) => count === 1), bar.join(','))
    await act(() => page.keyboard.press(`${mod}+c`))
    await until(async () => (await lastCopy()) != null, 20)
    let text = (await lastCopy())?.text ?? null
    // WebKit on Linux sends the pages no copy event for Ctrl+C (stage 8): read what a copy gives.
    const sent = text != null
    if (!sent) text = await copiedNow()
    await check('Cmd+C copies the selected line', text === '1.7 A quiet display grows over the hills before the rays and the curtain.', `${JSON.stringify(text)}${sent ? '' : ' (no copy event for the key; read with one sent to the pages)'}`)
  }, { note: 'Replace text is not on the bar (editing page text was cut)' })

  // N13: a word and a line.
  await thing(13, 'Select a word, a line', '1', async () => {
    await press('Escape')
    const found = await ask('text.find', { query: 'aurora' })
    const word = found.matches?.find((match) => match.page === 1)?.rects?.[0]
    const at = await toScreen(0, (word[0] + word[2]) / 2, (word[1] + word[3]) / 2)
    await act(() => page.mouse.dblclick(at.x, at.y))
    await until(async () => (await highlights()) > 0 && (await copiedNow()) === 'aurora', 40)
    await check('a double click selects the word', (await copiedNow()) === 'aurora', JSON.stringify(await copiedNow()))
    await page.waitForTimeout(700)
    await act(() => page.mouse.click(at.x, at.y, { clickCount: 3 }))
    await until(async () => (await copiedNow()).startsWith('1.5 A quiet aurora'), 40)
    await check('a triple click selects the line', (await copiedNow()) === '1.5 A quiet aurora grows over the hills before the rays and the curtain.', JSON.stringify(await copiedNow()))
  })

  // N14: select all and copy.
  await thing(14, 'Select all and copy', '2', async () => {
    await press('Escape')
    await listenCopy()
    await act(() => page.keyboard.press(`${mod}+a`))
    await until(async () => (await highlights()) > 0, 80)
    await act(() => page.keyboard.press(`${mod}+c`))
    await until(async () => (await lastCopy()) != null, 20)
    const text = (await lastCopy())?.text ?? (await copiedNow())
    await check('Cmd+A, Cmd+C copies every page\'s text', text.includes('Page 1 heading') && text.includes(`Page ${PAGES} heading`) && text.includes(`${PAGES - 1}.40 A quiet`), `${text.length} characters`)
    await press('Escape')
  }, { note: 'whole document up to 64 pages (the engine\'s text.select limit), the current page beyond' })

  // N15: find a word.
  const total = AURORA_PAGES.length * AURORA_PER_PAGE
  await thing(15, 'Find a word', '2', async () => {
    await press('Home')
    await act(() => page.keyboard.press(`${mod}+f`))
    await kit.getByTestId('pdf-find-text').waitFor({ timeout: 5000 })
    await act(() => page.keyboard.type('aurora'))
    await until(async () => (await findCount()) === `1/${total}`, 80)
    await settle(400)
    await check('the bar finds as it is typed and counts the matches', (await findCount()) === `1/${total}`, await findCount())
    const box = await kit.getByTestId('pdf-search-highlight').first().boundingBox()
    const area = await surface()
    await check('the first match is in view, marked on its own', !!box && box.y > area.y && box.y + box.height < area.y + area.height, JSON.stringify(box))
    await check('the other matches in view are marked too', (await kit.getByTestId('pdf-search-match').count()) > 0)
  })

  // N16: next, previous, close.
  await thing(16, 'Next and previous match, then close', '1', async () => {
    await act(() => press('Enter'))
    await check('Enter goes to the next match', (await findCount()) === `2/${total}`, await findCount())
    await check('the find field keeps the keyboard', (await focused()) === 'pdf-find-text', await focused())
    await act(() => press('Shift+Enter'))
    await act(() => press('Shift+Enter'))
    await check('Shift+Enter goes back, and round from the first to the last', (await findCount()) === `${total}/${total}`, await findCount())
    await act(() => press(`${mod}+g`))
    await check('Cmd+G goes on (round to the first)', (await findCount()) === `1/${total}`, await findCount())
    await act(() => press('Escape'))
    await check('Escape closes the bar and its marks', (await kit.getByTestId('pdf-find-bar').count()) === 0 && (await kit.getByTestId('pdf-search-match').count()) === 0 && (await kit.getByTestId('pdf-search-highlight').count()) === 0)
  })

  // N17: go to a page from its thumbnail.
  await thing(17, 'Go to a page by its thumbnail', '2', async () => {
    await act(() => kit.getByTestId('pdf-panel-pages').click())
    await kit.getByTestId('pdf-thumb-3').waitFor({ timeout: 10_000 })
    await act(() => kit.getByTestId('pdf-thumb-3').click())
    await until(async () => (await status()) === `Page 4 of ${PAGES}`, 40)
    await settle()
    const v = await view()
    const area = (await pages())[3].area
    await check('a click on thumbnail 4 goes to page 4\'s top', (await status()) === `Page 4 of ${PAGES}` && area.y >= v.y && area.y - v.y < 40, `${await status()}, ${(area.y - v.y).toFixed(1)} pt`)
    await check('the current page\'s thumbnail is marked', (await kit.getByTestId('pdf-thumb-3').getAttribute('data-current')) === 'true')
  })

  // N18: bookmarks.
  await thing(18, 'Go to a bookmark', '2', async () => {
    await act(() => kit.getByTestId('pdf-panel-bookmarks').click())
    const row = kit.getByTestId('pdf-bookmark').filter({ hasText: OUTLINE[0].children[1].title })
    await row.waitFor({ timeout: 10_000 })
    await act(() => row.click())
    await until(async () => (await status()) === `Page ${OUTLINE[0].children[1].page} of ${PAGES}`, 40)
    await check('a click on a bookmark goes to its page', (await status()) === `Page ${OUTLINE[0].children[1].page} of ${PAGES}`, await status())
  }, { note: 'to the page\'s top at the current zoom: bookmark.list answers no place or zoom' })

  // N19: a link and back.
  await thing(19, 'Follow a link and come back', '2', async () => {
    await calm()
    await toPage(1)
    await act(() => clickOn(0, middle(local(MARKS.pageLink.rect))))
    await until(async () => (await status()) === `Page ${MARKS.pageLink.to} of ${PAGES}`, 40)
    await check('a click on the link goes to its page', (await status()) === `Page ${MARKS.pageLink.to} of ${PAGES}`, await status())
    await act(() => press(`${mod}+[`))
    await until(async () => (await status()) === `Page 1 of ${PAGES}`, 40)
    await check('Cmd+[ goes back to the page the link was on', (await status()) === `Page 1 of ${PAGES}`, await status())
  })

  // N20: highlight what is selected.
  await thing(20, 'Highlight the selected text', '2', async () => {
    await toPage(1)
    const before = await comments()
    await act(() => selectLine(0, 9))
    await until(async () => (await kit.getByTestId('pdf-selection-bar').count()) === 1, 40)
    await act(() => kit.getByTestId('pdf-bar-highlight').click())
    await until(async () => (await added(before)).length === 1, 60)
    const made = await added(before)
    await check('a yellow highlight over the line', made.length === 1 && made[0].type === 'Highlight' && made[0].rect[1] < lineMiddle(9) && made[0].rect[3] > lineMiddle(9), JSON.stringify(made.map((item) => [item.type, item.color, item.rect])))
    await check('the selection is cleared, Select is still the tool', (await highlights()) === 0 && (await pressed('select')))
    await press(`${mod}+z`)
    await until(async () => (await added(before)).length === 0, 60)
    await check('one Cmd+Z takes it back', (await added(before)).length === 0)
    await press(`${mod}+Shift+z`)
    await until(async () => (await added(before)).length === 1, 60)
  })

  // N21: highlight several places in a row.
  await thing(21, 'Highlight several places', '1+N', async () => {
    const before = await comments()
    await act(() => kit.getByTestId('pdf-tool-highlight').click())
    for (const line of [11, 13, 15]) await act(() => dragOn(0, [70, lineMiddle(line)], [400, lineMiddle(line)]))
    await until(async () => (await added(before)).length === 3, 80)
    const made = await added(before)
    await check('each drag makes one highlight on its line', made.length === 3 && [11, 13, 15].every((line) => made.some((item) => item.rect[1] < lineMiddle(line) && item.rect[3] > lineMiddle(line))), JSON.stringify(made.map((item) => item.rect)))
    await check('the highlighter stays on', await pressed('highlight'))
    await act(() => press('Escape'))
    await act(() => press('Escape'))
    await check('Escape twice goes back to Select', await pressed('select'))
    await known('the Comments panel opens, the new highlight\'s note ready to type in', (await kit.getByTestId('pdf-panel-comments').getAttribute('aria-pressed')) === 'true')
  })

  // N22: a sticky note.
  await thing(22, 'Put a sticky note', '4', async () => {
    const before = await comments()
    await act(() => kit.getByTestId('pdf-tool-note').click())
    await act(() => clickOn(0, [470, 300]))
    await kit.getByTestId('pdf-note-text').waitFor({ timeout: 5000 })
    await check('a card to write in comes first, nothing is made yet', (await added(before)).length === 0)
    await act(() => page.keyboard.insertText('第一次的便签'))
    await act(() => kit.getByTestId('pdf-note-post').click())
    await until(async () => (await added(before)).length === 1, 60)
    const note = (await added(before))[0]
    await check('Post makes the note where the page was clicked, with its text', !!note && note.type === 'Text' && near(note.rect[0], 470, 2) && near(note.rect[1], 300, 2) && note.contents === '第一次的便签', JSON.stringify(note && [note.type, note.rect, note.contents]))
    await check('the tool goes back to Select', await pressed('select'))
    await press('Escape')
    await kit.getByTestId('pdf-tool-note').click()
    await clickOn(0, [470, 360])
    await kit.getByTestId('pdf-note-text').waitFor({ timeout: 5000 })
    await page.keyboard.type('never')
    await page.keyboard.press('Escape')
    await settle(500)
    await check('Escape in the card leaves nothing', (await added(before)).length === 1)
    await press('Escape')
  })

  // N23: a rectangle and an arrow.
  let square = null
  await thing(23, 'Draw a rectangle, an arrow', '2', async () => {
    let before = await comments()
    await act(() => kit.getByTestId('pdf-tool-rectangle').click())
    await act(() => dragOn(0, [100, 440], [220, 520]))
    await until(async () => (await added(before)).length === 1, 60)
    square = (await added(before))[0]
    await check('a drag makes the rectangle', square?.type === 'Square' && near(square.rect[0], 100, 3) && near(square.rect[3], 520, 3), JSON.stringify(square?.rect))
    await until(async () => (await kit.getByTestId('pdf-comment-handle').count()) === 8, 30)
    await check('the new rectangle is selected with its handles; the tool stays', (await kit.getByTestId('pdf-comment-handle').count()) === 8 && (await pressed('rectangle')))
    before = await comments()
    await act(() => kit.getByTestId('pdf-tool-arrow').click())
    await act(() => dragOn(0, [300, 460], [440, 470], { shift: true }))
    await until(async () => (await added(before)).length === 1, 60)
    const arrow = (await added(before))[0]
    // The arrow's rectangle takes its head in: level, it is centred on the line pressed on (y 460).
    await check('Shift keeps the arrow level', arrow?.type === 'Line' && near((arrow.rect[1] + arrow.rect[3]) / 2, 460, 1) && arrow.rect[2] - arrow.rect[0] > 120, JSON.stringify(arrow?.rect))
    await press('Escape')
    await press('Escape')
  })

  // N24: write a few strokes.
  await thing(24, 'Write a few strokes', '1+N', async () => {
    const before = await comments()
    await act(() => kit.getByTestId('pdf-tool-ink').click())
    await act(() => dragOn(0, [100, 560], [160, 600], { steps: 8 }))
    await act(() => dragOn(0, [180, 560], [240, 600], { steps: 8 }))
    await act(() => dragOn(0, [260, 560], [320, 600], { steps: 8 }))
    await until(async () => (await added(before)).length === 3, 80)
    await check('each stroke is one drawing', (await added(before)).length === 3 && (await added(before)).every((item) => item.type === 'Ink'))
    await check('the pen stays on', await pressed('ink'))
    await press(`${mod}+z`)
    await until(async () => (await added(before)).length === 2, 60)
    await check('one Cmd+Z takes one stroke back', (await added(before)).length === 2)
    await press('Escape')
    await press('Escape')
  })

  // N25: move, recolour and delete a comment.
  await thing(25, 'Move, recolour, delete a comment', '1–3 each', async () => {
    const byId = async () => (await comments()).find((item) => item.id === square.id)
    const start = (await byId()).rect
    const zoom = (await view()).zoom
    await act(() => dragOn(0, [start[0] + 8, start[1] + 8], [start[0] + 58, start[1] + 28]))
    await until(async () => !near((await byId()).rect[0], start[0], 0.5), 60)
    const moved = (await byId()).rect
    await check('a drag moves it as far as the pointer went', near(moved[0] - start[0], 50, 1.5 / zoom + 1) && near(moved[1] - start[1], 20, 1.5 / zoom + 1), JSON.stringify([start, moved]))
    await act(() => clickOn(0, [moved[0] + 8, moved[1] + 8], { button: 'right' }))
    await kit.getByTestId('pdf-menu-comment').waitFor({ timeout: 3000 })
    await act(() => kit.getByTestId('pdf-menu-colors').locator('button').nth(3).click())
    await until(async () => (await byId())?.color !== square.color, 60)
    const colour = (await byId()).color
    await check('right-click, a colour: the comment takes it', colour !== square.color, `${square.color} -> ${colour}`)
    await act(() => press('Delete'))
    await until(async () => !(await byId()), 60)
    await check('Delete deletes it', !(await byId()))
    await press(`${mod}+z`)
    await until(async () => !!(await byId()), 60)
    await check('one undo brings it back', (await byId())?.color === colour)
    await press(`${mod}+z`)
    await until(async () => (await byId())?.color === square.color, 60)
    await check('the next takes the colour back', (await byId())?.color === square.color)
    await press(`${mod}+z`)
    await until(async () => near((await byId())?.rect[0], start[0], 0.5), 60)
    await check('the next puts it where it was', near((await byId()).rect[0], start[0], 0.5))
    await press('Escape')
  }, { note: 'a drag shows a dashed frame, not the comment\'s own pixels' })

  // N26: fill a text field and go to the next.
  await thing(26, 'Fill a text field, Tab to the next', '3', async () => {
    await calm()
    await toPage(4)
    const city = local([380, 712, 540, 734])
    await act(() => clickOn(3, middle(city)))
    await check('a click in the field gives it the keyboard, with Select on', (await focused()) === 'pdf-page-field-city' && (await pressed('select')), await focused())
    await act(() => page.keyboard.type('Oslo'))
    await act(() => page.keyboard.press('Tab'))
    await until(async () => (await value('city')) === 'Oslo', 60)
    await check('Tab fills the field', (await value('city')) === 'Oslo', String(await value('city')))
    await check('Tab goes to the next field', (await focused()) === 'pdf-page-field-country', await focused())
    await page.keyboard.press('Shift+Tab')
    await settle(300)
    const all = await kit.getByTestId('pdf-page-field-city').evaluate((element) => element.selectionStart === 0 && element.selectionEnd === element.value.length && element.value === 'Oslo')
    await check('a field Tab comes to has its old value chosen', all)
    await page.keyboard.press('Escape')
  })

  // N27: a check box and a drop-down.
  await thing(27, 'Tick a check box, choose from a drop-down', '1; 2', async () => {
    await act(() => clickOn(3, middle(local([380, 656, 392, 668]))))
    await until(async () => (await value('subscribe')) === true, 60)
    await check('a click ticks the check box', (await value('subscribe')) === true)
    const country = kit.getByTestId('pdf-page-field-country')
    await act(() => country.click())
    // A native list's choices cannot be clicked in a headless browser: the choice is made as a person's pick.
    await act(() => country.selectOption('FI'))
    await until(async () => (await value('country')) === 'FI', 60)
    await check('a choice in the drop-down fills it', (await value('country')) === 'FI', String(await value('country')))
    // The keyboard is still on the drop-down (as after a choice in its list): Cmd+Z is the editor's.
    await check('(setup) the drop-down has the keyboard', (await focused()) === 'pdf-page-field-country', await focused())
    await press(`${mod}+z`)
    await until(async () => (await value('country')) !== 'FI', 60)
    await check('one undo takes the choice back, the keyboard on the drop-down', (await value('country')) !== 'FI' && (await value('subscribe')) === true, `${await value('country')} ${await value('subscribe')} (${await focused()})`)
    await press(`${mod}+z`)
    await until(async () => (await value('subscribe')) !== true, 60)
    await check('the next undoes the tick', (await value('subscribe')) !== true)
  })

  // N28: undo and redo.
  await thing(28, 'Undo, redo', '1', async () => {
    await act(() => press(`${mod}+Shift+z`))
    await until(async () => (await value('subscribe')) === true, 60)
    await check('Shift+Cmd+Z redoes the tick', (await value('subscribe')) === true)
    await act(() => press(`${mod}+z`))
    await until(async () => (await value('subscribe')) !== true, 60)
    await check('Cmd+Z undoes it again', (await value('subscribe')) !== true)
    const label = (await kit.getByTestId('pdf-undo').getAttribute('aria-label')) ?? (await kit.getByTestId('pdf-undo').textContent())
    await known('the Undo button names the step it undoes', !/^undo$/i.test(label.trim()), label)
  })

  // N29: turn, delete, move a page in the thumbnails (the list does not scroll while a drag is at its edge).
  await thing(29, 'Turn, delete, move a page', '2 each', async () => {
    await calm()
    await toPage(1)
    await kit.getByTestId('pdf-panel-pages').click()
    await kit.getByTestId('pdf-thumb-1').waitFor({ timeout: 10_000 })
    await settle(400)
    const start = await order()
    const thumb = (index) => kit.getByTestId(`pdf-thumb-${index}`)
    await act(() => thumb(1).click({ button: 'right' }))
    await kit.getByTestId('pdf-thumb-menu').waitFor({ timeout: 3000 })
    await act(() => kit.getByTestId('pdf-thumb-menu-rotate-right').click())
    await until(async () => { const area = (await pages())[1].area; return area.width > area.height }, 40)
    await check('the menu\'s Rotate right turns page 2', (await pages())[1].area.width > (await pages())[1].area.height)
    await act(() => thumb(2).click())
    await act(() => press('Delete'))
    await until(async () => (await pages()).length === PAGES - 1, 40)
    await check('Delete deletes the selected page, without asking', (await pages()).length === PAGES - 1 && (await kit.locator('[role="dialog"]').count()) === 0)
    // The list keeps the selected thumbnail in view, so the first is above it: wheel up to it.
    const list = await kit.getByTestId('pdf-thumbs').boundingBox()
    if ((await thumb(0).boundingBox()).y < list.y) {
      await act(async () => {
        await page.mouse.move(list.x + list.width / 2, list.y + list.height / 2)
        await page.mouse.wheel(0, -2000)
      })
      await settle(400)
    }
    const box = await thumb(2).boundingBox()
    // The part of thumbnail 3 inside the list.
    const top = Math.max(box.y, list.y)
    const from = { x: box.x, y: top, width: box.width, height: Math.min(box.y + box.height, list.y + list.height) - top }
    const above = await thumb(0).boundingBox()
    await act(async () => {
      await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2)
      await page.mouse.down()
      await page.waitForTimeout(80)
      await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2 + 8, { steps: 3 })
      await page.mouse.move(from.x + from.width / 2, above.y + 4, { steps: 16 })
      await page.waitForTimeout(120)
    })
    await check('a line shows where the page goes', (await kit.getByTestId('pdf-thumb-drop').count()) === 1)
    await page.mouse.up()
    await until(async () => (await order())[0] === 4, 40)
    await check('dropping it above the first moves it to the front', JSON.stringify(await order()) === JSON.stringify([4, 1, 2, 5, 6, 7, 8]), (await order()).join(','))
    for (let i = 0; i < 3; i += 1) await press(`${mod}+z`)
    await until(async () => (await pages()).length === PAGES && (await pages())[1].area.width < (await pages())[1].area.height, 60)
    await check('three undos, one a step, bring the pages back', JSON.stringify(await order()) === JSON.stringify(start) && (await pages())[1].area.width < (await pages())[1].area.height, (await order()).join(','))
  }, { note: 'a drag held at the list\'s edge does not scroll it: the list is wheeled up to the first thumbnail first' })

  // S2: add a line of text.
  await thing('S2', 'Add a line of text', '4', async () => {
    await calm()
    await toPage(1)
    await act(() => kit.getByTestId('pdf-tool-addText').click())
    await act(() => clickOn(0, [100, 750]))
    await kit.getByTestId('pdf-note-text').waitFor({ timeout: 5000 })
    await act(() => page.keyboard.type('Added by hand'))
    await act(() => kit.getByTestId('pdf-note-post').click())
    await until(async () => JSON.stringify(await ask('text.extract', { pages: [1] })).includes('Added by hand'), 60)
    await check('the text is on the page where it was clicked', JSON.stringify(await ask('text.extract', { pages: [1] })).includes('Added by hand'))
    await check('the tool goes back to Select', await pressed('select'))
  }, { note: 'the new text is not selected after it (page text, not a comment)' })

  // S4: fill and tick a PDF without fields.
  await thing('S4', 'Write and tick on a page without fields', '2', async () => {
    const before = await comments()
    const made = async () => (await comments()).filter((item) => item.page === 3 && !before.some((old) => old.id === item.id))
    await toPage(3)
    await act(() => kit.getByTestId('ribbon-tab-forms').click())
    await act(() => kit.getByTestId('pdf-fill-check').click())
    await kit.getByTestId('pdf-fill-bar').waitFor({ timeout: 5000 })
    await act(() => clickOn(2, [120, 300]))
    await act(() => clickOn(2, [160, 300]))
    await until(async () => (await made()).length === 2, 60)
    await check('each click puts a 12 pt check mark; the tool stays', (await made()).length === 2 && (await made()).every((item) => near(item.rect[2] - item.rect[0], 12, 0.6)))
    await act(() => kit.getByTestId('pdf-fill-text').click())
    await act(() => clickOn(2, [120, 340]))
    await kit.getByTestId('pdf-note-text').waitFor({ timeout: 5000 })
    await act(() => page.keyboard.type('Signed here'))
    await act(() => kit.getByTestId('pdf-note-post').click())
    await until(async () => (await made()).some((item) => item.contents === 'Signed here'), 60)
    await check('Text puts what is typed where the page was clicked', (await made()).some((item) => item.contents === 'Signed here'))
    await page.keyboard.press('Escape')
    await kit.getByTestId('ribbon-tab-home').click()
  })

  // S5: full screen.
  await thing('S5', 'Read full screen', '1', async () => {
    await calm()
    await toPage(2, false)
    const before = await view()
    const mode = () => kit.locator('.kui-app').getAttribute('data-mode')
    await act(() => press(`${mod}+l`))
    await until(async () => (await mode()) === 'focus', 30)
    await settle(600)
    await act(() => press('ArrowRight'))
    await settle(400)
    await check('Cmd+L shows one page at a time; Right turns it', (await mode()) === 'focus' && (await wholeInView()) === 2, `${await mode()} page ${(await wholeInView()) + 1}`)
    await act(() => press('Escape'))
    await until(async () => (await mode()) !== 'focus', 30)
    await settle(600)
    await check('Escape goes back to the zoom from before, at the page reached', near((await view()).zoom, before.zoom, 0.01) && (await status()) === `Page 3 of ${PAGES}`, `${(await view()).zoom.toFixed(3)} ${await status()}`)
  }, { note: 'no black background' })

  // S6: what a right-click offers.
  await thing('S6', 'Right-click to see what is there', '1', async () => {
    await calm()
    await toPage(2)
    const items = async (id) => kit.getByTestId(id).locator('[role="menuitem"]').count()
    await act(() => clickOn(1, [40, 500], { button: 'right' }))
    await kit.getByTestId('pdf-menu-page').waitFor({ timeout: 3000 })
    const onPage = await items('pdf-menu-page')
    await check('a right-click on the page opens its menu', onPage >= 5, `${onPage} items`)
    await page.keyboard.press('Escape')
    await act(() => clickOn(1, middle(local(MARKS.square.rect)), { button: 'right' }))
    await kit.getByTestId('pdf-menu-comment').waitFor({ timeout: 3000 })
    await check('a right-click on a comment selects it and opens its menu', (await outline().count()) === 1 && (await items('pdf-menu-comment')) >= 4, `${await items('pdf-menu-comment')} items`)
    await page.keyboard.press('Escape')
    await page.keyboard.press('Escape')
    await kit.getByTestId('pdf-panel-pages').click()
    await act(() => kit.getByTestId('pdf-thumb-1').click({ button: 'right' }))
    await kit.getByTestId('pdf-thumb-menu').waitFor({ timeout: 3000 })
    await check('a right-click on a thumbnail opens the pages\' menu', (await items('pdf-thumb-menu')) >= 5, `${await items('pdf-thumb-menu')} items`)
    await page.keyboard.press('Escape')
    await kit.getByTestId('pdf-panel-bookmarks').click()
    const row = kit.getByTestId('pdf-bookmark').filter({ hasText: OUTLINE[2].title })
    await row.waitFor({ timeout: 5000 })
    await act(() => row.click({ button: 'right' }))
    await kit.getByTestId('pdf-bookmark-menu').waitFor({ timeout: 3000 })
    await check('a right-click on a bookmark opens its menu', (await items('pdf-bookmark-menu')) >= 3, `${await items('pdf-bookmark-menu')} items`)
    await page.keyboard.press('Escape')
    await known('the page menu offers about what the reference\'s does (21 items)', onPage >= 18, `${onPage} items`)
  })

  cut('S1', 'Change text already on a page', '4', 'plan section 1 and 13: editing text in place is the second batch')
  cut('S3', 'Move and scale an image', '2', 'plan section 1 and 13: selecting, moving and scaling images is the second batch')

  // The keys the editor names in its menus and tool tips, each pressed once (the ones the
  // things above already pressed are not repeated: Cmd+0/1/2, Cmd+F, Cmd+C, Cmd+Z, Shift+Cmd+Z,
  // V, H, Delete on a comment and on a thumbnail).
  current = null
  await hand.part('keys the editor names', async () => {
    await kit.getByTestId('pdf-panel-bookmarks').click()
    const row = kit.getByTestId('pdf-bookmark').filter({ hasText: OUTLINE[2].title })
    await row.click()
    await press('F2')
    const field = kit.getByTestId('pdf-bookmark-title')
    await hand.step('keys: F2 (the bookmark menu\'s Rename) renames in place', (await field.count()) === 1)
    await press('Escape')
    await row.click()
    await press('Delete')
    const titles = async () => JSON.stringify((await ask('bookmark.list')).bookmarks ?? [])
    await until(async () => !(await titles()).includes(OUTLINE[2].title), 30)
    await hand.step('keys: Delete (the bookmark menu\'s Delete) deletes the bookmark', !(await titles()).includes(OUTLINE[2].title))
    await press(`${mod}+z`)
    await until(async () => (await titles()).includes(OUTLINE[2].title), 30)
    await calm()
    await toPage(1)
    await clickOn(0, [40, 500], { button: 'right' })
    await kit.getByTestId('pdf-menu-page').waitFor({ timeout: 3000 })
    const shown = await kit.getByTestId('pdf-menu-page').textContent()
    await page.keyboard.press('Escape')
    await hand.step('keys: the page menu names Cmd+0, Cmd+2 and Cmd+F', ['0', '2', 'F'].every((key) => shown.includes(`${mod === 'Meta' ? '⌘' : 'Ctrl+'}${key}`) || shown.includes(`${key}`)), shown)
  })

  // N30: save and close (last: what was done above is saved).
  await thing(30, 'Save, close', '2', async () => {
    await calm()
    const before = await page.evaluate((name) => window.devHost.files.get(name)?.version ?? 0, FILE)
    await check('(setup) there are unsaved changes', (await dirty()) === true)
    await act(() => press(`${mod}+s`))
    await until(async () => (await dirty()) === false, 60)
    const saved = await page.evaluate((name) => {
      const file = window.devHost.files.get(name)
      const bytes = new Uint8Array(file.bytes)
      const text = new TextDecoder('latin1').decode(bytes)
      return { version: file.version, head: text.slice(0, 5), tail: text.slice(-6).trim(), notes: (text.match(/\/Subtype\s*\/Text\b/g) ?? []).length, inks: (text.match(/\/Subtype\s*\/Ink\b/g) ?? []).length }
    }, FILE)
    await check('Cmd+S writes the file back through the host, and it is clean', (await dirty()) === false && saved.version > before && saved.head === '%PDF-' && saved.tail === '%%EOF', JSON.stringify(saved))
    await check('the saved file has what was made (the note, the strokes)', saved.notes >= 2 && saved.inks >= 2, JSON.stringify(saved))
    await act(() => page.keyboard.press(`${mod}+w`))
    const asked = async () => /^host shortcut .*w$/i.test(await page.locator('#status').textContent())
    await until(asked, 30)
    await check('Cmd+W goes to the host, which closes', await asked(), await page.locator('#status').textContent())
    // Undoing back to what was saved shows no unsaved changes.
    await toPage(1)
    const marks = await comments()
    await selectLine(0, 21)
    await kit.getByTestId('pdf-bar-highlight').click()
    await until(async () => (await comments()).length === marks.length + 1, 60)
    await check('(setup) an edit after saving is unsaved', (await dirty()) === true)
    await press(`${mod}+z`)
    await until(async () => (await comments()).length === marks.length, 60)
    await known('(R-L10, N28) undoing back to the save leaves nothing unsaved', (await dirty()) === false, `unsaved: ${await dirty()}`)
    await press(`${mod}+s`)
  }, { note: 'asking Save / Don\'t save / Cancel on close is the host\'s' })
} catch (error) {
  await hand.step('ran to the end', false, error instanceof Error ? error.message.split('\n').slice(0, 6).join(' / ') : String(error))
}

// The table: each thing, its result, its steps against the reference's.
const main = results.filter((item) => typeof item.n === 'number')
const more = results.filter((item) => typeof item.n !== 'number')
const passed = (list) => list.filter((item) => item.result.startsWith('passed')).length
const gapped = (list) => list.filter((item) => item.result === 'passed, with gaps').length
console.log(`\nFIRST TEN MINUTES (${which})`)
console.log('| # | thing | result | steps | reference | notes |')
console.log('|---|---|---|---|---|---|')
for (const item of [...main, ...more]) {
  const notes = [item.result === 'failed' ? `failed: ${item.why.join('; ')}` : item.result === 'skipped-by-plan' ? item.why[0] : '', item.gaps.length ? `gaps: ${item.gaps.join('; ')}` : '', item.note].filter(Boolean).join('; ')
  console.log(`| ${item.n} | ${item.title} | ${item.result} | ${item.steps ?? '-'} | ${item.reference} | ${notes} |`)
}
console.log(`\nthe thirty: ${passed(main)} passed (${gapped(main)} with gaps), ${main.filter((item) => item.result === 'failed').length} failed, ${main.filter((item) => item.result === 'skipped-by-plan').length} skipped by the plan`)
console.log(`the six more: ${passed(more)} passed (${gapped(more)} with gaps), ${more.filter((item) => item.result === 'failed').length} failed, ${more.filter((item) => item.result === 'skipped-by-plan').length} skipped by the plan`)
await finish()
