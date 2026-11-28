// The counted task (plan 2026-11-26 "PDF usable by hand", stage 9): one whole piece of work, done
// the way a person used to PDF editors does it, counting every action and every time the natural
// way does nothing (node tests/hand/counted-task.mjs [chromium|webkit]). The work: open the file
// (it asks to open with its bookmarks), find "aurora" and highlight three of its matches with the
// highlighter, put a sticky note with a Chinese sentence beside line 5 of page 6, move page 3 to
// the front by dragging its thumbnail, turn page 2, fill the city field of the form on the page,
// undo two steps (the field and the turn), save. One press, click, drag or typed value is one
// action; a "stuck" is a step whose natural way did nothing. The end reads back the comments, the
// page order, the field and the saved file, and prints the actions and the stuck steps; it fails
// on a stuck step or a wrong result, and when the task takes more than ACTIONS_AT_MOST actions.
// HAND_SHOTS=<folder> keeps a screenshot of the end.
import { AURORA_PAGES, AURORA_PER_PAGE, PAGES, handPdf } from './fixture.mjs'
import { FILE, openHand } from './harness.mjs'

/** The plan's goal for the task: 30 actions or fewer, no stuck step. */
const ACTIONS_AT_MOST = 30
const which = process.argv[2] ?? 'chromium'
const hand = await openHand('hand-counted-task', which)
const { page, kit, step, until, finish, mod, status, ask, pages, toScreen, settle, press } = hand

let actions = 0
const stuck = []
const log = []
const act = async (what, body, count = 1) => {
  actions += count
  const result = await body()
  log.push(`${String(actions).padStart(3)}  ${what}`)
  return result
}
const stick = (where) => {
  stuck.push(where)
  log.push(`     ** stuck ${stuck.length}: ${where}`)
}
const near = (a, b, within = 1) => Math.abs(a - b) <= within
const comments = async () => (await ask('comment.list')).comments ?? []
const dirty = () => page.evaluate(async () => { const answer = await window.devHost.connection().ai.inspect(); return answer.dirty ?? answer.details?.dirty })
const city = async () => (await ask('form.listFields')).fields?.find((item) => item.name === 'city')?.value ?? null
/** The heading of each page, in order ("Page N heading" names the page it was made as). */
const order = async () => {
  const result = []
  for (let n = 1; n <= (await pages()).length; n += 1) result.push(Number(JSON.stringify(await ask('text.extract', { pages: [n] })).match(/Page (\d+) heading/)?.[1] ?? 0))
  return result
}
const lineMiddle = (i) => 792 - (690 - 15 * (i - 1)) - 4

try {
  // 1. Open the file (the host's Choose File, then the file).
  await act('choose the file in the host and open it', async () => {
    await page.evaluate(() => { document.getElementById('status').textContent = 'opening…' })
    await page.setInputFiles('#file', { name: FILE, mimeType: 'application/pdf', buffer: handPdf({ outline: true }) })
    await page.waitForFunction((name) => document.getElementById('status')?.textContent?.includes(`${name} open in`), FILE, { timeout: 90_000 })
    await until(async () => /^Page 1 of \d+$/.test(await status()), 200)
    await settle(800)
  }, 2)
  const before = await comments()
  const matches = ((await ask('text.find', { query: 'aurora' })).matches ?? [])

  // 2. Find the word.
  await act('Cmd+F', () => page.keyboard.press(`${mod}+f`))
  if (!(await kit.getByTestId('pdf-find-text').waitFor({ timeout: 5000 }).then(() => true, () => false))) stick('Cmd+F right after opening opens no find bar')
  await act('type "aurora"', async () => {
    await page.keyboard.type('aurora', { delay: 20 })
    await until(async () => (await kit.getByTestId('pdf-find-count').textContent()) === `1/${AURORA_PAGES.length * AURORA_PER_PAGE}`, 80)
    await settle(500)
  })

  // 3. Highlight three matches: the highlighter on, a drag over the match the bar shows, Cmd+G to the next.
  await act('click the highlighter on the tool rail', () => kit.getByTestId('pdf-tool-highlight').click())
  for (let i = 1; i <= 3; i += 1) {
    const box = await kit.getByTestId('pdf-search-highlight').first().boundingBox().catch(() => null)
    if (!box) {
      stick(`no match shown to drag over (${i})`)
      break
    }
    await act(`drag over the match the bar shows (${await kit.getByTestId('pdf-find-count').textContent()})`, async () => {
      await page.mouse.move(box.x + 1, box.y + box.height / 2)
      await page.mouse.down()
      await page.mouse.move(box.x + box.width - 1, box.y + box.height / 2, { steps: 8 })
      await page.mouse.up()
      await settle(700)
    })
    if (i < 3) await act('Cmd+G (the next match)', () => press(`${mod}+g`))
  }
  await act('Escape (close the find bar)', () => press('Escape'))
  const marks = (await comments()).filter((item) => item.type === 'Highlight' && !before.some((old) => old.id === item.id))
  if (marks.length !== 3) stick(`the highlighter made ${marks.length} highlights, not 3`)

  // 4. A sticky note with a Chinese sentence beside line 5 of page 6.
  await act('page field: click it, type 6, Enter', async () => {
    await kit.getByTestId('pdf-page-field').click()
    await page.keyboard.press(`${mod}+a`)
    await page.keyboard.type('6')
    await press('Enter')
  }, 3)
  const spot = await toScreen(5, 540, lineMiddle(5))
  await act('click the sticky note tool', () => kit.getByTestId('pdf-tool-note').click())
  await act('click beside line 5, where the note goes', async () => {
    await page.mouse.click(spot.x, spot.y)
    await settle(400)
  })
  if (!(await kit.getByTestId('pdf-note-text').count())) stick('no card to write the note in')
  await act('type the sentence (an input method\'s text)', () => page.keyboard.insertText('这一段需要补充观测地点。'))
  await act('Cmd+Enter (Post)', async () => {
    await page.keyboard.press(`${mod}+Enter`)
    await until(async () => (await comments()).some((item) => item.contents === '这一段需要补充观测地点。'), 60)
  })
  if (!(await comments()).some((item) => item.contents === '这一段需要补充观测地点。')) stick('the note was not made')

  // 5. Page 3 to the front: drag its thumbnail above the first.
  const start = await order()
  if (!(await kit.getByTestId('pdf-thumbs').count())) await act('click the page thumbnails (the file opened with its bookmarks)', () => kit.getByTestId('pdf-panel-pages').click())
  await kit.getByTestId('pdf-thumbs').waitFor({ timeout: 10_000 })
  await settle(400)
  const list = await kit.getByTestId('pdf-thumbs').boundingBox()
  const first = await kit.getByTestId('pdf-thumb-0').boundingBox({ timeout: 1000 }).catch(() => null)
  if (!first || first.y < list.y) {
    // The list keeps the current page's thumbnail in view (and draws only what is near it), and
    // a drag does not scroll it.
    await act('wheel the thumbnail list up to the first page', async () => {
      await page.mouse.move(list.x + list.width / 2, list.y + list.height / 2)
      await page.mouse.wheel(0, -4000)
      await settle(400)
    })
  }
  await kit.getByTestId('pdf-thumb-2').waitFor({ timeout: 10_000 })
  await act('drag thumbnail 3 above thumbnail 1', async () => {
    const box = await kit.getByTestId('pdf-thumb-2').boundingBox()
    const top = Math.max(box.y, list.y)
    const from = { x: box.x + box.width / 2, y: (top + Math.min(box.y + box.height, list.y + list.height)) / 2 }
    const above = await kit.getByTestId('pdf-thumb-0').boundingBox()
    await page.mouse.move(from.x, from.y)
    await page.mouse.down()
    await page.waitForTimeout(80)
    await page.mouse.move(from.x, from.y + 8, { steps: 3 })
    await page.mouse.move(from.x, above.y + 4, { steps: 16 })
    await page.waitForTimeout(120)
    await page.mouse.up()
    await until(async () => (await order())[0] === 3, 40)
  })
  if ((await order())[0] !== 3) stick('dragging the thumbnail did not move the page')
  const moved = await order()

  // 6. Turn page 2 (its thumbnail, then the Home tab's Rotate Right).
  await act('click thumbnail 2', () => kit.getByTestId('pdf-thumb-1').click())
  await act('click Rotate Right', async () => {
    await kit.getByTestId('pdf-rotate-right').click()
    await until(async () => { const area = (await pages())[1].area; return area.width > area.height }, 40)
  })
  const turned = (await pages())[1].area
  if (!(turned.width > turned.height)) stick('Rotate Right did not turn page 2')

  // 7. Fill the city field of the form on page 4 on the page (its thumbnail is in view).
  await act('click thumbnail 4 (the form)', async () => {
    await kit.getByTestId('pdf-thumb-3').click()
    await until(async () => (await status()) === `Page 4 of ${PAGES}`, 40)
    await settle(400)
  })
  await act('click the city field on the page', async () => {
    const at = await toScreen(3, 460, 792 - 723)
    await page.mouse.click(at.x, at.y)
  })
  await act('type Oslo', () => page.keyboard.type('Oslo'))
  await act('Enter', async () => {
    await page.keyboard.press('Enter')
    await until(async () => (await city()) === 'Oslo', 60)
  })
  if ((await city()) !== 'Oslo') stick('the field on the page took no value')

  // 8. Undo two steps.
  await act('Cmd+Z', () => press(`${mod}+z`))
  await act('Cmd+Z', async () => {
    await press(`${mod}+z`)
    await until(async () => (await city()) !== 'Oslo' && (await pages())[1].area.width < (await pages())[1].area.height, 60)
  })
  if ((await city()) === 'Oslo') stick('Cmd+Z did not take the field back')

  // 9. Save.
  const version = await page.evaluate((name) => window.devHost.files.get(name)?.version ?? 0, FILE)
  await act('Cmd+S', async () => {
    await page.keyboard.press(`${mod}+s`)
    await until(async () => (await dirty()) === false, 60)
  })
  if (await dirty()) stick('Cmd+S did not save')
  if (process.env.HAND_SHOTS) await page.screenshot({ path: `${process.env.HAND_SHOTS.replace(/\/$/, '')}/counted-task-${which}.png` })

  // What the work left.
  const made = (await comments()).filter((item) => !before.some((old) => old.id === item.id))
  const highlights = made.filter((item) => item.type === 'Highlight')
  const onMatch = (item) => matches.some((match) => match.rects?.some((rect) => near(rect[0], item.rect[0], 1.5) && near(rect[2], item.rect[2], 1.5) && near(rect[1], item.rect[1], 3)))
  await step('the three highlights lie on matches of "aurora"', highlights.length === 3 && highlights.every(onMatch), JSON.stringify(highlights.map((item) => [item.page, item.rect.map(Math.round)])))
  const note = made.find((item) => item.type === 'Text')
  // Page 6 stays page 6 when page 3 moves to the front (pages 1 to 3 change places only).
  await step('the note is on page 6 where it was clicked, with its sentence', !!note && note.page === 6 && near(note.rect[0], 540, 2) && near(note.rect[1], lineMiddle(5), 2) && note.contents === '这一段需要补充观测地点。', JSON.stringify(note && [note.page, note.rect, note.contents]))
  await step('page 3 is first, the others in order', JSON.stringify(await order()) === JSON.stringify([3, 1, 2, 4, 5, 6, 7, 8]) && JSON.stringify(moved) === JSON.stringify(await order()), `${start.join(',')} -> ${(await order()).join(',')}`)
  await step('the two undos took back the field and the turn', (await city()) == null && (await pages())[1].area.width < (await pages())[1].area.height, `city ${await city()}`)
  const saved = await page.evaluate((name) => {
    const file = window.devHost.files.get(name)
    const text = new TextDecoder('latin1').decode(new Uint8Array(file.bytes))
    return { version: file.version, head: text.slice(0, 5), tail: text.slice(-6).trim(), highlights: (text.match(/\/Subtype\s*\/Highlight\b/g) ?? []).length, notes: (text.match(/\/Subtype\s*\/Text\b/g) ?? []).length }
  }, FILE)
  await step('the host has the saved file, with the highlights and the note', saved.version > version && saved.head === '%PDF-' && saved.tail === '%%EOF' && saved.highlights >= 3 && saved.notes >= 2 && !(await dirty()), JSON.stringify(saved))
} catch (error) {
  stick(`the task stopped: ${error instanceof Error ? error.message.split('\n')[0] : String(error)}`)
}

console.log(log.join('\n'))
console.log(`\nACTIONS ${actions}  STUCK ${stuck.length}`)
await step(`the task took ${ACTIONS_AT_MOST} actions or fewer`, actions <= ACTIONS_AT_MOST, String(actions))
await step('no step was stuck', stuck.length === 0, stuck.join('; '))
await finish()
