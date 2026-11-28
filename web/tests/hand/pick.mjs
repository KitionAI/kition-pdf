// Pointing at the page for a parameters dialog, and redacting selected text (plan 2026-11-26 "PDF
// usable by hand", stage 10): node tests/hand/pick.mjs [chromium|webkit]
// Real pointer and keys only, on the hand fixture. Add a link's "Drag on the page" puts the dialog
// away, a box dragged on page 1 comes back in its Rectangle with the page filled in and what was
// typed kept, and Run makes the link there (link.list, L4, I2); Escape while pointing brings the
// form back as it was; a drag too short to be a box picks nothing; Measure distance's "Click points
// on the page" takes two clicks and measures between them (measure.list); Fill & Sign's "Click on
// the page" puts a check mark centred where it was clicked, on a page with no fields (comment.list,
// N S4), and the Forms tab's Fill & Sign Check and Text stay on, put a mark or typed text where each
// click is and nothing for a drag. Text dragged over is marked for redaction from the selection bar and from its right-click
// menu (one mark over its lines, outlined on the page: comment.list); Protect's Apply redactions
// asks first, Cancel keeps the marks and Apply removes what is under them (text.find).
import { openHand } from './harness.mjs'

const which = process.argv[2] ?? 'chromium'
const hand = await openHand('hand-pick', which)
const { page, kit, step, until, part, finish, mod, openFixture, ask, toScreen, settle, press } = hand

const dialog = () => kit.getByTestId('pdf-params-dialog')
const pickBar = () => kit.getByTestId('pdf-pick-bar')
const field = (name) => kit.locator(`#param-${name}`)
const numbers = (text) => text.split(/[\s,;]+/).filter(Boolean).map(Number)
const near = (a, b, slack = 1.5) => a.length === b.length && a.every((value, i) => Math.abs(value - b[i]) <= slack)
const comments = async () => (await ask('comment.list')).comments ?? []
/** The middle of line `i` (1-based) of a fixture page, page-local points from the top-left. */
const lineMiddle = (i) => 792 - (690 - 15 * (i - 1)) - 4

/** Opens a ribbon command's dialog: its tab, then its button. */
async function openCommand(tab, id) {
  await kit.getByTestId(`ribbon-tab-${tab}`).click()
  await kit.getByTestId(`pdf-cmd-${id}`).first().click()
  await dialog().waitFor({ timeout: 10_000 })
}

/** Types `text` into a field of the dialog (opening More options when it is folded there). */
async function fill(name, text) {
  if (!(await field(name).isVisible())) await dialog().locator('summary').click()
  await field(name).click()
  await page.keyboard.press(`${mod}+a`)
  await page.keyboard.type(text)
}

/** Drags on page `index` (0-based) between two page-local points. */
async function drag(index, from, to) {
  const a = await toScreen(index, ...from)
  const b = await toScreen(index, ...to)
  await page.mouse.move(a.x, a.y)
  await page.mouse.down()
  await page.mouse.move(b.x, b.y, { steps: 10 })
  await page.mouse.up()
}

async function click(index, at) {
  const p = await toScreen(index, ...at)
  await page.mouse.click(p.x, p.y)
}

try {
  await openFixture()
  await press(`${mod}+0`)

  await part('L4 rectangle', async () => {
    await openCommand('edit', 'link.add')
    await fill('toPage', '3')
    const pick = kit.getByTestId('pdf-param-pick-rect')
    await step('L4 Add a link offers to drag its rectangle on the page', (await pick.count()) === 1 && (await pick.textContent()) === 'Drag on the page')
    await pick.click()
    await until(async () => (await dialog().count()) === 0 && (await pickBar().count()) === 1, 30)
    await step('L4 the dialog goes away while the page is pointed at, and the bar says what to do', (await dialog().count()) === 0 && (await pickBar().count()) === 1, await pickBar().textContent().catch(() => ''))
    // A click is no box: nothing is picked and the page is still pointed at.
    await click(0, [200, 200])
    await settle()
    await step('L4 a click is no box: the page is still pointed at', (await dialog().count()) === 0 && (await pickBar().count()) === 1)
    await drag(0, [100, 100], [250, 160])
    await dialog().waitFor({ timeout: 10_000 })
    const rect = numbers(await field('rect').inputValue())
    await step('L4 the box dragged comes back in Rectangle, in points from the page\'s top-left', near(rect, [100, 100, 250, 160]), JSON.stringify(rect))
    await step('L4 the page it was dragged on comes back in Page', (await field('page').inputValue()) === '1', await field('page').inputValue())
    if (!(await field('toPage').isVisible())) await dialog().locator('summary').click()
    await step('L4 what was typed before is kept', (await field('toPage').inputValue()) === '3', await field('toPage').inputValue())
    await kit.getByTestId('command-dialog-run').click()
    await until(async () => (await dialog().count()) === 0, 40)
    const links = (await ask('link.list')).links ?? []
    const made = links.find((link) => link.page === 1 && link.to_page === 3)
    await step('I2 Run makes the link where the box was dragged', !!made && near(made.rect, rect, 0.6), JSON.stringify(links))
  })

  await part('L4 escape', async () => {
    await openCommand('edit', 'link.add')
    await fill('url', 'https://example.org/kept')
    await kit.getByTestId('pdf-param-pick-rect').click()
    await until(async () => (await pickBar().count()) === 1, 30)
    await page.keyboard.press('Escape')
    await dialog().waitFor({ timeout: 10_000 })
    if (!(await field('url').isVisible())) await dialog().locator('summary').click()
    await step('L4 Escape while pointing brings the form back with what was typed', (await field('url').inputValue()) === 'https://example.org/kept' && (await field('rect').inputValue()) === '' && (await pickBar().count()) === 0, `${await field('url').inputValue()} / ${await field('rect').inputValue()}`)
    // The bar's Back to the form does the same.
    await kit.getByTestId('pdf-param-pick-rect').click()
    await until(async () => (await pickBar().count()) === 1, 30)
    await kit.getByTestId('pdf-pick-cancel').click()
    await dialog().waitFor({ timeout: 10_000 })
    await step('L4 Back to the form brings it back too', (await field('url').inputValue()) === 'https://example.org/kept')
    await page.keyboard.press('Escape')
    await until(async () => (await dialog().count()) === 0, 30)
    await step('L4 the dialog then closes with Escape as before', (await dialog().count()) === 0)
  })

  await part('L4 points', async () => {
    const before = (await ask('measure.list')).count ?? 0
    await openCommand('document', 'measure.distance')
    await kit.getByTestId('pdf-param-pick-points').click()
    await until(async () => (await pickBar().count()) === 1, 30)
    await click(0, [100, 300])
    await until(async () => (await kit.getByTestId('pdf-pick-point').count()) === 1, 30)
    await step('L4 a point clicked is marked on the page', (await kit.getByTestId('pdf-pick-point').count()) === 1)
    await click(0, [300, 300])
    await dialog().waitFor({ timeout: 10_000 })
    const points = numbers(await field('points').inputValue())
    await step('L4 a distance takes two points and comes back with them', near(points, [100, 300, 300, 300]) && (await field('page').inputValue()) === '1', await field('points').inputValue())
    await kit.getByTestId('command-dialog-run').click()
    await until(async () => (await dialog().count()) === 0, 40)
    const listed = await ask('measure.list')
    const made = (listed.measurements ?? []).at(-1)
    const xy = (point) => (Array.isArray(point) ? point : [point?.x, point?.y])
    await step('L4 Run measures between the points clicked', (listed.count ?? 0) === before + 1 && made?.page === 1 && near((made.points ?? []).flatMap(xy), points, 0.6), JSON.stringify(made))
  })

  await part('N S4 check mark', async () => {
    // Page 3 has no form fields.
    await press(`${mod}+PageDown`)
    await press(`${mod}+PageDown`)
    await settle()
    const before = await comments()
    await openCommand('forms', 'fillSign.add')
    await field('type').selectOption('check')
    await kit.getByTestId('pdf-param-pick-at').click()
    await until(async () => (await pickBar().count()) === 1, 30)
    await click(2, [300, 400])
    await dialog().waitFor({ timeout: 10_000 })
    const at = numbers(await field('at').inputValue())
    await step('N S4 the point clicked comes back in Position, and its page', near(at, [300, 400]) && (await field('page').inputValue()) === '3', `${at} p${await field('page').inputValue()}`)
    await kit.getByTestId('command-dialog-run').click()
    await until(async () => (await dialog().count()) === 0, 40)
    const made = (await comments()).filter((item) => !before.some((old) => old.id === item.id && old.page === item.page && old.index === item.index))
    const mark = made.find((item) => item.page === 3)
    const middle = mark?.rect ? [(mark.rect[0] + mark.rect[2]) / 2, (mark.rect[1] + mark.rect[3]) / 2] : []
    await step('N S4 a check mark is put centred where the page was clicked', made.length === 1 && near(middle, at, 2), JSON.stringify(made))
  })

  await part('N S4 Fill & Sign', async () => {
    // Still on page 3, which has no form fields: the Forms tab's Fill & Sign marks go where it is clicked.
    const before = await comments()
    const added = async () => (await comments()).filter((item) => item.page === 3 && !before.some((old) => old.id === item.id))
    await kit.getByTestId('ribbon-tab-forms').click()
    await kit.getByTestId('pdf-fill-check').click()
    await kit.getByTestId('pdf-fill-bar').waitFor({ timeout: 5000 })
    await click(2, [100, 500])
    await until(async () => (await added()).length === 1, 60)
    await click(2, [150, 500])
    await until(async () => (await added()).length === 2, 60)
    const centre = (item) => [(item.rect[0] + item.rect[2]) / 2, (item.rect[1] + item.rect[3]) / 2]
    let made = await added()
    await step('N S4 Check stays on: each click puts a 12 pt check mark centred on it', made.length === 2 && made.every((item) => item.type === 'Stamp' && Math.abs(item.rect[2] - item.rect[0] - 12) < 0.6) && near(centre(made[0]), [100, 500], 2) && near(centre(made[1]), [150, 500], 2), JSON.stringify(made.map((item) => [item.type, item.rect])))
    await drag(2, [200, 500], [260, 540])
    await settle(800)
    await step('N S4 a drag puts nothing', (await added()).length === 2)
    await kit.getByTestId('pdf-fill-text').click()
    await click(2, [100, 600])
    await kit.getByTestId('pdf-note-text').waitFor({ timeout: 5000 })
    await page.keyboard.type('Ada Lovelace')
    await kit.getByTestId('pdf-note-post').click()
    await until(async () => (await added()).some((item) => item.contents === 'Ada Lovelace'), 60)
    made = await added()
    const text = made.find((item) => item.contents === 'Ada Lovelace')
    await step('N S4 Text puts what is typed where the page was clicked', !!text && near([text.rect[0], text.rect[1]], [100, 600], 3), JSON.stringify(text))
    await page.keyboard.press('Escape')
    await until(async () => (await kit.getByTestId('pdf-fill-bar').count()) === 0, 30)
    await step('N S4 Escape stops Fill & Sign', (await kit.getByTestId('pdf-fill-bar').count()) === 0)
    await kit.getByTestId('ribbon-tab-home').click()
    await press('Home')
  })

  await part('redact selection', async () => {
    const marks = async () => (await comments()).filter((item) => item.type === 'Redact')
    const bar = () => kit.getByTestId('pdf-selection-bar')
    const selectLine = async (i) => {
      const from = await toScreen(0, 70, lineMiddle(i))
      const to = await toScreen(0, 400, lineMiddle(i))
      await page.mouse.move(from.x, from.y)
      await page.mouse.down()
      await page.mouse.move(to.x, to.y, { steps: 8 })
      await page.mouse.up()
      await until(async () => (await bar().count()) === 1, 40)
      return { from, to }
    }
    const onLine = (rect, i) => rect && rect[1] >= lineMiddle(i) - 10 && rect[3] <= lineMiddle(i) + 8 && rect[0] >= 68 && rect[0] <= 80
    await selectLine(5)
    await kit.getByTestId('pdf-bar-redact').click()
    await until(async () => (await marks()).length === 1, 60)
    let list = await marks()
    await step('redact: the bar\'s Mark for redaction marks the selected line', list.length === 1 && list[0].page === 1 && onLine(list[0].rect, 5), JSON.stringify(list.map((item) => item.rect)))
    await step('redact: the selection is cleared after marking', (await kit.getByTestId('pdf-selection-highlight').count()) === 0)
    const { from, to } = await selectLine(9)
    await page.mouse.click((from.x + to.x) / 2, from.y, { button: 'right' })
    await kit.getByTestId('pdf-menu-selection').waitFor({ timeout: 5000 })
    await kit.getByTestId('pdf-menu-redact').click()
    await until(async () => (await marks()).length === 2, 60)
    list = await marks()
    await step('redact: the selection\'s right-click menu marks it too', list.length === 2 && list.some((item) => onLine(item.rect, 9)), JSON.stringify(list.map((item) => item.rect)))
    // Apply asks first; Cancel keeps the marks.
    await kit.getByTestId('ribbon-tab-protect').click()
    await kit.getByTestId('pdf-cmd-redaction.apply').first().click()
    const confirm = kit.getByTestId('pdf-apply-redactions')
    await confirm.waitFor({ timeout: 10_000 })
    await step('redact: Apply redactions asks first, naming how many marks', ((await kit.getByTestId('pdf-apply-redactions-text').textContent()) ?? '').includes('2 redaction marks'), await kit.getByTestId('pdf-apply-redactions-text').textContent())
    await kit.getByTestId('pdf-apply-redactions-cancel').click()
    await until(async () => (await confirm.count()) === 0, 30)
    await step('redact: Cancel keeps the marks and the text', (await marks()).length === 2 && ((await ask('text.find', { query: '1.5 A quiet aurora' })).count ?? 0) === 1)
    await kit.getByTestId('pdf-cmd-redaction.apply').first().click()
    await confirm.waitFor({ timeout: 10_000 })
    await kit.getByTestId('pdf-apply-redactions-apply').click()
    await until(async () => (await marks()).length === 0, 80)
    const left = (await ask('text.find', { query: '1.5 A quiet aurora' })).count ?? -1
    const kept = (await ask('text.find', { query: '1.6 A quiet' })).count ?? -1
    await step('redact: Apply removes the marked lines and keeps the rest', (await marks()).length === 0 && left === 0 && kept === 1, `marks ${(await marks()).length}, marked line ${left}, next line ${kept}`)
    await kit.getByTestId('ribbon-tab-home').click()
  })
} catch (error) {
  await step('ran to the end', false, error instanceof Error ? error.message.split('\n').slice(0, 6).join(' / ') : String(error))
}
await finish()
