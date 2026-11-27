/**
 * The parameters dialog's values as a person types them (stage 2 of "PDF usable by hand"): a list
 * of pages as `3`, `3, 5` or `2-4`, a point as `300, 400`, a rectangle as four numbers, several
 * points as `x, y; x, y`, a list of words with commas; JSON still works for every array. The
 * command's schema is the engine's and is not changed: what is typed is turned into the arrays it
 * asks for before the command runs, and a value that cannot be is refused at its field.
 */

export type ParamNode = {
  type?: string
  description?: string
  enum?: unknown[]
  default?: unknown
  required?: boolean
  items?: ParamNode
  minItems?: number
  maxItems?: number
}

/**
 * A schema node with its list bounds: the editor's schemas carry `minItems` and `maxItems` in the
 * description (`… (minItems 2, maxItems 2)`, the value schema form keeps only types), so they are
 * read back from there, at every level.
 */
export function withBounds(node: ParamNode): ParamNode {
  const out: ParamNode = { ...node }
  const bound = (name: string) => {
    const found = new RegExp(`\\b${name} (\\d+)`).exec(node.description ?? '')
    return found ? Number(found[1]) : undefined
  }
  out.minItems ??= bound('minItems')
  out.maxItems ??= bound('maxItems')
  if (out.minItems === undefined) delete out.minItems
  if (out.maxItems === undefined) delete out.maxItems
  if (node.items) out.items = withBounds(node.items)
  return out
}

/** Why a typed value was refused, for the field's message. */
export type ParamError =
  | { kind: 'pages' }
  | { kind: 'numbers'; count: number | null }
  | { kind: 'points'; count: number | null }
  | { kind: 'number' }
  | { kind: 'whole' }
  | { kind: 'json' }
  | { kind: 'count'; min: number; max: number | null }

export type Parsed = { value: unknown } | { error: ParamError }

/** The most pages a range (`1-5000`) expands to. */
const MAX_RANGE = 10_000

const isNumbers = (node: ParamNode | undefined) => node?.type === 'array' && (node.items?.type === 'number' || node.items?.type === 'integer')
const isPoints = (node: ParamNode) => node.type === 'array' && isNumbers(node.items)

/** How a value shows in its field: lists without brackets, points with semicolons between them. */
export function shownValue(node: ParamNode, value: unknown): string {
  if (value === undefined || value === null) return ''
  if (typeof value === 'string') return value
  if (Array.isArray(value)) {
    if (isNumbers(node) && value.every((item) => typeof item === 'number')) return value.join(', ')
    if (isPoints(node) && value.every((item) => Array.isArray(item) && item.every((n) => typeof n === 'number'))) return value.map((item) => (item as number[]).join(', ')).join('; ')
    if (node.items?.type === 'string' && value.every((item) => typeof item === 'string' && !item.includes(','))) return value.join(', ')
  }
  return JSON.stringify(value)
}

/** The text a field starts with: the value given, else the schema's default. */
export function initialText(node: ParamNode, given: unknown): string {
  if (given !== undefined) return shownValue(node, given)
  if (node.default !== undefined) return shownValue(node, node.default)
  if (node.enum?.length) return node.required ? String(node.enum[0]) : ''
  return node.type === 'boolean' ? 'false' : ''
}

const SEPARATORS = /[\s,，;；、]+/

function numbersOf(text: string, whole: boolean): number[] | null {
  const parts = text.split(SEPARATORS).filter(Boolean)
  if (!parts.length) return null
  const out: number[] = []
  for (const part of parts) {
    const value = Number(part)
    if (!Number.isFinite(value) || (whole && !Number.isInteger(value))) return null
    out.push(value)
  }
  return out
}

/** `3`, `3, 5`, `2-4`, `1, 3-5` (and the en dash or `~`): page numbers, ranges expanded. */
export function pageList(text: string): number[] | null {
  const normal = text.trim().replace(/\s*[-–—~～]\s*/g, '-')
  const parts = normal.split(SEPARATORS).filter(Boolean)
  if (!parts.length) return null
  const out: number[] = []
  for (const part of parts) {
    const range = /^(\d+)-(\d+)$/.exec(part)
    if (range) {
      const [from, to] = [Number(range[1]), Number(range[2])]
      const step = from <= to ? 1 : -1
      if (Math.abs(to - from) + out.length >= MAX_RANGE) return null
      for (let page = from; page !== to + step; page += step) out.push(page)
    } else if (/^\d+$/.test(part)) out.push(Number(part))
    else return null
  }
  return out
}

function counted(node: ParamNode, list: unknown[]): Parsed {
  const min = node.minItems ?? 0
  const max = node.maxItems ?? null
  if (list.length < min || (max !== null && list.length > max)) return { error: { kind: 'count', min, max } }
  return { value: list }
}

function json(text: string): Parsed {
  try {
    return { value: JSON.parse(text) }
  } catch {
    return { error: { kind: 'json' } }
  }
}

/** One field's typed text as the command's value; `{ value: undefined }` leaves it out. */
export function parseValue(node: ParamNode, text: string): Parsed {
  const trimmed = text.trim()
  switch (node.type) {
    case 'number':
    case 'integer': {
      if (trimmed === '') return { value: undefined }
      const value = Number(trimmed)
      if (!Number.isFinite(value)) return { error: { kind: 'number' } }
      if (node.type === 'integer' && !Number.isInteger(value)) return { error: { kind: 'whole' } }
      return { value }
    }
    case 'boolean':
      return { value: text === 'true' }
    case 'string':
      return { value: text === '' ? undefined : text }
    case 'array': {
      if (trimmed === '') return { value: undefined }
      if (trimmed.startsWith('[')) {
        const parsed = json(trimmed)
        return 'error' in parsed || !Array.isArray(parsed.value) ? { error: { kind: 'json' } } : counted(node, parsed.value)
      }
      const items = node.items
      if (items?.type === 'integer') {
        const pages = pageList(trimmed)
        return pages ? counted(node, pages) : { error: { kind: 'pages' } }
      }
      if (items?.type === 'number') {
        const numbers = numbersOf(trimmed, false)
        const count = node.minItems !== undefined && node.minItems === node.maxItems ? node.minItems : null
        return numbers ? counted(node, numbers) : { error: { kind: 'numbers', count } }
      }
      if (items && isNumbers(items)) {
        const count = items.minItems !== undefined && items.minItems === items.maxItems ? items.minItems : null
        const points: number[][] = []
        for (const part of trimmed.split(/[;；\n]+/).map((item) => item.trim()).filter(Boolean)) {
          const point = numbersOf(part, items.items?.type === 'integer')
          if (!point || (count !== null && point.length !== count)) return { error: { kind: 'points', count } }
          points.push(point)
        }
        return counted(node, points)
      }
      if (items?.type === 'string') return counted(node, trimmed.split(/[,，\n]+/).map((item) => item.trim()).filter(Boolean))
      return json(trimmed)
    }
    default:
      if (trimmed === '') return { value: undefined }
      // An object (form.fill's values) or a value of several types: JSON, else the text itself.
      try {
        return { value: JSON.parse(trimmed) }
      } catch {
        return { value: text }
      }
  }
}

/**
 * The field an engine's error names (`pages must be an array of integers`, `"to" is required`,
 * `unknown argument "at"`), so the message shows under it; null when it names none of `keys`.
 */
export function errorField(message: string, keys: readonly string[]): string | null {
  // A message may start with the command it came from (`page.move: …`), which is no field.
  const text = message.replace(/^[A-Za-z_]+(\.[A-Za-z_]+)+:\s*/, '')
  let best: { key: string; at: number } | null = null
  for (const key of keys) {
    const pattern = new RegExp(`(^|[\\s\`"'(:])${key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?=$|[\\s\`"'):,.])`)
    const found = pattern.exec(text)
    if (!found) continue
    const at = found.index + found[1].length
    if (!best || at < best.at || (at === best.at && key.length > best.key.length)) best = { key, at }
  }
  return best?.key ?? null
}

/** `fontSize` → `Font size`, `out_dir` → `Out dir`: an English label for a parameter with none of its own. */
export function humanize(key: string): string {
  const words = key.replace(/_/g, ' ').replace(/([a-z0-9])([A-Z])/g, '$1 $2').toLowerCase().trim()
  return words ? words[0].toUpperCase() + words.slice(1) : key
}

/** People's names for the parameters a dialog shows most (English; translated by the phrases). */
export const PARAM_LABELS: Record<string, string> = {
  page: 'Page',
  pages: 'Pages',
  at: 'Position',
  to: 'To',
  from: 'From',
  contents: 'Content',
  text: 'Text',
  type: 'Type',
  rect: 'Rectangle',
  color: 'Color',
  fill: 'Fill color',
  opacity: 'Opacity',
  width: 'Line width',
  author: 'Author',
  name: 'Name',
  title: 'Title',
  find: 'Text to find',
  all: 'Every match',
  quads: 'Text areas',
  points: 'Points',
  strokes: 'Strokes',
  fontSize: 'Font size',
  icon: 'Icon',
  stamp: 'Stamp',
  dynamic: 'Dynamic stamp',
  endings: 'Line endings',
  knee: 'Bend point',
  url: 'Web address',
  field: 'Field',
  fields: 'Fields',
  values: 'Values',
  value: 'Value',
  degrees: 'Angle',
  status: 'Status',
  move: 'Move by',
  marked: 'Marked',
  locked: 'Locked',
  hidden: 'Hidden',
  options: 'Options',
  size: 'Size',
  rotation: 'Rotation',
  align: 'Alignment',
  bold: 'Bold',
  italic: 'Italic',
  font: 'Font',
  format: 'Format',
  language: 'Language',
  label: 'Label',
  mode: 'Mode',
  password: 'Password',
  scale: 'Scale',
  unit: 'Unit',
  words: 'Words',
  pattern: 'Pattern',
  wholePages: 'Whole pages',
  tooltip: 'Tooltip',
  required: 'Required',
  readOnly: 'Read-only',
  multiline: 'Multiple lines',
  maxLength: 'Maximum length',
  start: 'Start',
  startNumber: 'Start number',
  prefix: 'Prefix',
  position: 'Position',
  height: 'Height',
  toPage: 'To page',
  level: 'Level',
  kind: 'Kind',
  dpi: 'Resolution (dpi)',
  distance: 'Distance',
  image: 'Image',
  alt: 'Alternative text',
  reason: 'Reason',
  location: 'Location',
  id: 'Comment',
  index: 'Number on the page',
}

/** People's names for the choices of an enum a dialog shows (the rest are humanized). */
export const CHOICE_LABELS: Record<string, string> = {
  note: 'Note',
  strikeout: 'Strikethrough',
  textbox: 'Text box',
  polyline: 'Connected lines',
  caret: 'Insert text',
  replace: 'Replace text',
  ink: 'Freehand',
}
