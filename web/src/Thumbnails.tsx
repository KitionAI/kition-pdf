import { useEffect, useMemo, useRef, useState } from 'react'
import { Button, useKitEditor } from '@kition/kit-editor'
import type { PdfInfo } from './pdf'
import { ThumbnailQueue, THUMB_ROW_HEIGHT, thumbnailWindow, type Thumbnail } from './thumbnailQueue'

/** Only the visible thumbnail rows and a small overscan own canvases or queued work. */
export function Thumbnails({ info, current, onPage }: { info: PdfInfo; current: number; onPage: (index: number) => void }) {
  const { session: { doc, change }, t, phrase: p } = useKitEditor()
  const region = useRef<HTMLDivElement>(null)
  const [view, setView] = useState({ top: 0, height: 600 })
  const [, changed] = useState(0)
  const queue = useMemo(() => new ThumbnailQueue(() => changed((value) => value + 1)), [])
  const range = thumbnailWindow(info.pages.length, view.top, view.height)
  const rows = info.pages.slice(range.start, range.end)

  useEffect(() => () => queue.dispose(), [queue])
  useEffect(() => {
    const element = region.current
    if (!element) return
    const measure = () => setView({ top: element.scrollTop, height: element.clientHeight })
    const observer = new ResizeObserver(measure)
    observer.observe(element)
    measure()
    return () => observer.disconnect()
  }, [])

  useEffect(() => {
    const element = region.current
    if (!element) return
    const top = current * THUMB_ROW_HEIGHT
    if (top < element.scrollTop || top + THUMB_ROW_HEIGHT > element.scrollTop + element.clientHeight) {
      element.scrollTop = Math.max(0, top - (element.clientHeight - THUMB_ROW_HEIGHT) / 2)
    }
    setView({ top: element.scrollTop, height: element.clientHeight })
  }, [current, doc])

  useEffect(() => {
    const order = [current, ...Array.from({ length: range.end - range.start }, (_, offset) => range.start + offset)]
    queue.update(doc, change.revision, order.flatMap((index) => info.pages[index] ? [{ index, area: info.pages[index].area }] : []))
  }, [queue, doc, change.revision, current, info.pages, range.start, range.end])

  const focusPage = (index: number) => {
    const wanted = Math.max(0, Math.min(info.pages.length - 1, index))
    const element = region.current
    if (!element) return
    element.scrollTop = wanted * THUMB_ROW_HEIGHT
    setView({ top: element.scrollTop, height: element.clientHeight })
    onPage(wanted)
    requestAnimationFrame(() => region.current?.querySelector<HTMLButtonElement>(`[data-testid="pdf-thumb-${wanted}"]`)?.focus())
  }

  return (
    <div ref={region} className="pdf-thumbs" data-testid="pdf-thumbs" role="list" aria-label={p('Page thumbnails')}
      data-cache-bytes={queue.bytes} data-cache-count={queue.size}
      onScroll={(event) => setView({ top: event.currentTarget.scrollTop, height: event.currentTarget.clientHeight })}>
      <div className="pdf-thumbs-spacer" style={{ height: info.pages.length * THUMB_ROW_HEIGHT }}>
        {rows.map((page, offset) => {
          const index = range.start + offset
          const image = queue.get(index, doc, change.revision)
          const error = image && 'error' in image ? image.error : null
          return (
            <div className="pdf-thumb-row" role="listitem" aria-posinset={index + 1} aria-setsize={info.pages.length} key={index} style={{ top: index * THUMB_ROW_HEIGHT, height: THUMB_ROW_HEIGHT }}>
              <button type="button" className="pdf-thumb" data-current={index === current} aria-current={index === current ? 'page' : undefined}
                aria-label={`${p('Page')} ${page.label}`} onClick={() => onPage(index)} data-testid={`pdf-thumb-${index}`}
                onKeyDown={(event) => {
                  const target = event.key === 'ArrowDown' ? index + 1 : event.key === 'ArrowUp' ? index - 1 : event.key === 'Home' ? 0 : event.key === 'End' ? info.pages.length - 1 : null
                  if (target !== null) { event.preventDefault(); focusPage(target) }
                }}>
                <ThumbCanvas image={image} />
                <span>{page.label}</span>
              </button>
              {error ? <div className="pdf-thumb-error" role="status" title={error} data-testid={`pdf-thumb-error-${index}`}>
                <span>{t('renderFailed')}</span>
                <Button onClick={() => queue.retry(index)} aria-label={`${t('retryRender')}: ${p('Page')} ${page.label}`}>{t('retryRender')}</Button>
              </div> : null}
            </div>
          )
        })}
      </div>
    </div>
  )
}

function ThumbCanvas({ image }: { image: Thumbnail | undefined }) {
  const ref = useRef<HTMLCanvasElement>(null)
  const pixels = image && 'pixels' in image ? image : null
  useEffect(() => {
    const canvas = ref.current
    if (!canvas) return
    canvas.width = pixels?.width ?? 1
    canvas.height = pixels?.height ?? 1
    if (pixels) canvas.getContext('2d')?.putImageData(new ImageData(new Uint8ClampedArray(pixels.pixels.buffer as ArrayBuffer, pixels.pixels.byteOffset, pixels.pixels.byteLength), pixels.width, pixels.height), 0, 0)
  }, [pixels])
  return <span className="pdf-thumb-preview"><canvas ref={ref} width={1} height={1} style={{ width: pixels?.width ?? 132, height: pixels?.height ?? 180 }} aria-hidden="true" /></span>
}
