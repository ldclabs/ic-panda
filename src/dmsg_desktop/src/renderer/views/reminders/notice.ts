import type { Glyph } from '../../components/glyphs'
import { locale, t } from '../../i18n/index.svelte'
import { call } from '../../rpc'
import { titleOf } from '../../approval'
import { label } from '../channels/names.svelte'
import { notificationBody } from '../../../shared/notification-text'
import type { NoticeCategory, NoticeItem } from '../../../shared/notifications'

export const icons: Record<NoticeCategory, Glyph> = { security: 'security', action: 'approvals', message: 'channels', service: 'plan' }

export const category = (item: NoticeItem): NoticeCategory =>
  ['recovery'].includes(item.target.kind) ? 'security'
    : ['approval', 'wallet', 'task'].includes(item.target.kind) ? 'action'
      : ['plan', 'service'].includes(item.target.kind) ? 'service'
        : item.target.kind === 'summary' ? (item.target.id as NoticeCategory) : 'message'

/** What a reminder's names come from: the notifications themselves carry no content. */
export interface NoticeNames {
  channels: Map<string, string>
  letters: Map<string, string>
  approvals: Map<string, any>
}

export const noNames = (): NoticeNames => ({ channels: new Map(), letters: new Map(), approvals: new Map() })
export const noticeUnread = (item: NoticeItem): boolean => BigInt(item.read_version ?? '0') < BigInt(item.version)

const listOf = (method: string, params?: Record<string, unknown>) => call<any[]>(method, params).then((list) => list ?? [], () => [])

/** Read locally when a list of reminders opens; a locked device just shows the plain titles. */
export async function noticeNames(): Promise<NoticeNames> {
  const [channels, letters, approvals] = await Promise.all([listOf('channel/list'), listOf('inbox/list', { refresh: false }), listOf('approval/list')])
  return {
    channels: new Map(channels.map((c) => [c.id, c.name])),
    letters: new Map(letters.map((l) => [l.order_id, l.sender])),
    approvals: new Map(approvals.map((a) => [a.id, a]))
  }
}

/** A reminder's title: what it is about, by name once the names are known. */
export function noticeTitle(item: NoticeItem, names: NoticeNames): string {
  const { kind, id } = item.target
  switch (kind) {
    case 'recovery': return t('pendingRecovery')
    case 'approval': {
      const request = names.approvals.get(id)
      if (!request) return t('approvals')
      const title = titleOf(request)
      return title.detail ? `${t(title.label)} · ${title.detail}` : t(title.label)
    }
    case 'wallet': return `${t('twNav')} · ${t('approvals')}`
    case 'task': return id.startsWith('conflicts:') ? t('conflict') : t('syncFailed')
    case 'service': return id.startsWith('network:') ? t('reminderSyncTitle') : t('reminderConnectionTitle')
    case 'plan': return t('membership')
    case 'inbox': {
      const sender = id && names.letters.get(id)
      return sender ? `${t('inbox')} · ${label(sender)}` : t('inbox')
    }
    case 'channel': return names.channels.get(id) ?? t('channels')
    case 'agent_mail': return t('noticeAgent')
    default: return notificationBody(locale(), category(item), item.unread)
  }
}

export function noticeBody(item: NoticeItem): string {
  if (item.target.kind === 'service') return item.target.id.startsWith('network:') ? t('networkUnavailable') : t('reminderConnectionHint')
  return notificationBody(locale(), category(item), item.unread)
}
