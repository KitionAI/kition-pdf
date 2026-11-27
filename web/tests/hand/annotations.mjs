// Comments, links and menus on the page (plan 2026-11-26 "PDF usable by hand", stage 4):
// node tests/hand/annotations.mjs [chromium|webkit]
// Real pointer and keys only, on the hand fixture with its marks (fixture.mjs MARKS). The cursor
// says what is under it (C11); a link is followed with a click and Alt+Left comes back, a web
// address is asked about and goes to the host (F5); a comment is selected with a click, its author
// and text show on hover (G1), a drag moves it by as much as the pointer moved, one undo step, and
// Escape during the drag changes nothing (G2); a handle resizes a square; the arrow keys nudge;
// Delete deletes and Escape lets go (G3); the right-click menus of a comment, of selected text and
// of the page (L1, C12); a press on a form field starts no text selection; a double-click opens a
// note's text; a quarter-turned page moves its comment the way the pointer goes; an agent's read
// keeps the selection. Results are read back with comment.list. Prints PASS or FAIL per step.
import { MARKS, handPdf } from './fixture.mjs'
import { openHand } from './harness.mjs'

const which = process.argv[2] ?? 'chromium'
const hand = await openHand('hand-annotations', which, { query: 'locale=en-US&open=1' })
const { page, kit, step, until, part, finish, mod, status, openFixture, ask, view, toScreen, settle, press } = hand

/** A user-space rectangle of an unturned Letter page as the page-local one the tools answer (y down). */
const local = ([x0, y0, x1, y1]) => [x0, 792 - y1, x1, 792 - y0]
const middle = (rect) => [(rect[0] + rect[2]) / 2, (rect[1] + rect[3]) / 2]
const comments = async () => (await ask('comment.list')).comments ?? []
const byId = async (id) => (await comments()).find((item) => item.id === id)
const cursor = () => kit.getByTestId('kit-canvas-surface').evaluate((element) => getComputedStyle(element).cursor)
const outline = () => kit.getByTestId('pdf-comment-outline')
const near = (a, b, tolerance = 1.5) => Math.abs(a - b) <= tolerance
const sameRect = (a, b, tolerance = 0.6) => !!a && !!b && a.every((value, i) => near(value, b[i], tolerance))
/** The screen point of page-local point `point` on page `number` (1-based). */
const at = (number, point) => toScreen(number - 1, point[0], point[1])
async function hoverAt(number, point) {
  const p = await at(number, point)
  await page.mouse.move(p.x - 6, p.y - 6)
  await page.mouse.move(p.x, p.y, { steps: 3 })
  await page.waitForTimeout(200)
  return p
}
async function dragFrom(p, dx, dy, { release = true } = {}) {
  await page.mouse.move(p.x, p.y)
  await page.mouse.down()
  await page.mouse.move(p.x + dx / 2, p.y + dy / 2, { steps: 6 })
  await page.mouse.move(p.x + dx, p.y + dy, { steps: 6 })
  if (release) await page.mouse.up()
}
/** Shows page `number` (1-based) whole, through the page field and Cmd+0. */
async function showPage(number) {
  await kit.getByTestId('pdf-page-field').click()
  await page.keyboard.press(`${mod}+a`)
  await page.keyboard.type(String(number))
  await press('Enter')
  await press(`${mod}+0`)
  await until(async () => (await status()) === `Page ${number} of 8`, 40)
}

try {
  await openFixture(handPdf({ marks: true }))
  await press(`${mod}+0`)

  await part('C11 cursor', async () => {
    await hoverAt(1, [150, 792 - (690 - 15 * 9) - 4])
    await step('C11 the text cursor over text', (await cursor()) === 'text', await cursor())
    await hoverAt(1, [30, 400])
    await until(async () => (await cursor()) === 'default', 20)
    await step('C11 an arrow over the blank margin', (await cursor()) === 'default', await cursor())
    await hoverAt(1, middle(local(MARKS.pageLink.rect)))
    await step('C11 F5 a hand over a link', (await cursor()) === 'pointer', await cursor())
  })

  await part('F5 links', async () => {
    const p = await hoverAt(1, middle(local(MARKS.pageLink.rect)))
    await page.mouse.click(p.x, p.y)
    await until(async () => (await status()) === `Page ${MARKS.pageLink.to} of 8`, 40)
    await settle()
    await step('F5 a click on a link goes to its page', (await status()) === `Page ${MARKS.pageLink.to} of 8`, await status())
    await press('Alt+ArrowLeft')
    await until(async () => (await status()) === 'Page 1 of 8', 40)
    await step('F5 Alt+Left goes back to where the link was', (await status()) === 'Page 1 of 8', await status())
    const web = await hoverAt(1, middle(local(MARKS.urlLink.rect)))
    await page.mouse.click(web.x, web.y)
    await kit.getByTestId('pdf-link-dialog').waitFor({ timeout: 5000 })
    await step('F5 a web address is asked about first', (await kit.getByTestId('pdf-link-url').textContent()) === MARKS.urlLink.url)
    await kit.getByTestId('pdf-link-open').click()
    await until(async () => (await page.evaluate(() => window.devHost.opens.length)) > 0, 40)
    const opens = await page.evaluate(() => window.devHost.opens)
    await step('F5 Open sends the address to the host', opens.some((item) => item.method === 'host/openUrl' && item.params.url === MARKS.urlLink.url), JSON.stringify(opens))
  })

  await showPage(2)
  const note = local(MARKS.note.rect)
  const square = local(MARKS.square.rect)

  await part('G1 select', async () => {
    await hoverAt(2, middle(note))
    await kit.getByTestId('pdf-comment-tip').waitFor({ timeout: 3000 })
    const tip = await kit.getByTestId('pdf-comment-tip').textContent()
    await step('G1 hovering a note shows its author and text', tip.includes(MARKS.note.author) && tip.includes(MARKS.note.contents), tip)
    await step('C11 a move cursor over a comment', (await cursor()) === 'move', await cursor())
    const p = await at(2, middle(note))
    await page.mouse.click(p.x, p.y)
    await until(async () => (await outline().count()) === 1, 30)
    const box = await outline().boundingBox()
    await step('G1 a click selects the note: it is framed where it is', !!box && p.x > box.x && p.x < box.x + box.width && p.y > box.y && p.y < box.y + box.height, JSON.stringify([box, p]))
    await step('G1 a note has no resize handles', (await kit.getByTestId('pdf-comment-handle').count()) === 0)
    await step('G1 the Comments tab\'s Delete is on for it', await kit.getByTestId('ribbon-tab-comment').click().then(() => kit.getByTestId('pdf-cmd-comment.delete').isEnabled()))
    await kit.getByTestId('ribbon-tab-home').click()
    // An agent's read does not let go of it.
    await comments()
    await settle()
    await step('G1 an agent\'s read keeps the selection', (await outline().count()) === 1)
    await press('Escape')
    await step('G3 Escape lets go of the comment', (await outline().count()) === 0)
  })

  await part('G2 move', async () => {
    const before = (await byId(MARKS.note.id)).rect
    const zoom = (await view()).zoom
    await dragFrom(await at(2, middle(note)), -100, 40)
    await until(async () => !sameRect((await byId(MARKS.note.id))?.rect, before), 60)
    const after = (await byId(MARKS.note.id)).rect
    await step('G2 dragging a note moves it as far as the pointer went', near(after[0] - before[0], -100 / zoom) && near(after[1] - before[1], 40 / zoom), JSON.stringify([before, after, zoom]))
    await step('G2 the moved note stays selected', (await outline().count()) === 1)
    await press(`${mod}+z`)
    await until(async () => sameRect((await byId(MARKS.note.id))?.rect, before), 60)
    await step('G2 one Cmd+Z puts it back', sameRect((await byId(MARKS.note.id)).rect, before), JSON.stringify((await byId(MARKS.note.id)).rect))
  })

  await part('G2 resize', async () => {
    const before = (await byId(MARKS.square.id)).rect
    const zoom = (await view()).zoom
    const p = await at(2, [square[0] + 10, square[1] + 10])
    await page.mouse.click(p.x, p.y)
    await until(async () => (await kit.getByTestId('pdf-comment-handle').count()) === 8, 30)
    await step('G2 a square has eight handles', (await kit.getByTestId('pdf-comment-handle').count()) === 8)
    const corner = await at(2, [square[2], square[3]])
    await page.mouse.move(corner.x, corner.y)
    await page.waitForTimeout(150)
    await step('C11 a resize cursor over a handle', (await cursor()) === 'nwse-resize', await cursor())
    await dragFrom(corner, 50, 30)
    await until(async () => !sameRect((await byId(MARKS.square.id))?.rect, before), 60)
    const after = (await byId(MARKS.square.id)).rect
    await step('G2 dragging the corner handle resizes, the opposite corner stays', near(after[0], before[0]) && near(after[1], before[1]) && near(after[2] - before[2], 50 / zoom, 2) && near(after[3] - before[3], 30 / zoom, 2), JSON.stringify([before, after, zoom]))
    await press(`${mod}+z`)
    await until(async () => sameRect((await byId(MARKS.square.id))?.rect, before), 60)
    await step('G2 one Cmd+Z undoes the resize', sameRect((await byId(MARKS.square.id)).rect, before))
  })

  await part('G2 Escape', async () => {
    const start = (await byId(MARKS.square.id)).rect
    // A real move first, so an undo step the cancelled drag left would show.
    await dragFrom(await at(2, [square[0] + 10, square[1] + 10]), 0, 60)
    await until(async () => !sameRect((await byId(MARKS.square.id))?.rect, start), 60)
    const moved = (await byId(MARKS.square.id)).rect
    const p = await at(2, [moved[0] + 10, moved[1] + 10])
    await dragFrom(p, 80, 0, { release: false })
    await step('G2 the frame follows the drag', await outline().getAttribute('data-dragging') === 'move')
    await page.keyboard.press('Escape')
    await page.mouse.up()
    await settle(600)
    await step('G2 Escape during a drag leaves the comment where it was', sameRect((await byId(MARKS.square.id)).rect, moved), JSON.stringify((await byId(MARKS.square.id)).rect))
    await press(`${mod}+z`)
    await until(async () => sameRect((await byId(MARKS.square.id))?.rect, start), 60)
    await step('G2 a cancelled drag leaves no undo step (Cmd+Z undoes the move before it)', sameRect((await byId(MARKS.square.id)).rect, start), JSON.stringify((await byId(MARKS.square.id)).rect))
  })

  await part('G2 nudge', async () => {
    const start = (await byId(MARKS.square.id)).rect
    const p = await at(2, [start[0] + 10, start[1] + 10])
    await page.mouse.click(p.x, p.y)
    await until(async () => (await outline().count()) === 1, 30)
    await press('ArrowRight')
    await until(async () => near((await byId(MARKS.square.id))?.rect[0], start[0] + 1, 0.2), 60)
    await step('G2 the right arrow moves the comment one point', near((await byId(MARKS.square.id)).rect[0], start[0] + 1, 0.2))
    await press('Shift+ArrowDown')
    await until(async () => near((await byId(MARKS.square.id))?.rect[1], start[1] + 10, 0.2), 60)
    await step('G2 Shift+Down moves it ten points', near((await byId(MARKS.square.id)).rect[1], start[1] + 10, 0.2), JSON.stringify((await byId(MARKS.square.id)).rect))
    await press(`${mod}+z`)
    await press(`${mod}+z`)
    await until(async () => sameRect((await byId(MARKS.square.id))?.rect, start), 60)
    await step('G2 each nudge is one undo step', sameRect((await byId(MARKS.square.id)).rect, start))
  })

  await part('G3 Delete', async () => {
    const p = await at(2, [square[0] + 10, square[1] + 10])
    await page.mouse.click(p.x, p.y)
    await until(async () => (await outline().count()) === 1, 30)
    await press('Delete')
    await until(async () => !(await byId(MARKS.square.id)), 60)
    await step('G3 Delete deletes the selected comment', !(await byId(MARKS.square.id)))
    await step('G3 nothing is selected after it', (await outline().count()) === 0)
    await press(`${mod}+z`)
    await until(async () => !!(await byId(MARKS.square.id)), 60)
    await step('G3 Cmd+Z brings it back', !!(await byId(MARKS.square.id)))
  })

  await part('L1 menus', async () => {
    const p = await at(2, [square[0] + 10, square[1] + 10])
    await page.mouse.click(p.x, p.y, { button: 'right' })
    await kit.getByTestId('pdf-menu-comment').waitFor({ timeout: 3000 })
    await step('L1 a right-click on a comment opens its menu and selects it', (await outline().count()) === 1)
    await kit.getByTestId('pdf-menu-delete').click()
    await until(async () => !(await byId(MARKS.square.id)), 60)
    await step('L1 the comment menu\'s Delete deletes it', !(await byId(MARKS.square.id)))
    await press(`${mod}+z`)
    await until(async () => !!(await byId(MARKS.square.id)), 60)
    const blank = await at(2, [40, 500])
    await page.mouse.click(blank.x, blank.y, { button: 'right' })
    await kit.getByTestId('pdf-menu-page').waitFor({ timeout: 3000 })
    await step('L1 a right-click on the page opens the page menu', (await kit.getByTestId('pdf-menu-add-note').count()) === 1)
    await page.keyboard.press('Escape')
    await until(async () => (await kit.getByTestId('pdf-menu-page').count()) === 0, 20)
    await step('L1 Escape closes the menu', (await kit.getByTestId('pdf-menu-page').count()) === 0)
    // C12: the selection's menu marks it. No reads between the selection and the menu.
    const before = await comments()
    const line = 792 - (690 - 15 * 29) - 4
    const from = await at(2, [70, line])
    const to = await at(2, [280, line])
    await page.mouse.move(from.x, from.y)
    await page.mouse.down()
    await page.mouse.move(to.x, to.y, { steps: 8 })
    await page.mouse.up()
    await until(async () => (await kit.getByTestId('pdf-selection-highlight').count()) > 0, 60)
    const on = await at(2, [150, line])
    await page.mouse.click(on.x, on.y, { button: 'right' })
    await kit.getByTestId('pdf-menu-selection').waitFor({ timeout: 3000 })
    await step('C12 a right-click on selected text opens the selection menu', (await kit.getByTestId('pdf-menu-highlight').count()) === 1)
    await kit.getByTestId('pdf-menu-highlight').click()
    await until(async () => (await comments()).length === before.length + 1, 60)
    const made = (await comments()).filter((item) => !before.some((old) => old.id === item.id))
    await step('C12 its Highlight highlights the selected line', made.length === 1 && made[0].type === 'Highlight' && made[0].page === 2 && made[0].rect[1] < line && made[0].rect[3] > line, JSON.stringify(made.map((item) => [item.type, item.rect])))
  })

  await part('fields and notes', async () => {
    const field = await at(2, middle(local([380, 712, 540, 734])))
    await dragFrom(field, -200, 30)
    await settle()
    await step('a drag from a form field selects no text', (await kit.getByTestId('pdf-selection-highlight').count()) === 0)
    const p = await at(2, middle(note))
    await page.mouse.dblclick(p.x, p.y)
    await kit.getByTestId('pdf-note-text').waitFor({ timeout: 3000 })
    await page.keyboard.type(' edited')
    await page.keyboard.press(`${mod}+Enter`)
    await until(async () => (await byId(MARKS.note.id))?.contents === `${MARKS.note.contents} edited`, 60)
    await step('G1 a double-click opens the note\'s text to edit', (await byId(MARKS.note.id))?.contents === `${MARKS.note.contents} edited`, String((await byId(MARKS.note.id))?.contents))
    // A press elsewhere closes the card and keeps what was written.
    await page.mouse.dblclick(p.x, p.y)
    await kit.getByTestId('pdf-note-text').waitFor({ timeout: 3000 })
    await page.keyboard.type(' again')
    const away = await at(2, [40, 600])
    await page.mouse.click(away.x, away.y)
    await until(async () => (await kit.getByTestId('pdf-note-card').count()) === 0, 30)
    await until(async () => (await byId(MARKS.note.id))?.contents === `${MARKS.note.contents} edited again`, 60)
    await step('G1 a press elsewhere closes the note\'s card and keeps its text', (await kit.getByTestId('pdf-note-card').count()) === 0 && (await byId(MARKS.note.id))?.contents === `${MARKS.note.contents} edited again`, String((await byId(MARKS.note.id))?.contents))
    // Add a sticky note here (the page menu).
    const before = (await comments()).length
    const spot = await at(2, [60, 300])
    await page.mouse.click(spot.x, spot.y, { button: 'right' })
    await kit.getByTestId('pdf-menu-add-note').click()
    await kit.getByTestId('pdf-note-text').waitFor({ timeout: 3000 })
    await page.keyboard.type('Placed here')
    await kit.getByTestId('pdf-note-post').click()
    await until(async () => (await comments()).length === before + 1, 60)
    const placed = (await comments()).find((item) => item.contents === 'Placed here')
    await step('L1 Add a sticky note here puts it where the menu was opened', !!placed && placed.page === 2 && near(placed.rect[0], 60, 2) && near(placed.rect[1], 300, 2), JSON.stringify(placed?.rect))
  })

  await part('rotated page', async () => {
    await showPage(MARKS.rotated.page)
    const before = (await byId(MARKS.rotated.id)).rect
    const zoom = (await view()).zoom
    await dragFrom(await at(MARKS.rotated.page, middle(before)), 60, 20)
    await until(async () => !sameRect((await byId(MARKS.rotated.id))?.rect, before), 60)
    const after = (await byId(MARKS.rotated.id)).rect
    await step('G2 on a quarter-turned page the comment moves the way the pointer went', near(after[0] - before[0], 60 / zoom) && near(after[1] - before[1], 20 / zoom), JSON.stringify([before, after, zoom]))
  })
} catch (error) {
  await step('ran to the end', false, error instanceof Error ? error.message.split('\n').slice(0, 6).join(' / ') : String(error))
}
await finish()
