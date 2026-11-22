import assert from 'node:assert/strict'
import test from 'node:test'
import { closeResource, parseOptions } from './corpus-support.mjs'

test('CLI validates all options before starting a browser', () => {
  const valid = ['--manifest', '/tmp/cases.json', '--out', '/tmp/report.json']
  assert.equal(parseOptions(valid).browser, 'chromium')
  assert.equal(parseOptions([...valid, '--browser', 'webkit']).browser, 'webkit')
  assert.equal(parseOptions(['--help']).help, true)
  for (const extra of [['--unknown'], ['--browser'], ['--browser', '--help'], ['--out', '/tmp/other.json'], ['--browser', 'webkit', '--browser', 'chromium'], ['--browser', 'other']]) {
    assert.throws(() => parseOptions([...valid, ...extra]))
  }
  assert.throws(() => parseOptions(['--help', '--unknown']))
  assert.throws(() => parseOptions([]))
})

test('cleanup rejection is recorded and subsequent resources still close', async () => {
  const errors = []
  let closed = false
  await closeResource('context', { close: async () => { throw new Error('injected close rejection') } }, errors)
  await closeResource('server', { close: async () => { closed = true } }, errors)
  await closeResource('absent browser', undefined, errors)
  assert.equal(closed, true)
  assert.deepEqual(errors, ['context cleanup failed: Error: injected close rejection'])
})
