// Exercise the actual WASM SDK directly in Node; the editor still runs in a Worker.
import assert from 'node:assert/strict'
import { readFile, writeFile, mkdir } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { dirname, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

const options = {}
for (let i = 2; i < process.argv.length; i++) {
  const key = process.argv[i]
  if (!['--engine-dir', '--manifest', '--out'].includes(key)) throw new Error('Unknown option')
  const value = process.argv[++i]
  if (!value || value.startsWith('--')) throw new Error('Missing option value')
  options[key.slice(2)] = value
}
if (!options['engine-dir'] || !options.manifest || !options.out) throw new Error('Engine directory, pinned fixture manifest and report are required')
const hash = bytes => createHash('sha256').update(bytes).digest('hex')
const safe = error => String(error?.message ?? error).replace(/\/Users\/[^\s:)'"<>]+|\/home\/[^\s:)'"<>]+/g, '<local-path>')
const report = { schema: 1, status: 'running', scope: 'Direct actual WASM SDK save acknowledgement and replica reopening on the original Type3 fixture', engine: {}, cases: [], errors: [] }
const documents = new Set()
try {
  const engineDir = resolve(options['engine-dir'])
  for (const name of ['kition_pdf_kit.js', 'kition_pdf_kit_bg.wasm']) report.engine[name] = hash(await readFile(resolve(engineDir, name)))
  report.harnessSha256 = hash(await readFile(new URL(import.meta.url)))
  const manifestBytes = await readFile(options.manifest), manifest = JSON.parse(manifestBytes)
  const source = await readFile(resolve(dirname(resolve(options.manifest)), manifest.file))
  assert.equal(hash(source), manifest.sha256, 'Pinned original fixture')
  report.fixtureSha256 = hash(source)
  report.manifestSha256 = hash(manifestBytes)
  const sdk = await import(pathToFileURL(resolve(engineDir, 'kition_pdf_kit.js')).href)
  await sdk.default({ module_or_path: await readFile(resolve(engineDir, 'kition_pdf_kit_bg.wasm')) })
  const keep = doc => { documents.add(doc); return doc }
  const open = (bytes = source) => keep(sdk.PdfDocument.open('Save.pdf', bytes))
  const execute = (doc, name, params = {}) => JSON.parse(doc.execute(name, JSON.stringify(params)))
  const add = (doc, title) => execute(doc, 'bookmark_add', { page: 1, title })
  const titles = doc => execute(doc, 'bookmark_list').bookmarks.map(item => item.title)
  const prefix = (bytes, prior) => assert.deepEqual(Buffer.from(bytes.subarray(0, prior.length)), Buffer.from(prior), 'Full incremental prefix')
  const check = (name, fn) => {
    const row = { name, status: 'failed' }; report.cases.push(row)
    try { fn(); row.status = 'passed' } catch (error) { row.error = safe(error) }
    for (const doc of documents) doc.free()
    documents.clear()
  }
  check('stale local save acknowledgement retains new edits and dirty state', () => {
    const doc = open(); add(doc, 'Prepared A')
    const prepared = doc.save(); add(doc, 'Newer B')
    const revision = doc.revision, version = doc.version()
    doc.markSaved()
    assert.deepEqual(titles(doc), ['Prepared A', 'Newer B'])
    assert.equal(doc.dirty, true)
    assert.equal(doc.revision, revision)
    assert.deepEqual(doc.version(), version)
    assert.deepEqual(titles(open(prepared)), ['Prepared A'])
    assert.deepEqual(titles(open(doc.save())), ['Prepared A', 'Newer B'])
  })
  check('acknowledgement without a prepared save cannot clean an edit', () => {
    const doc = open(); add(doc, 'Unwritten')
    const revision = doc.revision
    doc.markSaved()
    assert.equal(doc.dirty, true)
    assert.equal(doc.revision, revision)
    assert.deepEqual(titles(doc), ['Unwritten'])
  })
  check('stale save acknowledgement cannot reverse a newer undo', () => {
    const doc = open(); add(doc, 'Undo A'); add(doc, 'Undo B')
    doc.save(); execute(doc, 'edit.undo')
    const revision = doc.revision
    doc.markSaved()
    assert.deepEqual(titles(doc), ['Undo A'])
    assert.equal(doc.dirty, true)
    assert.equal(doc.revision, revision)
    assert.deepEqual(titles(open(doc.save())), ['Undo A'])
  })
  check('remote import after preparation survives stale acknowledgement', () => {
    const doc = open(); add(doc, 'Local A')
    const prepared = doc.save(), version = doc.version()
    const peer = keep(sdk.PdfDocument.openWithReplica('Peer.pdf', prepared, doc.snapshot(), doc.blobs()))
    add(peer, 'Remote B')
    doc.addBlobs(peer.blobs()); doc.importChanges(peer.exportChanges(version))
    assert.deepEqual(titles(doc), ['Local A', 'Remote B'])
    const revision = doc.revision
    doc.markSaved()
    assert.deepEqual(titles(doc), ['Local A', 'Remote B'])
    assert.equal(doc.dirty, true)
    assert.equal(doc.revision, revision)
    assert.deepEqual(titles(open(doc.save())), ['Local A', 'Remote B'])
  })
  check('checksum-bearing rewrite is refused by the actual WASM artifact', () => {
    const marker = Buffer.from('trailer\n<<')
    const at = source.indexOf(marker) + marker.length
    assert.ok(at >= marker.length, 'Original fixture has a classic trailer')
    const checksum = Buffer.concat([source.subarray(0, at), Buffer.from('/DocChecksum /synthetic-save-checksum '), source.subarray(at)])
    const doc = open(checksum); add(doc, 'Retained checksum edit')
    const prepared = doc.save(), revision = doc.revision, version = doc.version()
    assert.throws(() => execute(doc, 'doc_protect', { open_password: 'synthetic-save-password' }), /DocChecksum/)
    assert.equal(doc.dirty, true); assert.equal(doc.revision, revision)
    assert.deepEqual(doc.version(), version)
    assert.deepEqual(titles(doc), ['Retained checksum edit'])
    prefix(prepared, checksum)
  })
  check('successful repeated saves preserve exact prefixes and replica identity', () => {
    const doc = open()
    assert.deepEqual(Buffer.from(doc.save()), source, 'Unedited bytes are identical')
    doc.markSaved(); assert.equal(doc.dirty, false)
    add(doc, 'First'); const first = doc.save(), firstVersion = doc.version(), snapshot = doc.snapshot(), blobs = doc.blobs(), id = doc.docId
    assert.equal(doc.dirty, true); prefix(first, source)
    doc.markSaved(); assert.equal(doc.dirty, false)
    const peer = keep(sdk.PdfDocument.openWithReplica('Saved.pdf', first, snapshot, blobs))
    assert.equal(peer.docId, id); assert.deepEqual(titles(peer), ['First']); assert.equal(peer.dirty, false)
    assert.deepEqual(peer.version(), firstVersion, 'Our first incremental save must not import a new collaboration epoch')
    add(doc, 'Second'); const second = doc.save(); prefix(second, first)
    const secondVersion = doc.version(), secondSnapshot = doc.snapshot(), secondBlobs = doc.blobs()
    doc.markSaved(); assert.equal(doc.dirty, false)
    assert.deepEqual(titles(open(second)), ['First', 'Second'])
    const secondPeer = keep(sdk.PdfDocument.openWithReplica('Second.pdf', second, secondSnapshot, secondBlobs))
    assert.equal(secondPeer.docId, id)
    assert.deepEqual(secondPeer.version(), secondVersion, 'Our second incremental save must not import a new collaboration epoch')
    assert.equal(secondPeer.dirty, false)
    add(secondPeer, 'Third'); const third = secondPeer.save(); prefix(third, second)
    const thirdVersion = secondPeer.version(), thirdSnapshot = secondPeer.snapshot(), thirdBlobs = secondPeer.blobs()
    secondPeer.markSaved()
    const thirdPeer = keep(sdk.PdfDocument.openWithReplica('Third.pdf', third, thirdSnapshot, thirdBlobs))
    assert.equal(thirdPeer.docId, id); assert.deepEqual(thirdPeer.version(), thirdVersion)
    assert.deepEqual(titles(thirdPeer), ['First', 'Second', 'Third'])
    execute(doc, 'edit.undo'); assert.deepEqual(titles(doc), ['First'])
    execute(doc, 'edit.redo'); assert.deepEqual(titles(doc), ['First', 'Second'])
  })
  report.passed = report.cases.filter(row => row.status === 'passed').length
  report.failed = report.cases.length - report.passed
  report.status = report.failed ? 'failed' : 'passed'
} catch (error) {
  report.errors.push(safe(error)); report.status = 'failed'
} finally {
  for (const doc of documents) doc.free()
  await mkdir(dirname(resolve(options.out)), { recursive: true })
  await writeFile(options.out, JSON.stringify(report, null, 2) + '\n')
}
console.log(JSON.stringify({ status: report.status, passed: report.passed, failed: report.failed, errors: report.errors }))
process.exitCode = report.status === 'passed' ? 0 : 1
