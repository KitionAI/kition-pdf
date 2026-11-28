/**
 * What shows while a parameters dialog is put away to point at the page (stage 10): a bar over
 * the pages saying what to do (with Done for several points, and Back to the form), and the
 * points picked so far, marked on their page. HTML over the canvas, never drawn into the page.
 */
import { Button, useKitEditor, type KitCanvasView } from '@kition/sdk/editor'
import { screenBox, type Area } from './geometry'
import type { PickSpec } from './pick'

export function PickBar({ spec, field, count, onDone, onCancel }: {
  spec: PickSpec
  /** The field's name as the dialog shows it. */
  field: string
  /** Points picked so far (several points). */
  count: number
  onDone: () => void
  onCancel: () => void
}) {
  const { t } = useKitEditor()
  const text =
    spec.kind === 'point' ? t('pickPointHelp', { field })
      : spec.kind === 'rect' ? t('pickRectHelp', { field })
        : t('pickPointsHelp', { field, count: String(count), min: String(spec.min) })
  return (
    <div className="kui-bar pdf-form-bar pdf-pick-bar" data-testid="pdf-pick-bar" data-kind={spec.kind} role="status">
      <span data-testid="pdf-pick-bar-text">{text}</span>
      {spec.kind === 'points' ? (
        <Button variant="primary" disabled={count < spec.min} onClick={onDone} data-testid="pdf-pick-done">{t('pickDone')}</Button>
      ) : null}
      <Button onClick={onCancel} data-testid="pdf-pick-cancel">{t('pickBack')}</Button>
    </div>
  )
}

/** The points picked so far, a dot each and a line between them, over their page. */
export function PickMarks({ area, points, view }: { area: Area | undefined; points: readonly (readonly [number, number])[]; view: KitCanvasView | null }) {
  const spots = points.map((point) => screenBox(area, [point[0], point[1], point[0], point[1]], view)).filter((box): box is NonNullable<typeof box> => box != null)
  if (!spots.length) return null
  return (
    <svg className="pdf-pick-marks" aria-hidden="true" data-testid="pdf-pick-marks">
      <polyline points={spots.map((spot) => `${spot.left},${spot.top}`).join(' ')} />
      {spots.map((spot, i) => <circle key={i} cx={spot.left} cy={spot.top} r={3.5} data-testid="pdf-pick-point" />)}
    </svg>
  )
}
