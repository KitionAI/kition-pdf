// The form fields panel and the parameters dialog by hand (plan 2026-11-26 "PDF usable by hand",
// stage 2): node tests/hand/fields-panel.mjs [chromium|webkit]
// The panel draws each field for its type: a drop-down lists its options by name, radio buttons are
// a group, labels are the fields' tooltips with their type and whether they are required (H4); a
// value an undo takes back shows in its box (H6). The parameters dialog names its fields as people
// do, takes pages as 3 and 2-4 and starts on the current page; a value it cannot use is refused at
// its field, and a command that fails keeps the dialog and what was typed, the error under the
// field it names (E8, L4). In Chinese the Comments tab and the dialog's fields are Chinese (L3).
// Real pointer and keys; a drop-down's option is picked with selectOption (a native list's popup is
// the browser's own). Prints PASS or FAIL per step.
import { COUNTRIES, PAGES, SIZES, handPdf } from './fixture.mjs'
import { FILE, openHand } from './harness.mjs'

const which = process.argv[2] ?? 'chromium'
const hand = await openHand('hand-fields', which)
const { page, kit, step, until, part, finish, mod, status, openFixture, ask, focused, press } = hand

const fields = async () => (await ask('form.listFields')).fields ?? []
const value = async (name) => (await fields()).find((item) => item.name === name)?.value ?? null
/** The first line of page `n`'s text: its heading says which page it was. */
const heading = async (n) => /Page (\d+) heading/.exec(JSON.stringify(await ask('text.extract', { pages: [n] })))?.[1] ?? null
const dialog = () => kit.getByTestId('pdf-params-dialog')

try {
  await openFixture()

  await part('H4', async () => {
    await kit.getByTestId('pdf-panel-fields').click()
    const country = kit.getByTestId('pdf-field-country')
    await country.waitFor({ timeout: 10_000 })
    const options = await country.locator('option').allTextContents()
    await step('H4 a drop-down lists its options by name', await country.evaluate((element) => element.tagName === 'SELECT') && COUNTRIES.every(([, label]) => options.includes(label)), JSON.stringify(options))
    await step('H4 a field shows its label, its type and whether it is required', (await kit.getByTestId('pdf-field-row-country').textContent()).includes('Country of residence') && (await kit.getByTestId('pdf-field-kind-country').textContent()) === 'Drop-down' && (await kit.getByTestId('pdf-field-kind-city').textContent()).includes('Required'), `${await kit.getByTestId('pdf-field-kind-country').textContent()} / ${await kit.getByTestId('pdf-field-kind-city').textContent()}`)
    await country.selectOption({ label: 'Iceland' })
    await until(async () => (await value('country')) === 'IS', 60)
    await step('H4 picking a drop-down\'s option fills it (by its export value)', (await value('country')) === 'IS', String(await value('country')))
    const radios = kit.getByTestId('pdf-field-size').locator('input[type="radio"]')
    await step('H4 radio buttons are a group of their options', (await radios.count()) === SIZES.length, String(await radios.count()))
    await kit.getByTestId('pdf-field-size').getByText('M', { exact: true }).click()
    await until(async () => (await value('size')) === 'M', 60)
    await step('H4 a radio button fills its group', (await value('size')) === 'M', String(await value('size')))
    await kit.getByTestId('pdf-field-subscribe').click()
    await until(async () => (await value('subscribe')) === true, 60)
    await step('H4 a check box fills', (await value('subscribe')) === true)
  })

  await part('H6', async () => {
    const city = kit.getByTestId('pdf-field-city')
    await city.click()
    await page.keyboard.type('Oslo')
    await page.keyboard.press('Enter')
    await until(async () => (await value('city')) === 'Oslo', 60)
    await step('H6 (setup) the city is filled', (await value('city')) === 'Oslo')
    await until(async () => kit.getByTestId('pdf-undo').isEnabled(), 40)
    await page.keyboard.press(`${mod}+z`)
    await until(async () => !(await value('city')), 60)
    await until(async () => (await kit.getByTestId('pdf-field-city').inputValue()) === '', 40)
    await step('H6 after an undo the box shows the field\'s value again', !(await value('city')) && (await kit.getByTestId('pdf-field-city').inputValue()) === '', `engine ${JSON.stringify(await value('city'))}, box ${JSON.stringify(await kit.getByTestId('pdf-field-city').inputValue())}`)
    await kit.getByTestId('pdf-panel-pages').click()
  })

  await part('E8 L4', async () => {
    await press('End')
    await until(async () => (await status()) === `Page ${PAGES} of ${PAGES}`, 30)
    await press('Home')
    await kit.getByTestId('ribbon-tab-pages').click()
    const move = kit.getByTestId('pdf-cmd-page.move')
    await move.click()
    await dialog().waitFor({ timeout: 10_000 })
    const pagesField = kit.locator('#param-pages')
    await step('L4 the pages field starts on the current page, without brackets', (await pagesField.inputValue()) === '1', await pagesField.inputValue())
    const labels = await dialog().locator('label').allTextContents()
    await step('L4 the fields have people\'s names', labels.some((text) => text.startsWith('Pages')) && labels.some((text) => text.startsWith('To')), JSON.stringify(labels))
    // Something that is no page: refused at the field, before anything runs; the rest is kept.
    await pagesField.click()
    await page.keyboard.press(`${mod}+a`)
    await page.keyboard.type('three')
    await kit.locator('#param-to').click()
    await page.keyboard.type('1')
    await kit.getByTestId('command-dialog-run').click()
    await kit.getByTestId('pdf-param-error-pages').waitFor({ timeout: 5000 })
    await step('L4 a value that is no list of pages is refused at its field, the dialog stays', (await dialog().count()) === 1 && (await kit.locator('#param-to').inputValue()) === '1')
    // A page the document does not have: the command fails, and the dialog keeps what was typed.
    await pagesField.click()
    await page.keyboard.press(`${mod}+a`)
    await page.keyboard.type('99')
    await kit.getByTestId('command-dialog-run').click()
    const failed = await until(async () => (await kit.getByTestId('pdf-param-error-pages').count()) + (await kit.getByTestId('pdf-param-failure').count()) > 0 && (await kit.getByTestId('pdf-param-error-pages').textContent().catch(() => '')) !== 'Type page numbers such as 3, or 3, 5, or 2-4', 60)
    const message = (await kit.getByTestId('pdf-param-error-pages').textContent().catch(() => '')) || (await kit.getByTestId('pdf-param-failure').textContent().catch(() => ''))
    await step('L4 a command that fails keeps the dialog and what was typed, and says why', failed && (await dialog().count()) === 1 && (await pagesField.inputValue()) === '99' && (await kit.locator('#param-to').inputValue()) === '1', message)
    // E8: 3 moves page 3 to the front.
    await pagesField.click()
    await page.keyboard.press(`${mod}+a`)
    await page.keyboard.type('3')
    await kit.getByTestId('command-dialog-run').click()
    await until(async () => (await dialog().count()) === 0, 60)
    await until(async () => (await heading(1)) === '3', 60)
    await step('E8 pages typed as 3 moves page 3 to the front', (await heading(1)) === '3' && (await heading(2)) === '1', `${await heading(1)}, ${await heading(2)}`)
    await until(async () => (await focused()) === 'pages', 20)
    await step('E8 after the dialog ran, the pages have the keyboard', (await focused()) === 'pages', await focused())
    // 2-4: a range.
    await move.click()
    await dialog().waitFor({ timeout: 10_000 })
    await pagesField.click()
    await page.keyboard.press(`${mod}+a`)
    await page.keyboard.type('6-8')
    await kit.locator('#param-to').click()
    await page.keyboard.type('1')
    await page.keyboard.press('Enter')
    await until(async () => (await dialog().count()) === 0, 60)
    await until(async () => (await heading(1)) === '6', 60)
    await step('E8 pages typed as 6-8 move three pages', (await heading(1)) === '6' && (await heading(2)) === '7' && (await heading(3)) === '8', `${await heading(1)}, ${await heading(2)}, ${await heading(3)}`)
    await kit.getByTestId('ribbon-tab-home').click()
  })

  await part('L3', async () => {
    // The same file in Chinese, in a second tab of the same browser.
    const zh = await page.context().newPage()
    await zh.goto(page.url().replace(/\?.*$/, '?locale=zh-CN'))
    await zh.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 90_000 })
    await zh.setInputFiles('#file', { name: FILE, mimeType: 'application/pdf', buffer: handPdf() })
    await zh.waitForFunction((name) => document.getElementById('status')?.textContent?.includes(`${name} open in`), FILE, { timeout: 90_000 })
    const kitZh = zh.frameLocator('#kit')
    await kitZh.getByTestId('pdf-status').waitFor({ timeout: 30_000 })
    await kitZh.getByTestId('ribbon-tab-comment').click()
    await zh.waitForTimeout(500)
    const english = ['List comments', "Set a comment's status", 'Mark a comment with a checkmark', 'Lock a comment', 'Reply to a comment', 'Edit a comment', 'Delete a comment', 'Preview image signature layers']
    const shown = await zh.frames().find((frame) => frame !== zh.mainFrame()).evaluate(() => [...document.querySelectorAll('[data-testid^="pdf-cmd-comment"]')].map((button) => `${button.getAttribute('aria-label') ?? ''} ${button.getAttribute('title') ?? ''} ${button.textContent ?? ''}`))
    await step('L3 the Comments tab\'s commands are Chinese', shown.length > 0 && !shown.some((text) => english.some((title) => text.includes(title))), JSON.stringify(shown.slice(0, 12)))
    await kitZh.getByTestId('ribbon-tab-home').click()
    await kitZh.getByTestId('pdf-sticky-note').click()
    await kitZh.getByTestId('pdf-params-dialog').waitFor({ timeout: 10_000 })
    const labels = (await kitZh.getByTestId('pdf-params-dialog').locator('label').allTextContents()).map((text) => text.replace(/\s*\*$/, '').trim())
    await step('L3 the dialog\'s fields are named in Chinese, not by their parameters', labels.includes('位置') && labels.includes('内容') && labels.includes('页面') && !labels.some((text) => /^(at|contents|page|type)$/.test(text)), JSON.stringify(labels))
    await zh.close()
  })
} catch (error) {
  await step('ran to the end', false, error instanceof Error ? error.message.split('\n').slice(0, 6).join(' / ') : String(error))
}
await finish()
