import { useState } from 'react'
import { TesseraIcons } from '@kition/sdk/editor'
import { Button, IconButton, useEngineQuery, useKitEditor } from '@kition/sdk/editor'
import type { PdfInfo } from './pdf'
import { Thumbnails } from './Thumbnails'
import { focusPages } from './focus'
import { STATUSES, commentRef, sameComment, shownDate, statusOf, type CommentItem } from './comments'


const Bookmark = TesseraIcons.BookmarkIcon
const Files = TesseraIcons.FilesIcon
const MessageSquareText = TesseraIcons.MessageSquareTextIcon
const TextCursorInput = TesseraIcons.TextCursorInputIcon
const Pencil = TesseraIcons.PencilIcon
const Trash = TesseraIcons.TrashIcon

type Panel = 'pages' | 'bookmarks' | 'comments' | 'fields'
type BookmarkItem = { title: string; page: number | null; children: BookmarkItem[] }
type FieldOption = string | { value: string; label: string }
type FieldItem = {
  name: string
  page: number
  type: string
  value: unknown
  read_only?: boolean
  required?: boolean
  tooltip?: string
  options?: FieldOption[]
  multi_select?: boolean
  editable?: boolean
  multiline?: boolean
}

/** The panels beside the pages (Kition PDF's right rail): thumbnails, bookmarks, comments, fields. */
export function SidePanel({ info, current, onPage, chosen, onChoose }: {
  info: PdfInfo
  current: number
  onPage: (index: number) => void
  /** The comment chosen in the Comments panel, and choosing one (null: none). */
  chosen: CommentItem | null
  onChoose: (item: CommentItem | null) => void
}) {
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
        {panel === 'comments' ? <Comments chosen={chosen} onChoose={onChoose} /> : null}
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

/**
 * The document's comments, each with its type, page, author, time and text. A click chooses one:
 * the view goes to it and it is framed on the page; the chosen one can be edited in place
 * (double-click too), replied to, given a status and deleted (its button, Delete). The Comments
 * tab's commands that act on one comment act on it.
 */
function Comments({ chosen, onChoose }: { chosen: CommentItem | null; onChoose: (item: CommentItem | null) => void }) {
  const { t, phrase: p, run, enabled, host } = useKitEditor()
  const items = useEngineQuery<{ comments: CommentItem[] }>('comment.list')?.comments ?? []
  const [editing, setEditing] = useState<{ item: CommentItem; text: string } | null>(null)
  const [reply, setReply] = useState('')
  const locale = host.init.locale || 'en-US'
  if (!items.length) return <p className="pdf-empty kui-muted">{t('noComments')}</p>
  const isChosen = (item: CommentItem) => chosen != null && sameComment(chosen, item)
  const edit = (item: CommentItem) => setEditing({ item, text: item.contents ?? '' })
  const save = async () => {
    if (!editing) return
    const { item, text } = editing
    setEditing(null)
    if (text !== (item.contents ?? '')) await run('comment.edit', { ...commentRef(item), contents: text })
  }
  const send = async (item: CommentItem) => {
    const text = reply.trim()
    if (!text) return
    setReply('')
    await run('comment.reply', { ...commentRef(item), text })
  }
  const remove = async (item: CommentItem) => {
    onChoose(null)
    await run('comment.delete', commentRef(item))
  }
  return (
    <div className="pdf-comments" data-testid="pdf-comments">
      {items.map((item) => {
        const open = isChosen(item)
        const when = shownDate(item.modified, locale)
        const replies = item.replies ?? []
        return (
          <div key={item.id ?? `${item.page}-${item.index}`} className="pdf-comment-row" data-chosen={open || undefined}>
            <button
              type="button"
              className="pdf-list-item pdf-comment"
              aria-pressed={open}
              onClick={() => onChoose(item)}
              onDoubleClick={() => { onChoose(item); edit(item) }}
              data-testid="pdf-comment"
            >
              <span className="pdf-comment-mark" style={{ background: item.color ?? 'var(--kit-primary)' }} />
              <span className="pdf-comment-body">
                <span className="kui-muted">
                  {p(item.type)} · {t('onPage', { page: item.page })}
                </span>
                {item.author || when ? <span className="kui-muted pdf-comment-by">{[item.author, when].filter(Boolean).join(' · ')}</span> : null}
                {statusOf(item) !== 'none' ? <span className="pdf-comment-status">{t(`status_${statusOf(item)}`)}</span> : null}
                <span>{item.contents || '—'}</span>
              </span>
            </button>
            {replies.length ? (
              <ul className="pdf-comment-replies">
                {replies.map((answer, index) => (
                  <li key={answer.id ?? index} data-testid="pdf-comment-reply">
                    <span className="kui-muted">{answer.author || t('someone')}: </span>
                    {answer.contents}
                  </li>
                ))}
              </ul>
            ) : null}
            {open && editing && sameComment(editing.item, item) ? (
              <div className="pdf-comment-tools">
                <textarea
                  className="kui-field"
                  rows={3}
                  value={editing.text}
                  autoFocus
                  aria-label={t('editComment')}
                  onChange={(event) => setEditing({ item, text: event.target.value })}
                  onKeyDown={(event) => {
                    if (event.key === 'Escape') {
                      event.preventDefault()
                      event.stopPropagation()
                      setEditing(null)
                    } else if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
                      event.preventDefault()
                      void save()
                    }
                  }}
                  data-testid="pdf-comment-edit-text"
                />
                <div className="pdf-comment-actions">
                  <Button onClick={() => setEditing(null)}>{t('cancel')}</Button>
                  <Button variant="primary" onClick={() => void save()} data-testid="pdf-comment-save">{t('save')}</Button>
                </div>
              </div>
            ) : open ? (
              <div className="pdf-comment-tools">
                <div className="pdf-comment-actions">
                  <IconButton icon={Pencil} label={t('editComment')} title={t('editComment')} disabled={!enabled('comment.edit') || item.locked} onClick={() => edit(item)} data-testid="pdf-comment-edit" />
                  <select
                    className="kui-field pdf-comment-status-field"
                    aria-label={t('status')}
                    value={statusOf(item)}
                    disabled={!enabled('comment.setStatus')}
                    onChange={(event) => void run('comment.setStatus', { ...commentRef(item), status: event.target.value })}
                    data-testid="pdf-comment-set-status"
                  >
                    {STATUSES.map((status) => <option key={status} value={status}>{t(`status_${status}`)}</option>)}
                  </select>
                  <IconButton icon={Trash} label={t('deleteComment')} title={t('deleteComment')} disabled={!enabled('comment.delete') || item.locked} onClick={() => void remove(item)} data-testid="pdf-comment-delete" />
                </div>
                <input
                  className="kui-field"
                  value={reply}
                  placeholder={t('replyPlaceholder')}
                  aria-label={t('reply')}
                  disabled={!enabled('comment.reply')}
                  onChange={(event) => setReply(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') {
                      event.preventDefault()
                      void send(item)
                    }
                  }}
                  data-testid="pdf-comment-reply-text"
                />
              </div>
            ) : null}
          </div>
        )
      })}
    </div>
  )
}

const optionValue = (option: FieldOption) => (typeof option === 'string' ? option : option.value)
const optionLabel = (option: FieldOption) => (typeof option === 'string' ? option : option.label || option.value)

/**
 * The form's fields, each with a control for its type: a text box, a check box, a group of radio
 * buttons, a drop-down (with free text when the field takes it) or a list; its label (its tooltip,
 * else its name), its type and whether it is required. A control is made again when its value
 * changes, so an undo or an agent's fill shows.
 */
function Fields({ onPage }: { onPage: (index: number) => void }) {
  const { run, enabled, t } = useKitEditor()
  // Filling a field edits the document: a read-only one shows the values only.
  const canFill = enabled('form.fill')
  const items = useEngineQuery<{ fields: FieldItem[] }>('form.listFields')?.fields ?? []
  if (!items.length) return <p className="pdf-empty kui-muted">{t('noFields')}</p>
  const fill = (field: FieldItem, value: unknown) => void run('form.fill', { values: { [field.name]: value } })
  return (
    <div className="pdf-fields">
      {items.map((field) => {
        const off = field.read_only || !canFill
        const id = `pdf-field-${field.name}`
        const key = `${field.name}:${JSON.stringify(field.value ?? null)}`
        const text = field.value == null ? '' : String(field.value)
        const options = field.options ?? []
        const kind = ['text', 'checkbox', 'radio', 'combo', 'list', 'button', 'signature'].includes(field.type) ? field.type : 'text'
        const textBox = (list?: string) => (
          <input
            key={key}
            className="kui-field"
            defaultValue={text}
            disabled={off}
            list={list}
            aria-label={field.tooltip || field.name}
            onKeyDown={(event) => {
              // The value is filled as the field lets go, and the keyboard goes back to the pages.
              if (event.key === 'Enter' && !focusPages()) event.currentTarget.blur()
            }}
            onBlur={(event) => {
              const value = event.currentTarget.value
              if (value !== text) fill(field, value)
            }}
            data-testid={id}
          />
        )
        let control
        if (kind === 'checkbox') {
          control = <input key={key} type="checkbox" checked={field.value === true || field.value === 'Yes'} disabled={off} aria-label={field.tooltip || field.name} onChange={(event) => fill(field, event.target.checked)} data-testid={id} />
        } else if (kind === 'radio') {
          control = (
            <span className="pdf-field-choices" role="radiogroup" aria-label={field.tooltip || field.name} data-testid={id}>
              {options.map((option) => (
                <label key={optionValue(option)} className="pdf-field-choice">
                  <input type="radio" name={id} value={optionValue(option)} checked={text === optionValue(option)} disabled={off} onChange={() => fill(field, optionValue(option))} />
                  {optionLabel(option)}
                </label>
              ))}
            </span>
          )
        } else if (kind === 'combo' && field.editable) {
          control = (
            <>
              {textBox(`${id}-options`)}
              <datalist id={`${id}-options`}>
                {options.map((option) => <option key={optionValue(option)} value={optionValue(option)}>{optionLabel(option)}</option>)}
              </datalist>
            </>
          )
        } else if (kind === 'combo' || kind === 'list') {
          const many = kind === 'list' && field.multi_select
          const chosen = Array.isArray(field.value) ? field.value.map(String) : text ? [text] : []
          control = (
            <select
              key={key}
              className="kui-field"
              multiple={many}
              value={many ? chosen : chosen[0] ?? ''}
              disabled={off}
              aria-label={field.tooltip || field.name}
              onChange={(event) => fill(field, many ? [...event.target.selectedOptions].map((option) => option.value) : event.target.value)}
              data-testid={id}
            >
              {many ? null : <option value="">{t('fieldNoChoice')}</option>}
              {options.map((option) => <option key={optionValue(option)} value={optionValue(option)}>{optionLabel(option)}</option>)}
            </select>
          )
        } else if (kind === 'button' || kind === 'signature') {
          control = <span className="kui-muted" data-testid={id}>{t(kind === 'button' ? 'fieldButtonNote' : 'fieldSignatureNote')}</span>
        } else {
          control = textBox()
        }
        return (
          <div key={field.name} className="pdf-field" onFocus={() => onPage(field.page - 1)} data-testid={`pdf-field-row-${field.name}`}>
            <span className="pdf-field-label" title={field.name}>{field.tooltip || field.name}</span>
            <span className="kui-muted pdf-field-kind" data-testid={`pdf-field-kind-${field.name}`}>
              {t(`field_${kind}`)}
              {field.required ? ` · ${t('fieldRequired')}` : ''}
            </span>
            {control}
          </div>
        )
      })}
    </div>
  )
}
