import assert from 'node:assert/strict'
import { installSaveEngineProbe } from './save-probe.mjs'

export const saveCheckNames = [
  'original opens clean with eight authored pages and no bookmarks',
  'bookmark edit undo and redo preserve the expected outline',
  'first protocol save acknowledges storage before marking clean',
  'failed host write retains dirty state and the previous stored bytes',
  'toolbar retry persists both bookmarks and clears dirty state',
  'second saved file reopens with both bookmark edits',
  'delayed acknowledgement leaves the serialized revision dirty',
  'agent edit proceeds while the earlier save acknowledgement is held',
  'old acknowledgement does not mark the newer edit clean',
  'next save persists the newer edit and marks it clean',
  'held snapshot reopens with exactly the first three bookmarks',
  'latest snapshot reopens with all four bookmarks',
  'bounded rectangle annotation saves through the real toolbar',
  'annotation and bookmarks survive reopening without changing page geometry',
]

export async function saveScenarios({ page, kit, bytes, manifest, result, snapshot, safe }) {
  const path = 'original-save-fixture.pdf'
  let frame, first, second, held, latest, annotated, rejectedAt, heldRevision
  const inspect = () => page.evaluate(() => window.devHost.connection().ai.inspect())
  const ask = async (command, params = {}) => {
    const answer = await page.evaluate(async ({ command, params }) => window.devHost.connection().ai.run([{ command, params }], false), { command, params })
    assert.equal(answer.completed.length, 1, 'One requested command must complete')
    return answer.completed[0]
  }
  const outline = async () => (await ask('bookmark_list', { limit: 1000 })).result.bookmarks.map(item => ({ title: item.title, page: item.page, children: item.children ?? [] }))
  const expected = count => Array.from({ length: count }, (_, index) => ({ title: `Original save bookmark ${index + 1}`, page: index + 1, children: [] }))
  const bookmarks = async count => assert.deepEqual(await outline(), expected(count))
  const add = async count => {
    assert.equal((await ask('bookmark_add', { title: expected(count).at(-1).title, page: count })).changed, true)
    assert.equal((await inspect()).dirty, true)
    await bookmarks(count)
  }
  const stored = () => page.evaluate(path => { const file = window.devHost.files.get(path); return { version: file.version, bytes: [...new Uint8Array(file.bytes)] } }, path)
  const marks = () => frame.evaluate(() => window.__pdfSaveEngine.marks)
  const saveProtocol = () => page.evaluate(() => window.devHost.connection().save())
  const state = async dirty => {
    assert.equal((await inspect()).dirty, dirty)
    await page.waitForFunction(dirty => window.devHost.changes.at(-1)?.dirty === dirty, dirty)
  }
  const dimensions = async () => {
    const info = await inspect()
    assert.deepEqual(info.details.pages.map(item => [item.area.width, item.area.height]), manifest.pages.map(item => [item.width, item.height]))
  }
  const open = async data => {
    // Reopening through the existing file picker remounts the editor, as the dev host does.
    // File.arrayBuffer() is asynchronous: the old same-name status can still say "open".
    // Require a newly mounted connection before sending any request to this document.
    await page.evaluate(() => { window.__pdfSaveHost.previousConnection = window.devHost.connection() })
    await page.locator('#file').setInputFiles({ name: path, mimeType: 'application/pdf', buffer: data })
    await page.waitForFunction(path => {
      const connection = window.devHost.connection()
      return connection && connection !== window.__pdfSaveHost.previousConnection
        && document.getElementById('status')?.textContent?.includes(`${path} open in `)
    }, path)
    await page.evaluate(() => {
      delete window.__pdfSaveHost.previousConnection
      window.__pdfSaveHost.event('new-document-connection-ready')
    })
    await kit.getByTestId('kit-canvas-surface').waitFor()
    frame = page.frames().find(candidate => candidate !== page.mainFrame())
    await frame.evaluate(installSaveEngineProbe)
    await dimensions(); await state(false)
  }
  const exported = async (id, count, prefix, annotation = null) => {
    const file = await stored()
    const data = Buffer.from(file.bytes)
    const output = await snapshot({ id, bytes: data, version: file.version, expectedBookmarks: expected(count), prefix, annotation })
    return { ...output, data }
  }
  const toolbarSave = async () => {
    const before = await page.evaluate(() => window.__pdfSaveHost.writes.length)
    await kit.getByTestId('pdf-save').click()
    await page.waitForFunction(before => window.__pdfSaveHost.writes.length === before + 1 && window.__pdfSaveHost.writes.at(-1).status === 'acknowledged', before)
    await frame.waitForFunction(() => window.__pdfSaveEngine.marks.at(-1)?.dirtyAfter === false)
    await state(false)
  }
  const run = async (index, action) => {
    const check = result.checks[index]; check.status = 'running'
    try { const evidence = await action(); Object.assign(check, { status: 'passed', ...(evidence ?? {}) }) }
    catch (error) { Object.assign(check, { status: 'failed', error: safe(error) }); throw error }
  }
  await run(0, async () => { await open(bytes); await bookmarks(0); assert.equal((await ask('comment_list')).result.comments.length, 0) })
  await run(1, async () => {
    await add(1); assert.equal((await inspect()).canUndo, true)
    assert.equal((await ask('edit.undo')).changed, true); await bookmarks(0)
    assert.equal((await inspect()).canRedo, true)
    assert.equal((await ask('edit.redo')).changed, true); await bookmarks(1); await state(true)
  })
  await run(2, async () => {
    const answer = await saveProtocol(); assert.equal(answer.saved, true); await state(false)
    const events = await page.evaluate(() => window.__pdfSaveHost.events)
    assert.ok(events.find(item => item.type === 'host-write-acknowledged').sequence < events.find(item => item.type === 'worker-mark-started').sequence)
    const mark = (await marks()).at(-1); assert.equal(mark.requestedRevision, mark.beforeRevision); assert.equal(mark.dirtyAfter, false)
    first = await exported('bookmark-first', 1, { file: 'original.pdf', data: bytes })
    return { mark }
  })
  await run(3, async () => {
    await add(2); const before = await stored(), previousMarks = (await marks()).length
    rejectedAt = await page.evaluate(() => { window.__pdfSaveHost.failNext = true; return window.__pdfSaveHost.writes.length })
    const answer = await page.evaluate(async () => { try { await window.devHost.connection().save(); return { rejected: false } } catch (error) { return { rejected: true, injected: String(error.message).includes('Injected original-fixture host write failure') } } })
    assert.deepEqual(answer, { rejected: true, injected: true })
    await state(true); assert.deepEqual(await stored(), before); assert.equal((await marks()).length, previousMarks)
    await bookmarks(2)
    return { rejectedWriteIndex: rejectedAt, storedVersion: before.version, unchangedStoredBytes: true, noMarkSaved: true }
  })
  await run(4, async () => {
    await toolbarSave(); await bookmarks(2)
    second = await exported('bookmark-retry', 2, first)
    const writes = await page.evaluate(() => window.__pdfSaveHost.writes)
    assert.equal(writes[rejectedAt].status, 'rejected'); assert.equal(writes[rejectedAt + 1].status, 'acknowledged')
  })
  await run(5, async () => { await open(second.data); await bookmarks(2) })
  await run(6, async () => {
    await add(3); heldRevision = (await inspect()).revision
    const previousMarks = (await marks()).length
    await page.evaluate(() => {
      const probe = window.__pdfSaveHost; probe.holdNext = true; probe.pending = { status: 'pending' }
      window.devHost.connection().save().then(answer => { probe.pending = { status: 'returned', answer } }, () => { probe.pending = { status: 'rejected' } })
    })
    await page.waitForFunction(() => Boolean(window.__pdfSaveHost.held))
    await state(true); assert.equal((await marks()).length, previousMarks)
    assert.equal(await page.evaluate(() => window.__pdfSaveHost.writes.at(-1).status), 'written')
    held = await exported('bookmark-held', 3, second)
    return { serializedRevision: heldRevision, noMarkSavedBeforeAcknowledgement: true }
  })
  await run(7, async () => { await add(4); assert.ok((await inspect()).revision > heldRevision); await state(true) })
  await run(8, async () => {
    await page.evaluate(() => window.__pdfSaveHost.release())
    await page.waitForFunction(() => window.__pdfSaveHost.pending?.status !== 'pending')
    const pending = await page.evaluate(() => window.__pdfSaveHost.pending)
    assert.equal(pending.status, 'returned'); assert.equal(pending.answer.saved, false); await state(true)
    const mark = (await marks()).at(-1)
    assert.equal(mark.requestedRevision, heldRevision); assert.ok(mark.afterRevision > heldRevision); assert.equal(mark.dirtyAfter, true)
    assert.deepEqual(Buffer.from((await stored()).bytes), held.data); await bookmarks(4)
    return { mark, storedSnapshotUnchanged: true }
  })
  await run(9, async () => {
    assert.equal((await saveProtocol()).saved, true); await state(false)
    // The held snapshot was not rebased: the previously acknowledged baseline remains the prefix.
    latest = await exported('bookmark-latest', 4, second)
  })
  await run(10, async () => { await open(held.data); await bookmarks(3) })
  await run(11, async () => { await open(latest.data); await bookmarks(4) })
  const annotation = { page: 1, rect: [170, 150, 210, 180], width: 2, color: '#ff0000', contents: 'Original save rectangle' }
  const comment = async () => {
    const comments = (await ask('comment_list')).result.comments
    assert.equal(comments.length, 1)
    const item = comments[0]
    assert.equal(item.page, annotation.page); assert.equal(item.contents, annotation.contents)
    assert.deepEqual(item.rect, annotation.rect)
    return { page: item.page, rect: item.rect, type: item.type, contents: item.contents }
  }
  await run(12, async () => {
    assert.equal((await ask('comment_add', { ...annotation, type: 'rectangle' })).changed, true)
    const observed = await comment(); await toolbarSave()
    annotated = await exported('annotation-rectangle', 4, latest, annotation)
    return { annotation: observed }
  })
  await run(13, async () => { await open(annotated.data); await bookmarks(4); return { annotation: await comment() } })
  result.events = await page.evaluate(() => window.__pdfSaveHost.events)
  result.writes = await page.evaluate(() => window.__pdfSaveHost.writes)
}
