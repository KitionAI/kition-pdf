// An agent's draft and a restored version in the editor (Kit UI protocol sections 7d and 7m), through
// the development host with a replica and checkpoints (`?replica=1&checkpoints=1`) on the sample PDF.
// The edits are comments: the person adds them with the Sticky note button, the agent on its draft.
// a. The person comments on page 1; the agent's draft comments on page 2 and is compared (the
//    comparison names it and the document does not change); the person comments on page 1 again
//    meanwhile; the draft merges: the Comments panel lists all three.
// b. The editor's Undo takes back the draft's comment only; Redo brings it back.
// c. A draft and the person both comment on page 1 (both rewrite page 1's object): the merge is
//    refused, the refusal names the key both wrote, and the document is unchanged.
// d. A labelled checkpoint of the saved document; a later comment; the checkpoint restored through
//    the host: the later comment is gone; the editor's Undo brings it back.
// e. Saved and reloaded (`devHost.reload()`): the comments are as saved, the checkpoint still listed.
// The editor has no draft bar and no "save a version" control: the host merges and compares the
// draft and records the checkpoint (`devHost.recordCheckpoint`), as Kition's own panels would.
// node tests/drafts.mjs [chromium|webkit]
import assert from 'node:assert/strict'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'

const which = process.argv[2] ?? 'chromium'
const path = 'Sample.pdf'
const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } })
const kit = page.frameLocator('#kit')
const errors = []
page.on('pageerror', (error) => errors.push(error.message))
const run = (command, params = {}, options = {}) =>
  page.evaluate(async ({ command, params, options }) => (await window.devHost.connection().ai.run([{ command, params }], false, options)).completed[0].result, { command, params, options })
const until = async (check, message) => {
  for (let n = 0; n < 300; n += 1) {
    if (await check().catch(() => false)) return
    await page.waitForTimeout(100)
  }
  assert.fail(typeof message === 'function' ? await message() : message)
}
/** What the editor's Comments panel lists: each comment's text, in its order. */
const shown = async () => (await kit.getByTestId('pdf-comment').allTextContents()).map((text) => text.trim())
const showsText = async (text) => (await shown()).some((item) => item.endsWith(text))
/** The comments' contents as the engine answers them, sorted (the draft's with `draft`). */
const contents = async (draft = false) => ((await run('comment.list', {}, draft ? { draft: true } : {})).comments ?? []).map((item) => item.contents ?? '').sort()
const version = () => page.evaluate(() => window.devHost.connection().ai.version())
const fileVersion = () => page.evaluate((path) => window.devHost.files.get(path)?.version ?? 0, path)
/** The person adds a sticky note on `at` page with the parameters dialog under the Home tab's More. */
const noteByHand = async (pageNumber, text) => {
  const before = (await shown()).length
  await kit.getByTestId('ribbon-tab-home').click()
  // The parameters dialog of a sticky note is under the Comment group's More (Sticky Note is the note tool).
  await kit.getByTestId('pdf-comment-more').click()
  await kit.getByTestId('pdf-note-dialog').click()
  await kit.locator('#param-page').fill(String(pageNumber))
  await kit.locator('#param-contents').fill(text)
  await kit.getByTestId('command-dialog-run').click()
  await until(async () => (await shown()).length === before + 1 && (await showsText(text)), async () => `the person's comment "${text}" shows: ${JSON.stringify(await shown())}`)
}
const press = async (name) => {
  await until(() => kit.getByTestId(`pdf-${name}`).isEnabled(), `${name} is enabled`)
  await kit.getByTestId(`pdf-${name}`).click()
}
const openComments = async () => {
  await kit.getByTestId('pdf-panel-comments').click()
  await until(async () => (await shown()).length > 0, 'the Comments panel lists the sample\'s comments')
}
const save = async () => {
  const saved = await fileVersion()
  await press('save')
  await until(async () => (await fileVersion()) > saved, 'saved')
  await until(() => page.evaluate((path) => window.devHost.history.replica(path)?.fileStamp != null, path), 'the replica is stored with the save')
}
let failures = 0

try {
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/dev.html?locale=en-US&replica=1&checkpoints=1`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await openComments()
  const sample = (await shown()).length

  // a. The person comments; the agent's draft comments on another page and is compared.
  await noteByHand(1, 'Person first')
  await run('comment.add', { page: 2, type: 'note', at: [40, 40], contents: 'From the draft' }, { draft: true })
  assert.ok((await contents(true)).includes('From the draft'), 'the draft holds the agent\'s comment')
  const beforeCompare = await version()
  const comparison = await page.evaluate(() => window.devHost.connection().draft.compare({ maxSize: 256 }))
  const summary = comparison.summary
  assert.ok([...summary.created, ...summary.changed].some((change) => change.type === 'Comment'), `the comparison lists the draft's comment: ${JSON.stringify(summary)}`)
  assert.equal(await version(), beforeCompare, 'comparing changes nothing')
  assert.ok(!(await showsText('From the draft')), 'the document does not show the draft\'s comment')
  assert.ok(!(await contents()).includes('From the draft'))
  // The person comments again meanwhile, then accepts the draft (the host passes the agent's origin).
  await noteByHand(1, 'Person meanwhile')
  const merged = await page.evaluate(() => window.devHost.connection().draft.merge({ kind: 'agent', principal: 'agent:test' }))
  assert.equal(merged.merged, true, 'the draft merged')
  await until(() => showsText('From the draft'), async () => `the merged comment shows: ${JSON.stringify(await shown())}`)
  assert.ok((await showsText('Person first')) && (await showsText('Person meanwhile')), 'both of the person\'s comments stay')
  assert.equal((await shown()).length, sample + 3)
  console.log(`PASS ${which}: a draft compared and merged keeps the person's edits made meanwhile`)

  // b. One Undo takes back the draft's comment only; Redo brings it back.
  await press('undo')
  await until(async () => !(await showsText('From the draft')), 'Undo takes the draft back')
  assert.ok((await showsText('Person first')) && (await showsText('Person meanwhile')), 'the person\'s comments stay')
  assert.equal((await shown()).length, sample + 2)
  await press('redo')
  await until(() => showsText('From the draft'), 'Redo brings the draft back')
  assert.equal((await shown()).length, sample + 3)
  console.log(`PASS ${which}: one Undo takes the draft back, Redo brings it again`)

  // c. A draft and the person both comment on page 1: the merge is refused and says why.
  await run('comment.add', { page: 1, type: 'note', at: [60, 60], contents: 'Draft same page' }, { draft: true })
  await noteByHand(1, 'Person same page')
  const listed = await shown()
  const at = await version()
  const refusal = await page.evaluate(async () => {
    try {
      await window.devHost.connection().draft.merge({ kind: 'agent', principal: 'agent:test' })
      return null
    } catch (error) {
      return { message: String(error?.message ?? error), reason: error?.data?.reason ?? null }
    }
  })
  assert.ok(refusal, 'the conflicting draft is refused')
  assert.match(refusal.message, /stale/, `the refusal says the draft is stale: ${JSON.stringify(refusal)}`)
  assert.match(refusal.message, /overlay\["[^"]+"\]/, `the refusal names the key both sides wrote: ${refusal.message}`)
  assert.equal(await version(), at, 'a refused merge changes nothing')
  assert.deepEqual(await shown(), listed, 'the editor shows the same comments')
  assert.ok(!(await showsText('Draft same page')))
  const discarded = await page.evaluate(() => window.devHost.connection().draft.discard())
  assert.equal(discarded.state, 'none', 'the refused draft is discarded')
  console.log(`PASS ${which}: a draft that rewrites what the person rewrote is refused, naming it`)

  // d. A labelled checkpoint; a later comment; the checkpoint restored; Undo brings the comment back.
  await save()
  const checkpoint = await page.evaluate(() => window.devHost.recordCheckpoint({ origin: 'save', label: 'Before the review' }))
  assert.ok(checkpoint?.id, 'a checkpoint is recorded')
  assert.equal(checkpoint.label, 'Before the review')
  const atCheckpoint = await shown()
  await noteByHand(2, 'After the checkpoint')
  await page.evaluate((id) => window.devHost.restoreCheckpoint(id), checkpoint.id)
  await until(async () => !(await showsText('After the checkpoint')), 'the restore takes the later comment away')
  assert.deepEqual(await shown(), atCheckpoint, 'the document shows the checkpoint\'s comments')
  await press('undo')
  await until(() => showsText('After the checkpoint'), 'Undo takes the restore back')
  assert.equal((await shown()).length, atCheckpoint.length + 1)
  console.log(`PASS ${which}: a restored checkpoint is one step of the person's Undo`)

  // e. Saved and reloaded: the document is as saved and the checkpoint is still listed.
  await save()
  const saved = await shown()
  const savedContents = await contents()
  await page.evaluate(() => window.devHost.reload())
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  await until(async () => {
    await kit.getByTestId('pdf-panel-comments').click()
    return JSON.stringify(await shown()) === JSON.stringify(saved)
  }, async () => `the reloaded document shows the saved comments: ${JSON.stringify(await shown())} for ${JSON.stringify(saved)}`)
  assert.deepEqual(await contents(), savedContents)
  const kept = await page.evaluate((path) => window.devHost.history.checkpoints(path), path)
  assert.ok(kept.some((item) => item.id === checkpoint.id && item.label === 'Before the review'), `the checkpoint is still listed: ${JSON.stringify(kept)}`)
  console.log(`PASS ${which}: a save survives a reload with its checkpoint`)
  assert.deepEqual(errors, [], 'no page errors')
} catch (error) {
  failures += 1
  console.log(`FAIL ${which}: ${error instanceof Error ? error.message.split('\n')[0] : String(error)}`)
  await page.screenshot({ path: `test-results/drafts-${which}.png` }).catch(() => undefined)
} finally {
  if (errors.length) console.log(`page errors:\n${errors.slice(0, 8).join('\n')}`)
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
