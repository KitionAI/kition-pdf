// The comment tools on the page (plan 2026-11-26 "PDF usable by hand", stage 5):
// node tests/hand/annotation-tools.mjs [chromium|webkit]
// Real pointer and keys only, on the hand fixture. The rail's tools and V / H (K5); a sticky note is
// placed with a click where the pointer is and written at once, in Chinese too, then selected,
// and the tool goes back to Select; Escape in its card makes nothing and leaves no undo step (G5);
// the highlighter marks three lines one after another and stays on, one Cmd+Z takes the last one
// back (G6); the Home tab's buttons turn the tools on; a rectangle, an oval (Shift: a circle), a
// line (Shift: level), an arrow, a drawing, a text box dragged and one clicked, and page text are
// each made with one gesture, read back with comment.list (G7, I3); a shape is selected once made;
// the options bar sets the next comment's colour and changes the selected comment's colour, width
// and opacity (L5). Prints PASS or FAIL per step.
import { handPdf } from './fixture.mjs'
import { openHand } from './harness.mjs'

const which = process.argv[2] ?? 'chromium'
const hand = await openHand('hand-annotation-tools', which)
const { page, kit, step, until, part, finish, mod, openFixture, ask, view, toScreen, settle, press } = hand

/** The middle of line `i` (1-based) of a fixture page, page-local points from the top-left. */
const lineMiddle = (i) => 792 - (690 - 15 * (i - 1)) - 4
const comments = async () => (await ask('comment.list')).comments ?? []
const added = async (before) => (await comments()).filter((item) => !before.some((old) => old.id === item.id))
const near = (a, b, tolerance = 1.5) => Math.abs(a - b) <= tolerance
const outline = () => kit.getByTestId('pdf-comment-outline')
const pressed = async (id) => (await kit.getByTestId(`pdf-tool-${id}`).getAttribute('aria-pressed')) === 'true'
const at = (point) => toScreen(0, point[0], point[1])
async function drag(from, to, { shift = false, steps = 10 } = {}) {
  const a = await at(from)
  const b = await at(to)
  await page.mouse.move(a.x, a.y)
  if (shift) await page.keyboard.down('Shift')
  await page.mouse.down()
  await page.mouse.move((a.x + b.x) / 2, (a.y + b.y) / 2 + 7, { steps })
  await page.mouse.move(b.x, b.y, { steps })
  await page.mouse.up()
  if (shift) await page.keyboard.up('Shift')
}
async function click(point) {
  const p = await at(point)
  await page.mouse.click(p.x, p.y)
}
const tool = async (id) => {
  await kit.getByTestId(`pdf-tool-${id}`).click()
  await until(() => pressed(id), 20)
}
/** One new comment made since `before`, once it is listed. */
async function made(before) {
  await until(async () => (await added(before)).length >= 1, 60)
  return (await added(before))[0] ?? null
}
/** The pixels of the pages on screen round a page-local rectangle of page 1, as the editor drew them. */
async function pixels(rect) {
  const a = await at([rect[0], rect[1]])
  const b = await at([rect[2], rect[3]])
  await settle(500)
  return (await page.screenshot({ clip: { x: a.x, y: a.y, width: b.x - a.x, height: b.y - a.y } })).toString('base64')
}

try {
  await openFixture(handPdf({ marks: true }))
  await press(`${mod}+0`)

  await part('K5 rail and keys', async () => {
    await step('G7 the rail has the twelve tools and Add text', (await kit.getByTestId('pdf-tool-rail').locator('[data-testid^="pdf-tool-"]').count()) === 13)
    await step('K5 Select is on to start with', await pressed('select'))
    await press('h')
    await step('K5 H turns the hand on', await pressed('hand'))
    const before = await view()
    await drag([300, 400], [300, 300])
    await settle()
    const after = await view()
    await step('K5 the hand drags the pages', after.y > before.y + 20, `${before.y.toFixed(1)} -> ${after.y.toFixed(1)}`)
    await press(`${mod}+0`)
    await press('v')
    await step('K5 V turns Select back on', await pressed('select'))
  })

  await part('G5 sticky note', async () => {
    const before = await comments()
    await tool('note')
    await click([450, 60])
    await kit.getByTestId('pdf-note-text').waitFor({ timeout: 5000 })
    await step('G5 the note\'s card has the keyboard at once', await kit.getByTestId('pdf-note-text').evaluate((element) => element === document.activeElement))
    await page.keyboard.insertText('中文便签')
    await page.keyboard.press(`${mod}+Enter`)
    const note = await made(before)
    await step('G5 a click places the note where the pointer was', !!note && note.type === 'Text' && note.page === 1 && near(note.rect[0], 450, 2) && near(note.rect[1], 60, 2), JSON.stringify(note && [note.type, note.rect]))
    await step('G5 what was written (Chinese) is the note\'s text', note?.contents === '中文便签', String(note?.contents))
    await until(async () => (await outline().count()) === 1, 30)
    await step('G5 the new note is selected', (await outline().count()) === 1)
    await step('G5 the tool goes back to Select', await pressed('select'))
    // Escape in the card: nothing is made, and the one undo step is still the note's.
    await press('Escape')
    const count = (await comments()).length
    await tool('note')
    await click([450, 160])
    await kit.getByTestId('pdf-note-text').waitFor({ timeout: 5000 })
    await page.keyboard.type('Never mind')
    await page.keyboard.press('Escape')
    await until(async () => (await kit.getByTestId('pdf-note-card').count()) === 0, 20)
    await settle(400)
    await step('G5 Escape in the card makes no note', (await comments()).length === count)
    await step('G5 the note tool stays on after Escape in its card', await pressed('note'))
    await press('Escape')
    await step('G5 a second Escape goes back to Select', await pressed('select'))
    await press(`${mod}+z`)
    await until(async () => (await comments()).length === count - 1, 60)
    await step('G5 a cancelled note leaves no undo step (Cmd+Z takes back the note before it)', !(await comments()).some((item) => item.contents === '中文便签'))
  })

  await part('G6 highlighter', async () => {
    const before = await comments()
    await tool('highlight')
    for (const line of [5, 9, 13]) await drag([70, lineMiddle(line)], [400, lineMiddle(line)])
    await until(async () => (await added(before)).length === 3, 80)
    const marks = await added(before)
    const onLine = (rect, i) => rect[1] >= lineMiddle(i) - 10 && rect[3] <= lineMiddle(i) + 8
    await step('G6 the highlighter marks three lines one after another', marks.length === 3 && marks.every((item) => item.type === 'Highlight' && item.page === 1) && [5, 9, 13].every((line) => marks.some((item) => onLine(item.rect, line))), JSON.stringify(marks.map((item) => item.rect)))
    await step('G6 the highlighter stays on', await pressed('highlight'))
    await step('G6 no dialog opens', (await kit.getByTestId('pdf-params-dialog').count()) === 0)
    await press(`${mod}+z`)
    await until(async () => (await added(before)).length === 2, 60)
    await step('G6 one Cmd+Z takes one highlight back', (await added(before)).length === 2)
    await press('Escape')
    await press('Escape')
    await step('G6 Escape lets go and goes back to Select', await pressed('select'))
  })

  await part('ribbon', async () => {
    await kit.getByTestId('pdf-sticky-note').click()
    await step('G5 the Home tab\'s Sticky Note turns the note tool on, no dialog', (await pressed('note')) && (await kit.getByTestId('pdf-params-dialog').count()) === 0)
    await kit.getByTestId('pdf-highlight').click()
    await step('G6 Highlight Text with nothing selected turns the highlighter on', await pressed('highlight'))
    await kit.getByTestId('pdf-add-text').click()
    await step('I3 Add Text turns the page text tool on', await pressed('addText'))
    await kit.getByTestId('pdf-comment-more').click()
    await kit.getByTestId('pdf-note-dialog').click()
    await kit.getByTestId('pdf-params-dialog').waitFor({ timeout: 5000 })
    await step('the parameters dialog is still under More', (await kit.getByTestId('pdf-params-dialog').count()) === 1)
    await page.keyboard.press('Escape')
    await until(async () => (await kit.getByTestId('pdf-params-dialog').count()) === 0, 20)
    await press('v')
  })

  await part('G7 shapes', async () => {
    let before = await comments()
    await tool('rectangle')
    await drag([100, 200], [220, 260])
    const square = await made(before)
    await step('G7 a drag makes a rectangle from the press to the release', !!square && square.type === 'Square' && near(square.rect[0], 100, 3) && near(square.rect[1], 200, 3) && near(square.rect[2], 220, 3) && near(square.rect[3], 260, 3), JSON.stringify(square?.rect))
    await until(async () => (await kit.getByTestId('pdf-comment-handle').count()) === 8, 30)
    await step('G7 the new rectangle is selected, with its handles', (await kit.getByTestId('pdf-comment-handle').count()) === 8)
    await step('G7 the rectangle tool stays on', await pressed('rectangle'))
    before = await comments()
    await tool('oval')
    await drag([300, 200], [380, 240], { shift: true })
    const circle = await made(before)
    const w = circle ? circle.rect[2] - circle.rect[0] : 0
    const h = circle ? circle.rect[3] - circle.rect[1] : 0
    await step('G7 Shift makes the oval a circle', !!circle && circle.type === 'Circle' && near(w, h, 3) && w > 60, JSON.stringify(circle?.rect))
    before = await comments()
    await tool('line')
    await drag([100, 320], [260, 334], { shift: true })
    const line = await made(before)
    await step('G7 Shift keeps a line level', !!line && line.type === 'Line' && line.rect[3] - line.rect[1] < 12 && line.rect[2] - line.rect[0] > 150, JSON.stringify(line?.rect))
    before = await comments()
    await tool('arrow')
    await drag([300, 320], [400, 380])
    const arrow = await made(before)
    await step('G7 the arrow tool draws a line with an arrowhead', !!arrow && arrow.type === 'Line' && arrow.rect[2] - arrow.rect[0] > 90, JSON.stringify(arrow?.rect))
    before = await comments()
    await tool('ink')
    await drag([120, 420], [240, 470], { steps: 14 })
    const ink = await made(before)
    await step('G7 a drag draws one stroke', !!ink && ink.type === 'Ink' && ink.rect[0] < 125 && ink.rect[2] > 235, JSON.stringify(ink?.rect))
    await step('G7 a drawing is not selected (it would cover the next stroke)', (await outline().count()) === 0)
    // A press on nothing that is not dragged makes no shape.
    before = await comments()
    await tool('rectangle')
    await click([500, 500])
    await settle(500)
    await step('G7 a click with the rectangle tool makes nothing', (await added(before)).length === 0)
  })

  await part('G7 text', async () => {
    let before = await comments()
    await tool('textbox')
    await drag([100, 560], [260, 600])
    await kit.getByTestId('pdf-note-text').waitFor({ timeout: 5000 })
    await page.keyboard.type('Box text')
    await page.keyboard.press(`${mod}+Enter`)
    const box = await made(before)
    await step('G7 a dragged text box has its text and the dragged size', !!box && box.type === 'FreeText' && box.contents === 'Box text' && near(box.rect[0], 100, 3) && near(box.rect[2], 260, 3), JSON.stringify(box && [box.contents, box.rect]))
    await step('G7 the text box tool goes back to Select', await pressed('select'))
    before = await comments()
    await tool('textbox')
    await click([320, 560])
    await kit.getByTestId('pdf-note-text').waitFor({ timeout: 5000 })
    await page.keyboard.type('Fits')
    await page.keyboard.press(`${mod}+Enter`)
    const fitted = await made(before)
    await step('G7 a clicked text box fits its text (at least 40 points wide)', !!fitted && fitted.type === 'FreeText' && near(fitted.rect[0], 320, 2) && fitted.rect[2] - fitted.rect[0] >= 39 && fitted.rect[2] - fitted.rect[0] < 120, JSON.stringify(fitted?.rect))
    await press('Escape')
    await tool('addText')
    await click([100, 740])
    await kit.getByTestId('pdf-note-text').waitFor({ timeout: 5000 })
    await page.keyboard.type('Typed on the page')
    await page.keyboard.press(`${mod}+Enter`)
    await until(async () => JSON.stringify(await ask('text.extract', { pages: [1] })).includes('Typed on the page'), 60)
    await step('I3 Add text puts page text where it was clicked', JSON.stringify(await ask('text.extract', { pages: [1] })).includes('Typed on the page'))
  })

  await part('L5 options bar', async () => {
    // A tool's colour: what it makes next.
    const before = await comments()
    await tool('rectangle')
    await kit.getByTestId('pdf-options-colors').locator('[aria-label="#1A73E8"]').click()
    await step('L5 the bar shows the tool\'s new colour', (await kit.getByTestId('pdf-options-color').getAttribute('data-color')) === '#1A73E8')
    await drag([400, 420], [500, 480])
    const blue = await made(before)
    await step('L5 the next rectangle has the colour the bar set', blue?.color === '#1A73E8', String(blue?.color))
    await press('Escape')
    await press('Escape')
    // The selected comment's colour, width and opacity, with Select on.
    await click([450, 422])
    await until(async () => (await outline().count()) === 1, 30)
    await step('L5 with a comment selected the bar is the comment\'s', (await kit.getByTestId('pdf-options').getAttribute('data-target')) === 'comment')
    await kit.getByTestId('pdf-options-colors').locator('[aria-label="#2E9E5B"]').click()
    await until(async () => (await comments()).find((item) => item.id === blue.id)?.color === '#2E9E5B', 60)
    await step('L5 a colour changes the selected comment', (await comments()).find((item) => item.id === blue.id)?.color === '#2E9E5B')
    // The rectangle's own pixels (its edge, inside the frame drawn round it).
    const around = [401, 421, 499, 479]
    const look = await pixels(around)
    await kit.getByTestId('pdf-options-width').selectOption('6')
    await until(async () => (await pixels(around)) !== look, 30)
    await step('L5 a line width changes the selected comment', (await pixels(around)) !== look && (await kit.getByTestId('pdf-options-width').inputValue()) === '6')
    const look2 = await pixels(around)
    await kit.getByTestId('pdf-options-opacity').selectOption('50')
    await until(async () => (await pixels(around)) !== look2, 30)
    const look3 = await pixels(around)
    await step('L5 an opacity changes the selected comment', look3 !== look2 && (await kit.getByTestId('pdf-options-opacity').inputValue()) === '50')
    await step('L5 the keyboard is the pages\' after a choice in the bar', (await hand.focused()) === 'pages', await hand.focused())
    await press(`${mod}+z`)
    await until(async () => (await pixels(around)) !== look3, 30)
    await step('L5 Cmd+Z takes the last change back', (await pixels(around)) !== look3)
  })
} catch (error) {
  await step('ran to the end', false, error instanceof Error ? error.message.split('\n').slice(0, 6).join(' / ') : String(error))
}
await finish()
