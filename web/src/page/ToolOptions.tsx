/**
 * The tool rail's tools and the options bar under the ribbon (stage 5, L5). With a comment tool
 * on, the bar sets what the tool makes next: its colour, opacity and line width (a colour also
 * goes to the selected comment when that tool made its kind); a one-shot tool (sticky note, text
 * box, page text) can be kept on. With Select on and a comment selected, the bar changes that
 * comment (`comment.edit`: one undo step each). It is always drawn, its controls off when there is
 * nothing for them, so choosing a comment never moves the pages under the pointer.
 */
import { BarSeparator, Checkbox, Choice, ColorGrid, Hint, TesseraIcons, useKitEditor, type RailTool } from '@kition/sdk/editor'
import { COMMENT_COLORS } from './PageMenu'
import { OPACITIES, TOOLS, WIDTHS, toolDef, type ToolId, type ToolStyle } from './tools'
import type { PageComment } from './geometry'

const ICONS: Record<ToolId, RailTool['icon']> = {
  select: TesseraIcons.MousePointer2Icon,
  hand: TesseraIcons.HandIcon,
  note: TesseraIcons.StickyNoteIcon,
  textbox: TesseraIcons.TextCursorInputIcon,
  addText: TesseraIcons.TypeIcon,
  highlight: TesseraIcons.HighlighterIcon,
  underline: TesseraIcons.UnderlineIcon,
  strikeout: TesseraIcons.StrikethroughIcon,
  ink: TesseraIcons.PenLineIcon,
  rectangle: TesseraIcons.SquareIcon,
  oval: TesseraIcons.CircleIcon,
  line: TesseraIcons.MinusIcon,
  arrow: TesseraIcons.MoveUpRightIcon,
}

/** Each tool's name in the editor's strings (`strings.ts`). */
export const TOOL_LABELS: Record<ToolId, string> = {
  select: 'toolSelect',
  hand: 'toolHand',
  note: 'toolNote',
  textbox: 'toolTextBox',
  addText: 'toolAddText',
  highlight: 'toolHighlight',
  underline: 'toolUnderline',
  strikeout: 'toolStrikeout',
  ink: 'toolInk',
  rectangle: 'toolRectangle',
  oval: 'toolOval',
  line: 'toolLine',
  arrow: 'toolArrow',
}

export function toolIcon(id: ToolId) {
  return ICONS[id]
}

/** The rail's tools, named in the editor's language. */
export function railTools(t: (key: string) => string): RailTool[] {
  return TOOLS.map((tool) => ({ id: tool.id, label: t(TOOL_LABELS[tool.id]), icon: ICONS[tool.id], shortcut: tool.shortcut ?? null, section: tool.section }))
}

/** The comment kinds whose line width can be changed (`comment.edit` `width`). */
const WIDE = new Set(['Square', 'Circle', 'Line', 'Ink', 'Polygon', 'PolyLine', 'FreeText'])
/** The comment kinds that take no opacity from the bar (a stamp keeps its own look). */
const OPAQUE = new Set(['Stamp'])

export function ToolOptions({ tool, style, comment, known, keep, onColor, onOpacity, onWidth, onKeep }: {
  tool: ToolId
  /** The active comment tool's look (unused with Select or the hand). */
  style: ToolStyle
  /** The comment Select has chosen (null: none, or another tool is on). */
  comment: PageComment | null
  /** What the editor knows of the chosen comment's opacity and width (it set them this session). */
  known: { opacity?: number; width?: number } | null
  keep: boolean
  onColor: (color: string) => void
  onOpacity: (opacity: number) => void
  onWidth: (width: number) => void
  onKeep: (keep: boolean) => void
}) {
  const { t, enabled } = useKitEditor()
  const def = toolDef(tool)
  const drawing = def.kind !== 'view'
  const target = drawing ? 'tool' : comment ? 'comment' : 'none'
  const canEdit = target === 'tool' || (target === 'comment' && !!comment && !comment.locked && enabled('comment.edit'))
  const color = target === 'tool' ? style.color : comment?.color ?? ''
  const hasWidth = target === 'tool' ? !!def.width : !!comment && WIDE.has(comment.type)
  const hasOpacity = target === 'tool' ? def.opacity : !!comment && !OPAQUE.has(comment.type)
  const opacity = target === 'tool' ? style.opacity : known?.opacity
  const width = target === 'tool' ? style.width : known?.width
  const minWidth = target === 'tool' ? def.width?.min ?? 0.5 : comment?.type === 'FreeText' ? 0 : 0.5
  const unknown: [string, string] = ['', '—']
  // The value now, and the list it is chosen from (the value itself added when it is not a step).
  const opacityValue = opacity == null ? '' : String(Math.round(opacity * 100))
  const opacityChoices: Array<[string, string]> = OPACITIES.map((value) => [String(value), `${value}%`])
  if (!opacityChoices.some(([value]) => value === opacityValue)) opacityChoices.unshift(opacity == null ? unknown : [opacityValue, `${opacityValue}%`])
  const widthValue = width == null ? '' : String(width)
  const widthChoices: Array<[string, string]> = WIDTHS.filter((value) => value >= minWidth).map((value) => [String(value), value === 0 ? t('widthNone') : `${value} pt`])
  if (!widthChoices.some(([value]) => value === widthValue)) widthChoices.unshift(width == null ? unknown : [widthValue, `${widthValue} pt`])
  const hint = target === 'tool' ? t('optionsForTool', { tool: t(TOOL_LABELS[tool]) }) : target === 'comment' ? t('optionsForComment') : t('optionsNone')
  return (
    <div className="pdf-tool-options" data-testid="pdf-options" data-target={target}>
      <Hint testId="pdf-options-hint">{hint}</Hint>
      <BarSeparator />
      <span className="pdf-tool-color" aria-hidden="true" style={{ background: color || 'transparent' }} data-testid="pdf-options-color" data-color={color} />
      {canEdit ? <ColorGrid colors={COMMENT_COLORS} onPick={onColor} testId="pdf-options-colors" /> : <span className="pdf-tool-colors-off" aria-hidden="true" />}
      <BarSeparator />
      <Choice label={t('opacity')} value={hasOpacity ? opacityValue : ''} choices={hasOpacity ? opacityChoices : [unknown]} disabled={!canEdit || !hasOpacity} onChange={(value) => value && onOpacity(Number(value) / 100)} testId="pdf-options-opacity" />
      <Choice label={t('lineWidth')} value={hasWidth ? widthValue : ''} choices={hasWidth ? widthChoices : [unknown]} disabled={!canEdit || !hasWidth} onChange={(value) => value !== '' && onWidth(Number(value))} testId="pdf-options-width" />
      <BarSeparator />
      <Checkbox label={t('keepTool')} checked={keep} disabled={!def.oneShot} onChange={onKeep} testId="pdf-options-keep" />
    </div>
  )
}
