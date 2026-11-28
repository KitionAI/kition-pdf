// Organizing pages and bookmarks by hand (plan 2026-11-26 "PDF usable by hand", stage 7):
// node tests/hand/pages.mjs [chromium|webkit]
// Real pointer and keys only, on the hand fixture with its bookmarks (fixture.mjs OUTLINE, and
// /PageMode /UseOutlines). The file opens with its bookmarks shown (A2); a folded bookmark unfolds
// and folds (F2); the arrows walk the bookmarks and Enter goes to one; a double-click renames in
// place, Escape drops it, one undo brings the old title back (F3); Delete deletes one; the menu adds
// one under another; Cmd+B writes a new one for the current page in place (F4). In the thumbnails:
// thumbnail 3 dragged above thumbnail 1 is one page.move, one undo (E2); Escape during a drag and a
// drop outside the list move nothing; Shift+click selects a run, Cmd+click adds a page (E3); the
// right-click menu rotates, duplicates and inserts (E4); Delete deletes the selected pages, never
// every page, and the keyboard stays on the thumbnails (E5); the ribbon's Rotate acts on the
// selection. The Pages tab's buttons each have their own icon and name themselves with the kit's
// tooltip in Single line (E9). Results are read back with bookmark.list, text.extract and inspect.
// Prints PASS or FAIL per step.
import { OUTLINE, PAGES, handPdf } from './fixture.mjs'
import { openHand } from './harness.mjs'

const which = process.argv[2] ?? 'chromium'
const hand = await openHand('hand-pages', which)
const { page, kit, frame, step, until, part, finish, mod, status, openFixture, ask, pages, focused, settle, press } = hand

const titles = async () => {
  const flat = []
  const walk = (list, depth) => list.forEach((item) => { flat.push(`${'-'.repeat(depth)}${item.title}@${item.page}`); walk(item.children ?? [], depth + 1) })
  walk((await ask('bookmark.list')).bookmarks ?? [], 0)
  return flat
}
const row = (title) => kit.getByTestId('pdf-bookmark').filter({ hasText: title })
const visibleTitles = async () => (await kit.getByTestId('pdf-bookmark').locator('.pdf-bookmark-text').allTextContents())
/** The heading of each page, in order ("Page N heading" names the page it was made as). */
const order = async () => {
  const count = (await pages()).length
  const result = []
  for (let n = 1; n <= count; n += 1) {
    const text = JSON.stringify(await ask('text.extract', { pages: [n] }))
    result.push(Number(text.match(/Page (\d+) heading/)?.[1] ?? 0))
  }
  return result
}
const thumb = (index) => kit.getByTestId(`pdf-thumb-${index}`)
const centre = async (locator) => {
  const box = await locator.boundingBox()
  return { x: box.x + box.width / 2, y: box.y + box.height / 2, box }
}
const selectedThumbs = () => frame().evaluate(() => [...document.querySelectorAll('.pdf-thumb[data-selected="true"]')].map((element) => Number(element.dataset.testid.replace('pdf-thumb-', ''))))
const undo = async () => press(`${mod}+z`)
/** Goes to page `number` with the page field. */
async function showPage(number) {
  await kit.getByTestId('pdf-page-field').click()
  await page.keyboard.press(`${mod}+a`)
  await page.keyboard.type(String(number))
  await press('Enter')
  await until(async () => (await status()) === `Page ${number} of ${PAGES}`, 40)
}
/** Drags from one point to another in small steps, holding the button a moment first. */
async function drag(from, to, { release = true } = {}) {
  await page.mouse.move(from.x, from.y)
  await page.mouse.down()
  await page.waitForTimeout(80)
  await page.mouse.move(from.x, from.y + 8, { steps: 3 })
  await page.mouse.move(to.x, to.y, { steps: 16 })
  await page.waitForTimeout(120)
  if (release) await page.mouse.up()
}

try {
  await openFixture(handPdf({ outline: true }))

  await part('A2 opens with the bookmarks', async () => {
    await until(async () => (await kit.getByTestId('pdf-bookmarks').count()) > 0, 40)
    await step('A2 a file with /PageMode /UseOutlines opens with its bookmarks shown', (await kit.getByTestId('pdf-panel-bookmarks').getAttribute('aria-pressed')) === 'true' && (await kit.getByTestId('pdf-bookmarks').count()) === 1)
    await step('A2 the keyboard is the pages\' all the same', (await focused()) === 'pages', await focused())
  })

  await part('F2 folding', async () => {
    const shown = await visibleTitles()
    await step('F2 an unfolded bookmark shows its children, a folded one hides them', shown.includes('Chapter 1') && !shown.includes('Chapter 3'), shown.join(', '))
    const toggle = kit.locator('.pdf-bookmark-row').filter({ hasText: 'Part two' }).getByTestId('pdf-bookmark-toggle')
    await toggle.click()
    await until(async () => (await visibleTitles()).includes('Chapter 3'), 20)
    await step('F2 its arrow unfolds a bookmark', (await visibleTitles()).includes('Chapter 3'))
    await toggle.click()
    await until(async () => !(await visibleTitles()).includes('Chapter 3'), 20)
    await step('F2 and folds it again (the file is not changed)', !(await visibleTitles()).includes('Chapter 3') && !(await hand.inspect()).dirty)
  })

  await part('F3 keys', async () => {
    await row('Part one').click()
    await until(async () => (await status()) === `Page 1 of ${PAGES}`, 20)
    await press('ArrowDown')
    await step('F3 Down goes to the next bookmark', (await frame().evaluate(() => document.activeElement?.textContent ?? '')).startsWith('Chapter 1'))
    await press('ArrowDown')
    await press('Enter')
    await until(async () => (await status()) === `Page 3 of ${PAGES}`, 30)
    await step('F3 Enter goes to its page', (await status()) === `Page 3 of ${PAGES}`, await status())
    await press('ArrowUp')
    await press('ArrowUp')
    await press('ArrowLeft')
    await step('F3 Left folds a bookmark', !(await visibleTitles()).includes('Chapter 1'), (await visibleTitles()).join(', '))
    await press('ArrowRight')
    await step('F3 Right unfolds it', (await visibleTitles()).includes('Chapter 1'))
  })

  await part('F3 rename', async () => {
    const before = await titles()
    const target = await centre(row('Chapter 2'))
    await page.mouse.dblclick(target.x, target.y)
    const field = kit.getByTestId('pdf-bookmark-title')
    await field.waitFor({ timeout: 3000 })
    await page.keyboard.type('Methods')
    await press('Escape')
    await step('F3 Escape drops a rename: nothing changes', JSON.stringify(await titles()) === JSON.stringify(before) && (await field.count()) === 0)
    await page.mouse.dblclick(target.x, target.y)
    await field.waitFor({ timeout: 3000 })
    await step('F3 a double-click writes the title in place, all of it chosen', (await field.evaluate((element) => element.selectionStart === 0 && element.selectionEnd === element.value.length && element.value === 'Chapter 2')))
    await page.keyboard.type('Methods')
    await press('Enter')
    await until(async () => (await titles()).includes('-Methods@3'), 30)
    await step('F3 Enter keeps the new title (bookmark_rename)', (await titles()).includes('-Methods@3'), (await titles()).join(', '))
    await step('F3 the keyboard is back on the bookmark', (await frame().evaluate(() => document.activeElement?.textContent ?? '')).startsWith('Methods'), await focused())
    await undo()
    await until(async () => (await titles()).includes('-Chapter 2@3'), 30)
    await step('F3 one undo brings the old title back', JSON.stringify(await titles()) === JSON.stringify(before), (await titles()).join(', '))
  })

  await part('F3 delete and menu', async () => {
    await row('Appendix').click()
    await press('Delete')
    await until(async () => !(await titles()).includes('Appendix@8'), 30)
    await step('F3 Delete deletes the bookmark', !(await titles()).includes('Appendix@8'), (await titles()).join(', '))
    await step('F3 the keyboard stays on the bookmarks', ['pdf-bookmark', 'pdf-bookmarks'].includes(await focused()), await focused())
    await undo()
    await until(async () => (await titles()).includes('Appendix@8'), 30)
    await step('F3 undo brings it back', (await titles()).includes('Appendix@8'))
    await showPage(4)
    const target = await centre(row('Part one'))
    await page.mouse.click(target.x, target.y, { button: 'right' })
    await kit.getByTestId('pdf-bookmark-menu').waitFor({ timeout: 3000 })
    await step('F3 a right-click opens the bookmark\'s menu', (await kit.getByTestId('pdf-bookmark-menu-rename').count()) === 1 && (await kit.getByTestId('pdf-bookmark-menu-delete').count()) === 1)
    await kit.getByTestId('pdf-bookmark-menu-child').click()
    await kit.getByTestId('pdf-bookmark-title').waitFor({ timeout: 3000 })
    await page.keyboard.type('Section 1a')
    await press('Enter')
    await until(async () => (await titles()).some((title) => title.startsWith('-Section 1a@')), 30)
    const added = (await titles()).find((title) => title.startsWith('-Section 1a@'))
    await step('F3 Add a bookmark under it: a child, for the current page', added === '-Section 1a@1' || added === '-Section 1a@4', `${added}; ${(await titles()).join(', ')}`)
    await undo()
  })

  await part('F4 new bookmark', async () => {
    await showPage(5)
    const before = await titles()
    await press(`${mod}+b`)
    const field = kit.getByTestId('pdf-bookmark-title')
    await field.waitFor({ timeout: 3000 })
    await step('F4 Cmd+B writes a new bookmark in place, no dialog', (await kit.locator('[role="dialog"]').count()) === 0 && (await field.inputValue()) === 'Untitled')
    await press('Escape')
    await step('F4 Escape adds nothing', JSON.stringify(await titles()) === JSON.stringify(before))
    await press(`${mod}+b`)
    await field.waitFor({ timeout: 3000 })
    await page.keyboard.type('Results')
    await press('Enter')
    await until(async () => (await titles()).includes('Results@5'), 30)
    await step('F4 Enter adds it at the top level, for the current page', (await titles()).at(-1) === 'Results@5', (await titles()).join(', '))
    await undo()
    await until(async () => JSON.stringify(await titles()) === JSON.stringify(before), 30)
    await step('F4 one undo takes it away', JSON.stringify(await titles()) === JSON.stringify(before))
  })

  await kit.getByTestId('pdf-panel-pages').click()
  await showPage(1)
  await settle(500)

  await part('E2 drag', async () => {
    const start = await order()
    const from = await centre(thumb(2))
    const above = (await thumb(0).boundingBox())
    await drag(from, { x: from.x, y: above.y + 4 }, { release: false })
    await step('E2 a line shows where the page goes', (await kit.getByTestId('pdf-thumb-drop').count()) === 1)
    await page.mouse.up()
    await until(async () => (await order())[0] === 3, 40)
    const moved = await order()
    await step('E2 thumbnail 3 dropped above thumbnail 1 moves page 3 to the front', JSON.stringify(moved) === JSON.stringify([3, 1, 2, 4, 5, 6, 7, 8]), moved.join(','))
    await step('E2 the moved page stays selected', JSON.stringify(await selectedThumbs()) === '[0]', JSON.stringify(await selectedThumbs()))
    await step('E2 the keyboard stays on the thumbnails', (await focused()).startsWith('pdf-thumb-'), await focused())
    await undo()
    await until(async () => (await order())[0] === 1, 40)
    await step('E2 one undo puts it back', JSON.stringify(await order()) === JSON.stringify(start), (await order()).join(','))
    // Escape during a drag, and a drop outside the list, move nothing.
    const a = await centre(thumb(1))
    await drag(a, { x: a.x, y: (await thumb(0).boundingBox()).y + 4 }, { release: false })
    await page.keyboard.press('Escape')
    await page.mouse.up()
    await settle(400)
    const outside = await centre(thumb(1))
    await drag(outside, { x: outside.x - 400, y: outside.y - 100 })
    await settle(400)
    await step('E2 Escape during a drag, or a drop outside the list, moves nothing', JSON.stringify(await order()) === JSON.stringify(start), (await order()).join(','))
  })

  await part('E3 E5 selection and Delete', async () => {
    await thumb(0).click()
    await page.keyboard.down('Shift')
    await thumb(1).click()
    await page.keyboard.up('Shift')
    await settle(200)
    await step('E3 Shift+click selects a run of pages', JSON.stringify(await selectedThumbs()) === '[0,1]', JSON.stringify(await selectedThumbs()))
    await step('E3 the panel says how many', ((await kit.getByTestId('pdf-pages-selected').textContent()) ?? '').includes('2 pages selected'))
    await press('Delete')
    await until(async () => (await pages()).length === PAGES - 2, 40)
    await step('E5 Delete deletes the selected pages, one step', (await pages()).length === PAGES - 2 && JSON.stringify(await order()) === JSON.stringify([3, 4, 5, 6, 7, 8]), (await order()).join(','))
    await step('E5 the keyboard stays on the thumbnails', (await focused()).startsWith('pdf-thumb-'), await focused())
    await undo()
    await until(async () => (await pages()).length === PAGES, 40)
    await step('E5 one undo brings both back', JSON.stringify(await order()) === JSON.stringify([1, 2, 3, 4, 5, 6, 7, 8]))
    await thumb(0).click()
    await press(`${mod}+a`)
    await step('E3 Cmd+A on the thumbnails selects every page', (await selectedThumbs()).length >= 3, JSON.stringify(await selectedThumbs()))
    await press('Delete')
    await settle(500)
    await step('E5 Delete never deletes every page', (await pages()).length === PAGES)
    await press('Escape')
    await thumb(0).click()
    await page.keyboard.down(mod)
    await thumb(2).click()
    await page.keyboard.up(mod)
    await settle(200)
    await step('E3 Cmd+click adds a page to the selection', JSON.stringify(await selectedThumbs()) === '[0,2]', JSON.stringify(await selectedThumbs()))
    await step('E3 without going to it', (await status()) === `Page 1 of ${PAGES}`, await status())
    await kit.getByTestId('ribbon-tab-home').click()
    await kit.getByTestId('pdf-rotate-right').click()
    await until(async () => { const list = await pages(); return list[0].area.width > list[0].area.height && list[2].area.width > list[2].area.height }, 40)
    const list = await pages()
    await step('E3 the ribbon\'s Rotate acts on the selected pages', list[0].area.width > list[0].area.height && list[2].area.width > list[2].area.height && list[1].area.width < list[1].area.height)
    await undo()
    await until(async () => { const now = await pages(); return now[0].area.width < now[0].area.height }, 40)
  })

  await part('E4 menu', async () => {
    await thumb(1).click()
    const p = await centre(thumb(1))
    await page.mouse.click(p.x, p.y, { button: 'right' })
    await kit.getByTestId('pdf-thumb-menu').waitFor({ timeout: 3000 })
    const rows = []
    for (const id of ['pdf-thumb-menu-rotate-left', 'pdf-thumb-menu-rotate-right', 'pdf-thumb-menu-delete', 'pdf-thumb-menu-insert', 'pdf-thumb-menu-duplicate']) rows.push(await kit.getByTestId(id).count())
    await step('E4 a right-click opens the pages\' menu: rotate left and right, insert, duplicate, delete', rows.every((n) => n === 1), rows.join(','))
    await kit.getByTestId('pdf-thumb-menu-rotate-right').click()
    await until(async () => { const area = (await pages())[1].area; return area.width > area.height }, 40)
    await step('E4 Rotate right turns the page', (await pages())[1].area.width > (await pages())[1].area.height)
    await undo()
    await page.mouse.click(p.x, p.y, { button: 'right' })
    await kit.getByTestId('pdf-thumb-menu-duplicate').click()
    await until(async () => (await pages()).length === PAGES + 1, 40)
    await step('E4 Duplicate puts a copy after the page', JSON.stringify((await order()).slice(0, 4)) === '[1,2,2,3]', (await order()).join(','))
    await undo()
    await until(async () => (await pages()).length === PAGES, 40)
    await page.mouse.click(p.x, p.y, { button: 'right' })
    await kit.getByTestId('pdf-thumb-menu-insert').click()
    await until(async () => (await pages()).length === PAGES + 1, 40)
    await step('E4 Insert puts a blank page after it', JSON.stringify((await order()).slice(0, 4)) === '[1,2,0,3]', (await order()).join(','))
    await undo()
    await until(async () => (await pages()).length === PAGES, 40)
  })

  await part('E9 Pages tab', async () => {
    await kit.getByTestId('ribbon-tab-pages').click()
    await settle(300)
    const ids = ['pdf-pages-rotate-left', 'pdf-pages-rotate-right', 'pdf-pages-insert', 'pdf-pages-duplicate', 'pdf-pages-delete', 'pdf-cmd-page.move', 'pdf-pages-number', 'pdf-pages-new-bookmark']
    const icons = []
    for (const id of ids) icons.push(await kit.getByTestId(id).first().locator('svg').evaluate((svg) => svg.innerHTML).catch(() => 'missing'))
    await step('E9 every page button of the Pages tab has its own icon', !icons.includes('missing') && new Set(icons).size === icons.length, `${new Set(icons).size} of ${icons.length}`)
    const button = kit.getByTestId('pdf-cmd-page.move').first()
    await step('E9 no browser title on the row\'s buttons', (await button.getAttribute('title')) === null)
    await button.hover()
    await until(async () => (await kit.getByTestId('pdf-ribbon-tip').count()) > 0, 30)
    await step('E9 resting on one shows its name in the kit\'s tooltip', ((await kit.getByTestId('pdf-ribbon-tip').first().textContent()) ?? '').startsWith('Move Pages'), await kit.getByTestId('pdf-ribbon-tip').first().textContent().catch(() => 'none'))
    await kit.getByTestId('pdf-thumb-3').click()
    await kit.getByTestId('pdf-cmd-page.move').first().click()
    await kit.locator('[role="dialog"]').waitFor({ timeout: 3000 })
    await step('E9 Move Pages starts from the page acted on', (await kit.locator('#param-pages').inputValue()).replace(/\s/g, '') === '4', await kit.locator('#param-pages').inputValue())
    await press('Escape')
  })
} finally {
  await finish()
}
