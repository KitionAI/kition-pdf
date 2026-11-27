/**
 * The comment tools (stage 5 of "PDF usable by hand"): what each tool on the rail makes, how it
 * looks by default, and the geometry a drag on the page turns into for `comment.add`. Points here
 * are page-local, as everywhere in the page layer (`geometry.ts`): from the displayed page's
 * top-left, y down, the space `comment.add` takes its `at`, `rect`, `from`/`to`, `quads` and
 * `strokes` in.
 */
import type { Rect } from './geometry'

export type ToolId =
  | 'select'
  | 'hand'
  | 'note'
  | 'textbox'
  | 'addText'
  | 'highlight'
  | 'underline'
  | 'strikeout'
  | 'rectangle'
  | 'oval'
  | 'line'
  | 'arrow'
  | 'ink'

/**
 * How a tool works on the page: `view` tools are the page layer and the hand; `place` tools open a
 * card where they are clicked (a text box also takes a dragged rectangle); `markup` tools mark the
 * text dragged over; `shape` tools drag out a rectangle, an oval or a line; `ink` draws a stroke.
 */
export type ToolKind = 'view' | 'place' | 'markup' | 'shape' | 'ink'

export type ToolStyle = { color: string; opacity: number; width: number }

export type ToolDef = {
  id: ToolId
  kind: ToolKind
  /** The `comment.add` type it makes (`addText` makes page text with `page.addText`). */
  type: string | null
  /** The comment type `comment.list` answers for what it makes (its subtype). */
  subtype: string | null
  /** A one-shot tool goes back to Select once it has made one (unless kept); the others stay. */
  oneShot: boolean
  /** Whether the line width applies, and its smallest value. */
  width: { min: number } | null
  /** Whether opacity applies. */
  opacity: boolean
  /** Whether what it makes is selected once made (a drawing is not, so it never covers the next stroke). */
  selects: boolean
  /** The rail's section: a hairline divides one from the next. */
  section: number
  shortcut?: string
}

export const TOOLS: readonly ToolDef[] = [
  { id: 'select', kind: 'view', type: null, subtype: null, oneShot: false, width: null, opacity: false, selects: false, section: 0, shortcut: 'V' },
  { id: 'hand', kind: 'view', type: null, subtype: null, oneShot: false, width: null, opacity: false, selects: false, section: 0, shortcut: 'H' },
  { id: 'note', kind: 'place', type: 'note', subtype: 'Text', oneShot: true, width: null, opacity: true, selects: true, section: 1 },
  { id: 'textbox', kind: 'place', type: 'textbox', subtype: 'FreeText', oneShot: true, width: { min: 0 }, opacity: true, selects: true, section: 1 },
  { id: 'addText', kind: 'place', type: null, subtype: null, oneShot: true, width: null, opacity: false, selects: false, section: 1 },
  { id: 'highlight', kind: 'markup', type: 'highlight', subtype: 'Highlight', oneShot: false, width: null, opacity: true, selects: true, section: 2 },
  { id: 'underline', kind: 'markup', type: 'underline', subtype: 'Underline', oneShot: false, width: null, opacity: true, selects: true, section: 2 },
  { id: 'strikeout', kind: 'markup', type: 'strikeout', subtype: 'StrikeOut', oneShot: false, width: null, opacity: true, selects: true, section: 2 },
  { id: 'ink', kind: 'ink', type: 'ink', subtype: 'Ink', oneShot: false, width: { min: 0.5 }, opacity: true, selects: false, section: 3 },
  { id: 'rectangle', kind: 'shape', type: 'rectangle', subtype: 'Square', oneShot: false, width: { min: 0.5 }, opacity: true, selects: true, section: 3 },
  { id: 'oval', kind: 'shape', type: 'oval', subtype: 'Circle', oneShot: false, width: { min: 0.5 }, opacity: true, selects: true, section: 3 },
  { id: 'line', kind: 'shape', type: 'line', subtype: 'Line', oneShot: false, width: { min: 0.5 }, opacity: true, selects: true, section: 3 },
  { id: 'arrow', kind: 'shape', type: 'arrow', subtype: 'Line', oneShot: false, width: { min: 0.5 }, opacity: true, selects: true, section: 3 },
]

export function toolDef(id: ToolId): ToolDef {
  return TOOLS.find((tool) => tool.id === id) ?? TOOLS[0]
}

export function isToolId(value: string): value is ToolId {
  return TOOLS.some((tool) => tool.id === value)
}

/** Each tool's look until the person changes it, the engine's own defaults (`comment.add` without a style). */
export const DEFAULT_STYLES: Readonly<Record<ToolId, ToolStyle>> = {
  select: { color: '#000000', opacity: 1, width: 1 },
  hand: { color: '#000000', opacity: 1, width: 1 },
  note: { color: '#FFD100', opacity: 1, width: 1 },
  textbox: { color: '#000000', opacity: 1, width: 0 },
  addText: { color: '#000000', opacity: 1, width: 0 },
  highlight: { color: '#FFF000', opacity: 1, width: 1 },
  underline: { color: '#0078D6', opacity: 1, width: 1 },
  strikeout: { color: '#E32121', opacity: 1, width: 1 },
  rectangle: { color: '#E32121', opacity: 1, width: 2 },
  oval: { color: '#E32121', opacity: 1, width: 2 },
  line: { color: '#E32121', opacity: 1, width: 2 },
  arrow: { color: '#E32121', opacity: 1, width: 2 },
  ink: { color: '#0066DE', opacity: 1, width: 2 },
}

/** The opacities and line widths the options bar offers (percent; points). */
export const OPACITIES = [100, 90, 80, 70, 60, 50, 40, 30, 20, 10] as const
export const WIDTHS = [0, 0.5, 1, 1.5, 2, 3, 4, 6, 8, 12] as const

/**
 * The style values a new comment of `tool` is given: only those that differ from the engine's own
 * default for it, so a comment made with the defaults is exactly what the engine makes.
 */
export function styleParams(tool: ToolId, style: ToolStyle): Record<string, unknown> {
  const def = toolDef(tool)
  const base = DEFAULT_STYLES[tool]
  const out: Record<string, unknown> = {}
  if (style.color.toUpperCase() !== base.color.toUpperCase()) out.color = style.color.toUpperCase()
  if (def.opacity && Math.abs(style.opacity - base.opacity) > 1e-6) out.opacity = round2(style.opacity)
  if (def.width && Math.abs(style.width - base.width) > 1e-6) out.width = round2(style.width)
  return out
}

/** The smallest side (or length) a shape is made with, in points: a smaller drag makes nothing. */
export const MIN_SHAPE = 2
/** A text box dragged less than this many CSS pixels either way is a click: the box fits its text. */
export const PLACE_DRAG_PX = 6
/** An area highlight's smallest side, in CSS pixels. */
export const AREA_MIN_PX = 3
/** At most this many points in one stroke (a longer drag keeps every other one, and so on). */
export const MAX_STROKE_POINTS = 2000

export const round2 = (value: number) => Math.round(value * 100) / 100

/** The rectangle a drag from `a` to `b` makes; with `square`, a square of the larger side, from `a`. */
export function shapeRect(a: readonly [number, number], b: readonly [number, number], square = false): Rect {
  let [x, y] = b
  if (square) {
    const side = Math.max(Math.abs(b[0] - a[0]), Math.abs(b[1] - a[1]))
    x = a[0] + Math.sign(b[0] - a[0] || 1) * side
    y = a[1] + Math.sign(b[1] - a[1] || 1) * side
  }
  return [Math.min(a[0], x), Math.min(a[1], y), Math.max(a[0], x), Math.max(a[1], y)]
}

/** Whether a shape's rectangle is big enough to make (both sides at least `MIN_SHAPE`). */
export function shapeBigEnough(rect: Rect) {
  return rect[2] - rect[0] >= MIN_SHAPE && rect[3] - rect[1] >= MIN_SHAPE
}

/** A line's end: with `level`, held level or plumb (whichever is nearer the pointer), not at 45°. */
export function lineEnd(from: readonly [number, number], to: readonly [number, number], level = false): [number, number] {
  if (!level) return [to[0], to[1]]
  return Math.abs(to[0] - from[0]) >= Math.abs(to[1] - from[1]) ? [to[0], from[1]] : [from[0], to[1]]
}

export function lineLongEnough(from: readonly [number, number], to: readonly [number, number]) {
  return Math.hypot(to[0] - from[0], to[1] - from[1]) >= MIN_SHAPE
}

/** A point clamped to a page `width` by `height` points. */
export function onPage(point: readonly [number, number], width: number, height: number): [number, number] {
  return [Math.max(0, Math.min(width, point[0])), Math.max(0, Math.min(height, point[1]))]
}

/**
 * A stroke's points as `comment.add` takes them: rounded to 1/100 pt, without repeats closer than
 * `step` points, and thinned to at most `MAX_STROKE_POINTS`. A click's single point becomes a dot
 * (the point twice, a hair apart), so it still draws.
 */
export function strokePoints(points: readonly (readonly [number, number])[], step = 0.5): number[][] {
  const out: number[][] = []
  for (const point of points) {
    if (!point.every(Number.isFinite)) continue
    const last = out.at(-1)
    if (last && Math.hypot(point[0] - last[0], point[1] - last[1]) < step) continue
    out.push([round2(point[0]), round2(point[1])])
  }
  const end = points.at(-1)
  if (end && end.every(Number.isFinite) && out.length > 1) {
    const last = out[out.length - 1]
    if (last[0] !== round2(end[0]) || last[1] !== round2(end[1])) out.push([round2(end[0]), round2(end[1])])
  }
  if (out.length === 1) out.push([round2(out[0][0] + 0.1), out[0][1]])
  let thinned = out
  while (thinned.length > MAX_STROKE_POINTS) thinned = thinned.filter((_, i) => i % 2 === 0 || i === thinned.length - 1)
  return thinned
}

/** A rectangle as one quad, upper-left, upper-right, lower-left, lower-right (as `comment.add` takes `quads`). */
export function rectQuadOf(rect: Rect): number[] {
  const [l, t, r, b] = rect.map(round2)
  return [l, t, r, t, l, b, r, b]
}

/** The text box a click makes for `text`: fitted to it (40 to 300 points wide, wrapped beyond), 2 points of padding, lines 1.2 times the size. */
export const TEXTBOX = { minWidth: 40, maxWidth: 300, pad: 2, size: 12, lineHeight: 1.2 }

/**
 * The rectangle of a text box clicked at `at` for `text`, `measure` giving a string's width in
 * points at the box's size: as wide as its longest line (wrapping at the widest), as tall as its
 * lines, moved left and up to stay on a page `width` by `height`.
 */
export function fitTextBox(text: string, at: readonly [number, number], measure: (line: string) => number, width: number, height: number): Rect {
  const { minWidth, maxWidth, pad, size, lineHeight } = TEXTBOX
  const inner = maxWidth - pad * 2
  let lines = 0
  let widest = 0
  for (const paragraph of text.split('\n')) {
    let line = ''
    let count = 1
    for (const word of paragraph.split(/(\s+)/)) {
      const next = line + word
      if (line && measure(next.trimEnd()) > inner) {
        widest = Math.max(widest, measure(line.trimEnd()))
        line = word.trimStart()
        count += 1
      } else line = next
    }
    widest = Math.max(widest, Math.min(inner, measure(line.trimEnd())))
    lines += count
  }
  const w = Math.max(minWidth, Math.min(maxWidth, Math.ceil(widest + pad * 2 + 1)))
  const h = Math.ceil(Math.max(1, lines) * size * lineHeight + pad * 2)
  const x = Math.max(0, Math.min(at[0], width - w))
  const y = Math.max(0, Math.min(at[1], height - h))
  return [round2(x), round2(y), round2(x + w), round2(y + h)]
}
