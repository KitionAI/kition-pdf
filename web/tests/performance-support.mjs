/** Nearest rank: sort ascending, then select ceil(p * n), with one-based ranks. */
export function nearestRank(values, probability) {
  if (!(probability > 0 && probability <= 1)) throw new Error('probability must be in (0, 1]')
  if (values.some(value => !Number.isFinite(value) || value < 0)) throw new Error('samples must be finite and nonnegative')
  if (!values.length) return null
  return [...values].sort((a, b) => a - b)[Math.ceil(probability * values.length) - 1]
}

export function summarize(samples) {
  const count = status => samples.filter(sample => sample.status === status).length
  const completed = samples.filter(sample => sample.status === 'passed')
  if (samples.some(sample => !['passed', 'failed', 'timeout', 'untested'].includes(sample.status))) throw new Error('unknown sample status')
  if (completed.some(sample => !Number.isFinite(sample.presentMs) || sample.presentMs < 0)) throw new Error('passed samples require a finite presentation duration')
  return {
    planned: samples.length, completed: completed.length, failed: count('failed'), timedOut: count('timeout'), untested: count('untested'),
    completionRate: samples.length ? completed.length / samples.length : null,
    successfulPresentMs: { p50: nearestRank(completed.map(sample => sample.presentMs), 0.5), p95: nearestRank(completed.map(sample => sample.presentMs), 0.95) },
    percentilePopulation: 'Successful samples only; failures/timeouts/untested remain in the explicit planned denominator.',
  }
}

export function planSamples(pages, cycles = 100, windowPages = 40) {
  if (![pages, cycles, windowPages].every(value => Number.isInteger(value) && value > 0)) throw new Error('pages, cycles, and window must be positive integers')
  const samples = []
  const add = (phase, page, action = 'navigate', extra = {}) => samples.push({ id: `${phase}-${samples.length}`, phase, page, action, status: 'untested', ...extra })
  add('cold-open', 1, 'open')
  for (let page = 1; page <= Math.min(pages, windowPages); page++) add('sequential-window', page)
  for (const page of [...new Set([1, Math.ceil(pages / 2), pages])]) add('cross-document', page)
  for (const page of [1, pages]) add('pair-warmup', page)
  for (let cycle = 1; cycle <= cycles; cycle++) for (const page of [1, pages]) add('repeated-pair', page, 'navigate', { cycle })
  for (const mode of ['pdf-fit-page', 'pdf-actual-size', 'pdf-fit-width']) add('zoom', pages, 'zoom', { mode })
  add('edit-refresh', pages, 'edit')
  return samples
}

export function parsePerformanceOptions(args) {
  const options = { browser: 'chromium', cycles: 100, windowPages: 40, timeoutMs: 120000, scrollPages: 0 }
  const keys = new Map([['--manifest', 'manifest'], ['--out', 'out'], ['--browser', 'browser'], ['--cycles', 'cycles'], ['--window-pages', 'windowPages'], ['--timeout-ms', 'timeoutMs'], ['--scroll-pages', 'scrollPages']])
  const seen = new Set()
  for (let index = 0; index < args.length; index++) {
    if (args[index] === '--help') { options.help = true; continue }
    const key = keys.get(args[index])
    if (!key || seen.has(key)) throw new Error('unknown or duplicate option')
    seen.add(key)
    const value = args[++index]
    if (!value || value.startsWith('--')) throw new Error('missing option value')
    options[key] = ['cycles', 'windowPages', 'timeoutMs', 'scrollPages'].includes(key) ? Number(value) : value
  }
  if (!options.help && (!options.manifest || !options.out)) throw new Error('--manifest and --out are required')
  if (!['chromium', 'webkit', 'firefox'].includes(options.browser)) throw new Error('unsupported browser')
  if (![options.cycles, options.windowPages, options.timeoutMs].every(value => Number.isSafeInteger(value) && value > 0)) throw new Error('numeric options must be positive integers')
  if (!Number.isSafeInteger(options.scrollPages) || options.scrollPages < 0) throw new Error('--scroll-pages must be a nonnegative integer')
  return options
}
