/**
 * The view's keys and zoom steps (plan 2026-11-26 stage 1): how far a key scrolls, the zoom
 * ladder Cmd+= and Cmd+- step along, a typed percentage, and how much a mouse wheel's notch with
 * Cmd zooms. Distances are CSS pixels on screen; the canvas's view is in document points.
 */
import type { KitCanvasHandle } from '@kition/sdk/editor'

/** One arrow key's scroll, and what a screen's scroll (Space, PageDown) keeps of the last one. */
export const KEY_LINE = 40

/** The zoom range the keys, the status bar and a mouse wheel keep to (10 % to 3200 %). */
export const MIN_ZOOM = 0.1
export const MAX_ZOOM = 32

/** The zoom steps of Cmd+= and Cmd+- and of the status bar's list. */
export const ZOOM_LADDER: readonly number[] = [0.1, 0.25, 0.33, 0.5, 0.66, 0.75, 1, 1.25, 1.5, 2, 3, 4, 6, 8, 12, 16, 32]

/** How much one notch of a mouse wheel zooms, with Cmd (Ctrl off the Mac). */
export const WHEEL_NOTCH_ZOOM = 1.2

export function clampZoom(zoom: number) {
  return Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, zoom))
}

/** The ladder's next step above (`1`) or below (`-1`) `zoom`; a zoom on a step moves one step. */
export function ladderStep(zoom: number, direction: 1 | -1): number {
  if (!Number.isFinite(zoom) || zoom <= 0) return 1
  if (direction > 0) return ZOOM_LADDER.find((step) => step > zoom * 1.001) ?? MAX_ZOOM
  return [...ZOOM_LADDER].reverse().find((step) => step < zoom * 0.999) ?? MIN_ZOOM
}

/** A typed zoom (`150`, `150%`, `62.5 %`) as a zoom kept in range; null when it is no number. */
export function parseZoom(text: string): number | null {
  const match = /^\s*(\d+(?:[.,]\d+)?)\s*%?\s*$/.exec(text)
  if (!match) return null
  const percent = Number(match[1].replace(',', '.'))
  return Number.isFinite(percent) && percent > 0 ? clampZoom(percent / 100) : null
}

/** A zoom as the status bar shows it: `197%`, `33%`, `12.5%`. */
export function formatZoom(zoom: number) {
  const percent = zoom * 100
  return `${percent >= 10 ? Math.round(percent) : Math.round(percent * 10) / 10}%`
}

/**
 * The number of notches a wheel event is when it comes from a mouse wheel: whole lines or pages,
 * or a pixel delta that is a whole number of 100 px notches (in CSS pixels, or in device pixels:
 * at a pixel ratio of 2 a browser may report a notch as 50); null for a trackpad's (and a
 * pinch's) small, smooth deltas, which keep the canvas's own zoom.
 */
export function wheelNotches(event: Pick<WheelEvent, 'deltaMode' | 'deltaX' | 'deltaY'>, pixelRatio = 1): number | null {
  if (event.deltaX !== 0 || event.deltaY === 0 || !Number.isFinite(event.deltaY)) return null
  if (event.deltaMode === 1) return Math.sign(event.deltaY) * Math.max(1, Math.round(Math.abs(event.deltaY) / 3))
  if (event.deltaMode === 2) return Math.sign(event.deltaY) * Math.max(1, Math.round(Math.abs(event.deltaY)))
  const ratios = Number.isFinite(pixelRatio) && pixelRatio > 1 ? [1, pixelRatio] : [1]
  for (const ratio of ratios) {
    const notches = (event.deltaY * ratio) / 100
    const whole = Math.round(notches)
    if (whole !== 0 && Math.abs(notches - whole) < 0.02) return whole
  }
  return null
}

/** Zooms the canvas to `zoom` (kept in range) keeping the document point under (`clientX`, `clientY`) there. */
export function zoomAround(handle: KitCanvasHandle, surface: Element, zoom: number, clientX: number, clientY: number) {
  const before = handle.view()
  if (!before || !(before.zoom > 0)) return
  const box = surface.getBoundingClientRect()
  const sx = clientX - box.left
  const sy = clientY - box.top
  const x = before.x + sx / before.zoom
  const y = before.y + sy / before.zoom
  handle.zoomTo(clampZoom(zoom))
  const after = handle.view()
  if (!after || !(after.zoom > 0)) return
  handle.panTo(x - sx / after.zoom, y - sy / after.zoom)
}

/** Scrolls the view by (`dx`, `dy`) screen pixels; the canvas stops it at the pages' bounds. */
export function scrollView(handle: KitCanvasHandle, dx: number, dy: number) {
  const view = handle.view()
  if (!view || !(view.zoom > 0)) return
  handle.panTo(view.x + dx / view.zoom, view.y + dy / view.zoom)
}

/** A screen's scroll: the view's height less one line, so the last line stays in view. */
export function screenStep(height: number) {
  return Math.max(KEY_LINE, height - KEY_LINE)
}
