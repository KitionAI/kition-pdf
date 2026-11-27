/** A comment as `comment.list` answers it, and what the Comments panel needs from one. */
export type CommentItem = {
  id: string | null
  page: number
  index: number
  type: string
  author?: string | null
  contents: string | null
  modified?: string | null
  rect?: number[] | null
  color?: string | null
  status?: string | null
  locked?: boolean
  replies?: CommentItem[]
}

/** The review statuses `comment.setStatus` takes, in its order. */
export const STATUSES = ['none', 'accepted', 'rejected', 'cancelled', 'completed'] as const

/** What names a comment to the commands that act on one: its id, else its page and place on it. */
export function commentRef(item: Pick<CommentItem, 'id' | 'page' | 'index'>): Record<string, unknown> {
  return item.id ? { id: item.id } : { page: item.page, index: item.index }
}

export function sameComment(a: Pick<CommentItem, 'id' | 'page' | 'index'>, b: Pick<CommentItem, 'id' | 'page' | 'index'>) {
  return a.id && b.id ? a.id === b.id : !a.id && !b.id && a.page === b.page && a.index === b.index
}

/** A comment's status as one of `STATUSES` (the list answers the state's name, `Accepted`). */
export function statusOf(item: Pick<CommentItem, 'status'>): (typeof STATUSES)[number] {
  const status = (item.status ?? 'none').toLowerCase()
  return (STATUSES as readonly string[]).includes(status) ? (status as (typeof STATUSES)[number]) : 'none'
}

/** A PDF date (`D:20261127093000+01'00'`, or an ISO date) as a time, or null. */
export function pdfDate(text: string | null | undefined): Date | null {
  if (!text) return null
  // `comment.list` answers `2026-11-20 10:00` (WebKit's Date does not read that form).
  const plain = /^(\d{4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2})(?::(\d{2}))?$/.exec(text.trim())
  if (plain) {
    const [year, month, day, hour, minute, second = '0'] = plain.slice(1).map((part) => part ?? '0')
    const local = new Date(Number(year), Number(month) - 1, Number(day), Number(hour), Number(minute), Number(second))
    return Number.isFinite(local.getTime()) ? local : null
  }
  const match = /^(?:D:)?(\d{4})(\d{2})?(\d{2})?(\d{2})?(\d{2})?(\d{2})?([Zz+-])?(\d{2})?'?(\d{2})?'?$/.exec(text.trim())
  if (!match) {
    const iso = new Date(text)
    return Number.isFinite(iso.getTime()) ? iso : null
  }
  const [year, month = '01', day = '01', hour = '00', minute = '00', second = '00'] = match.slice(1, 7)
  const sign = match[7]
  const offset = sign === '+' || sign === '-' ? (sign === '-' ? -1 : 1) * (Number(match[8] ?? 0) * 60 + Number(match[9] ?? 0)) : 0
  const utc = Date.UTC(Number(year), Number(month) - 1, Number(day), Number(hour), Number(minute), Number(second)) - offset * 60_000
  return Number.isFinite(utc) ? new Date(utc) : null
}

/** When a comment was made, as the panel shows it: relative within a week, else the date. */
export function shownDate(text: string | null | undefined, locale: string, now = Date.now()): string {
  const date = pdfDate(text)
  if (!date) return ''
  const seconds = Math.round((date.getTime() - now) / 1000)
  const relative = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' })
  const ago = Math.abs(seconds)
  if (ago < 60) return relative.format(0, 'second')
  if (ago < 3600) return relative.format(Math.round(seconds / 60), 'minute')
  if (ago < 86_400) return relative.format(Math.round(seconds / 3600), 'hour')
  if (ago < 7 * 86_400) return relative.format(Math.round(seconds / 86_400), 'day')
  return new Intl.DateTimeFormat(locale, { dateStyle: 'medium' }).format(date)
}
