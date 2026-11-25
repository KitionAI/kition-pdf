import assert from 'node:assert/strict'
import { selectionCases, crossPageCase, fixtureGlyphs, assertSelected } from './selection-support.mjs'
import { dispatchCopy } from './selection-probe.mjs'

export const lifecycleNames = [
  'synthetic and trusted keyboard copy events carry selected plain text; clipboard reads are reported separately',
  'copy preserves real toolbar inputs and editable controls inside the selection event region',
  'Escape clears selection and disables Copy Selection',
  'single click, pasteboard click, and returning a drag to its anchor clear selection',
  'pointercancel clears pending selection and rejects its delayed reply',
  'Escape during an active drag cancels further pointer updates and the delayed reply',
  'latest-only requests stay serialized and never publish the superseded held result',
  'Space and middle-button gestures remain hand pans without selection queries',
  'forward cross-page drag preserves page order, plain text, and both page rectangles',
  'reverse cross-page drag preserves page order, plain text, and both page rectangles',
  'Alt cross-page drag clamps the selection to its anchor page',
  'edit revision clears selection and invalidates a delayed result',
  'saved document replacement starts without selection or enabled copy',
  'starting a drag closes Find and preserves the selected geometry after the layout change',
]

export async function selectionLifecycle(harness) {
  const { page, frame, kit, canvas, result, manifest, upload, place, point, drag, verify, settle, snap, none } = harness
  const full = { ...selectionCases(manifest.pages[0])[0], page: 1 }
  const partial = { ...selectionCases(manifest.pages[0])[2], page: 1 }
  const run = async (index, task) => { result.checks[index].status = 'running'; await task(); result.checks[index].status = 'passed' }
  const select = async (spec = full) => { await drag(spec); return verify(spec, 1) }
  const quiet = () => frame.waitForFunction(() => window.__pdfSelection.active === 0)
  const release = () => frame.evaluate(() => window.__pdfSelection.release())
  const hold = () => frame.evaluate(() => { window.__pdfSelection.holdNext = true })
  const held = () => frame.waitForFunction(() => window.__pdfSelection.held.length === 1)
  const disabled = async () => assert.equal(await kit.getByTestId('pdf-copy-selection').isDisabled(), true)
  const calls = () => frame.evaluate(() => window.__pdfSelection.calls.length)
  const readClipboard = async (expected, action) => {
    const actual = await frame.evaluate(async expectedText => {
      try {
        if (!navigator.clipboard?.readText) return { available: false }
        return { available: true, matches: await navigator.clipboard.readText() === expectedText }
      } catch { return { available: false } }
    }, expected)
    if (actual.available) assert.equal(actual.matches, true, 'Native clipboard differs from selected plain text')
    return { action, status: actual.available ? 'passed' : 'untested', ...(actual.available ? {} : { reason: 'Browser clipboard read unavailable or permission denied' }) }
  }
  await place(1, 1, 1)
  await run(0, async () => {
    await select()
    const copied = await frame.evaluate(dispatchCopy)
    assert.equal(copied.prevented, true); assert.ok(copied.text === full.expected.text)
    assert.deepEqual(copied.types, ['text/plain'])
    const beforeKeyboard = await frame.evaluate(() => window.__pdfSelection.trustedCopyCount)
    await page.keyboard.press('ControlOrMeta+c')
    await frame.waitForFunction(before => window.__pdfSelection.trustedCopyCount > before, beforeKeyboard)
    const trusted = await frame.evaluate(() => {
      const copy = window.__pdfSelection.trustedCopy
      return { trusted: copy.event.isTrusted, prevented: copy.event.defaultPrevented, text: copy.text, types: copy.types }
    })
    assert.equal(trusted.trusted, true); assert.equal(trusted.prevented, true)
    assert.ok(trusted.text === full.expected.text, 'Trusted copy event must contain the selected plain text')
    assert.deepEqual(trusted.types, ['text/plain'])
    result.trustedKeyboardCopy = { status: 'passed', trusted: true, defaultPrevented: true, text: full.expected.text, types: trusted.types }
    const keyboard = await readClipboard(full.expected.text, 'keyboard')
    await select(partial)
    assert.equal(await kit.getByTestId('pdf-copy-selection').isEnabled(), true)
    await kit.getByTestId('pdf-copy-selection').click()
    const button = await readClipboard(partial.expected.text, 'button')
    result.clipboard = { status: keyboard.status === 'passed' && button.status === 'passed' ? 'passed' : 'partially-tested', keyboard, button }
  })
  await run(1, async () => {
    await select()
    await kit.getByTestId('pdf-page-field').focus()
    const actual = await frame.evaluate(dispatchCopy)
    assert.equal(actual.prevented, false); assert.equal(actual.text, '')
    const guarded = await frame.evaluate(() => {
      const region = document.querySelector('.pdf-pages'), results = []
      for (const tag of ['input', 'textarea', 'div']) {
        const control = document.createElement(tag)
        if (tag === 'div') control.contentEditable = 'true'
        region.append(control); control.focus()
        const clipboard = new DataTransfer(), event = new ClipboardEvent('copy', { clipboardData: clipboard, bubbles: true, cancelable: true })
        control.dispatchEvent(event)
        results.push({ prevented: event.defaultPrevented, text: clipboard.getData('text/plain') }); control.remove()
      }
      return results
    })
    for (const actual of guarded) { assert.equal(actual.prevented, false); assert.equal(actual.text, '') }
  })
  await run(2, async () => {
    await select(); await page.keyboard.press('Escape'); await none(); await disabled()
  })
  await run(3, async () => {
    await select()
    const blank = await point(1, [manifest.pages[0].width - 8, manifest.pages[0].height - 8])
    await page.mouse.click(blank.x, blank.y); await none(); await disabled()
    await select()
    const outside = await point(1, [-20, manifest.pages[0].height / 2])
    await page.mouse.click(outside.x, outside.y); await none(); await disabled()
    const start = await point(1, full.anchor), end = await point(1, full.focus)
    await page.mouse.move(start.x, start.y); await page.mouse.down(); await page.mouse.move(end.x, end.y, { steps: 4 })
    await page.mouse.move(start.x, start.y); await page.mouse.up(); await quiet(); await none(); await disabled()
  })
  await run(4, async () => {
    await hold()
    const start = await point(1, full.anchor), end = await point(1, full.focus)
    await page.mouse.move(start.x, start.y); await page.mouse.down(); await page.mouse.move(end.x, end.y, { steps: 4 }); await held()
    await canvas.evaluate(element => element.dispatchEvent(new PointerEvent('pointercancel', { pointerId: window.__pdfSelection.pointerId, pointerType: 'mouse', isPrimary: true, bubbles: true })))
    await page.mouse.up(); await none(); await release(); await quiet(); await settle(); await none(); await disabled()
  })
  await run(5, async () => {
    await hold()
    const start = await point(1, full.anchor), end = await point(1, full.focus)
    await page.mouse.move(start.x, start.y); await page.mouse.down(); await page.mouse.move(end.x, end.y, { steps: 4 }); await held()
    const before = await calls()
    await page.keyboard.press('Escape')
    await page.mouse.move(end.x + 20, end.y + 10); await page.mouse.up(); await none()
    await release(); await quiet(); await settle(); await none(); await disabled()
    assert.equal(await calls(), before)
  })
  await run(6, async () => {
    await hold(); await drag(full); await held()
    await drag(partial)
    assert.equal(await frame.evaluate(() => window.__pdfSelection.active), 1)
    await frame.evaluate(expected => {
      const probe = window.__pdfSelection, surface = document.querySelector('[data-testid="kit-canvas-surface"]')
      probe.transientUnexpected = false
      probe.observer = new MutationObserver(() => {
        const shown = [...document.querySelectorAll('[data-testid="pdf-selection-highlight"]')]
        if (!shown.length) return
        const area = window.__pdfSearch.engine.info.inspected.pages[0].area, view = window.__pdfSearch.viewport.view, bounds = surface.getBoundingClientRect()
        if (shown.length !== expected.length) { probe.transientUnexpected = true; return }
        shown.forEach((element, index) => {
          const actual = element.getBoundingClientRect(), [x0, y0, x1, y1] = expected[index]
          const a = [actual.left - bounds.left, actual.top - bounds.top, actual.width, actual.height]
          const b = [(area.x + x0 - view.x) * view.zoom, (area.y + y0 - view.y) * view.zoom, (x1 - x0) * view.zoom, (y1 - y0) * view.zoom]
          if (a.some((value, axis) => Math.abs(value - b[axis]) > 0.75)) probe.transientUnexpected = true
        })
      })
      probe.observer.observe(document.querySelector('.pdf-pages'), { subtree: true, childList: true, attributes: true })
    }, partial.expected.pages[0].rects)
    await release(); await verify(partial, 1)
    const transient = await frame.evaluate(() => { window.__pdfSelection.observer.disconnect(); return { unexpected: window.__pdfSelection.transientUnexpected, maxActive: window.__pdfSelection.maxActive } })
    assert.equal(transient.unexpected, false); assert.equal(transient.maxActive, 1)
  })
  await run(7, async () => {
    for (const modifier of ['Space', 'middle']) {
      await place(1, 1, 1)
      const from = await point(1, [manifest.pages[0].width - 12, manifest.pages[0].height - 12]), before = await snap(), count = await calls()
      if (modifier === 'Space') { await kit.locator('.pdf-pages').focus(); await page.keyboard.down('Space') }
      try {
        await page.mouse.move(from.x, from.y); await page.mouse.down({ button: modifier === 'middle' ? 'middle' : 'left' })
        await page.mouse.move(from.x + 30, from.y + 20, { steps: 4 }); await page.mouse.up({ button: modifier === 'middle' ? 'middle' : 'left' })
      } finally { if (modifier === 'Space') await page.keyboard.up('Space') }
      await settle()
      const after = await snap()
      assert.ok(Math.abs(after.view.x - before.view.x) > 1 || Math.abs(after.view.y - before.view.y) > 1)
      assert.equal(await calls(), count)
    }
  })
  await place(1, 2, 1)
  await run(8, async () => { const spec = crossPageCase(manifest.pages[0], manifest.pages[1]); await drag(spec); await verify(spec, 1) })
  await run(9, async () => { const spec = crossPageCase(manifest.pages[0], manifest.pages[1], true); await drag(spec); await verify(spec, 1) })
  await run(10, async () => {
    const { a } = fixtureGlyphs(manifest.pages[0]), { b } = fixtureGlyphs(manifest.pages[1])
    await drag({ page: 1, endPage: 2, mode: 'column', anchor: a.slice(0, 2), focus: [(b[0] + b[2]) / 2, (b[1] + b[3]) / 2] })
    await quiet(); await settle()
    const actual = await snap()
    assert.equal(actual.latest.params.mode, 'column'); assert.equal(actual.latest.params.endPage, 1)
    assertSelected(actual.latest.result, full.expected)
  })
  await place(1, 1, 1)
  await run(11, async () => {
    await select(); await hold(); await drag(partial); await held()
    await page.evaluate(() => window.devHost.connection().ai.run([{ command: 'bookmark.add', params: { page: 1, title: 'Original selection revision check' } }], false))
    await none(); await disabled(); await release(); await quiet(); await settle(); await none(); await disabled()
  })
  await run(13, async () => {
    await kit.getByTestId('pdf-actual-size').press('ControlOrMeta+f')
    await kit.getByTestId('pdf-find-text').fill('AB CD')
    await frame.waitForFunction(() => document.querySelector('[data-testid="pdf-find-count"]')?.textContent === '1/8')
    await place(1, 1, 1)
    const start = await point(1, full.anchor)
    await page.mouse.move(start.x, start.y); await page.mouse.down()
    await kit.getByTestId('pdf-find-bar').waitFor({ state: 'hidden' }); await settle()
    // Follow the glyph's current screen position after closing Find changes the canvas extent.
    const end = await point(1, full.focus)
    await page.mouse.move(end.x, end.y, { steps: 4 }); await page.mouse.up()
    await verify(full, 1)
    assert.equal(await kit.getByTestId('pdf-search-highlight').count(), 0)
  })
  await run(12, async () => {
    await select()
    await kit.getByTestId('pdf-save').click()
    await page.waitForFunction(() => window.devHost.files.get('selection-geometry.pdf')?.version > 1)
    await upload('selection-geometry-replacement.pdf')
    await page.waitForFunction(() => document.getElementById('status')?.textContent?.includes('selection-geometry-replacement.pdf open in'))
    await canvas.waitFor()
    assert.equal(await kit.getByTestId('pdf-selection-highlight').count(), 0)
    await disabled()
  })
}
