import assert from 'node:assert/strict'
import test from 'node:test'
import { SearchSession, SEARCH_DELAY_MS, SEARCH_CACHE_ENTRIES, SEARCH_CACHE_UNITS } from './searchSession.ts'

const match = (page = 1) => ({ page, rects: [[1, 2, 3, 4], [5, 6, 7, 8]] })
const flush = async () => { await Promise.resolve(); await Promise.resolve() }
const advance = async (t, milliseconds = SEARCH_DELAY_MS) => { t.mock.timers.tick(milliseconds); await flush() }
function setup(t, find = async () => ({ matches: [match()] })) {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const changes = [], session = new SearchSession(find, result => changes.push(result))
  t.after(() => session.dispose())
  return { session, changes }
}

test('rapid edits produce one query after 200ms and retain every match rectangle', async t => {
  const calls = [], { session, changes } = setup(t, async text => { calls.push(text); return { matches: [match()] } })
  session.search('A'); await advance(t, 100)
  session.search('AB'); await advance(t, 199)
  assert.deepEqual(calls, [])
  await advance(t, 1)
  assert.deepEqual(calls, ['AB'])
  assert.deepEqual(changes.at(-1), [match()])
})

test('composition suppresses queries and invalidates an already running request', async t => {
  let finish
  const calls = [], { session, changes } = setup(t, text => { calls.push(text); return new Promise(resolve => { finish = resolve }) })
  session.search('before'); await advance(t)
  session.search('intermediate', true); await advance(t, 1000)
  finish({ matches: [match(8)] }); await flush()
  assert.equal(changes.at(-1), null)
  assert.deepEqual(calls, ['before'])
  session.search('committed'); await advance(t)
  assert.deepEqual(calls, ['before', 'committed'])
  finish({ matches: [match()] }); await flush()
  assert.deepEqual(changes.at(-1), [match()])
})

for (const action of ['empty', 'clear', 'dispose']) test(`${action} cancels scheduled work and ignores stale results`, async t => {
  let finish, calls = 0
  const { session, changes } = setup(t, () => { calls++; return new Promise(resolve => { finish = resolve }) })
  session.search('first'); await advance(t)
  session.search('queued')
  if (action === 'empty') session.search('  ')
  else session[action]()
  const before = changes.length
  finish({ matches: [match()] }); await advance(t, 1000)
  assert.equal(calls, 1)
  assert.equal(changes.length, before)
  assert.equal(changes.at(-1), null)
})

test('out-of-order replies cannot replace or populate the current query cache', async t => {
  const pending = new Map(), { session, changes } = setup(t, text => new Promise(resolve => pending.set(text, resolve)))
  session.search('old'); await advance(t)
  session.search('new'); await advance(t)
  pending.get('new')({ matches: [match(2)] }); await flush()
  pending.get('old')({ matches: [match(1)] }); await flush()
  assert.deepEqual(changes.at(-1), [match(2)])
  assert.equal(session.size, 1)
})

test('revision replacement disposes prior cache and replies, even for the same query', async t => {
  let finish
  const { session: old, changes } = setup(t, () => new Promise(resolve => { finish = resolve }))
  old.search('same'); await advance(t)
  old.dispose()
  const fresh = new SearchSession(async () => ({ matches: [match(3)] }), value => changes.push(value))
  t.after(() => fresh.dispose())
  fresh.search('same'); await advance(t)
  finish({ matches: [match(1)] }); await flush()
  assert.deepEqual(changes.at(-1), [match(3)])
  assert.equal(old.size, 0)
})

test('empty results are cached but failed and unavailable results are not', async t => {
  let calls = 0
  const { session, changes } = setup(t, async text => { calls++; if (text === 'failure') throw new Error('synthetic'); if (text === 'unavailable') return null; return { matches: [] } })
  for (const text of ['empty', 'empty', 'failure', 'failure', 'unavailable', 'unavailable']) {
    session.search(text); await advance(t)
    assert.deepEqual(changes.at(-1), text === 'empty' ? [] : null)
  }
  assert.equal(calls, 5)
  assert.equal(session.size, 1)
})

test('query cache evicts least-recently-used entries and bounds retained geometry', async t => {
  const calls = [], { session } = setup(t, async text => { calls.push(text); return { matches: [match()] } })
  for (let i = 0; i < SEARCH_CACHE_ENTRIES; i++) { session.search(String(i)); await advance(t) }
  session.search('0'); await advance(t)
  session.search('extra'); await advance(t)
  session.search('0'); await advance(t)
  assert.equal(calls.filter(value => value === '0').length, 1)
  session.search('1'); await advance(t)
  assert.equal(calls.filter(value => value === '1').length, 2)
  assert.equal(session.size, SEARCH_CACHE_ENTRIES)
  assert.ok(session.units <= SEARCH_CACHE_UNITS)
})

test('oversized answers remain available to navigation without being cached', async t => {
  const huge = { page: 1, rects: Array.from({ length: SEARCH_CACHE_UNITS }, () => [0, 0, 1, 1]) }
  let calls = 0
  const { session, changes } = setup(t, async () => { calls++; return { matches: [huge] } })
  session.search('large'); await advance(t)
  assert.equal(changes.at(-1)[0].rects.length, SEARCH_CACHE_UNITS)
  session.search('large'); await advance(t)
  assert.equal(calls, 2)
  assert.equal(session.size, 0)
  assert.equal(session.units, 0)
})

test('aggregate geometry budget evicts old answers before the entry limit', async t => {
  const many = Array.from({ length: 2000 }, () => match())
  const { session } = setup(t, async () => ({ matches: many }))
  for (let i = 0; i < 8; i++) { session.search(`dense-${i}`); await advance(t) }
  assert.ok(session.size < 8)
  assert.ok(session.units <= SEARCH_CACHE_UNITS)
})


test('retains only navigation geometry, excluding engine matched text from the cache', async t => {
  const raw = { ...match(), text: 'Synthetic engine-only matched text', extra: { unused: true } }
  let calls = 0
  const { session, changes } = setup(t, async () => { calls++; return { matches: [raw] } })
  session.search('synthetic'); await advance(t)
  assert.deepEqual(changes.at(-1), [match()])
  assert.notEqual(changes.at(-1)[0], raw)
  session.search('synthetic'); await advance(t)
  assert.equal(calls, 1)
  assert.deepEqual(changes.at(-1), [match()])
})
