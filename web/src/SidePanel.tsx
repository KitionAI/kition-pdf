import { useState } from 'react'
import { TesseraIcons } from '@kition/sdk/editor'
import { IconButton, useEngineQuery, useKitEditor } from '@kition/sdk/editor'
import type { PdfInfo } from './pdf'
import { Thumbnails } from './Thumbnails'


const Bookmark = TesseraIcons.BookmarkIcon
const Files = TesseraIcons.FilesIcon
const MessageSquareText = TesseraIcons.MessageSquareTextIcon
const TextCursorInput = TesseraIcons.TextCursorInputIcon

type Panel = 'pages' | 'bookmarks' | 'comments' | 'fields'
type BookmarkItem = { title: string; page: number | null; children: BookmarkItem[] }
type CommentItem = { id: string; page: number; type: string; contents: string | null; author?: string; color?: string }
type FieldItem = { name: string; page: number; type: string; value: unknown; read_only?: boolean }

/** The panels beside the pages (Kition PDF's right rail): thumbnails, bookmarks, comments, fields. */
export function SidePanel({ info, current, onPage }: { info: PdfInfo; current: number; onPage: (index: number) => void }) {
  const { phrase: p } = useKitEditor()
  const [panel, setPanel] = useState<Panel>('pages')
  const panels: Array<{ id: Panel; icon: typeof Files; label: string }> = [
    { id: 'pages', icon: Files, label: p('Page thumbnails') },
    { id: 'bookmarks', icon: Bookmark, label: p('Bookmarks') },
    { id: 'comments', icon: MessageSquareText, label: p('Comments') },
    { id: 'fields', icon: TextCursorInput, label: p('Form fields') },
  ]
  return (
    <aside className="kui-panel pdf-side" data-side="right" data-testid="pdf-side">
      <div className="pdf-side-tabs" role="tablist">
        {panels.map((item) => (
          <IconButton key={item.id} icon={item.icon} label={item.label} pressed={panel === item.id} onClick={() => setPanel(item.id)} data-testid={`pdf-panel-${item.id}`} />
        ))}
      </div>
      <div className="kui-panel-header">{panels.find((item) => item.id === panel)?.label}</div>
      <div className={`kui-panel-body${panel === 'pages' ? ' pdf-thumbs-body' : ''}`}>
        {panel === 'pages' ? <Thumbnails info={info} current={current} onPage={onPage} /> : null}
        {panel === 'bookmarks' ? <Bookmarks onPage={onPage} /> : null}
        {panel === 'comments' ? <Comments onPage={onPage} /> : null}
        {panel === 'fields' ? <Fields onPage={onPage} /> : null}
      </div>
    </aside>
  )
}

function Bookmarks({ onPage }: { onPage: (index: number) => void }) {
  const { t } = useKitEditor()
  const items = useEngineQuery<{ bookmarks: BookmarkItem[] }>('bookmark.list', { limit: 1000 })?.bookmarks ?? []
  if (!items.length) return <p className="pdf-empty kui-muted">{t('noBookmarks')}</p>
  const tree = (list: BookmarkItem[], depth: number): React.ReactNode =>
    list.map((item, index) => (
      <div key={`${depth}-${index}`}>
        <button type="button" className="pdf-list-item" style={{ paddingLeft: 12 + depth * 14 }} disabled={item.page == null} onClick={() => item.page != null && onPage(item.page - 1)} data-testid="pdf-bookmark">
          {item.title}
        </button>
        {tree(item.children ?? [], depth + 1)}
      </div>
    ))
  return <div>{tree(items, 0)}</div>
}

function Comments({ onPage }: { onPage: (index: number) => void }) {
  const { t, phrase: p } = useKitEditor()
  const items = useEngineQuery<{ comments: CommentItem[] }>('comment.list')?.comments ?? []
  if (!items.length) return <p className="pdf-empty kui-muted">{t('noComments')}</p>
  return (
    <div>
      {items.map((item) => (
        <button key={item.id} type="button" className="pdf-list-item pdf-comment" onClick={() => onPage(item.page - 1)} data-testid="pdf-comment">
          <span className="pdf-comment-mark" style={{ background: item.color ?? 'var(--kit-primary)' }} />
          <span className="pdf-comment-body">
            <span className="kui-muted">
              {p(item.type)} · {t('onPage', { page: item.page })}
            </span>
            <span>{item.contents || '—'}</span>
          </span>
        </button>
      ))}
    </div>
  )
}

function Fields({ onPage }: { onPage: (index: number) => void }) {
  const { run, enabled, t } = useKitEditor()
  // Filling a field edits the document: a read-only one shows the values only.
  const canFill = enabled('form.fill')
  const items = useEngineQuery<{ fields: FieldItem[] }>('form.listFields')?.fields ?? []
  if (!items.length) return <p className="pdf-empty kui-muted">{t('noFields')}</p>
  return (
    <div className="pdf-fields">
      {items.map((field) => (
        <label key={field.name} className="pdf-field" onFocus={() => onPage(field.page - 1)}>
          <span className="kui-muted">{field.name}</span>
          {field.type === 'checkbox' ? (
            <input type="checkbox" checked={field.value === true || field.value === 'Yes'} disabled={field.read_only || !canFill} onChange={(event) => void run('form.fill', { values: { [field.name]: event.target.checked } })} />
          ) : (
            <input
              className="kui-field"
              defaultValue={field.value == null ? '' : String(field.value)}
              disabled={field.read_only || !canFill}
              onKeyDown={(event) => {
                if (event.key === 'Enter') event.currentTarget.blur()
              }}
              onBlur={(event) => {
                const value = event.currentTarget.value
                if (value !== (field.value == null ? '' : String(field.value))) void run('form.fill', { values: { [field.name]: value } })
              }}
              data-testid={`pdf-field-${field.name}`}
            />
          )}
        </label>
      ))}
    </div>
  )
}
