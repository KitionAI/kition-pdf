/**
 * Filling a form on the page (stage 6 of "PDF usable by hand"): a control over each widget of
 * each field, where `form.listFields` says it is (its page and rect), moving with the view's zoom
 * and scroll and with the page's rotation: a text box (a text area when the field takes several
 * lines), a drop-down, a list, a check box, a radio button. A text box is filled when it lets go
 * (Tab, a press elsewhere) or on Enter; Escape puts the field's value back; a check box, a radio
 * button or a drop-down's choice fills at once. Each fill is one `form.fill`, one undo step. Tab
 * and Shift+Tab go through the document's tab order (`tab`), across pages, bringing the next field
 * into view. A read-only field takes no input (a press says so). A control is made again when its
 * field's value changes, so an undo or an agent's fill shows, as in the Fields panel: both read the
 * same `form.listFields`.
 *
 * The page's own pixels show a field that has an appearance; the control over it is clear until it
 * has the keyboard. A field without one (a file that leaves them to the viewer, `/NeedAppearances`)
 * would show nothing, so its control shows the field itself. With "Highlight fields" on, every
 * field that takes input is tinted, a required one framed in the danger colour. Never in the
 * rendered pixels: this is HTML over the canvas.
 */
import { useEffect, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type Ref } from 'react'
import { Button, IconButton, TesseraIcons, useKitEditor, type RasterView } from '@kition/sdk/editor'
import { asRect, screenBox, widgetsOf, type Area, type FieldOption, type FieldWidget, type PageField, type Rect } from './geometry'
import { focusPages, pagesRegion } from '../focus'

const X = TesseraIcons.XIcon

/** One widget's control: its field, which of its widgets, where it is. */
export type FieldSlot = { key: string; field: PageField; widget: FieldWidget; index: number; page: number; rect: Rect; order: number }

/** The kinds of field a person fills on the page (buttons and signatures are the page's own). */
const FILLABLE = new Set(['text', 'checkbox', 'radio', 'combo', 'list'])

/** Every widget of the fillable fields that is on a page and shown, in the document's tab order (then page and place). */
export function fieldSlots(fields: readonly PageField[]): FieldSlot[] {
  const out: FieldSlot[] = []
  for (const field of fields) {
    if (!FILLABLE.has(field.type)) continue
    widgetsOf(field).forEach((widget, index) => {
      const rect = asRect(widget.rect)
      if (widget.hidden || typeof widget.page !== 'number' || !rect) return
      out.push({ key: `${field.name}@${index}`, field, widget, index, page: widget.page, rect, order: typeof widget.tab === 'number' ? widget.tab : Number.MAX_SAFE_INTEGER })
    })
  }
  return out.sort((a, b) => a.order - b.order || a.page - b.page || a.rect[1] - b.rect[1] || a.rect[0] - b.rect[0])
}

/** How many fields the document asks to be filled: every field but buttons (the info bar's count). */
export function fillableCount(fields: readonly PageField[]) {
  return fields.filter((field) => field.type !== 'button').length
}

const optionValue = (option: FieldOption) => (typeof option === 'string' ? option : option.value)
const optionLabel = (option: FieldOption) => (typeof option === 'string' ? option : option.label || option.value)
const textOf = (value: unknown) => (value == null || typeof value === 'boolean' ? '' : Array.isArray(value) ? value.map(String).join(', ') : String(value))

/** The type size a text control shows its field's text in, in points: its own, else what the engine's auto size takes. */
function pointSize(field: PageField, rect: Rect) {
  const own = typeof field.font_size === 'number' && field.font_size > 0 ? field.font_size : 0
  if (own) return own
  const inner = rect[3] - rect[1] - 6
  return field.multiline ? 12 : Math.max(4, Math.min(12, inner / 1.15))
}

export type FormFieldsHandle = {
  /** Tab from the pages: the first field (the last, `back`) at or after page `from`; answers whether one took the keyboard. */
  enter: (back: boolean, from: number) => boolean
}

type Props = {
  fields: readonly PageField[]
  pages: readonly { area: Area }[]
  view: RasterView | null
  /** Only with Select do the fields take the pointer; another tool's press is that tool's. */
  active: boolean
  highlight: boolean
  /** Whether the document takes fills (a read-only session shows the values only). */
  canFill: boolean
  /** Fills one field (`form.fill`); answers when it is done. */
  fill: (name: string, value: unknown) => Promise<unknown>
  /** A field took the keyboard (the text selection and the chosen comment let go). */
  onEnter: () => void
  /** A press on a field that takes no input. */
  onReadOnly: (field: PageField) => void
  /** Brings a document rectangle into view. */
  reveal: (rect: Rect) => void
  handle?: Ref<FormFieldsHandle>
}

export function FormFields({ fields, pages, view, active, highlight, canFill, fill, onEnter, onReadOnly, reveal, handle }: Props) {
  const { t } = useKitEditor()
  const slots = useMemo(() => fieldSlots(fields), [fields])
  const layer = useRef<HTMLDivElement>(null)
  const [focused, setFocused] = useState<string | null>(null)
  // A field Tab moves to: given the keyboard once it is drawn (after the view brought it in).
  const pending = useRef<{ key: string; select: boolean } | null>(null)
  const [, redraw] = useState(0)
  const takes = (slot: FieldSlot) => canFill && !slot.field.read_only
  const docRect = (slot: FieldSlot): Rect | null => {
    const area = pages[slot.page - 1]?.area
    return area ? [area.x0 + slot.rect[0], area.y0 + slot.rect[1], area.x0 + slot.rect[2], area.y0 + slot.rect[3]] : null
  }
  /** Gives slot `slot` the keyboard, bringing it into view first. */
  const goTo = (slot: FieldSlot) => {
    const rect = docRect(slot)
    if (rect) reveal(rect)
    pending.current = { key: slot.key, select: slot.field.type === 'text' || (slot.field.type === 'combo' && !!slot.field.editable) }
    redraw((n) => n + 1)
  }
  const tabbable = () => slots.filter(takes)
  /** Tab or Shift+Tab from `from` (a slot's key): the next field in the tab order, round to the first. */
  const step = (from: string, back: boolean) => {
    const list = tabbable()
    if (!list.length) return
    const at = list.findIndex((slot) => slot.key === from)
    const next = list[at < 0 ? 0 : (at + (back ? -1 : 1) + list.length) % list.length]
    if (next) goTo(next)
  }
  useImperativeHandle(handle, () => ({
    enter: (back, from) => {
      const list = tabbable()
      if (!list.length) return false
      const after = back ? [...list].reverse().find((slot) => slot.page <= from) : list.find((slot) => slot.page >= from)
      goTo(after ?? (back ? list[list.length - 1] : list[0]))
      return true
    },
  }))
  useLayoutEffect(() => {
    const want = pending.current
    if (!want) return
    const element = layer.current?.querySelector<HTMLElement>(`[data-slot="${CSS.escape(want.key)}"]`)
    if (!element) return
    pending.current = null
    element.focus({ preventScroll: true })
    if (want.select && (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement)) element.select()
  })
  // Whether the layer is there: a view to place the controls in and a field to place.
  const drawn = !!view && slots.length > 0
  // The layer never scrolls (a focused control near its edge would scroll it away from the view).
  // A wheel over a field turns the pages: the canvas hears the wheel over its whole region.
  useEffect(() => {
    const element = layer.current
    if (!element) return undefined
    const back = () => { if (element.scrollTop || element.scrollLeft) element.scrollTo(0, 0) }
    element.addEventListener('scroll', back)
    return () => element.removeEventListener('scroll', back)
  }, [drawn])

  if (!drawn) return null
  // The controls of the pages in view (and a little round them), and the one with the keyboard.
  const height = (layer.current?.clientHeight ?? 2000) / view.zoom
  const width = (layer.current?.clientWidth ?? 2000) / view.zoom
  const near: Rect = [view.x - width / 2, view.y - height / 2, view.x + width * 1.5, view.y + height * 1.5]
  const want = pending.current?.key ?? null
  const shown = slots.filter((slot) => {
    if (slot.key === focused || slot.key === want) return true
    const rect = docRect(slot)
    return !!rect && rect[2] >= near[0] && rect[0] <= near[2] && rect[3] >= near[1] && rect[1] <= near[3]
  })

  return (
    <div ref={layer} className="pdf-form-layer" data-active={active || undefined} data-highlight={highlight || undefined} data-testid="pdf-form-layer">
      {shown.map((slot) => {
        const box = screenBox(pages[slot.page - 1]?.area, slot.rect, view)
        if (!box) return null
        return (
          <FieldControl
            // A text control is made again when its field's value changes (it holds what is typed);
            // the others follow the value as it is, so the one with the keyboard keeps it.
            key={slot.field.type === 'text' || slot.field.type === 'combo' && slot.field.editable ? `${slot.key}:${JSON.stringify(slot.field.value ?? null)}` : slot.key}
            slot={slot}
            box={box}
            zoom={view.zoom}
            takes={takes(slot)}
            focused={focused === slot.key}
            label={slot.field.tooltip || slot.field.name}
            noChoice={t('fieldNoChoice')}
            fill={fill}
            onFocus={() => { setFocused(slot.key); onEnter() }}
            onBlur={() => setFocused((now) => (now === slot.key ? null : now))}
            onTab={(back) => step(slot.key, back)}
            onArrow={(back) => {
              const group = slots.filter((other) => other.field.name === slot.field.name && takes(other))
              const at = group.findIndex((other) => other.key === slot.key)
              const next = group[(at + (back ? -1 : 1) + group.length) % group.length]
              if (!next || next === slot) return
              if (next.widget.export != null) void fill(next.field.name, next.widget.export)
              goTo(next)
            }}
            onReadOnly={() => onReadOnly(slot.field)}
          />
        )
      })}
    </div>
  )
}

type ControlProps = {
  slot: FieldSlot
  box: { left: number; top: number; width: number; height: number }
  zoom: number
  takes: boolean
  focused: boolean
  label: string
  noChoice: string
  fill: (name: string, value: unknown) => Promise<unknown>
  onFocus: () => void
  onBlur: () => void
  onTab: (back: boolean) => void
  /** A radio button's arrow keys: the group's previous (`back`) or next button, chosen. */
  onArrow: (back: boolean) => void
  onReadOnly: () => void
}

/** One widget's control. */
function FieldControl({ slot, box, zoom, takes, focused, label, noChoice, fill, onFocus, onBlur, onTab, onArrow, onReadOnly }: ControlProps) {
  const { field, widget } = slot
  const sent = useRef<string | null>(null)
  const name = field.name
  const text = textOf(field.value)
  const style = { left: box.left, top: box.top, width: box.width, height: box.height, fontSize: Math.max(3, pointSize(field, slot.rect) * zoom) }
  const common = {
    className: 'pdf-form-control',
    style,
    title: field.tooltip || undefined,
    'aria-label': label,
    'aria-required': field.required || undefined,
    'data-slot': slot.key,
    'data-field': name,
    'data-kind': field.type,
    'data-required': field.required || undefined,
    'data-drawn': widget.appearance || undefined,
    'data-focused': focused || undefined,
    'data-testid': `pdf-page-field-${name}`,
    tabIndex: -1,
    onFocus,
  }
  // The keys a field takes: Tab goes on through the form, Escape gives the keyboard back to the
  // pages; a key without Cmd stays the field's (no tool or command letter, no page scrolling).
  const keys = (event: KeyboardEvent<HTMLElement>, more?: (event: KeyboardEvent<HTMLElement>) => boolean) => {
    if (event.nativeEvent.isComposing || event.keyCode === 229) return
    // Cmd+Z and Shift+Cmd+Z (Cmd+Y) on a drop-down, list, check box or radio button, which take no
    // typed text, are the editor's undo and redo: the pages take the keyboard and hear the key
    // (a choice made in a drop-down's list leaves the keyboard on it).
    const control = event.currentTarget
    const choice = control instanceof HTMLSelectElement || (control instanceof HTMLInputElement && (control.type === 'checkbox' || control.type === 'radio'))
    const undoKey = (event.metaKey || event.ctrlKey) && !event.altKey && ['z', 'y'].includes(event.key.toLowerCase())
    if (choice && undoKey && focusPages()) {
      event.preventDefault()
      event.stopPropagation()
      const { key, code, metaKey, ctrlKey, shiftKey } = event
      pagesRegion()?.dispatchEvent(new globalThis.KeyboardEvent('keydown', { key, code, metaKey, ctrlKey, shiftKey, bubbles: true, cancelable: true }))
      return
    }
    if (event.key === 'Tab' && !event.metaKey && !event.ctrlKey && !event.altKey) {
      event.preventDefault()
      event.stopPropagation()
      onTab(event.shiftKey)
      return
    }
    if (more?.(event)) {
      event.preventDefault()
      event.stopPropagation()
      return
    }
    if (event.key === 'Escape' && !event.metaKey && !event.ctrlKey && !event.altKey && !event.shiftKey) {
      event.preventDefault()
      event.stopPropagation()
      focusPages()
      return
    }
    if (!event.metaKey && !event.ctrlKey) event.stopPropagation()
  }

  if (!takes) {
    // A read-only field (or a session that only reads): what it holds, and a press says why it takes nothing.
    return (
      <div {...common} role="note" data-readonly="true" onPointerDown={(event) => { event.preventDefault(); onReadOnly() }}>
        {!widget.appearance && field.type !== 'checkbox' && field.type !== 'radio' ? <span className="pdf-form-ink">{text}</span> : null}
        {!widget.appearance && widget.on ? <span className="pdf-form-mark" style={{ inset: 0, fontSize: box.height * 0.75 }} aria-hidden="true">{field.type === 'radio' ? '●' : '✓'}</span> : null}
      </div>
    )
  }

  if (field.type === 'checkbox' || field.type === 'radio') {
    const on = !!widget.on
    const radio = field.type === 'radio'
    const choose = () => {
      if (radio) {
        if (!on && widget.export != null) void fill(name, widget.export)
      } else void fill(name, !on)
    }
    return (
      <>
        <input
          {...common}
          type={radio ? 'radio' : 'checkbox'}
          name={radio ? `pdf-page-field-${name}` : undefined}
          checked={on}
          onChange={choose}
          onBlur={onBlur}
          onKeyDown={(event) => keys(event, (e) => {
            // Enter turns a check box (and chooses a radio button) as Space does.
            if (e.key === 'Enter' && !e.metaKey && !e.ctrlKey && !e.altKey) {
              choose()
              return true
            }
            // A radio button's arrows go round its group (the browser's would scroll the layer).
            if (radio && /^Arrow/.test(e.key) && !e.metaKey && !e.ctrlKey && !e.altKey) {
              onArrow(e.key === 'ArrowUp' || e.key === 'ArrowLeft')
              return true
            }
            return false
          })}
        />
        {!widget.appearance && on ? <span className="pdf-form-mark" style={{ left: box.left, top: box.top, width: box.width, height: box.height, fontSize: box.height * 0.75 }} aria-hidden="true">{radio ? '●' : '✓'}</span> : null}
      </>
    )
  }

  if (field.type === 'combo' && !field.editable || field.type === 'list') {
    const many = field.type === 'list' && !!field.multi_select
    const chosen = Array.isArray(field.value) ? field.value.map(String) : text ? [text] : []
    const options = field.options ?? []
    return (
      <select
        {...common}
        multiple={many}
        size={field.type === 'list' ? Math.max(2, Math.min(options.length, Math.floor(box.height / Math.max(1, style.fontSize * 1.15)))) : undefined}
        value={many ? chosen : chosen[0] ?? ''}
        onChange={(event) => void fill(name, many ? [...event.target.selectedOptions].map((option) => option.value) : event.target.value)}
        onBlur={onBlur}
        onKeyDown={(event) => keys(event)}
      >
        {many || field.type === 'list' ? null : <option value="">{noChoice}</option>}
        {options.map((option) => <option key={optionValue(option)} value={optionValue(option)}>{optionLabel(option)}</option>)}
      </select>
    )
  }

  // A text field, or a drop-down that takes typed text: filled as it lets go, or on Enter.
  const listId = field.type === 'combo' ? `pdf-page-field-${slot.key.replace(/\W/g, '_')}-options` : undefined
  // Enter fills and then gives the keyboard to the pages, whose blur would fill it again: a value
  // already sent is not sent twice (and one the field holds is not sent at all).
  const commit = (element: HTMLInputElement | HTMLTextAreaElement) => {
    if (element.value === text || element.value === sent.current) return
    sent.current = element.value
    void fill(name, element.value)
  }
  const textKeys = (event: KeyboardEvent<HTMLInputElement | HTMLTextAreaElement>) => keys(event, (e) => {
    const element = e.currentTarget as HTMLInputElement | HTMLTextAreaElement
    if (e.key === 'Escape' && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) {
      // What was typed is dropped: the field keeps its value.
      element.value = text
      focusPages()
      return true
    }
    if (e.key === 'Enter' && !(element instanceof HTMLTextAreaElement && !e.metaKey && !e.ctrlKey)) {
      commit(element)
      focusPages()
      return true
    }
    return false
  })
  if (field.multiline) {
    return (
      <textarea {...common} defaultValue={text} maxLength={field.max_length || undefined} onBlur={(event) => { commit(event.currentTarget); onBlur() }} onKeyDown={textKeys} />
    )
  }
  return (
    <>
      <input
        {...common}
        type="text"
        defaultValue={text}
        list={listId}
        maxLength={field.max_length || undefined}
        autoComplete="off"
        spellCheck={false}
        onBlur={(event) => { commit(event.currentTarget); onBlur() }}
        onKeyDown={textKeys}
      />
      {listId ? (
        <datalist id={listId}>
          {(field.options ?? []).map((option) => <option key={optionValue(option)} value={optionValue(option)}>{optionLabel(option)}</option>)}
        </datalist>
      ) : null}
    </>
  )
}

/**
 * The bar a document with a form opens with: how many fields it asks to be filled, and Highlight
 * fields / Hide field highlights (what new documents start with too); its close button puts it
 * away for this document.
 */
export function FormBar({ count, highlight, onHighlight, onClose }: { count: number; highlight: boolean; onHighlight: (on: boolean) => void; onClose: () => void }) {
  const { t } = useKitEditor()
  return (
    <div className="kui-bar pdf-form-bar" data-testid="pdf-form-bar">
      <span data-testid="pdf-form-bar-text">{count === 1 ? t('fieldsBarOne') : t('fieldsBar', { count: String(count) })}</span>
      <Button aria-pressed={highlight} onClick={() => { onHighlight(!highlight); focusPages() }} data-testid="pdf-form-highlight">
        {highlight ? t('fieldsHideHighlight') : t('fieldsHighlight')}
      </Button>
      <IconButton icon={X} label={t('close')} onClick={() => { onClose(); focusPages() }} data-testid="pdf-form-bar-close" />
    </div>
  )
}
