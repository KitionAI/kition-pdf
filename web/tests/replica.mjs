// A PDF changed outside Kition keeps its stored replica (Kit UI protocol UI-R1): through the
// development host with a replica store, the editor opens, edits and saves the sample; another
// program (the engine in Node, without the replica) then appends a bookmark to the saved file, and
// `devHost.reopenChanged` reopens the editor on it with the stored replica handed over. The outside
// bookmark shows, the document keeps its replica id, and the replica it saves next holds the
// objects made before the change under the same ids. Opening the file alone (`devHost.open`) is
// the control: another replica, whose line does not hold the versions from before.
// Undo steps live in the session, not in the replica, so nothing here asks for undo after a reopen.
// node tests/replica.mjs [chromium|webkit]
import { readFile } from 'node:fs/promises'
import { chromium, webkit } from 'playwright'
import { createServer } from 'vite'
import { Replica, parseSchema } from '@kition/sdk/kdm'

const which = process.argv[2] ?? 'chromium'
const path = 'Sample.pdf'
const engineDir = new URL('../public/engine/', import.meta.url)
const sdk = await import(new URL('kition_pdf_kit.js', engineDir).href)
await sdk.default({ module_or_path: await readFile(new URL('kition_pdf_kit_bg.wasm', engineDir)) })

const server = await createServer({ configFile: new URL('../vite.config.ts', import.meta.url).pathname, server: { port: 0, host: '127.0.0.1' }, logLevel: 'error' })
await server.listen()
const origin = `http://127.0.0.1:${server.httpServer.address().port}`
const browser = await (which === 'webkit' ? webkit : chromium).launch()
const page = await (await browser.newContext({ viewport: { width: 1280, height: 800 } })).newPage()
const errors = []
let failures = 0
page.on('pageerror', (error) => errors.push(error.message))
const step = (name, ok, detail = '') => {
  if (!ok) failures += 1
  console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` - ${detail}` : ''}`)
}
const until = async (check, tries = 150) => {
  for (let i = 0; i < tries; i += 1) {
    if (await check().catch(() => false)) return true
    await page.waitForTimeout(100)
  }
  return false
}
const ask = (command, params = {}) => page.evaluate(async ([command, params]) => (await window.devHost.connection().ai.run([{ command, params }], false)).completed[0].result, [command, params])
const titles = async () => (await ask('bookmark_list', { limit: 1000 })).bookmarks.map((item) => item.title)
const docId = () => page.evaluate(() => window.devHost.connection().replica.info())
const version = () => page.evaluate(() => window.devHost.connection().ai.version())
// The kit's schema, as Kition sends it with `ai/changes` (the engine does not describe its own).
const schema = JSON.parse(await readFile(new URL('../../crates/kdm/kdm-model/model/schema.json', import.meta.url), 'utf8'))
// What changed since `since` (`changes: null` when the replica's history does not hold it), or the refusal.
const changesSince = (since) =>
  page.evaluate(async ([since, schema]) => {
    try {
      return await window.devHost.connection().ai.changes(since, schema)
    } catch (error) {
      return { changes: null, error: String(error?.message ?? error) }
    }
  }, [since, schema])
const why = (answer) => answer.error ?? answer.unavailable ?? ''
const createdIds = (changes) => (changes?.created ?? []).map((object) => object.id).sort()
const fileVersion = () => page.evaluate((path) => window.devHost.files.get(path).version, path)
/** The replica the host stores beside the file, as the KDM reads it (`null` without one). */
const storedReplica = async () => {
  const bytes = await page.evaluate((path) => {
    const stored = window.devHost.history.replica(path)
    return stored ? Array.from(new Uint8Array(stored.snapshot)) : null
  }, path)
  return bytes ? Replica.load(parseSchema(schema), new Uint8Array(bytes)) : null
}

/** Another program's save: the file opened without its replica, a bookmark appended, saved. */
function outsideChange(bytes) {
  const doc = sdk.PdfDocument.open(path, bytes)
  try {
    doc.execute('bookmark_add', JSON.stringify({ page: 1, title: 'Outside' }))
    return doc.save()
  } finally {
    doc.free()
  }
}

try {
  await page.goto(`${origin}/dev.html?replica=1`)
  await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes(' open in '), null, { timeout: 60000 })
  const kit = page.frameLocator('#kit')
  await until(async () => Array.isArray(await titles()))

  // Edit and save inside Kition: a bookmark, then a comment whose replica objects are tracked.
  await ask('bookmark_add', { page: 1, title: 'Inside' })
  const before = await version()
  await ask('comment_add', { page: 1, type: 'note', at: [40, 40], contents: 'kept' })
  const first = await changesSince(before)
  const made = createdIds(first.changes)
  const keptId = async () => (await ask('comment_list')).comments.find((comment) => comment.contents === 'kept')?.id ?? null
  const madeComment = await keptId()
  step('the comment makes replica objects', made.length > 0, made.join(' ') || why(first))
  const id = await docId()
  step('the document has a replica id', typeof id === 'string' && id.length > 0, String(id))
  await kit.getByTestId('pdf-save').click()
  await until(async () => (await fileVersion()) > 1 && (await page.evaluate((path) => window.devHost.history.replica(path) !== null, path)))
  const stored = await page.evaluate((path) => window.devHost.history.replica(path)?.fileStamp ?? null, path)
  step('the save stores the replica beside the file', stored !== null)

  // Another program changes the saved file; the editor reopens on it with the stored replica.
  const saved = await page.evaluate((path) => Array.from(new Uint8Array(window.devHost.files.get(path).bytes)), path)
  const changed = outsideChange(new Uint8Array(saved))
  step('the outside program appended to the file', changed.length > saved.length)
  await page.evaluate((bytes) => window.devHost.reopenChanged(new Uint8Array(bytes).buffer), Array.from(changed))
  const shows = await until(async () => (await titles()).includes('Outside'))
  step('the outside change shows', shows, (await titles()).join(', '))
  step('the edits saved before it are there', (await titles()).includes('Inside'))
  const comments = (await ask('comment_list')).comments
  step('the comment is there', comments.some((comment) => comment.contents === 'kept'), JSON.stringify(comments))
  step('the document keeps its replica id', (await docId()) === id, String(await docId()))
  step('the outside change moved the replica on', (await version()) !== before)
  // Edited and saved again, the replica it reopened with holds the objects made before the change.
  await ask('bookmark_add', { page: 1, title: 'After' })
  const stamp = await fileVersion()
  await kit.getByTestId('pdf-save').click()
  await until(async () => (await fileVersion()) > stamp)
  const reopened = await storedReplica()
  step('the next save stores the same document', reopened?.docId === id, `${reopened?.docId} with ${reopened?.objectsOfType('Comment').length} comments`)
  const now = await keptId()
  step('the objects made before the change keep their ids', madeComment !== null && made.includes(madeComment) && now === madeComment, `${madeComment} then ${now}`)
  step('the stored replica is still kept', await page.evaluate((path) => window.devHost.history.replica(path) !== null, path))

  // The control: the same file opened alone starts another history, which does not hold it.
  await page.evaluate((bytes) => window.devHost.open('Sample.pdf', new Uint8Array(bytes).buffer), Array.from(changed))
  await until(async () => (await titles()).includes('Outside'))
  await ask('bookmark_add', { page: 1, title: 'Alone' })
  const alone = await changesSince(before)
  step('a file opened alone has another replica', (await docId()) !== id && alone.changes === null && !alone.error, `${await docId()} ${why(alone)}`)
} catch (error) {
  failures += 1
  console.log(`FAIL ${error instanceof Error ? error.message.split('\n')[0] : String(error)}`)
} finally {
  if (errors.length) console.log(`page errors:\n${errors.slice(0, 8).join('\n')}`)
  await browser.close()
  await server.close()
}
process.exit(failures ? 1 : 0)
