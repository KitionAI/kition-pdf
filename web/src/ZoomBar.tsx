/**
 * The status bar's zoom: out and in by the zoom ladder, a slider, the percentage (click it and
 * type one), a list of the ladder's steps and the three fits, and Fit, which fits the current page
 * (the canvas's own Fit fitted every page at once: a thin strip at 7 % for a long document).
 */
import { useState } from 'react'
import { MenuButton, MenuItem, MenuSeparator, TesseraIcons, isMac, useKitEditor } from '@kition/sdk/editor'
import { MAX_ZOOM, MIN_ZOOM, ZOOM_LADDER, clampZoom, formatZoom, ladderStep, parseZoom } from './view'
import { focusPages, selectOnFocus } from './focus'

const ChevronDown = TesseraIcons.ChevronDownIcon

export type Fit = 'page' | 'width' | 'actual'

export function ZoomBar({ zoom, onZoom, onFit }: { zoom: number; onZoom: (zoom: number) => void; onFit: (to: Fit) => void }) {
  const { t } = useKitEditor()
  const [draft, setDraft] = useState<string | null>(null)
  const mod = isMac() ? '⌘' : 'Ctrl+'
  const toSlider = (value: number) => Math.log(clampZoom(value) / MIN_ZOOM) / Math.log(MAX_ZOOM / MIN_ZOOM)
  const fromSlider = (value: number) => MIN_ZOOM * Math.pow(MAX_ZOOM / MIN_ZOOM, value)
  const typed = () => {
    const value = draft == null ? null : parseZoom(draft)
    setDraft(null)
    if (value != null) onZoom(value)
  }
  return (
    <div className="kui-zoom pdf-zoom" data-testid="pdf-zoom">
      <button type="button" className="kui-icon-button" aria-label={t('zoomOut')} title={`${t('zoomOut')} (${mod}-)`} onClick={() => onZoom(ladderStep(zoom, -1))} data-testid="pdf-zoom-out">−</button>
      <input type="range" min={0} max={1} step={0.001} value={toSlider(zoom)} onChange={(event) => onZoom(fromSlider(Number(event.target.value)))} aria-label={t('zoom')} data-testid="pdf-zoom-slider" />
      <button type="button" className="kui-icon-button" aria-label={t('zoomIn')} title={`${t('zoomIn')} (${mod}=)`} onClick={() => onZoom(ladderStep(zoom, 1))} data-testid="pdf-zoom-in">+</button>
      <input
        className="kui-field pdf-zoom-field"
        value={draft ?? formatZoom(zoom)}
        aria-label={t('zoomPercent')}
        title={t('zoomPercent')}
        {...selectOnFocus}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Enter') {
            event.preventDefault()
            typed()
            focusPages()
          } else if (event.key === 'Escape') {
            event.preventDefault()
            setDraft(null)
            focusPages()
          }
        }}
        onBlur={(event) => {
          setDraft(null)
          delete event.currentTarget.dataset.selecting
        }}
        data-testid="pdf-zoom-field"
      />
      <MenuButton
        align="end"
        placement="auto"
        trigger={({ open, toggle }) => (
          <button type="button" className="kui-icon-button" aria-haspopup="menu" aria-expanded={open} aria-label={t('zoomLevels')} title={t('zoomLevels')} onClick={toggle} data-testid="pdf-zoom-menu">
            <ChevronDown size={14} aria-hidden="true" />
          </button>
        )}
      >
        <MenuItem label={t('fitPage')} shortcut={`${mod}0`} onSelect={() => onFit('page')} testId="pdf-zoom-fit-page" />
        <MenuItem label={t('actualSize')} shortcut={`${mod}1`} onSelect={() => onFit('actual')} testId="pdf-zoom-actual" />
        <MenuItem label={t('fitWidth')} shortcut={`${mod}2`} onSelect={() => onFit('width')} testId="pdf-zoom-fit-width" />
        <MenuSeparator />
        {ZOOM_LADDER.map((step) => (
          <MenuItem key={step} label={formatZoom(step)} checked={Math.abs(step - zoom) < step * 0.001} onSelect={() => onZoom(step)} testId={`pdf-zoom-${Math.round(step * 100)}`} />
        ))}
      </MenuButton>
      <button type="button" className="kui-button" data-variant="ghost" title={`${t('fitPage')} (${mod}0)`} onClick={() => onFit('page')} data-testid="pdf-zoom-fit">{t('fit')}</button>
    </div>
  )
}
