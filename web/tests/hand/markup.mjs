// Marking text and working with comments by hand (plan 2026-11-26 "PDF usable by hand", stage 2):
// node tests/hand/markup.mjs [chromium|webkit]
// Real pointer and keys only. A line dragged over and the Home tab's Highlight makes a highlight on
// that line at once, no dialog (C9); the bar beside a selection shows once the drag is let go and
// underlines, strikes out, adds a highlight with a note, and copies (G6, R-C3); a selection across
// two pages marks each; with the find bar open Highlight marks the current match (C9); the Comments
// panel chooses a comment (the view goes to it, it is framed), edits its text, replies, sets its
// status, and the Comments tab's Delete acts on it (disabled with none chosen), as Delete does (G4,
// G8). Results are read back with comment.list. Prints PASS or FAIL per step.
import { AURORA_PAGES, AURORA_PER_PAGE, NOTE } from './fixture.mjs'
import { openHand } from './harness.mjs'

const which = process.argv[2] ?? 'chromium'
const hand = await openHand('hand-markup', which)
const { page, kit, step, until, part, finish, mod, status, openFixture, ask, toScreen, settle, press } = hand

/** The middle of line `i` (1-based) of a fixture page, page-local points from the top-left. */
const lineMiddle = (i) => 792 - (690 - 15 * (i - 1)) - 4
// A read through the host's agent connection refreshes the editor (its selection and the find
// bar's match start again), so the steps read before they select, never between.
const comments = async () => (await ask('comment.list')).comments ?? []
const added = async (before) => (await comments()).filter((item) => !before.some((old) => old.id === item.id))
/** Whether a comment's rectangle lies on line `i` (within its height) from about x = 72. */
const onLine = (rect, i) => rect && rect[1] >= lineMiddle(i) - 10 && rect[3] <= lineMiddle(i) + 8 && rect[0] >= 68 && rect[0] <= 80
const bar = () => kit.getByTestId('pdf-selection-bar')
const dialogs = () => kit.getByTestId('pdf-params-dialog').count()

/** Drags over line `i` of page `index` (0-based) from x = 70 to x = 400 and waits for the selection. */
async function selectLine(index, i, { release = true } = {}) {
  const from = await toScreen(index, 70, lineMiddle(i))
  const to = await toScreen(index, 400, lineMiddle(i))
  await page.mouse.move(from.x, from.y)
  await page.mouse.down()
  await page.mouse.move(to.x, to.y, { steps: 8 })
  await until(async () => (await kit.getByTestId('pdf-selection-highlight').count()) > 0, 60)
  if (release) {
    await page.mouse.up()
    await until(async () => (await bar().count()) === 1, 40)
  }
}

try {
  await openFixture()
  // The whole of page 1 in view.
  await press(`${mod}+0`)

  await part('C9 ribbon', async () => {
    const before = await comments()
    await selectLine(0, 5)
    await kit.getByTestId('pdf-highlight').click()
    await until(async () => (await added(before)).length === 1, 60)
    const made = await added(before)
    await step('C9 Highlight with a line selected opens no dialog', (await dialogs()) === 0)
    await step('C9 Highlight with a line selected highlights it', made.length === 1 && made[0].type === 'Highlight' && made[0].page === 1 && onLine(made[0].rect, 5), JSON.stringify(made.map((item) => [item.type, item.page, item.rect])))
    await step('C9 the selection is cleared after marking', (await kit.getByTestId('pdf-selection-highlight').count()) === 0)
  })

  await part('G6 bar', async () => {
    let before = await comments()
    await selectLine(0, 9, { release: false })
    await step('G6 the selection bar waits while the mouse is held', (await bar().count()) === 0)
    await page.mouse.up()
    await until(async () => (await bar().count()) === 1, 40)
    await step('G6 the selection bar shows once the drag is let go', (await bar().count()) === 1)
    for (const [name, type, line] of [['underline', 'Underline', 9], ['strikeout', 'StrikeOut', 11]]) {
      if (line !== 9) {
        before = await comments()
        await selectLine(0, line)
      }
      await kit.getByTestId(`pdf-bar-${name}`).click()
      await until(async () => (await added(before)).length === 1, 60)
      const made = await added(before)
      await step(`G6 the bar's ${name} marks the selected line`, made.length === 1 && made[0].type === type && onLine(made[0].rect, line), JSON.stringify(made.map((item) => [item.type, item.rect])))
    }
    // Add comment: a highlight with a note.
    before = await comments()
    await selectLine(0, 13)
    await kit.getByTestId('pdf-bar-comment').click()
    await kit.getByTestId('pdf-selection-note').waitFor({ timeout: 5000 })
    await page.keyboard.type('Bar note')
    await kit.getByTestId('pdf-selection-post').click()
    await until(async () => (await added(before)).length === 1, 60)
    const made = await added(before)
    await step('G6 the bar\'s Add comment makes a highlight with the note', made.length === 1 && made[0].type === 'Highlight' && made[0].contents === 'Bar note' && onLine(made[0].rect, 13), JSON.stringify(made.map((item) => [item.type, item.contents])))
    // Copy puts the bar away and keeps the selection.
    await selectLine(0, 15)
    await kit.getByTestId('pdf-bar-copy').click()
    await until(async () => (await bar().count()) === 0, 30)
    await step('G6 the bar\'s Copy puts it away and keeps the selection', (await bar().count()) === 0 && (await kit.getByTestId('pdf-selection-highlight').count()) > 0)
    await press('Escape')
  })

  await part('C9 across pages', async () => {
    // Small enough for the end of page 1 and the top of page 2 to be in view together.
    await kit.getByTestId('pdf-zoom-field').click()
    await page.keyboard.type('40')
    await press('Enter')
    await press('Home')
    const from = await toScreen(0, 70, lineMiddle(39))
    const to = await toScreen(1, 300, lineMiddle(2))
    const before = await comments()
    await page.mouse.move(from.x, from.y)
    await page.mouse.down()
    await page.mouse.move(to.x, to.y, { steps: 12 })
    await page.mouse.up()
    await until(async () => (await kit.getByTestId('pdf-selection-highlight').count()) > 0, 60)
    await until(async () => (await bar().count()) === 1, 40)
    await step('C9 (setup) the selection spans two pages', await hand.frame().evaluate(() => new Set([...document.querySelectorAll('[data-testid="pdf-selection-highlight"]')].map((item) => item.getAttribute('data-page'))).size === 2))
    await step('G6 Add comment is off for a selection across pages', await kit.getByTestId('pdf-bar-comment').isDisabled())
    await kit.getByTestId('pdf-highlight').click()
    await until(async () => (await added(before)).length === 2, 60)
    const made = await added(before)
    await step('C9 a selection across two pages highlights each page', made.length === 2 && made.some((item) => item.page === 1) && made.some((item) => item.page === 2) && made.every((item) => item.type === 'Highlight'), JSON.stringify(made.map((item) => [item.page, item.rect])))
    await press(`${mod}+0`)
  })

  await part('C9 find', async () => {
    await press('Home')
    const before = await comments()
    await page.keyboard.press(`${mod}+f`)
    await kit.getByTestId('pdf-find-text').waitFor({ timeout: 5000 })
    await page.keyboard.type('aurora')
    const total = AURORA_PAGES.length * AURORA_PER_PAGE
    const count = () => kit.getByTestId('pdf-find-count').textContent()
    await until(async () => (await count()) === `1/${total}`, 80)
    await settle(600)
    // Highlight, Cmd+G, Highlight: no reads in between (each would start the find bar's match again).
    for (const index of [1, 2]) {
      await kit.getByTestId('pdf-highlight').click()
      await settle(1200)
      await step(`C9 Highlight with the find bar open opens no dialog (${index})`, (await dialogs()) === 0)
      await step(`C9 marking a match keeps it current (${index})`, (await count()) === `${index}/${total}`, await count())
      await page.keyboard.press(`${mod}+g`)
      await until(async () => (await count()) === `${index + 1}/${total}`, 40)
    }
    await until(async () => (await added(before)).length === 2, 60)
    const made = await added(before)
    // The fifth and the seventeenth lines of page 1 say "aurora", after "1.5 A quiet " (x about 128).
    const onWord = (item, line) => item.page === 1 && item.type === 'Highlight' && item.rect[1] >= lineMiddle(line) - 10 && item.rect[3] <= lineMiddle(line) + 8 && item.rect[0] > 120 && item.rect[2] - item.rect[0] < 45
    await step('C9 Highlight with the find bar open marks the current match, each time', made.length === 2 && onWord(made[0], 5) && onWord(made[1], 17), JSON.stringify(made.map((item) => [item.page, item.rect])))
    await press('Escape')
  })

  await part('G4 G8 panel', async () => {
    await kit.getByTestId('ribbon-tab-comment').click()
    const remove = kit.getByTestId('pdf-cmd-comment.delete')
    await step('G4 the Comments tab\'s Delete is off with no comment chosen', await remove.isDisabled())
    await kit.getByTestId('pdf-panel-comments').click()
    const row = kit.getByTestId('pdf-comment').filter({ hasText: NOTE.contents })
    await row.waitFor({ timeout: 10_000 })
    await step('G8 the panel shows the author', (await row.textContent()).includes(NOTE.author), await row.textContent())
    await row.click()
    await until(async () => (await status()) === 'Page 6 of 8', 40)
    await settle()
    await step('G8 choosing a comment goes to its page', (await status()) === 'Page 6 of 8', await status())
    const outline = await kit.getByTestId('pdf-comment-outline').boundingBox()
    const at = await toScreen(NOTE.page - 1, 510, 82)
    await step('G8 the chosen comment is framed where it is on the page', !!outline && at.x > outline.x && at.x < outline.x + outline.width && at.y > outline.y && at.y < outline.y + outline.height, JSON.stringify([outline, at]))
    await step('G4 the Comments tab\'s Delete is on with a comment chosen', await remove.isEnabled())
    const note = async () => (await comments()).find((item) => item.id === 'fixture-note' || item.contents?.startsWith('Fixture') || item.contents === 'Edited note')
    // Edit in place.
    await kit.getByTestId('pdf-comment-edit').click()
    const text = kit.getByTestId('pdf-comment-edit-text')
    await text.waitFor({ timeout: 5000 })
    await page.keyboard.press(`${mod}+a`)
    await page.keyboard.type('Edited note')
    await kit.getByTestId('pdf-comment-save').click()
    await until(async () => (await note())?.contents === 'Edited note', 60)
    await step('G4 the panel edits the comment\'s text', (await note())?.contents === 'Edited note', JSON.stringify(await note()))
    // Reply.
    await kit.getByTestId('pdf-comment-reply-text').click()
    await page.keyboard.type('A reply')
    await page.keyboard.press('Enter')
    await until(async () => ((await note())?.replies ?? []).some((item) => item.contents === 'A reply'), 60)
    await step('G4 the panel replies to the comment', ((await note())?.replies ?? []).some((item) => item.contents === 'A reply'))
    await until(async () => (await kit.getByTestId('pdf-comment-reply').count()) > 0, 30)
    await step('G8 the reply shows under it', (await kit.getByTestId('pdf-comment-reply').filter({ hasText: 'A reply' }).count()) === 1)
    // Status (a list a person picks from).
    await kit.getByTestId('pdf-comment-set-status').selectOption('accepted')
    await until(async () => (await note())?.status === 'Accepted', 60)
    await step('G4 the panel sets the comment\'s status', (await note())?.status === 'Accepted', String((await note())?.status))
    // The ribbon's Delete acts on the chosen comment.
    const count = (await comments()).length
    await until(() => remove.isEnabled(), 30)
    await remove.click()
    await until(async () => (await comments()).length === count - 1, 60)
    await step('G4 the Comments tab\'s Delete deletes the chosen comment, no dialog', (await comments()).length === count - 1 && !(await note()) && (await dialogs()) === 0, `${count} -> ${(await comments()).length}`)
    await step('G4 nothing is chosen after the delete', (await kit.getByTestId('pdf-comment-outline').count()) === 0 && (await remove.isDisabled()))
    // Delete with a row holding the keyboard.
    const first = kit.getByTestId('pdf-comment').first()
    await first.click()
    const left = (await comments()).length
    await page.keyboard.press('Delete')
    await until(async () => (await comments()).length === left - 1, 60)
    await step('G4 Delete deletes the chosen comment', (await comments()).length === left - 1, `${left} -> ${(await comments()).length}`)
    await kit.getByTestId('ribbon-tab-home').click()
  })
} catch (error) {
  await step('ran to the end', false, error instanceof Error ? error.message.split('\n').slice(0, 6).join(' / ') : String(error))
}
await finish()
