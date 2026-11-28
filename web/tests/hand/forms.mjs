// Filling the form on the page (plan 2026-11-26 "PDF usable by hand", stage 6):
// node tests/hand/forms.mjs [chromium|webkit]
// On the hand fixture's form (page 4: a required text field without an appearance, a drop-down,
// a check box, three radio buttons; a text field on page 2). The document opens with a bar that
// counts its fields and turns their highlight off and on (H2); every field shows on the page where
// it is, even without an appearance, and follows the zoom (H2); a click in the text field and
// typing fills it as it lets go, Tab goes on to the drop-down (H1); a drop-down's choice, a check
// box and each radio button (not just the group's first: issue-hand-radio-widgets) fill at once;
// the arrow keys go round a radio group; a wheel over a field scrolls; Chinese fills as itself (H5); Escape drops what was typed
// and gives the keyboard to the pages, not the body; a letter or an arrow typed in a field is the
// field's; Tab goes on across pages, round to the first field, and Tab from the pages comes into
// the form; an undo shows the field's value on the page and in the Fields panel at once (H6); a
// read-only field takes nothing and says so; with the hand the fields take no press. Real pointer
// and keys; a drop-down's option is picked with selectOption (a native list's popup is the
// browser's own). Results are read back with form.listFields. Prints PASS or FAIL per step.
import { READ_ONLY, handPdf } from './fixture.mjs'
import { openHand } from './harness.mjs'

const which = process.argv[2] ?? 'chromium'
const hand = await openHand('hand-forms', which)
const { page, kit, frame, step, until, part, finish, mod, status, openFixture, ask, focused, toScreen, settle, press, view } = hand

const fields = async () => (await ask('form.listFields')).fields ?? []
const value = async (name) => (await fields()).find((item) => item.name === name)?.value ?? null
const control = (name) => kit.getByTestId(`pdf-page-field-${name}`)
/** A user-space rectangle of the Letter page as the page-local one (y down). */
const local = ([x0, y0, x1, y1]) => [x0, 792 - y1, x1, 792 - y0]
const middle = (rect) => [(rect[0] + rect[2]) / 2, (rect[1] + rect[3]) / 2]
const near = (a, b, tolerance = 2) => Math.abs(a - b) <= tolerance
const CITY = local([380, 712, 540, 734])
const SUBSCRIBE = local([380, 656, 392, 668])
const SIZE = [0, 1, 2].map((i) => local([380 + i * 30, 630, 392 + i * 30, 642]))
const click = async (pageNumber, rect) => {
  const p = await toScreen(pageNumber - 1, ...middle(rect))
  await page.mouse.click(p.x, p.y)
  await settle(150)
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
/** Whether the control over a field's widget lies on it (CSS pixels, within 2). */
async function onWidget(name, pageNumber, rect, nth = 0) {
  const box = await control(name).nth(nth).boundingBox()
  const a = await toScreen(pageNumber - 1, rect[0], rect[1])
  const b = await toScreen(pageNumber - 1, rect[2], rect[3])
  return !!box && near(box.x, a.x) && near(box.y, a.y) && near(box.x + box.width, b.x) && near(box.y + box.height, b.y)
}

try {
  await openFixture()

  await part('H2 bar', async () => {
    await kit.getByTestId('pdf-form-bar').waitFor({ timeout: 10_000 })
    const text = await kit.getByTestId('pdf-form-bar-text').textContent()
    await step('H2 the document opens with how many fields it has', text === 'This document contains 5 interactive form fields.', text)
    const highlighted = () => kit.getByTestId('pdf-form-layer').evaluate((element) => element.hasAttribute('data-highlight'))
    await step('H2 the fields are highlighted at first', await highlighted())
    await kit.getByTestId('pdf-form-highlight').click()
    await step('H2 Hide field highlights takes the highlight away', !(await highlighted()) && (await kit.getByTestId('pdf-form-highlight').textContent()) === 'Highlight fields')
    await kit.getByTestId('pdf-form-highlight').click()
    await step('H2 Highlight fields brings it back', await highlighted())
    await step('H2 the bar leaves the keyboard on the pages', (await focused()) === 'pages', await focused())
  })

  await part('H2 fields show', async () => {
    await showPage(4)
    await step('H2 the text field shows on the page where it is', await onWidget('city', 4, CITY))
    const look = await control('city').evaluate((element) => { const style = getComputedStyle(element); return { background: style.backgroundColor, border: style.borderTopWidth } })
    await step('H2 a field without an appearance shows itself (not clear)', look.background !== 'rgba(0, 0, 0, 0)' && look.background !== 'transparent', JSON.stringify(look))
    await step('H2 the required field is framed', parseFloat(look.border) >= 2, look.border)
    await step('every radio button has its control (issue-hand-radio-widgets)', (await control('size').count()) === 3 && await onWidget('size', 4, SIZE[2], 2), String(await control('size').count()))
    await press(`${mod}+=`)
    await settle(400)
    await step('H2 the controls follow the zoom', await onWidget('city', 4, CITY), JSON.stringify(await control('city').boundingBox()))
    await press(`${mod}+0`)
    const over = await toScreen(3, ...middle(CITY))
    await page.mouse.move(over.x, over.y)
    const before = (await view()).y
    await page.mouse.wheel(0, 120)
    await settle(400)
    await step('a wheel turned over a field scrolls the pages', (await view()).y > before + 20, `${before} -> ${(await view()).y}`)
    await page.mouse.wheel(0, -120)
    await settle(400)
  })

  await part('H1 type and Tab', async () => {
    await click(4, CITY)
    await step('H1 a click in a text field gives it the keyboard', (await focused()) === 'pdf-page-field-city', await focused())
    await page.keyboard.type('Oslo')
    await step('H1 typing changes nothing until the field lets go', (await value('city')) == null, String(await value('city')))
    await page.keyboard.press('Tab')
    await until(async () => (await value('city')) === 'Oslo', 60)
    await step('H1 Tab fills the field', (await value('city')) === 'Oslo', String(await value('city')))
    await step('H1 Tab goes on to the next field in the tab order (the drop-down)', (await focused()) === 'pdf-page-field-country', await focused())
    await control('country').selectOption('IS')
    await until(async () => (await value('country')) === 'IS', 60)
    await step('H1 a drop-down\'s choice fills it', (await value('country')) === 'IS', String(await value('country')))
  })

  await part('check box and radio buttons', async () => {
    await click(4, SUBSCRIBE)
    await until(async () => (await value('subscribe')) === true, 60)
    await step('H1 a click on a check box ticks it', (await value('subscribe')) === true)
    await page.keyboard.press(' ')
    await until(async () => (await value('subscribe')) === false, 60)
    await step('H1 Space turns it back, the keyboard still on it', (await value('subscribe')) === false && (await focused()) === 'pdf-page-field-subscribe', await focused())
    await click(4, SIZE[1])
    await until(async () => (await value('size')) === 'M', 60)
    await step('a click on the second radio button chooses it (issue-hand-radio-widgets)', (await value('size')) === 'M', String(await value('size')))
    await click(4, SIZE[2])
    await until(async () => (await value('size')) === 'L', 60)
    await step('a click on the third radio button chooses it (issue-hand-radio-widgets)', (await value('size')) === 'L', String(await value('size')))
    await page.keyboard.press('ArrowLeft')
    await until(async () => (await value('size')) === 'M', 60)
    await step('ArrowLeft chooses the group\'s previous button', (await value('size')) === 'M' && (await focused()) === 'pdf-page-field-size', `${await value('size')} ${await focused()}`)
  })

  await part('H5 Chinese', async () => {
    await click(4, CITY)
    await page.keyboard.press(`${mod}+a`)
    await page.keyboard.insertText('奥斯陆')
    await page.keyboard.press('Enter')
    await until(async () => (await value('city')) === '奥斯陆', 60)
    await step('H5 Chinese fills the field as itself', (await value('city')) === '奥斯陆', String(await value('city')))
    await step('Enter gives the keyboard back to the pages', (await focused()) === 'pages', await focused())
  })

  await part('keys in a field', async () => {
    await click(4, CITY)
    await page.keyboard.press('End')
    await page.keyboard.type('vh')
    await page.keyboard.press('ArrowLeft')
    const typed = await control('city').inputValue()
    const tool = await frame().evaluate(() => document.querySelector('.pdf-form-layer')?.hasAttribute('data-active'))
    await step('K a letter typed in a field is the field\'s (no tool key), an arrow moves in it', typed === '奥斯陆vh' && tool === true && (await focused()) === 'pdf-page-field-city', `${typed} ${tool} ${await focused()}`)
    await page.keyboard.press('Escape')
    await settle()
    await step('Escape gives the keyboard to the pages, not the body', (await focused()) === 'pages', await focused())
    await step('Escape drops what was typed', (await value('city')) === '奥斯陆' && (await control('city').inputValue()) === '奥斯陆', `${await value('city')} / ${await control('city').inputValue()}`)
  })

  await part('H6 undo', async () => {
    await until(async () => kit.getByTestId('pdf-undo').isEnabled(), 40)
    await page.keyboard.press(`${mod}+z`)
    await until(async () => (await value('city')) === 'Oslo', 60)
    await until(async () => (await control('city').inputValue()) === 'Oslo', 40)
    await step('H6 an undo shows the field\'s value on the page', (await value('city')) === 'Oslo' && (await control('city').inputValue()) === 'Oslo', `${await value('city')} / ${await control('city').inputValue()}`)
    await kit.getByTestId('pdf-panel-fields').click()
    await kit.getByTestId('pdf-field-city').waitFor({ timeout: 5000 })
    await step('H6 and in the Fields panel, the same value', (await kit.getByTestId('pdf-field-city').inputValue()) === 'Oslo', await kit.getByTestId('pdf-field-city').inputValue())
    await kit.getByTestId('pdf-panel-pages').click()
  })

  await part('Tab across pages', async () => {
    await click(4, SIZE[1])
    await settle()
    await page.keyboard.press('Tab')
    await page.keyboard.press('Tab')
    await settle(400)
    await step('Tab from the form\'s last field goes round to its first, on page 2', (await focused()) === 'pdf-page-field-observer' && (await status()) === 'Page 2 of 8', `${await focused()} ${await status()}`)
    await page.keyboard.press('Shift+Tab')
    await settle(400)
    await step('Shift+Tab goes back to page 4\'s last field', (await focused()) === 'pdf-page-field-size' && (await status()) === 'Page 4 of 8', `${await focused()} ${await status()}`)
    await page.keyboard.press('Escape')
    await settle()
    await page.keyboard.press('Tab')
    await settle(300)
    await step('Tab from the pages comes into the form on the current page', (await focused()) === 'pdf-page-field-city', await focused())
    await page.keyboard.press('Escape')
  })

  await part('hand tool', async () => {
    await press('h')
    const takes = await control('city').evaluate((element) => getComputedStyle(element).pointerEvents)
    await step('with the hand the fields take no press', takes === 'none', takes)
    await press('v')
  })

  await part('read-only', async () => {
    await openFixture(handPdf({ readOnly: true }))
    await showPage(4)
    const rect = local(READ_ONLY.rect)
    await click(4, rect)
    await step('a read-only field takes no keyboard', (await focused()) !== `pdf-page-field-${READ_ONLY.name}`, await focused())
    const said = await kit.getByText(`${READ_ONLY.name} is read-only`).count()
    await step('a press on a read-only field says so', said > 0, String(said))
    await step('its value is unchanged', (await value(READ_ONLY.name)) === READ_ONLY.value, String(await value(READ_ONLY.name)))
    await step('the bar counts it with the others', (await kit.getByTestId('pdf-form-bar-text').textContent()) === 'This document contains 6 interactive form fields.')
  })
} catch (error) {
  await step('ran to the end', false, error instanceof Error ? error.message.split('\n').slice(0, 6).join(' / ') : String(error))
}
await finish()
