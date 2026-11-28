/**
 * Pointing at the page for a parameters dialog (stage 10 of "PDF usable by hand"): a command
 * with no tool of its own (a link, a redaction box, a measurement, a Fill & Sign mark, page
 * text, a form field) still takes its place from the page. Its dialog offers, beside a point
 * (`at`), a rectangle (`rect`) or a list of points (`points`), a button that puts the dialog
 * away while the person clicks, drags a box or clicks several points on a page; the value comes
 * back into the field, and the page into `page`. Points are page-local, as everywhere in the page
 * layer (`geometry.ts`): points from the displayed page's top-left, y down, the space these
 * commands take their `at`, `rect` and `points` in.
 */
import type { ParamNode } from '../params'

export type PickKind = 'point' | 'rect' | 'points'

/** What a field takes from the page: its kind and, for points, how many. */
export type PickSpec = { key: string; kind: PickKind; min: number; max: number }

/** At most this many points are picked for one field (the measurements take up to 2048). */
export const MAX_PICK_POINTS = 200

/** The fewest points a measurement needs, and the most a distance takes. */
const POINTS_MIN: Record<string, number> = { 'measure.distance': 2, 'measure.perimeter': 2, 'measure.area': 3, 'measure.getInfo': 1 }
const POINTS_MAX: Record<string, number> = { 'measure.distance': 2 }

const isNumbers = (node: ParamNode | undefined, count: number) =>
  node?.type === 'array' &&
  (node.items?.type === 'number' || node.items?.type === 'integer') &&
  (node.minItems === undefined || node.minItems === count) &&
  (node.maxItems === undefined || node.maxItems === count)

/**
 * What field `key` of `command` (its schema, bounds read) can take from the page, or null: only
 * a command that also takes a `page` (the page is filled in with the value), and only the fields
 * named `at` (two numbers), `rect` (four) and `points` (pairs).
 */
export function pickSpec(command: string, key: string, node: ParamNode, schema: Record<string, ParamNode>): PickSpec | null {
  if (!('page' in schema) || schema.page?.type !== 'integer') return null
  if (key === 'at' && isNumbers(node, 2)) return { key, kind: 'point', min: 1, max: 1 }
  if (key === 'rect' && isNumbers(node, 4)) return { key, kind: 'rect', min: 1, max: 1 }
  if (key === 'points' && node.type === 'array' && isNumbers(node.items, 2)) {
    const max = Math.min(MAX_PICK_POINTS, POINTS_MAX[command] ?? node.maxItems ?? MAX_PICK_POINTS)
    const min = Math.min(max, POINTS_MIN[command] ?? Math.max(1, node.minItems ?? 1))
    return { key, kind: 'points', min, max }
  }
  return null
}

const round2 = (value: number) => Math.round(value * 100) / 100

/** A point clamped to a `width` by `height` page and rounded to 1/100 pt. */
export function pagePoint(point: readonly [number, number], width: number, height: number): [number, number] {
  return [round2(Math.max(0, Math.min(width, point[0]))), round2(Math.max(0, Math.min(height, point[1])))]
}

/** The rectangle between two page points, `[left, top, right, bottom]`, rounded to 1/100 pt. */
export function pickedRect(a: readonly [number, number], b: readonly [number, number]): [number, number, number, number] {
  return [round2(Math.min(a[0], b[0])), round2(Math.min(a[1], b[1])), round2(Math.max(a[0], b[0])), round2(Math.max(a[1], b[1]))]
}

/** A drag shorter than this many CSS pixels either way is a click: it makes no box. */
export const PICK_DRAG_PX = 6

/** Whether a drag from `a` to `b` (client pixels) is long enough both ways to be a box. */
export function dragIsBox(a: { clientX: number; clientY: number }, b: { clientX: number; clientY: number }) {
  return Math.abs(b.clientX - a.clientX) >= PICK_DRAG_PX && Math.abs(b.clientY - a.clientY) >= PICK_DRAG_PX
}

/** The value a field gets from what was picked. */
export function pickedValue(kind: PickKind, points: readonly (readonly [number, number])[]): number[] | number[][] | null {
  if (kind === 'point') return points[0] ? [points[0][0], points[0][1]] : null
  if (kind === 'rect') return points.length >= 2 ? pickedRect(points[0], points[1]) : null
  return points.length ? points.map((point) => [point[0], point[1]]) : null
}
