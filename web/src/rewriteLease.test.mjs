// The whole document's lease around a full rewrite: taken before the edit, released after it
// (also when it fails), a rewrite refused while someone else holds it, and no edit run then.
import test from 'node:test'
import assert from 'node:assert/strict'
import { REWRITE_LEASE_TTL, WHOLE_DOCUMENT, rewritingHolder, withRewriteLease } from './rewriteLease.ts'

const alice = { principalId: 'p1', name: 'Alice', self: false }
const me = { principalId: 'p2', name: 'Bob', self: true }

/** A kit's lease store as `kitLeases` answers it, with the host's answers scripted. */
function leases({ others = [], refuse = null, readOnly = false, available = true, offline = false } = {}) {
  const calls = []
  return {
    calls,
    available,
    list: () => others,
    subscribe: () => () => undefined,
    heldByOther: (scope) => others.find((lease) => !lease.holder.self && (lease.scope === scope || lease.scope === '*' || scope === '*')) ?? null,
    acquire: async (scope, ttl) => {
      calls.push(['acquire', scope, ttl])
      if (readOnly) return { granted: false, reason: 'read-only', holder: null }
      if (refuse) return { granted: false, reason: 'held', holder: refuse }
      return { granted: true, lease: offline ? null : { scope, holder: me } }
    },
    release: async (scope) => {
      calls.push(['release', scope])
    },
  }
}

/** An edit that records when it ran, among the lease calls. */
function edit(store, result = 'done') {
  return async () => {
    store.calls.push(['edit'])
    return result
  }
}

test('a full rewrite holds the whole document while it runs', async () => {
  const store = leases()
  const refused = []
  const answer = await withRewriteLease(store, WHOLE_DOCUMENT, true, edit(store), (name) => refused.push(name))
  assert.equal(answer, 'done')
  assert.deepEqual(store.calls, [['acquire', '*', REWRITE_LEASE_TTL], ['edit'], ['release', '*']])
  assert.deepEqual(refused, [])
})

test('the lease is released when the rewrite fails', async () => {
  const store = leases()
  const failing = async () => {
    store.calls.push(['edit'])
    throw new Error('nothing to apply')
  }
  await assert.rejects(withRewriteLease(store, WHOLE_DOCUMENT, true, failing, () => undefined), /nothing to apply/)
  assert.deepEqual(store.calls, [['acquire', '*', REWRITE_LEASE_TTL], ['edit'], ['release', '*']])
})

test('a rewrite is refused, and does not run, while someone else holds the document', async () => {
  const store = leases({ refuse: alice })
  const refused = []
  const answer = await withRewriteLease(store, WHOLE_DOCUMENT, true, edit(store), (name) => refused.push(name))
  assert.equal(answer, null)
  assert.deepEqual(store.calls, [['acquire', '*', REWRITE_LEASE_TTL]])
  assert.deepEqual(refused, ['Alice'])
})

test('an edit that merges takes no lease, and waits for nobody but a rewrite', async () => {
  const free = leases({ others: [{ scope: 'layer:x', holder: alice }] })
  assert.equal(await withRewriteLease(free, '', true, edit(free), () => assert.fail('not refused')), 'done')
  assert.deepEqual(free.calls, [['edit']])

  const rewriting = leases({ others: [{ scope: '*', holder: alice }] })
  assert.equal(rewritingHolder(rewriting), 'Alice')
  const refused = []
  assert.equal(await withRewriteLease(rewriting, '', true, edit(rewriting), (name) => refused.push(name)), null)
  assert.deepEqual(rewriting.calls, [])
  assert.deepEqual(refused, ['Alice'])
  // Reading changes nothing a rewrite could drop: it runs.
  assert.equal(await withRewriteLease(rewriting, '', false, edit(rewriting, 'read'), () => assert.fail('not refused')), 'read')
})

test('my own lease does not stop my edits', async () => {
  const store = leases({ others: [{ scope: '*', holder: me }] })
  assert.equal(rewritingHolder(store), null)
  assert.equal(await withRewriteLease(store, '', true, edit(store), () => assert.fail('not refused')), 'done')
})

test('without co-editing, offline, or read-only, everything runs as before', async () => {
  const alone = leases({ available: false })
  assert.equal(await withRewriteLease(alone, WHOLE_DOCUMENT, true, edit(alone), () => assert.fail('not refused')), 'done')
  assert.deepEqual(alone.calls, [['edit']])

  // Granted with no lease: nobody to coordinate with, so nothing to release.
  const offline = leases({ offline: true })
  assert.equal(await withRewriteLease(offline, WHOLE_DOCUMENT, true, edit(offline), () => assert.fail('not refused')), 'done')
  assert.deepEqual(offline.calls, [['acquire', '*', REWRITE_LEASE_TTL], ['edit']])

  // A viewer's edit goes on to the editor's own read-only refusal.
  const viewer = leases({ readOnly: true })
  assert.equal(await withRewriteLease(viewer, WHOLE_DOCUMENT, true, edit(viewer, 'refused by the editor'), () => assert.fail('not refused')), 'refused by the editor')
})
