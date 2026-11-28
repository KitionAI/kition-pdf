/**
 * A command's parameters dialog, in place of the SDK's general one (whose form closes on any
 * failure, drops what was typed and names every field by its parameter): the fields have people's
 * names and say what to type; lists, points and rectangles are typed as `3, 5`, `2-4` or
 * `300, 400` (`params.ts`); a value that is not one is refused at its field before anything runs,
 * and an error the command answers stays in the dialog, under the field it names, with everything
 * typed kept. Field ids are the SDK form's (`param-<name>`), and the Run button's test id too.
 * A point, a rectangle or a list of points of a command that takes a page has a button beside it
 * (stage 10): the dialog goes away while the person points at the page (`page/pick.ts`), and comes
 * back with the value and the page filled in and everything else as it was typed.
 */
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { Button, Dialog, useKitEditor, type KitCommand } from '@kition/sdk/editor'
import { CHOICE_LABELS, PARAM_LABELS, errorField, humanize, initialText, parseValue, withBounds, type ParamError, type ParamNode } from './params'
import { pickSpec, type PickSpec } from './page/pick'

/** A long form shows what must be (or was) given first, the rest under "More options". */
const FOLD_AFTER = 6

export type Asking = {
  command: KitCommand
  values: Record<string, unknown>
  /** Values the dialog passes on without a field (the selected comment's id). */
  fixed?: Record<string, unknown>
  /** What was typed before the dialog went away to point at the page (it wins over `values`). */
  texts?: Record<string, string>
  /** The field to give the keyboard to as the dialog opens (the one just picked). */
  focus?: string
}

/** A field's label as the dialog shows it, in the editor's language. */
export function paramLabel(key: string, phrase: (text: string) => string) {
  return phrase(PARAM_LABELS[key] ?? humanize(key))
}

export function ParamsDialog({ asking, onRun, onClose, onPick }: {
  asking: Asking
  onRun: (params: Record<string, unknown>) => Promise<unknown>
  onClose: () => void
  /** Puts the dialog away to point at the page for `spec`'s field, with what is typed now. */
  onPick?: (spec: PickSpec, texts: Record<string, string>) => void
}) {
  const { t, phrase: p } = useKitEditor()
  const { command, values: given, fixed = {} } = asking
  const schema = useMemo(() => Object.fromEntries(Object.entries((command.schema ?? {}) as Record<string, ParamNode>).filter(([key]) => !(key in fixed)).map(([key, node]) => [key, withBounds(node)])), [command, fixed])
  const entries = Object.entries(schema)
  const [texts, setTexts] = useState<Record<string, string>>(() => ({ ...Object.fromEntries(entries.map(([key, node]) => [key, initialText(node, given[key])])), ...asking.texts }))
  const [errors, setErrors] = useState<Record<string, string>>({})
  const [failure, setFailure] = useState<string | null>(null)
  const [running, setRunning] = useState(false)
  const form = useRef<HTMLFormElement>(null)

  const fold = entries.length > FOLD_AFTER
  const askedFor = ([key, node]: [string, ParamNode]) => !!node.required || given[key] !== undefined
  const pickable = ([key, node]: [string, ParamNode]) => !!onPick && pickSpec(command.id, key, node, schema) != null
  // A command whose place is none of the fields shown first (a redaction's box) shows its
  // fields that can be picked on the page first too.
  const placeFirst = !entries.some((entry) => askedFor(entry) && pickable(entry))
  const shownFirst = (entry: [string, ParamNode]) => !fold || askedFor(entry) || (placeFirst && pickable(entry))
  const main = entries.filter(shownFirst)
  const more = entries.filter((entry) => !shownFirst(entry))
  const moreOpen = more.some(([key]) => errors[key] || key === asking.focus)

  const label = (key: string) => paramLabel(key, p)
  const hint = (node: ParamNode): string | null => {
    if (node.type !== 'array') return null
    if (node.items?.type === 'integer') return t('hintPages')
    if (node.items?.type === 'number') return node.maxItems === 2 && node.minItems === 2 ? t('hintPoint') : node.maxItems === 4 && node.minItems === 4 ? t('hintRect') : t('hintNumbers')
    if (node.items?.type === 'array') return t('hintPoints')
    if (node.items?.type === 'string') return t('hintWords')
    return null
  }
  const message = (error: ParamError): string => {
    switch (error.kind) {
      case 'pages': return t('errorPages')
      case 'numbers': return error.count ? t('errorNumbersCount', { count: error.count }) : t('errorNumbers')
      case 'points': return t('errorPoints')
      case 'number': return t('errorNumber')
      case 'whole': return t('errorWhole')
      case 'json': return t('errorJson')
      case 'count': return error.max === error.min ? t('errorCountExact', { count: error.min }) : error.max === null ? t('errorCountMin', { min: error.min }) : t('errorCountRange', { min: error.min, max: error.max })
    }
  }
  const focusField = (key: string) => requestAnimationFrame(() => form.current?.querySelector<HTMLElement>(`#param-${CSS.escape(key)}`)?.focus())
  // Back from the page: the keyboard on the field that was picked (after the dialog's own focus).
  useEffect(() => {
    if (!asking.focus) return undefined
    const late = window.setTimeout(() => focusField(asking.focus!), 0)
    return () => window.clearTimeout(late)
  }, [asking.focus])

  const missing = entries.some(([key, node]) => node.required && (texts[key] ?? '').trim() === '')
  const submit = async () => {
    if (missing || running) return
    const params: Record<string, unknown> = { ...fixed }
    const refused: Record<string, string> = {}
    for (const [key, node] of entries) {
      const parsed = parseValue(node, texts[key] ?? '')
      if ('error' in parsed) refused[key] = message(parsed.error)
      else if (parsed.value !== undefined) params[key] = parsed.value
    }
    setErrors(refused)
    setFailure(null)
    const first = Object.keys(refused)[0]
    if (first) {
      focusField(first)
      return
    }
    setRunning(true)
    try {
      await onRun(params)
    } catch (error) {
      const text = error instanceof Error ? error.message : String(error)
      const key = errorField(text, entries.map(([name]) => name))
      if (key) {
        setErrors({ [key]: text })
        focusField(key)
      } else setFailure(text)
    } finally {
      setRunning(false)
    }
  }

  const field = ([key, node]: [string, ParamNode]): ReactNode[] => {
    const set = (value: string) => {
      setTexts((current) => ({ ...current, [key]: value }))
      if (errors[key]) setErrors(({ [key]: _gone, ...rest }) => rest)
    }
    const id = `param-${key}`
    const help = hint(node)
    const error = errors[key]
    const described = error ? `${id}-error` : help ? `${id}-hint` : undefined
    const control = node.enum?.length ? (
      <select id={id} className="kui-field" value={texts[key]} aria-invalid={!!error} aria-describedby={described} onChange={(event) => set(event.target.value)}>
        {node.required ? null : <option value="" />}
        {node.enum.map((choice) => (
          <option key={String(choice)} value={String(choice)}>
            {p(CHOICE_LABELS[String(choice)] ?? humanize(String(choice)))}
          </option>
        ))}
      </select>
    ) : node.type === 'boolean' ? (
      <input id={id} type="checkbox" checked={texts[key] === 'true'} onChange={(event) => set(String(event.target.checked))} />
    ) : (
      <input
        id={id}
        className="kui-field"
        type={node.type === 'number' || node.type === 'integer' ? 'number' : 'text'}
        value={texts[key]}
        aria-invalid={!!error}
        aria-describedby={described}
        onChange={(event) => set(event.target.value)}
      />
    )
    const pick = onPick ? pickSpec(command.id, key, node, schema) : null
    const pickLabel = pick ? t(pick.kind === 'point' ? 'pickPoint' : pick.kind === 'rect' ? 'pickRect' : 'pickPoints') : ''
    const picked = pick ? (
      <div className="pdf-param-pick">
        {control}
        <Button onClick={() => onPick?.(pick, texts)} disabled={running} data-testid={`pdf-param-pick-${key}`}>{pickLabel}</Button>
      </div>
    ) : control
    return [
      <label key={`${key}-label`} htmlFor={id} title={node.description}>
        {label(key)}
        {node.required ? <span title={t('required')}> *</span> : null}
      </label>,
      <div key={key} className="pdf-param">
        {picked}
        {error ? (
          <span id={`${id}-error`} className="pdf-param-error" role="alert" data-testid={`pdf-param-error-${key}`}>{error}</span>
        ) : help ? (
          <span id={`${id}-hint`} className="pdf-param-hint kui-muted">{help}</span>
        ) : null}
      </div>,
    ]
  }

  return (
    <Dialog
      title={p(command.title)}
      onClose={onClose}
      testId="pdf-params-dialog"
      actions={
        <>
          <Button onClick={onClose}>{t('cancel')}</Button>
          <Button variant="primary" disabled={missing || running} onClick={() => void submit()} data-testid="command-dialog-run">
            {t('run')}
          </Button>
        </>
      }
    >
      <form
        ref={form}
        onSubmit={(event) => {
          event.preventDefault()
          void submit()
        }}
      >
        {failure ? <p className="pdf-param-error pdf-param-failure" role="alert" data-testid="pdf-param-failure">{failure}</p> : null}
        <div className="kui-param-grid">{main.flatMap(field)}</div>
        {more.length ? (
          <details open={moreOpen || undefined}>
            <summary>{t('moreOptions')}</summary>
            <div className="kui-param-grid">{more.flatMap(field)}</div>
          </details>
        ) : null}
        <button type="submit" hidden />
      </form>
    </Dialog>
  )
}
