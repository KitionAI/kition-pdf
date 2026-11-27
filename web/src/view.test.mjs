import test from 'node:test'
import assert from 'node:assert/strict'
import { formatZoom, ladderStep, parseZoom, screenStep, wheelNotches, zoomAround, MAX_ZOOM, MIN_ZOOM } from './view.ts'

test('Cmd+= and Cmd+- step along the zoom ladder and stop at its ends', () => {
  assert.equal(ladderStep(1, 1), 1.25)
  assert.equal(ladderStep(1, -1), 0.75)
  assert.equal(ladderStep(1.97, 1), 2)
  assert.equal(ladderStep(1.97, -1), 1.5)
  assert.equal(ladderStep(MAX_ZOOM, 1), MAX_ZOOM)
  assert.equal(ladderStep(MIN_ZOOM, -1), MIN_ZOOM)
  assert.equal(ladderStep(0.07, -1), MIN_ZOOM)
})

test('a typed zoom takes 150, 150% and 62.5 %, kept to 10-3200 %', () => {
  assert.equal(parseZoom('150'), 1.5)
  assert.equal(parseZoom('150%'), 1.5)
  assert.equal(parseZoom(' 62.5 % '), 0.625)
  assert.equal(parseZoom('5'), MIN_ZOOM)
  assert.equal(parseZoom('9000'), MAX_ZOOM)
  for (const text of ['', 'abc', '0', '-20', '1e3']) assert.equal(parseZoom(text), null, text)
  assert.equal(formatZoom(1.97), '197%')
  assert.equal(formatZoom(0.33), '33%')
})

test('a mouse wheel notch is a whole 100 px or a line; a trackpad delta is none', () => {
  assert.equal(wheelNotches({ deltaMode: 0, deltaX: 0, deltaY: -100 }), -1)
  assert.equal(wheelNotches({ deltaMode: 0, deltaX: 0, deltaY: 300 }), 3)
  assert.equal(wheelNotches({ deltaMode: 1, deltaX: 0, deltaY: 3 }), 1)
  assert.equal(wheelNotches({ deltaMode: 0, deltaX: 0, deltaY: -10 }), null)
  assert.equal(wheelNotches({ deltaMode: 0, deltaX: 4, deltaY: -100 }), null)
  assert.equal(wheelNotches({ deltaMode: 0, deltaX: 0, deltaY: -50 }, 2), -1)
  assert.equal(wheelNotches({ deltaMode: 0, deltaX: 0, deltaY: -50 }), null)
  assert.equal(wheelNotches({ deltaMode: 0, deltaX: 0, deltaY: -5 }, 2), null)
})

test('a screen scroll keeps one line; a wheel zoom keeps the point under the pointer', () => {
  assert.equal(screenStep(844), 804)
  assert.equal(screenStep(20), 40)
  let view = { x: 100, y: 200, zoom: 2, width: 800, height: 600 }
  const handle = { view: () => view, zoomTo: (zoom) => { view = { ...view, zoom } }, panTo: (x, y) => { view = { ...view, x, y } } }
  const surface = { getBoundingClientRect: () => ({ left: 10, top: 20 }) }
  zoomAround(handle, surface, 2.4, 410, 320)
  assert.equal(view.zoom, 2.4)
  assert.ok(Math.abs(view.x + 400 / 2.4 - (100 + 400 / 2)) < 1e-9)
  assert.ok(Math.abs(view.y + 300 / 2.4 - (200 + 300 / 2)) < 1e-9)
})
