<script lang="ts">
  import { daemonBlocked } from '../connection-gate'
  import { watchContent } from '../content-refresh'
  import type { Delivery, MessageDeliveries } from '../message-delivery'
  import Notice from '../components/page/Notice.svelte'
  import Empty from '../components/page/Empty.svelte'
  import { flip } from 'svelte/animate'
  import { arrive, enter, flipDuration, pop, shrink } from '../components/motion'
  import Count from '../components/Count.svelte'
  import { ContextMenu, DropdownMenu } from 'bits-ui'
  import { onMount, tick, untrack } from 'svelte'
  import { noticeVisibility } from '../notification-visibility'
  import { tomorrowMorning } from '../../shared/notification-time'
  import { localeDir } from '../../shared/locales'
  import Avatar from '../components/Avatar.svelte'
  import Dialog from '../components/Dialog.svelte'
  import FileLightbox from '../components/FileLightbox.svelte'
  import type { PreviewItem } from '../components/preview'
  import type { ChannelAttachmentItem } from '../../shared/contract'
  import HighlightText from '../components/HighlightText.svelte'
  import { messagePreview, type HighlightSpan } from '../components/highlight'
  import Icon from '../components/Icon.svelte'
  import Locked from '../components/page/Locked.svelte'
  import Field from '../components/page/Field.svelte'
  import PaneHandle from '../components/PaneHandle.svelte'
  import TargetMenu from '../components/TargetMenu.svelte'
  import TextField from '../components/TextField.svelte'
  import { toast } from '../components/toast.svelte'
  import Tooltip from '../components/Tooltip.svelte'
  import { iconButtonClass, buttonClass, cn, menuContentClass, menuItemClass } from '../components/ui'
  import { locale, t } from '../i18n/index.svelte'
  import { bridge, call, closed, errorText, onChange, planAction, single } from '../rpc'
  import Access from './channels/Access.svelte'
  import { arrange, kindLabel, listTime, powers, roleLabel, statusLabel } from './channels/channels'
  import { pinsOf, readReceipts, setPins } from './channels/preferences'
  import { remember, remembered, rememberedPlain, rememberPlain } from '../view-memory'
  import LibraryPicker from './channels/LibraryPicker.svelte'
  import LegacyHistory from './channels/LegacyHistory.svelte'
  import Members from './channels/Members.svelte'
  import { accountOf, label, visibleName } from './channels/names.svelte'
  import NewChannel from './channels/NewChannel.svelte'
  import Packet from './channels/Packet.svelte'
  import Thread, { type Outgoing } from './channels/Thread.svelte'
  import Transfer from './channels/Transfer.svelte'
  import PackageImport from './secrets/PackageImport.svelte'
  import { workflowText } from './secrets/workflow-text'

  /**
   * Channels: the list, pinned ones and then latest activity first with what
   * is unread, the open channel's timeline and composer, and its members in a
   * drawer. Creating, joining, transfers, history grants, access and
   * confirmations open in dialogs; a `dmsg://` invitation link opens the join
   * dialog with it, and a profile or a letter opens a conversation with its
   * person. Each change comes back through the daemon's `channel/changed`.
   * Leaving the view keeps which channel was open, and what was being written.
   * ⌘F searches the channel, ⌘N starts a new one, ⌥↑ and ⌥↓ move through the
   * list (Ctrl for ⌘ elsewhere).
   */
  let { status, route, go, deliveries, messageDeliveries }: {
    status: any; route: string; go: (path: string) => void; deliveries: Delivery[]; messageDeliveries: MessageDeliveries
  } = $props()

  let alive = true
  let openRequest = 0
  let channelVisit = 0
  let channels = $state<any[]>([])
  let legacyChannels = $state<any[]>([])
  let legacySelected = $state<any>(null)
  let legacyError = $state('')
  let legacyRequested: string | null = null
  let selected = $state<string | null>(remembered<string>('channels.selected') ?? null)
  let channel = $state<any>(null)
  let messages = $state<any[]>([])
  /** Earlier messages exist above the first one shown. */
  let more = $state(false)
  let nextBefore = $state<number | null>(null)
  let loadingEarlier = $state(false)
  /** Newer messages exist below the last one shown when viewing search context. */
  let moreAfter = $state(false)
  let nextAfter = $state<number | null>(null)
  let loadingNewer = $state(false)
  let files = $state<any[]>([])
  let query = $state('')
  let listSearchHits = $state<{ key: string; map: Map<string, any> } | null>(null)
  let crossMessageGroups = $state<{ key: string; groups: any[] } | null>(null)
  let listSearchSeq = 0
  let busy = $state(false)
  let error = $state('')
  /** Unsent text and attachments per channel; the text is kept by the daemon too. */
  let drafts = $state<Record<string, string>>(rememberedPlain('channels.drafts') ?? {})
  let attached = $state<Record<string, { id: string; name: string }[]>>(rememberedPlain('channels.attached') ?? {})
  /** The message each channel's next one answers. */
  let replies = $state<Record<string, any>>(rememberedPlain('channels.replies') ?? {})
  let editing = $state<{ message: any; text: string } | null>(null)
  /** The timeline shows a search result's surroundings until the person asks for the latest again. */
  let reading = $state(false)
  let pins = $state<string[]>([])
  let onlyUnread = $state(remembered<boolean>('channels.unread') ?? false)
  let searchInput = $state<HTMLInputElement | null>(null)
  let searchButton = $state<HTMLButtonElement | null>(null)
  let listInput = $state<HTMLInputElement | null>(null)
  /** The daemon's outbox of the open channel, the messages this window sends now, and why the last try failed. */
  let outbox = $state<Outgoing[]>([])
  let receipts = $state<any>(null)
  let images = $state<Record<number, string>>({})
  let lightboxOpen = $state(false)
  let lightboxKey = $state<string | null>(null)
  let channelAttachmentList = $state<ChannelAttachmentItem[] | null>(null)
  let attachmentListChannel = $state<string | null>(null)
  let attachmentListReq = 0
  let search = $state<{
    open: boolean
    query: string
    hasFile: boolean
    results: any[] | null
    nextBefore: number | null
    activeIndex: number
    busy?: boolean
    loadingMore?: boolean
    error?: string
  }>({ open: false, query: '', hasFile: false, results: null, nextBefore: null, activeIndex: -1 })
  let searchSeq = 0
  let highlight = $state<number | null>(null)
  let highlightSpans = $state<HighlightSpan[] | null>(null)
  let membersOpen = $state(stored('dmsg.members') === 'open')
  let showOther = $state(stored('dmsg.channels-other') === 'open')
  let showLegacy = $state(false)
  let creating = $state(false)
  let createTab = $state<'create' | 'join'>('create')
  let joinText = $state('')
  let peer = $state<{ account: string; name?: string } | null>(null)
  let transferDraft = $state<{ target: string; text: string; channel?: string } | null>(null)

  let settling = $state(false)
  let transfer = $state<{ open: boolean; kind: 'owner' | 'sponsor' }>({ open: false, kind: 'owner' })
  let history = $state<{ open: boolean; member: any; from: number; to: number }>({ open: false, member: null, from: 1, to: 1 })
  let leaving = $state(false)
  let deletingLegacyDraft = $state(false)
  let archiving = $state(false)
  let retracting = $state<any>(null)
  let renaming = $state<{ open: boolean; name: string }>({ open: false, name: '' })
  let access = $state(false)
  let picking = $state(false)
  let snapshotOpen=$state(false), snapshot=$state<any>(null), snapshotError=$state('')
  let snapshotGeneration=0
  async function receiveSnapshot(message:any){
    const generation=++snapshotGeneration
    const destination=selected
    snapshot=null;snapshotError='';snapshotOpen=true
    try{const result=await call('secrets/packages/channel_preview',{channel_id:destination,seq:message.seq});if(generation===snapshotGeneration&&snapshotOpen&&selected===destination)snapshot=result}catch(e){if(generation===snapshotGeneration&&snapshotOpen)snapshotError=errorText(e)}
  }
  onMount(()=>()=>{snapshotGeneration++;snapshot=null})

  /** How many messages a channel opens with, and each page of earlier ones. */
  const PAGE = 200

  const unlocked = $derived(status?.state === 'unlocked')
  const me = $derived<string | undefined>(status?.account)
  const ledger = $derived(channel?.ledger)
  const can = $derived(powers(ledger, me))
  const trimmedListQuery = $derived(query.trim())
  const activeChannelHits = $derived(listSearchHits && listSearchHits.key === trimmedListQuery ? listSearchHits.map : null)
  const activeCrossMessages = $derived(crossMessageGroups && crossMessageGroups.key === trimmedListQuery ? crossMessageGroups.groups : [])
  const listed = $derived.by(() => {
    const q = trimmedListQuery
    const filtered = channels.filter((c) => {
      if (onlyUnread && !(c.unread || c.news || c.id === selected)) return false
      if (!q) return true
      if (activeChannelHits) return activeChannelHits.has(c.id)
      return c.name.toLowerCase().includes(q.toLowerCase())
    })
    return arrange(filtered, me, pins)
  })
  const anyUnread = $derived(channels.some((c) => c.unread || c.news))
  const oldChannels = $derived(legacyChannels.filter((c) => !query.trim() || `${c.name} ${c.source}`.toLowerCase().includes(query.trim().toLowerCase())))
  const currentDeliveries = $derived(deliveries.filter(row => row.channel === channel?.id))
  const shownMessages = $derived.by(() => {
    if (reading) return messages
    const rows = new Map(messages.map((m: any) => [m.seq, m]))
    for (const row of currentDeliveries) if (row.state === 'confirmed') rows.set(row.message.seq, row.message)
    return [...rows.values()].sort((a, b) => a.seq - b.seq)
  })
  const lightboxItems = $derived.by<PreviewItem[]>(() => {
    const id = channel?.id
    if (!id || !unlocked || !lightboxOpen) return []
    const bySeq = new Map<number, PreviewItem>()
    if (channelAttachmentList && attachmentListChannel === id) {
      for (const item of channelAttachmentList) {
        bySeq.set(item.seq, {
          key: String(item.seq),
          source: { source: 'channel', channel: id, seq: item.seq },
          name: item.name,
          mime: item.mime,
          size: item.size,
          updatedAt: item.stored_at || item.created_at,
          previewClass: item.preview_class,
          previewable: item.previewable,
          placeholderUrl: images[item.seq] || undefined
        })
      }
    }
    for (const m of shownMessages) {
      if (m.retracted || m.hidden || m.unreadable || m.file_deleted || !m.file || m.file.mime === 'application/vnd.dmsg.secret-package') {
        bySeq.delete(m.seq)
        continue
      }
      if (!channelAttachmentList || attachmentListChannel !== id) {
        bySeq.set(m.seq, {
          key: String(m.seq),
          source: { source: 'channel', channel: id, seq: m.seq },
          name: m.file.name,
          mime: m.file.mime ?? '',
          size: m.file.size ?? 0,
          updatedAt: m.stored_at || m.created_at,
          placeholderUrl: images[m.seq] || undefined
        })
      }
    }
    return [...bySeq.entries()].sort((a, b) => a[0] - b[0]).map(([, item]) => item)
  })

  function openAttachmentPreview(message: any) {
    const id = channel?.id
    if (!id || !message?.file || message.file_deleted || message.retracted || message.file.mime === 'application/vnd.dmsg.secret-package') return
    lightboxKey = String(message.seq)
    channelAttachmentList = null
    lightboxOpen = true
    refreshAttachments(id)
  }

  function refreshAttachments(id: string) {
    const req = ++attachmentListReq
    void call<ChannelAttachmentItem[]>('channel/attachments', { id })
      .then((list) => {
        if (!alive || !lightboxOpen || selected !== id || req !== attachmentListReq) return
        channelAttachmentList = list
        attachmentListChannel = id
      })
      .catch(() => { /* keep currently loaded thread attachments */ })
  }
  const shownOutbox = $derived(reading ? [] : [
    ...currentDeliveries.filter(row => row.state !== 'confirmed' && !shownMessages.some((m: any) => m.message_id === row.message_id)),
    ...outbox.filter(row => !currentDeliveries.some(pending => pending.message_id === row.message_id))
  ])
  const transferring = $derived(channel?.billing?.lifecycle === 'transferring')
  const direct = $derived(ledger?.type === 'direct')
  const shortDay = $derived(new Intl.DateTimeFormat(locale(), { month: 'numeric', day: 'numeric' }))

  function stored(key: string): string | null {
    try {
      return localStorage.getItem(key)
    } catch {
      return null
    }
  }

  function store(key: string, value: string) {
    try {
      localStorage.setItem(key, value)
    } catch {
      /* kept until the window closes */
    }
  }

  $effect(() => store('dmsg.members', membersOpen ? 'open' : 'closed'))
  $effect(() => store('dmsg.channels-other', showOther ? 'open' : 'closed'))
  $effect(() => {
    pins = pinsOf(me)
  })
  $effect(() => remember('channels.selected', selected))
  $effect(() => remember('channels.unread', onlyUnread || null))

  // Left sidebar search: match channels by name/member/draft/preview (pinyin-aware) and cross-channel messages.
  $effect(() => {
    const q = trimmedListQuery
    void channels
    if (!q || !unlocked) {
      listSearchHits = null
      crossMessageGroups = null
      return
    }
    const req = ++listSearchSeq
    let cancelled = false
    const timer = setTimeout(async () => {
      try {
        const [channelHits, msgGroups] = await Promise.all([
          call<any[]>('channel/search_list', { query: q }).catch(() => null),
          call<any[]>('channel/search_all', { query: q, limit: 3, max_channels: 15, has_file: false }).catch(() => null)
        ])
        if (cancelled || !alive || req !== listSearchSeq) return
        if (channelHits) listSearchHits = { key: q, map: new Map(channelHits.map((h) => [h.id, h])) }
        if (msgGroups) crossMessageGroups = { key: q, groups: msgGroups }
      } catch { /* fallback to local name filter */ }
    }, 110)
    return () => { cancelled = true; clearTimeout(timer) }
  })

  const fail = (e: unknown) => toast(() => errorText(e), planAction(e))

  async function attempt(work: () => Promise<unknown>) {
    busy = true
    try {
      await work()
    } catch (e) {
      fail(e)
    } finally {
      busy = false
    }
  }

  const random = () => Array.from(crypto.getRandomValues(new Uint8Array(32)), (b) => b.toString(16).padStart(2, '0')).join('')
  const ref = (message: any) => ({ seq: message.seq, digest: message.digest })

  const reload = single(async () => {
    try {
      channels = await call('channel/list')
      error = ''
      readAhead()
      if (selected && !channels.some((c) => c.id === selected)) selected = channel = null
      if (selected) await open(selected)
    } catch (e) {
      channels = []
      error = closed(e, status) ? 'LOCKED' : errorText(e)
    }
  })

  /** What came elsewhere is read ahead while this list is on screen; the daemon paces it. */
  function readAhead() {
    if (document.visibilityState !== 'visible' || daemonBlocked()) return
    for (const entry of listed.live.filter((c) => c.news && !c.muted && c.id !== selected).slice(0, 8))
      void call('channel/catch_up', { id: entry.id }).catch(() => {})
  }

  const refreshChannel = single(async (id: string | null = selected) => {
    if (!alive || !unlocked || !id || selected !== id || document.visibilityState !== 'visible' || daemonBlocked()) return
    let failure = ''
    try { await call('channel/sync', { id, recent: true }) }
    catch (e) { failure = closed(e, status) ? 'LOCKED' : errorText(e) }
    if (alive && selected === id) { await reload(); if (alive && selected === id) error = failure }
  })

  function pickLegacy(entry: any) {
    flushDraft()
    selected = channel = null
    legacySelected = entry
    showLegacy = true
  }

  const readLegacy = single(async () => {
    try {
      const result: any = await call('legacy/channels')
      legacyChannels = result.channels
      if (legacySelected && !legacyChannels.some((c) => c.archive === legacySelected.archive && c.source === legacySelected.source)) legacySelected = null
      legacyError = result.errors.map((e: any) => e.error).join('; ')
      if (legacyRequested) {
        const first = legacyChannels.find((c) => c.archive === legacyRequested)
        if (first) pickLegacy(first)
        legacyRequested = null
      }
    } catch (e) {
      legacyChannels = []
      legacySelected = null
      legacyError = closed(e, status) ? '' : errorText(e)
    }
  })

  /**
   * A channel, its latest messages and outbox. The one already shown reads
   * its last page again with the messages after it, as edits, withdrawals
   * and hides change messages already shown.
   */
  async function open(id: string, atSeq?: number, spans?: HighlightSpan[] | null) {
    const request = ++openRequest
    const sendRevision = messageDeliveries.revision
    legacySelected = null
    const switching = channel?.id !== id
    if (selected !== id || atSeq !== undefined) { channelVisit++; loadingEarlier = false; loadingNewer = false }
    selected = id
    if (atSeq !== undefined && Number.isSafeInteger(atSeq) && atSeq > 0) {
      const [next, win, box] = await Promise.all([
        call<any>('channel/show', { id }),
        call<{ items: any[]; anchor: number; has_before: boolean; has_after: boolean; next_before: number | null; next_after: number | null }>('channel/messages_around', { id, seq: atSeq, before: 30, after: 30 }),
        call<Outgoing[]>('channel/outbox', { id }).catch(() => [])
      ])
      if (!alive || selected !== id || openRequest !== request) return
      if (switching) {
        flushDraft()
        editing = null
        images = {}
        lightboxOpen = false
        channelAttachmentList = null
        attachmentListChannel = null
        search = { open: false, query: '', hasFile: false, results: null, nextBefore: null, activeIndex: -1 }
        drafts[id] ??= next.draft ?? ''
        marked = 0
        void loadReceipts(id)
      }
      messages = win.items
      more = win.has_before
      nextBefore = win.next_before
      moreAfter = win.has_after
      nextAfter = win.next_after
      highlight = win.anchor
      highlightSpans = spans ?? null
      reading = win.has_after
      channel = next
      outbox = box
      messageDeliveries.loaded(id, win.items.map((m: any) => m.seq), sendRevision)
      for (const item of box) if (messageDeliveries.adopt(id, item)) retry(item)
      if (switching && next.ledger?.members?.[me ?? '']) void refreshChannel(id)
      return
    }
    // Edits and withdrawals update their original rows; reread the opening
    // page rather than assuming only the last few messages can change.
    const [next, page, box] = await Promise.all([
      call<any>('channel/show', { id }),
      call<{ items: any[]; has_more: boolean; next_before: number | null }>('channel/message_page', { id, before: Number.MAX_SAFE_INTEGER, limit: PAGE }),
      call<Outgoing[]>('channel/outbox', { id }).catch(() => [])
    ])
    if (!alive || selected !== id || openRequest !== request) return
    const list = page.items
    if (switching) {
      flushDraft()
      messages = list
      more = page.has_more
      nextBefore = page.next_before
      moreAfter = false
      nextAfter = null
      editing = null
      reading = false
      images = {}
      lightboxOpen = false
      channelAttachmentList = null
      attachmentListChannel = null
      highlight = null
      highlightSpans = null
      search = { open: false, query: '', hasFile: false, results: null, nextBefore: null, activeIndex: -1 }
      drafts[id] ??= next.draft ?? ''
      marked = 0
      void loadReceipts(id)
      // Show local history immediately, then catch up this channel without
      // waiting for its place in the background pass.
      if (next.ledger?.members?.[me ?? '']) void refreshChannel(id)
    } else if (reading) {
      // Around a search result: what is shown may change, what came later waits.
      const changed = new Map(list.map((m: any) => [m.seq, m]))
      messages = messages.map((m: any) => changed.get(m.seq) ?? m)
    } else {
      // A local send can be acknowledged before earlier incoming messages
      // arrive. Merge by sequence so those gaps appear in the right place.
      const oldest = messages[0]?.seq
      const changed = new Map([...messages, ...list].map((m: any) => [m.seq, m]))
      messages = [...changed.values()].sort((a, b) => a.seq - b.seq)
      if (oldest === undefined || list[0]?.seq <= oldest) {
        more = page.has_more
        nextBefore = page.next_before
      }
    }
    channel = next
    outbox = box
    messageDeliveries.loaded(id, list.map((m: any) => m.seq), sendRevision)
    for (const item of box) if (messageDeliveries.adopt(id, item)) retry(item)
  }

  let receiptsAt = 0
  async function loadReceipts(id: string) {
    receiptsAt = Date.now()
    const loaded = await call('channel/receipts', { id }).catch(() => null)
    if (selected === id) receipts = loaded
  }

  const pick = (id: string, atSeq?: number, spans?: HighlightSpan[] | null) => attempt(() => open(id, atSeq, spans))

  /** The library's latest files, read when the attachment menu opens. */
  async function attachments() {
    const all = await call<any[]>('files/search', { query: '' }).catch(() => [])
    files = all.filter((f) => f.status !== 'cloud_only' && f.size <= 100 * 1024 * 1024).slice(0, 8)
  }

  const read = () => attempt(() => call('channel/sync', { id: channel.id, recent: true }))

  function saveDraft(id: string, text: string) {
    if (daemonBlocked()) return
    void call('channel/draft', { id, text }).then(() => {
      if (alive && channel?.id === id) channel.draft = text
    }).catch(e => { if (alive && channel?.id === id) fail(e) })
  }

  function flushDraft() {
    if (channel && (drafts[channel.id] ?? '') !== (channel.draft ?? '')) saveDraft(channel.id, drafts[channel.id] ?? '')
  }

  // Also flush before replacing the current channel or leaving the view.
  $effect(() => {
    const id = channel?.id
    if (!id) return
    const text = drafts[id] ?? ''
    if (text === (channel.draft ?? '')) return
    const timer = setTimeout(() => saveDraft(id, text), 600)
    return () => clearTimeout(timer)
  })

  /** Send one message; it stays in the daemon's outbox until the relay stores it. */
  function deliver(id: string, item: Outgoing, params: Record<string, unknown>, file?: { id: string; name: string }) {
    const draft = JSON.parse(JSON.stringify(params))
    const [source, version] = file?.id.split('@') ?? []
    const pending: Outgoing = { ...item, draft, file: file ? { name: file.name, size: item.file?.size ?? 0, source: file.id } : item.file }
    const payload = { id, message_id: item.message_id, ...draft, ...(file ? { file: source, version } : {}) }
    void messageDeliveries.submit(id, pending,
      () => call('channel/queue', payload),
      () => call(file ? 'channel/send_file' : 'channel/send', payload))
  }

  async function send() {
    const id = channel.id
    if (reading) void latest()
    if (editing) {
      const { message, text } = editing
      editing = null
      if (text.trim() && text !== message.text) deliver(id, { message_id: random(), draft: { text } }, { text, edit_of: ref(message) })
      return
    }
    const text = drafts[id] ?? ''
    const files = attached[id] ?? []
    if (!text.trim() && !files.length) return
    const replyTo = replies[id] ? ref(replies[id]) : null
    drafts[id] = ''
    attached[id] = []
    delete replies[id]
    // A text goes with the first file; each further file goes in a message of its own.
    if (!files.length) deliver(id, { message_id: random(), draft: { text } }, { text, reply_to: replyTo })
    for (const [i, file] of files.entries()) {
      const words = i === 0 ? text : ''
      deliver(id, { message_id: random(), draft: { text: words }, file: { name: file.name, size: 0 } }, { text: words, reply_to: i === 0 ? replyTo : null }, file)
    }
  }

  /** The same message ID again: one the relay stored meanwhile is not sent twice. */
  function retry(item: Outgoing) {
    const draft: any = item.draft ?? {}
    const params = { text: draft.text ?? '', reply_to: draft.reply_to ?? null, edit_of: draft.edit_of ?? null, retract: draft.retract ?? null, packet: draft.packet ?? null }
    const file = (item.file as any)?.source ? { id: (item.file as any).source, name: item.file!.name } : undefined
    void deliver(channel.id, item, params, file)
  }

  const discard = (item: Outgoing) =>
    attempt(async () => {
      await call('channel/discard', { id: channel.id, message_id: item.message_id })
      messageDeliveries.discard(channel.id, item.message_id)
      await reload()
    })

  /** Files attached to messages wait in a folder of their own, not among the person's own. */
  async function attachmentFolder(): Promise<string> {
    const name = t('channelAttachments')
    const listing = await call<{ folders: { id: string; name: string }[] }>('files/list', { folder: 'root' })
    const found = listing.folders.find((folder) => folder.name === name)
    if (found) return found.id
    const made = await call<{ id: string }>('files/mkdir', { parent: 'root', name })
    return made.id
  }

  /** New files enter the library through the system dialog, a drop or a paste, then go with the next message; a taken name keeps both. */
  async function addFiles(add: (folder: string) => Promise<unknown>) {
    const id = channel.id
    await attempt(async () => {
      const folder = await attachmentFolder()
      const added: any = await bridge(() => add(folder))
      const fresh = (added?.files ?? []).map((f: any) => f.file).filter(Boolean)
      // A file added again, as a new version, stays attached once and goes at its latest version.
      const list = attached[id] ?? []
      attached[id] = [...list, ...fresh.filter((f: any) => !list.some((a) => a.id === f.id)).map((f: any) => ({ id: f.id, name: f.name }))]
    })
  }

  const download = (message: any) =>
    attempt(async () => {
      const manifest: any = await bridge(() => window.dmsg.channelDownload(channel.id, message.seq, message.file.name))
      if (manifest)
        toast(() => `${t('savedVerified')} · ${message.file.name} · sha256 ${String(manifest.sha256).slice(0, 12)}…`, { testid: 'channel-notice' })
    })

  /** A shared file becomes a file of the library, re-encrypted under a key of its own. */
  const save = (message: any) =>
    attempt(async () => {
      const result: any = await call('files/save_shared', { channel: channel.id, seq: message.seq })
      toast(() => `${t('savedToLibrary')} · ${result.file?.name ?? message.file.name}`, { testid: 'channel-notice' })
    })

  async function preview(message: any) {
    const id = selected, digest = message.digest
    images[message.seq] = ''
    const result: any = await call('channel/preview', { id, seq: message.seq }).catch(() => null)
    if (alive && selected === id && messages.some((m) => m.seq === message.seq && m.digest === digest) && result?.url) images[message.seq] = result.url
  }

  const hide = (message: any, hidden: boolean) =>
    attempt(async () => {
      const id = channel.id
      const changed = await call<any>('channel/hide', { id, seq: message.seq, hidden })
      if (alive && channel?.id === id) messages = messages.map((m) => (m.seq === message.seq ? changed : m))
    })

  const retract = () =>
    attempt(async () => {
      const target = retracting
      retracting = null
      await deliver(channel.id, { message_id: random(), draft: { text: '' } }, { retract: ref(target) })
    })

  /** A transfer packet in the timeline: accepted here goes back into the channel; committed here completes it. */
  const settlePacket = (message: any, step: 'accept' | 'commit') =>
    attempt(async () => {
      const kind = message.packet.format === 'dmsg-channel-owner-transfer/1' ? 'owner' : 'sponsor'
      const result = await call(`channel/${kind}_${step}`, { packet: message.packet })
      if (step === 'accept') await call('channel/send', { id: channel.id, packet: result, message_id: random() })
      toast(() => (step === 'accept' ? t('packetAccepted') : t('packetCommitted')))
      await reload()
    })

  let marked = 0
  let noticeRead = $state<{ channel: string; seq: number } | null>(null)
  const reportNotice = noticeVisibility(() => unlocked && !reading && channel && noticeRead && noticeRead.channel === channel.id
    ? { account: status.account, session: status.session, target: { kind: 'channel', id: channel.id }, watermark: String(noticeRead!.seq) } : null,
    '[data-testid="channel-messages"]')
  $effect(() => { void reading; void noticeRead; void channel?.id; void status?.session; reportNotice() })
  /** The messages shown through `seq` are read; a read receipt goes only when chosen (C04). */
  function markRead(seq: number) {
    if (reading) return
    seq = Math.min(seq, channel?.latest_cursor ?? channel?.message_cursor ?? 0)
    if (channel && Number.isSafeInteger(seq) && seq > 0) noticeRead = { channel: channel.id, seq }
    if (!channel || seq <= Math.max(channel.read ?? 0, marked)) return
    marked = seq
    const id = channel.id
    void call<any>('channel/mark', { id, through: seq, receipt: readReceipts() }).then((view) => {
      if (!alive) return
      channels = channels.map((c) => (c.id === id && view.read >= (c.read ?? 0) ? { ...c, read: view.read, unread: view.unread } : c))
      if (channel?.id === id && view.read >= (channel.read ?? 0)) channel = { ...channel, read: view.read, unread: view.unread }
    }).catch(e => {
      if (alive && channel?.id === id && marked === seq) {
        marked = channel.read ?? 0
        fail(e)
      }
    })
  }

  async function earlier() {
    if (!channel || selected !== channel.id || !more || !nextBefore || loadingEarlier) return
    const id = channel.id, before = nextBefore, visit = channelVisit
    loadingEarlier = true
    try {
      const page = await call<{ items: any[]; has_more: boolean; next_before: number | null }>('channel/earlier', { id, before, limit: PAGE })
      if (!alive || selected !== id || channelVisit !== visit) return
      const changed = new Map([...messages, ...page.items].map((m: any) => [m.seq, m]))
      messages = [...changed.values()].sort((a, b) => a.seq - b.seq)
      more = page.has_more
      nextBefore = page.next_before
    } catch (error) { if (alive && selected === id && channelVisit === visit) fail(error) }
    finally { if (channelVisit === visit) loadingEarlier = false }
  }

  async function loadNewer() {
    if (!channel || selected !== channel.id || !moreAfter || !nextAfter || loadingNewer) return
    const id = channel.id, after = nextAfter, visit = channelVisit
    loadingNewer = true
    try {
      const page = await call<{ items: any[]; has_more: boolean; next_before: number | null }>('channel/messages_after', { id, after, limit: PAGE })
      if (!alive || selected !== id || channelVisit !== visit) return
      const changed = new Map([...messages, ...page.items].map((m: any) => [m.seq, m]))
      messages = [...changed.values()].sort((a, b) => a.seq - b.seq)
      moreAfter = page.has_more
      nextAfter = page.next_before
      if (!page.has_more) reading = false
    } catch (error) { if (alive && selected === id && channelVisit === visit) fail(error) }
    finally { if (channelVisit === visit) loadingNewer = false }
  }

  function showSearch(open: boolean) {
    if (open !== search.open) {
      search = { ...search, open, busy: false }
      if (!open) { highlight = null; highlightSpans = null }
    }
    if (open) void tick().then(() => searchInput?.focus())
  }

  // Live debounced in-channel message search whenever search panel is open and query/filter changes.
  $effect(() => {
    const req = ++searchSeq
    const id = channel?.id
    const asked = search.query.trim()
    const hasFile = search.hasFile
    search.results = null
    search.nextBefore = null
    search.activeIndex = -1
    search.error = ''
    search.busy = false
    search.loadingMore = false
    if (!search.open || !id || (!asked && !hasFile)) return
    const timer = setTimeout(() => {
      if (alive && selected === id && req === searchSeq) void find(false, req)
    }, 140)
    return () => clearTimeout(timer)
  })

  async function find(append = false, explicitSeq?: number) {
    const id = channel?.id
    const asked = search.query.trim()
    const hasFile = search.hasFile
    if ((!asked && !hasFile) || !id || selected !== id) return
    const req = explicitSeq ?? ++searchSeq
    const before = append ? search.nextBefore : null
    if (append) search.loadingMore = true
    else { search.busy = true; search.error = '' }
    try {
      const page = await call<{ hits: any[]; next_before: number | null }>('channel/search_rich', {
        id,
        query: asked,
        before,
        limit: 40,
        has_file: hasFile
      })
      if (!alive || selected !== id || req !== searchSeq) return
      const hits = append && search.results ? [...search.results, ...(page.hits ?? [])] : (page.hits ?? [])
      search.results = hits
      search.nextBefore = page.next_before ?? null
      if (!append) search.activeIndex = -1
    } catch (e) {
      if (alive && selected === id && req === searchSeq) search.error = errorText(e)
    } finally {
      if (alive && selected === id && req === searchSeq) {
        search.busy = false
        search.loadingMore = false
      }
    }
  }

  /** A search result opens with bidirectional surroundings; the search bar stays open for stepping through hits. */
  async function jump(seq: number, spans?: HighlightSpan[] | null, index = -1) {
    const id = channel.id, visit = ++channelVisit
    loadingEarlier = false
    loadingNewer = false
    const current = () => alive && selected === id && channelVisit === visit
    try {
      const win = await call<{ items: any[]; anchor: number; has_before: boolean; has_after: boolean; next_before: number | null; next_after: number | null }>('channel/messages_around', { id, seq, before: 30, after: 30 })
      if (!current()) return
      messages = win.items
      more = win.has_before
      nextBefore = win.next_before
      moreAfter = win.has_after
      nextAfter = win.next_after
      highlight = win.anchor
      highlightSpans = spans ?? null
      reading = win.has_after
      if (index >= 0) search.activeIndex = index
    } catch (e) { if (current()) fail(e) }
  }

  function stepSearch(delta: 1 | -1) {
    const hits = search.results
    if (!hits || !hits.length) { void find(false); return }
    const next = search.activeIndex < 0
      ? (delta > 0 ? 0 : hits.length - 1)
      : (search.activeIndex + delta + hits.length) % hits.length
    const hit = hits[next]
    void jump(hit.message.seq, hit.snippet_spans, next)
  }

  /** Back from a search result to the channel's newest messages. */
  async function latest() {
    const id = channel.id, visit = ++channelVisit
    loadingEarlier = false
    loadingNewer = false
    try {
      const page = await call<{ items: any[]; has_more: boolean; next_before: number | null }>('channel/message_page', { id, before: Number.MAX_SAFE_INTEGER, limit: PAGE })
      if (!alive || selected !== id || channelVisit !== visit) return false
      messages = page.items
      more = page.has_more
      nextBefore = page.next_before
      moreAfter = false
      nextAfter = null
      highlight = null
      highlightSpans = null
      reading = false
      return true
    } catch (e) {
      if (alive && selected === id && channelVisit === visit) fail(e)
      return false
    }
  }

  const settings = (change: Record<string, unknown>) => attempt(() => call('channel/settings', { id: channel.id, ...change }))
  const settingsOf = (id: string, change: Record<string, unknown>) => attempt(() => call('channel/settings', { id, ...change }))

  function togglePin(id: string) {
    pins = pins.includes(id) ? pins.filter((pin) => pin !== id) : [id, ...pins]
    if (me) setPins(me, pins)
  }

  const markAllRead = (entry: any) => attempt(() => call('channel/mark', { id: entry.id, through: entry.latest_cursor ?? entry.message_cursor, receipt: readReceipts() }))
  const channelOf = (element: Element) => channels.find((c) => c.id === element.closest('[data-channel]')?.getAttribute('data-channel')) ?? null

  function onKey(event: KeyboardEvent) {
    if (event.isComposing || document.querySelector('[aria-modal="true"]')) return
    const held = window.dmsg.platform === 'darwin' ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey
    const key = event.key.toLowerCase()
    if (held && !event.altKey && !event.shiftKey && key === 'f') {
      event.preventDefault()
      if (channel) showSearch(true)
      else listInput?.focus()
    } else if (held && !event.altKey && !event.shiftKey && key === 'n' && unlocked) {
      event.preventDefault()
      createTab = 'create'
      creating = true
    } else if (event.altKey && !event.metaKey && !event.ctrlKey && (event.key === 'ArrowUp' || event.key === 'ArrowDown')) {
      const list = listed.live
      if (!list.length) return
      event.preventDefault()
      const at = list.findIndex((c) => c.id === selected)
      const next = list[at < 0 ? 0 : (at + (event.key === 'ArrowUp' ? -1 : 1) + list.length) % list.length]
      void pick(next.id)
    } else if (event.key === 'Escape' && search.open) {
      event.preventDefault()
      search = { open: false, query: '', hasFile: false, results: null, nextBefore: null, activeIndex: -1 }
      highlight = null
      highlightSpans = null
      searchButton?.focus()
    }
  }
  const control = (action: Record<string, unknown>) => attempt(() => call('channel/control', { id: channel.id, action }))
  const rotate = () => attempt(() => call('channel/rotate', { id: channel.id }))
  const reactivate = () => attempt(() => call('channel/reactivate', { id: channel.id }))
  const cancelSponsor = () => attempt(() => call('channel/sponsor_cancel', { id: channel.id }))

  /** A new epoch now: who joins later reads nothing before it. */
  async function rekey() {
    const view: any = await call('channel/rekey', { id: channel.id })
    toast(() => t('rekeyed', { epoch: view.ledger?.epoch }))
  }

  const leave = () =>
    attempt(async () => {
      await call('channel/leave', { id: channel.id })
      leaving = false
      toast(() => t('left'))
    })

  const deleteLegacyDraft = () => attempt(async () => {
    await call('channel/delete_legacy_draft', { id: channel.id })
    deletingLegacyDraft = false
    selected = channel = null
    await reload()
  })

  /** An invitation by account ID or name, after a new epoch when what was written stays from the invited account. */
  async function invite(target: string, role: string, fresh: boolean): Promise<any | null> {
    try {
      const account = await accountOf(target)
      if (fresh) await rekey()
      return await call('channel/invite', { id: channel.id, account, role })
    } catch (e) {
      fail(e)
      return null
    }
  }

  /** Older epochs' keys sealed to the member's devices. */
  const grantHistory = () =>
    attempt(async () => {
      const { member, from, to } = history
      await call('channel/history', { id: channel.id, account: member.account, from: Number(from), to: Number(to) })
      toast(() => `${t('historyGranted')} ${from}–${to}`)
      history.open = false
    })

  /** History starts at the earliest epoch this account can read itself. */
  function openHistory(member: any) {
    const from = Math.min(...(channel.readable_epochs?.length ? channel.readable_epochs : [1]))
    history = { open: true, member, from, to: member.joined_epoch - 1 }
  }

  /** A channel created or joined opens once the list holds it. */
  async function opened(id: string) {
    selected = id
    await reload()
  }

  function directFor(entry: any, target: string, invited = false) {
    const members = Object.keys(entry?.ledger?.members ?? {})
    return !!me && entry?.ledger?.type === 'direct' && entry.ledger.status !== 'archived' && !entry.hidden
      && members.includes(me) && members.every(member => member === me || member === target) && (invited || members.includes(target))
  }
  function appendTransferLink(id: string, text: string) {
    const current = drafts[id] ?? channel?.draft ?? ''
    if (!current.split('\n').includes(text)) drafts[id] = current ? current + '\n' + text : text
  }
  async function openNameConversation(target: string, name?: string, text?: string) {
    const visit = channelVisit
    await reload()
    if (!alive || !unlocked || error || visit !== channelVisit) return
    const existing = arrange(channels, me, pins).live.find(entry => directFor(entry, target))
    if (existing) {
      await pick(existing.id)
      if (text && alive && channel?.id === existing.id && directFor(channel, target)) appendTransferLink(existing.id, text)
    } else {
      transferDraft = text ? { target, text } : null
      peer = { account: target, name: name ?? label(target) }
      creating = true
    }
  }
  function invitedForTransfer(id: string, account: string) {
    if (transferDraft?.target === account) transferDraft.channel = id
  }
  $effect(() => {
    const draft = transferDraft
    if (draft?.channel && channel?.id === draft.channel) {
      const id = draft.channel
      untrack(() => {
        if (directFor(channel, draft.target, true)) appendTransferLink(id, draft.text)
        transferDraft = null
      })
    } else if (draft && !creating && !draft.channel) transferDraft = null
  })

  // Links and other views arrive as the route's query: an invitation, a person, a channel.
  $effect(() => {
    const at = route.indexOf('?')
    if (at < 0) return
    const params = new URLSearchParams(route.slice(at + 1))
    const join = params.get('join')
    const direct = params.get('direct')
    const target = params.get('open')
    const legacy = params.get('legacy')
    if (join) {
      joinText = `dmsg://channel/join#${join}`
      createTab = 'join'
      creating = true
    } else if (direct) {
      const draft = params.get('compose') === 'transfer'
        ? rememberedPlain<{ target: string; text: string }>('plan.transfer-draft') : null
      rememberPlain('plan.transfer-draft', null)
      if (draft?.target === direct || params.get('compose') === 'message') {
        untrack(() => void openNameConversation(direct, params.get('name') ?? undefined, draft?.target === direct ? draft.text : undefined))
      }
      else {
        peer = { account: direct, name: params.get('name') ?? label(direct) }
        creating = true
      }
    } else if (target) {
      const rawSeq = params.get('seq')
      const seqNum = rawSeq ? Number(rawSeq) : undefined
      void pick(target, seqNum && Number.isSafeInteger(seqNum) ? seqNum : undefined)
    }
    else if (legacy) {
      legacyRequested = legacy
      void readLegacy()
    }
    go('/channels')
  })

  onMount(() => {
    void reload()
    void readLegacy()
    const offs = [
      onChange(['channel/changed', 'session/changed', 'sync/changed'], () => {
        void reload()
        if (lightboxOpen && selected) refreshAttachments(selected)
        if (channel && Date.now() - receiptsAt > 10_000) void loadReceipts(channel.id)
      }),
      onChange(['legacy/changed', 'session/changed', 'sync/changed'], readLegacy)
    ]
    const stop = watchContent('channel', () => selected, () => refreshChannel())
    return () => {
      alive = false
      stop()
      offs.forEach((off) => off())
      rememberPlain('channels.drafts', $state.snapshot(drafts))
      rememberPlain('channels.attached', $state.snapshot(attached))
      rememberPlain('channels.replies', $state.snapshot(replies))
      // The moment after typing stops, the daemon had not kept the text yet.
      flushDraft()
    }
  })
</script>

<svelte:window onkeydown={onKey} />

{#snippet channelMenu(entry: any)}
  <ContextMenu.Item class={menuItemClass} onSelect={() => togglePin(entry.id)} data-testid="channel-pin">
    <span class="flex items-center gap-2"><Icon name="pin" class="text-faint" />{pins.includes(entry.id) ? t('unpin') : t('pin')}</span>
  </ContextMenu.Item>
  {#if entry.unread}
    <ContextMenu.Item class={menuItemClass} onSelect={() => markAllRead(entry)} data-testid="channel-mark-read">
      <span class="flex items-center gap-2"><Icon name="check" class="text-faint" />{t('markRead')}</span>
    </ContextMenu.Item>
  {/if}
  <ContextMenu.Separator class="my-1 h-px bg-line" />
  {#if entry.muted || entry.muted_until > Date.now()}
    <ContextMenu.Item class={menuItemClass} onSelect={() => settingsOf(entry.id, { muted: false })}>
      <span class="flex items-center gap-2"><Icon name="bell" class="text-faint" />{t('unmuteChannel')}</span>
    </ContextMenu.Item>
  {:else}
    <ContextMenu.Item class={menuItemClass} onSelect={() => settingsOf(entry.id, { muted: true })}>
      <span class="flex items-center gap-2"><Icon name="eye-off" class="text-faint" />{t('muteChannel')}</span>
    </ContextMenu.Item>
    <ContextMenu.Item class={menuItemClass} onSelect={() => settingsOf(entry.id, { muted_until: Date.now() + 3_600_000 })}>
      <span class="ps-6">{t('noticePauseHour')}</span>
    </ContextMenu.Item>
    <ContextMenu.Item class={menuItemClass} onSelect={() => settingsOf(entry.id, { muted_until: tomorrowMorning(Date.now(), Intl.DateTimeFormat().resolvedOptions().timeZone) })}>
      <span class="ps-6">{t('noticePauseTomorrow')}</span>
    </ContextMenu.Item>
  {/if}
{/snippet}

{#snippet item(entry: any)}
  {@const draft = (drafts[entry.id] ?? entry.draft)?.trim()}
  {@const at = Math.max(entry.last?.stored_at ?? 0, entry.active_at ?? 0)}
  {@const hit = activeChannelHits?.get(entry.id)}
    <button
      class={cn(
        'grid w-full grid-cols-[auto_1fr_auto] items-center gap-2.5 rounded-lg px-2 py-2 text-start outline-none transition-colors duration-100 focus-visible:ring-3 focus-visible:ring-focus/35',
        selected === entry.id ? 'bg-pale' : 'hover:bg-wash'
      )}
      aria-current={selected === entry.id ? 'true' : undefined}
      onclick={() => pick(entry.id)}
      data-testid="channel-item"
    >
      <Avatar id={entry.id} name={entry.name} person={entry.ledger?.type === 'direct'} class="size-9" />
      <span class="min-w-0">
        <span class="flex min-w-0 items-center gap-1">
          <span class={cn('truncate text-body leading-snug transition-[font-weight] duration-180 ease-calm', entry.unread && !entry.muted ? 'font-semibold' : 'font-medium')}>
            <HighlightText text={entry.name} spans={hit?.name_spans} />
          </span>
          {#if entry.muted}<Icon name="eye-off" class="size-3 shrink-0 text-faint" />{/if}
          {#if pins.includes(entry.id)}<Icon name="pin" class="size-3 shrink-0 text-faint" label={t('pinned')} />{/if}
        </span>
        {#if hit?.match_field === 'member' && hit.match_value}
          <span class="block truncate text-callout leading-snug text-muted">
            {t('matchedMember', { name: hit.match_value })}
          </span>
        {:else if draft}
          <span class="block truncate text-callout leading-snug text-warn">{t('draft')}: {draft}</span>
        {:else if entry.last}
          <span {@attach visibleName(entry.last.account === me ? undefined : entry.last.account)} class="block truncate text-callout leading-snug text-muted" data-testid="channel-preview">
            {entry.last.account === me ? t('you') : label(entry.last.account, false)}: {entry.last.text || '…'}
          </span>
        {:else}
          <span class="block truncate text-callout leading-snug text-muted">
            {entry.ledger ? `${t(kindLabel[entry.ledger.type] ?? 'collaboration')} · ${t('memberCount', { n: Object.keys(entry.ledger.members).length })}` : '…'}
          </span>
        {/if}
      </span>
      <span class="grid justify-items-end gap-0.5">
        {#if at}<span class="text-caption text-faint tabular-nums">{listTime(at, Date.now(), locale())}</span>{/if}
        {#if entry.ledger && entry.ledger.status !== 'active'}
          <span class={cn('rounded px-1 text-caption font-medium', entry.ledger.status === 'archived' ? 'bg-wash text-muted' : 'bg-warn-pale text-warn')}>
            {t(statusLabel[entry.ledger.status] ?? 'channelArchived')}
          </span>
        {:else if entry.unread}
          <span
            class={cn('min-w-4.5 rounded-full px-1.5 text-center text-caption leading-[18px] font-semibold tabular-nums', entry.muted ? 'bg-wash text-muted' : 'bg-forest text-on-forest')}
            in:pop
            out:shrink
            data-testid="channel-unread"><Count value={entry.unread} max={99} /></span
          >
        {:else if entry.news && selected !== entry.id}
          <span
            class={cn('me-1 mt-1 size-2 rounded-full', entry.muted ? 'bg-faint' : 'bg-forest')}
            role="img" aria-label={t('unread')} data-testid="channel-news"></span>
        {/if}
      </span>
    </button>
{/snippet}

<div class="@container relative flex h-full min-h-0">
  {#if error === 'LOCKED'}
    <Locked {status} />
  {:else}
    <section class="flex w-[var(--pane,248px)] shrink-0 flex-col border-e border-line @min-[1100px]:w-[var(--pane,284px)]">
      <div class="flex items-center gap-1.5 p-3 pb-2">
        <label class="relative min-w-0 flex-1">
          <Icon name="search" class="pointer-events-none absolute start-2.5 top-1/2 -translate-y-1/2 text-faint" />
          <input bind:this={listInput} bind:value={query} type="search" class="ps-8" placeholder={t('searchChannels')} aria-label={t('searchChannels')} />
        </label>
        {#if unlocked}
          <DropdownMenu.Root dir={localeDir(locale())}>
            <Tooltip content={t('newChannel')}>
              {#snippet children(props)}
                <DropdownMenu.Trigger {...props} class={iconButtonClass('primary', 'md')} aria-label={t('newChannel')} data-testid="channel-new">
                  <Icon name="add" />
                </DropdownMenu.Trigger>
              {/snippet}
            </Tooltip>
            <DropdownMenu.Portal>
              <DropdownMenu.Content align="end" sideOffset={6} class={cn(menuContentClass, 'min-w-56')}>
                <DropdownMenu.Item class={menuItemClass} onSelect={() => ((createTab = 'create'), (creating = true))}>
                  <span class="flex items-center gap-2"><Icon name="add" class="text-faint" />{t('newChannel')}</span>
                </DropdownMenu.Item>
                <DropdownMenu.Item class={menuItemClass} onSelect={() => ((createTab = 'join'), (creating = true))}>
                  <span class="flex items-center gap-2"><Icon name="enter" class="text-faint" />{t('joinChannel')}</span>
                </DropdownMenu.Item>
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>
        {/if}
      </div>
      {#if anyUnread || onlyUnread}
        <div class="px-3 pb-1.5">
          <button class={buttonClass(onlyUnread ? 'secondary' : 'ghost', 'xs')} aria-pressed={onlyUnread} onclick={() => (onlyUnread = !onlyUnread)} data-testid="channel-unread-filter">{t('unread')}</button>
        </div>
      {/if}
      {#if error}<Notice failure={error} tone="danger" testid="channel-error">{error}</Notice>{/if}
      <TargetMenu find={channelOf} testid="channel-menu">
        {#snippet menu(entry)}{@render channelMenu(entry)}{/snippet}
      <ul data-list class="min-h-0 flex-1 overflow-y-auto px-2 pb-3" data-testid="channel-list">
        {#each listed.live as entry (entry.id)}
          <!-- A channel that moves up on news slides there; one that joins fades in. -->
          <li data-channel={entry.id} animate:flip={{ duration: flipDuration(listed.live.length) }} in:enter={listed.live.length}>{@render item(entry)}</li>
        {:else}
          {#if !listed.other.length && !oldChannels.length && !activeCrossMessages.length}
            <li>
              {#if status?.sync_initialized === false && unlocked}
                <Empty icon="channels" title={t(status?.syncing ? 'syncInitial' : 'syncInitialWaiting')} description={t('syncInitialHint')} testid="channels-initial-sync" class="px-4 py-10" />
              {:else}
                <Empty icon="channels" title={query.trim() ? t('noMatch', { query }) : onlyUnread ? t('noResults') : t('noChannels')} class="px-4 py-10">
                  {#snippet actions()}
                    {#if unlocked && !query.trim() && !onlyUnread}
                      <button class={buttonClass('primary', 'sm')} onclick={() => ((createTab = 'create'), (creating = true))}>{t('newChannel')}</button>
                    {/if}
                  {/snippet}
                </Empty>
              {/if}
            </li>
          {/if}
        {/each}
        {#if activeCrossMessages.length}
          <li class="px-2 pt-3 pb-1 text-caption font-medium text-faint" data-testid="channel-cross-messages-heading">{t('matchingMessages')}</li>
          {#each activeCrossMessages as group (group.channel_id)}
            {#each group.hits as hit (`${group.channel_id}:${hit.message.seq}`)}
              <li>
                <button
                  class="grid w-full gap-0.5 rounded-lg px-2 py-2 text-start outline-none transition-colors duration-100 hover:bg-wash focus-visible:ring-3 focus-visible:ring-focus/35"
                  onclick={() => pick(group.channel_id, hit.message.seq, hit.snippet_spans)}
                  data-testid="channel-cross-message"
                >
                  <span class="flex items-baseline justify-between gap-2">
                    <span class="truncate text-caption font-medium text-ink">{group.channel_name}</span>
                    <span class="shrink-0 text-caption text-faint tabular-nums">{shortDay.format(hit.message.stored_at)}</span>
                  </span>
                  <span class="truncate text-callout text-muted">
                    <span class="font-medium text-ink-2">{hit.message.account === me ? t('you') : (hit.sender_name || label(hit.message.account, false))}:</span>
                    <HighlightText {...messagePreview(hit)} />
                  </span>
                </button>
              </li>
            {/each}
          {/each}
        {/if}
        {#if listed.other.length}
          <li class="px-2 pt-3">
            <button class="flex w-full items-center gap-1 text-caption font-medium text-faint hover:text-ink" onclick={() => (showOther = !showOther)} data-testid="channel-other-toggle">
              <Icon name={showOther ? 'chevron-down' : 'chevron-end'} class="size-3.5" />{t('otherChannels', { n: listed.other.length })}
            </button>
          </li>
          {#if showOther}
            {#each listed.other as entry (entry.id)}
              <li data-channel={entry.id}>{@render item(entry)}</li>
            {/each}
          {/if}
        {/if}
        {#if oldChannels.length}
          <li class="px-2 pt-4 pb-1">
            <button class="flex w-full items-center gap-1 text-caption font-medium text-faint hover:text-ink" aria-expanded={showLegacy} onclick={() => (showLegacy = !showLegacy)} data-testid="legacy-channel-toggle">
              <Icon name={showLegacy ? 'chevron-down' : 'chevron-end'} class="size-3.5" />{t('legacyHistory')} · {oldChannels.length}
            </button>
          </li>
          {#if showLegacy}
            {#each oldChannels as entry (`${entry.archive}:${entry.source}`)}
              <li>
                <button class={cn('flex w-full items-center gap-2 rounded-lg px-2 py-2 text-start hover:bg-wash', legacySelected?.archive === entry.archive && legacySelected?.source === entry.source && 'bg-pale')} onclick={() => pickLegacy(entry)} data-testid="legacy-channel-item">
                  <Icon name="legacy" class="shrink-0 text-faint" />
                  <span class="min-w-0">
                    <b class="block truncate text-body font-medium">{entry.name || entry.source}</b>
                    <span class="block text-caption text-faint">{t('legacyReadable', { n: entry.messages })}{entry.unreadable ? ` · ${t('legacyUnreadable', { n: entry.unreadable })}` : ''}</span>
                  </span>
                </button>
              </li>
            {/each}
          {/if}
        {/if}
        {#if legacyError}<li class="px-2 py-2 text-callout text-danger">{legacyError}</li>{/if}
      </ul>
      </TargetMenu>
    </section>

    <PaneHandle name="channels" />
    <section class="@container/conversation flex min-w-0 flex-1 flex-col" use:arrive={channel?.id}>
      {#if legacySelected}
        {#key `${legacySelected.archive}:${legacySelected.source}`}
          <LegacyHistory entry={legacySelected} onDeleted={() => { legacySelected = null; void readLegacy() }} />
        {/key}
      {:else if channel}
        <header class="flex min-h-14 shrink-0 flex-wrap items-center gap-x-3 gap-y-1 border-b border-line px-5 py-2 @min-[420px]/conversation:flex-nowrap">
          <Avatar id={channel.id} name={channel.name} size="lg" person={ledger?.type === 'direct'} class="hidden @min-[420px]/conversation:grid" />
          <div class="min-w-0 flex-1 basis-full @min-[420px]/conversation:basis-auto">
            <div class="flex min-w-0 items-center gap-2">
              <h2 class="truncate text-title-3 leading-snug font-semibold" data-testid="channel-title">{channel.name}</h2>
              {#if channel.muted}<Icon name="eye-off" class="size-3.5 shrink-0 text-faint" />{/if}
            </div>
            {#if ledger}
              <p class="truncate text-callout text-muted">
                {t(kindLabel[ledger.type] ?? 'collaboration')} · {t('memberCount', { n: Object.keys(ledger.members).length })}{me && ledger.members[me]
                  ? ` · ${t(roleLabel[ledger.members[me].role] ?? 'roleMember')}`
                  : ''}{channel.billing?.billing_account && channel.billing.billing_account !== ledger.owner ? ` · ${t('paidBy', { name: label(channel.billing.billing_account) })}` : ''}
              </p>
            {/if}
          </div>
          <Tooltip content={t('searchMessages')}>
            {#snippet children(props)}
              <button
                {...props}
                class={iconButtonClass('ghost', 'md', cn(search.open && 'bg-wash text-ink'))}
                bind:this={searchButton}
                aria-label={t('searchMessages')}
                aria-expanded={search.open}
                aria-controls={search.open ? 'channel-search-panel' : undefined}
                onclick={() => showSearch(!search.open)}
                data-testid="channel-search-toggle"
              >
                <Icon name="search" />
              </button>
            {/snippet}
          </Tooltip>
          {#if ledger && !direct}
            <Tooltip content={t('members')}>
              {#snippet children(props)}
                <button
                  {...props}
                  class={iconButtonClass('ghost', 'md', cn(membersOpen && 'bg-wash text-ink'))}
                  aria-label={t('members')}
                  aria-pressed={membersOpen}
                  onclick={() => (membersOpen = !membersOpen)}
                  data-testid="channel-members-toggle"
                >
                  <Icon name="members" />
                </button>
              {/snippet}
            </Tooltip>
          {/if}
          {#if unlocked}
            <DropdownMenu.Root dir={localeDir(locale())}>
              <Tooltip content={t('manageChannel')}>
                {#snippet children(props)}
                  <DropdownMenu.Trigger {...props} class={iconButtonClass('ghost', 'md')} aria-label={t('manageChannel')} data-testid="channel-manage">
                    <Icon name="more" />
                  </DropdownMenu.Trigger>
                {/snippet}
              </Tooltip>
              <DropdownMenu.Portal>
                <DropdownMenu.Content align="end" sideOffset={6} class={cn(menuContentClass, 'min-w-60')} data-testid="channel-manage-menu">
                  {#if channel.deletable_legacy_draft}
                    <DropdownMenu.Item class={cn(menuItemClass, 'text-danger')} onSelect={() => (deletingLegacyDraft = true)} data-testid="legacy-draft-delete">
                      <span class="flex items-center gap-2"><Icon name="trash" />{t('legacyDeleteDraft')}</span>
                    </DropdownMenu.Item>
                  {/if}
                  <DropdownMenu.Item class={menuItemClass} onSelect={() => (renaming = { open: true, name: channel.name })} data-testid="channel-rename">
                    <span class="flex items-center gap-2"><Icon name="edit" class="text-faint" />{t('renameChannel')}…</span>
                  </DropdownMenu.Item>
                  <DropdownMenu.Sub>
                    <DropdownMenu.SubTrigger class={menuItemClass} data-testid="channel-mute-menu">
                      <span class="flex items-center gap-2"><Icon name="eye-off" class="text-faint" />{t('muteChannel')}</span>
                      <Icon name="chevron-end" class="size-3.5 text-faint" />
                    </DropdownMenu.SubTrigger>
                    <DropdownMenu.SubContent class={cn(menuContentClass, 'min-w-48')} sideOffset={4}>
                      <DropdownMenu.Item class={menuItemClass} onSelect={() => settings({ muted_until: Date.now() + 3_600_000 })} data-testid="channel-mute-hour">{t('noticePauseHour')}</DropdownMenu.Item>
                      <DropdownMenu.Item class={menuItemClass} onSelect={() => settings({ muted_until: tomorrowMorning(Date.now(), Intl.DateTimeFormat().resolvedOptions().timeZone) })} data-testid="channel-mute-tomorrow">{t('noticePauseTomorrow')}</DropdownMenu.Item>
                      <DropdownMenu.CheckboxItem checked={channel.muted} onCheckedChange={(on) => settings({ muted: on })} class={menuItemClass} data-testid="channel-mute">
                        {#snippet children({ checked })}
                          {t('muteAlways')}
                          {#if checked}<Icon name="check" class="size-3.5" />{/if}
                        {/snippet}
                      </DropdownMenu.CheckboxItem>
                    </DropdownMenu.SubContent>
                  </DropdownMenu.Sub>
                  {#if channel.notification_pending}
                    <DropdownMenu.Item class={menuItemClass} disabled={!channel.notification_conflict} onSelect={() => settings(channel.muted_until > Date.now() ? {muted_until:channel.muted_until} : {muted:channel.muted})}>
                      <span class="whitespace-normal">{channel.notification_conflict ? t('retry') : t('noticePending')}</span>
                    </DropdownMenu.Item>
                  {/if}
                  <DropdownMenu.Item class={menuItemClass} disabled={busy} onSelect={read} data-testid="channel-read">
                    <span class="flex items-center gap-2"><Icon name="sync-now" class="text-faint" />{t('read')}</span>
                  </DropdownMenu.Item>
                  {#if direct}
                    <DropdownMenu.Item class={menuItemClass} onSelect={() => (membersOpen = !membersOpen)} data-testid="channel-members-menu">
                      <span class="flex items-center gap-2"><Icon name="members" class="text-faint" />{t('members')}</span>
                    </DropdownMenu.Item>
                  {/if}
                  {#if can.member}
                    <DropdownMenu.Item class={menuItemClass} onSelect={() => (access = true)} data-testid="channel-access-open">
                      <span class="flex items-center gap-2"><Icon name="agents" class="text-faint" />{t('channelAccess')}…</span>
                    </DropdownMenu.Item>
                  {/if}
                  {#if can.rotates && ledger.status === 'active'}
                    <DropdownMenu.Item class={menuItemClass} onSelect={() => attempt(rekey)} data-testid="channel-rekey">
                      <span class="flex items-center gap-2"><Icon name="epoch" class="text-faint" />{t('rekey')}</span>
                    </DropdownMenu.Item>
                  {/if}
                  {#if can.owner}
                    <DropdownMenu.Separator class="my-1 h-px bg-line" />
                    <DropdownMenu.Item class={menuItemClass} onSelect={() => (transfer = { open: true, kind: 'owner' })} data-testid="owner-transfer">
                      <span class="flex items-center gap-2"><Icon name="owner" class="text-faint" />{t('transferOwner')}…</span>
                    </DropdownMenu.Item>
                    <DropdownMenu.Item class={menuItemClass} disabled={transferring} onSelect={() => (transfer = { open: true, kind: 'sponsor' })}>
                      <span class="flex items-center gap-2"><Icon name="coins" class="text-faint" />{t('transferSponsor')}…</span>
                    </DropdownMenu.Item>
                    {#if ledger.status !== 'archived'}
                      <DropdownMenu.Separator class="my-1 h-px bg-line" />
                      <DropdownMenu.Item class={cn(menuItemClass, 'text-danger')} onSelect={() => (archiving = true)} data-testid="channel-archive">
                        <span class="flex items-center gap-2"><Icon name="archive" />{t('archive')}…</span>
                      </DropdownMenu.Item>
                    {/if}
                  {:else if can.member}
                    <DropdownMenu.Separator class="my-1 h-px bg-line" />
                    <DropdownMenu.Item class={cn(menuItemClass, 'text-danger')} onSelect={() => (leaving = true)} data-testid="channel-leave">
                      <span class="flex items-center gap-2"><Icon name="leave" />{t('leave')}…</span>
                    </DropdownMenu.Item>
                  {/if}
                  <DropdownMenu.Item class={menuItemClass} onSelect={() => (settling = true)} data-testid="channel-packet-open">
                    <span class="flex items-center gap-2"><Icon name="transfer" class="text-faint" />{t('transferPacket')}…</span>
                  </DropdownMenu.Item>
                  {#if !can.member || ledger?.status === 'archived' || channel.hidden}
                    <DropdownMenu.Item class={menuItemClass} onSelect={() => settings({ hidden: !channel.hidden })} data-testid="channel-hide">
                      <span class="flex items-center gap-2"><Icon name="archive" class="text-faint" />{channel.hidden ? t('showInList') : t('hideFromList')}</span>
                    </DropdownMenu.Item>
                  {/if}
                </DropdownMenu.Content>
              </DropdownMenu.Portal>
            </DropdownMenu.Root>
          {/if}
        </header>

        {#if search.open}
          <div id="channel-search-panel" class="border-b border-line px-5 py-2" data-testid="channel-search">
            <div class="flex flex-wrap items-center gap-1.5">
              <input
                bind:this={searchInput}
                bind:value={search.query}
                type="search"
                class="min-w-0 flex-1"
                placeholder={t('searchMessages')}
                aria-label={t('searchMessages')}
                onkeydown={(e) => {
                  if (e.isComposing) return
                  if (e.key === 'Enter') {
                    e.preventDefault()
                    stepSearch(e.shiftKey ? -1 : 1)
                  }
                }}
                data-testid="channel-search-query"
              />
              <button
                class={buttonClass(search.hasFile ? 'secondary' : 'ghost', 'sm')}
                aria-pressed={search.hasFile}
                onclick={() => (search.hasFile = !search.hasFile)}
                data-testid="channel-search-files"
              >
                <Icon name="attach" />{t('attachments')}
              </button>
              {#if search.results && search.results.length > 0}
                <span class="px-1 text-caption text-faint tabular-nums" data-testid="channel-search-count">
                  {search.activeIndex >= 0 ? `${search.activeIndex + 1} / ` : ''}{search.results.length}{search.nextBefore ? '+' : ''}
                </span>
                <button class={iconButtonClass('ghost', 'md')} aria-label={t('searchPrevMatch')} onclick={() => stepSearch(-1)} data-testid="channel-search-prev">
                  <Icon name="chevron-down" class="rotate-180" />
                </button>
                <button class={iconButtonClass('ghost', 'md')} aria-label={t('searchNextMatch')} onclick={() => stepSearch(1)} data-testid="channel-search-next">
                  <Icon name="chevron-down" />
                </button>
              {/if}
              <button class={iconButtonClass('secondary', 'md')} aria-label={t('searchMessages')} disabled={(!search.query.trim() && !search.hasFile) || search.busy} onclick={() => find(false)} data-testid="channel-search-submit"><Icon name="search" /></button>
              <button class={iconButtonClass('ghost', 'md')} aria-label={t('close')} onclick={() => showSearch(false)} data-testid="channel-search-close"><Icon name="close" /></button>
            </div>
            {#if search.busy}<p class="mt-1.5 text-callout text-muted" role="status">{t('loading')}</p>{/if}
            {#if search.error}<Notice tone="danger" failure={search.error}>{search.error}</Notice>{/if}
            {#if search.results}
              <ul class="mt-1.5 max-h-56 overflow-y-auto" data-testid="channel-search-results">
                {#each search.results as hit, idx (hit.message.seq)}
                  <li>
                    <button
                      class={cn('flex w-full items-baseline gap-2 rounded-md px-2 py-1 text-start text-callout hover:bg-wash', search.activeIndex === idx && 'bg-pale')}
                      onclick={() => jump(hit.message.seq, hit.snippet_spans, idx)}
                    >
                      <span {@attach visibleName(hit.message.account === me ? undefined : hit.message.account)} class="shrink-0 font-medium">
                        {hit.message.account === me ? t('you') : (hit.sender_name || label(hit.message.account, false))}
                      </span>
                      <span class="min-w-0 flex-1 truncate text-muted">
                        <HighlightText {...messagePreview(hit)} />
                      </span>
                      <span class="shrink-0 text-caption text-faint tabular-nums">{shortDay.format(hit.message.stored_at)}</span>
                    </button>
                  </li>
                {:else}
                  <li class="px-2 py-1.5 text-callout text-muted">{search.query.trim() ? t('noMatch', { query: search.query }) : t('noResults')}</li>
                {/each}
                {#if search.nextBefore}
                  <li class="pt-1 text-center">
                    <button class={buttonClass('ghost', 'xs')} disabled={search.loadingMore} onclick={() => find(true)} data-testid="channel-search-more">
                      {search.loadingMore ? t('loading') : t('loadMore')}
                    </button>
                  </li>
                {/if}
              </ul>
            {/if}
          </div>
        {/if}

        {#if transferring}
          <div class="flex items-center gap-2.5 border-b border-warn/30 bg-warn-pale px-5 py-2 text-callout text-ink-2" data-testid="channel-transferring">
            <Icon name="coins" class="shrink-0 text-warn" />
            <span class="min-w-0 flex-1">{t('sponsorUnderway', { name: label(channel.billing?.transition?.to_payer) })}</span>
            {#if can.owner && unlocked}<button class={buttonClass('subtle', 'xs')} disabled={busy} onclick={cancelSponsor} data-testid="sponsor-cancel">{t('cancelTransfer')}</button>{/if}
          </div>
        {/if}
        {#if ledger?.status === 'rotation_required'}
          <div class="flex items-center gap-2.5 border-b border-warn/30 bg-warn-pale px-5 py-2 text-callout text-ink-2">
            <Icon name="epoch" class="shrink-0 text-warn" />
            <span class="min-w-0 flex-1" data-testid="channel-rotation">{t(can.rotates ? 'rotationRequired' : 'rotationWaiting')}</span>
            {#if unlocked && can.rotates}<button class={buttonClass('subtle', 'xs')} disabled={busy} onclick={rotate}>{t('rotate')}</button>{/if}
          </div>
        {:else if ledger?.status === 'archived'}
          <div class="flex items-center gap-2.5 border-b border-line bg-surface-2 px-5 py-2 text-callout text-muted">
            <Icon name="archive" class="shrink-0" />
            <span class="min-w-0 flex-1">{t('archivedChannel')}</span>
            {#if can.owner && unlocked}
              <button class={buttonClass('subtle', 'xs')} disabled={busy} onclick={reactivate} data-testid="channel-reactivate">
                <Icon name="unarchive" />{t('reactivate')}
              </button>
            {/if}
          </div>
        {:else if ledger && !can.member}
          <div class="flex items-center gap-2.5 border-b border-line bg-surface-2 px-5 py-2 text-callout text-muted">
            <Icon name="leave" class="shrink-0" /><span class="min-w-0 flex-1">{t('notMember')}</span>
            {#if unlocked && !channel.hidden}<button class={buttonClass('subtle', 'xs')} onclick={() => settings({ hidden: true })}>{t('hideFromList')}</button>{/if}
          </div>
        {/if}

        {#key channel.id}
        <Thread
          {channel}
          messages={shownMessages}
          outbox={shownOutbox}
          {files}
          {me}
          {receipts}
          {more}
          {moreAfter}
          {loadingEarlier}
          {loadingNewer}
          {highlight}
          {highlightSpans}
          {images}
          writes={unlocked && can.writes}
          bind:draft={() => drafts[channel.id] ?? '', (text) => (drafts[channel.id] = text)}
          bind:attachments={() => attached[channel.id] ?? [], (list) => (attached[channel.id] = list)}
          bind:reply={() => replies[channel.id] ?? null, (message) => (replies[channel.id] = message)}
          bind:editing
          history={reading}
          onLatest={latest}
          onNewer={loadNewer}
          onSend={send}
          onRetry={retry}
          onDiscard={discard}
          onImport={() => addFiles((folder) => window.dmsg.filesAdd(folder, undefined, false, true))}
          onAttach={attachments}
          onBrowse={() => (picking = true)}
          onDrop={(dropped) => addFiles((folder) => window.dmsg.filesAdd(folder, dropped, false, true))}
          onPaste={(pasted) => addFiles((folder) => window.dmsg.filesPaste(folder, pasted))}
          onDownload={download}
          onSave={save}
          onSnapshot={receiveSnapshot}
          onEarlier={earlier}
          onRetract={(message) => (retracting = message)}
          onHide={hide}
          onPreview={preview}
          onOpenAttachment={openAttachmentPreview}
          onPacket={settlePacket}
          onRead={markRead}
        />
        {/key}
      {:else}
        <div class="grid flex-1 place-items-center p-8 text-center text-body text-faint">
          <p>{channels.length ? t('pickChannel') : ''}</p>
        </div>
      {/if}
    </section>

    {#if channel && ledger && membersOpen}
      <aside class="absolute inset-y-0 end-0 z-10 w-[256px] shrink-0 border-s border-line bg-paper shadow-pop @min-[860px]:static @min-[860px]:shadow-none @min-[1100px]:w-[280px]">
        <Members
          {channel}
          {ledger}
          {me}
          owner={unlocked && can.owner}
          manages={unlocked && can.manages}
          rotates={unlocked && can.rotates}
          {busy}
          onControl={control}
          onHistory={openHistory}
          onInvite={invite}
          onClose={() => (membersOpen = false)}
        />
      </aside>
    {/if}
  {/if}
</div>

<NewChannel bind:open={creating} bind:tab={createTab} bind:invitation={joinText} bind:peer onOpened={opened} onDirectInvited={invitedForTransfer} />
<Packet bind:open={settling} />
<LibraryPicker
  bind:open={picking}
  onPick={(list) => channel && (attached[channel.id] = [...(attached[channel.id] ?? []), ...list.filter((f) => !(attached[channel.id] ?? []).some((a) => a.id === f.id))])}
/>
{#if channel}
  <Transfer bind:open={transfer.open} kind={transfer.kind} {channel} {me} onSent={reload} />
  <Access bind:open={access} {channel} manages={unlocked && can.manages} onControl={control} />
  <Dialog bind:open={leaving} title={t('leaveTitle', { name: channel.name })} description={t('leaveHint')} icon="leave" tone="danger" testid="channel-leave-dialog">
    <p class="text-body text-muted">{t('leaveBody')}</p>
    {#snippet footer()}
      <span class="flex-1"></span>
      <button class={buttonClass('ghost')} onclick={() => (leaving = false)}>{t('cancel')}</button>
      <button class={buttonClass('danger')} disabled={busy} onclick={leave} data-testid="channel-leave-confirm">{t('leave')}</button>
    {/snippet}
  </Dialog>
  <Dialog bind:open={archiving} title={t('archiveTitle', { name: channel.name })} description={t('archiveHint')} icon="archive" tone="danger" testid="channel-archive-dialog">
    <p class="text-body text-muted">{t('archiveBody')}</p>
    {#snippet footer()}
      <span class="flex-1"></span>
      <button class={buttonClass('ghost')} onclick={() => (archiving = false)}>{t('cancel')}</button>
      <button
        class={buttonClass('danger')}
        disabled={busy}
        onclick={() => {
          archiving = false
          void control({ type: 'archive' })
        }}
        data-testid="channel-archive-confirm">{t('archive')}</button
      >
    {/snippet}
  </Dialog>
  <Dialog bind:open={renaming.open} title={t('renameChannel')} description={t('renameHint')} icon="edit" testid="channel-rename-dialog">
    <TextField bind:value={renaming.name} label={t('channelName')} hideLabel class="text-body" data-testid="channel-rename-input" />
    {#snippet footer()}
      <span class="flex-1"></span>
      <button class={buttonClass('ghost')} onclick={() => (renaming.open = false)}>{t('cancel')}</button>
      <button
        class={buttonClass('primary')}
        disabled={busy || !renaming.name.trim()}
        onclick={() => {
          renaming.open = false
          void settings({ name: renaming.name.trim() })
        }}
        data-testid="channel-rename-save">{t('save')}</button
      >
    {/snippet}
  </Dialog>
{/if}
{#if retracting}
  <Dialog bind:open={() => !!retracting, (open) => !open && (retracting = null)} title={t('retractTitle')} description={t('retractHint')} icon="restore" tone="danger" testid="message-retract-dialog">
    <p class="line-clamp-3 text-body text-muted">{retracting.text || retracting.file?.name}</p>
    {#snippet footer()}
      <span class="flex-1"></span>
      <button class={buttonClass('ghost')} onclick={() => (retracting = null)}>{t('cancel')}</button>
      <button class={buttonClass('danger')} disabled={busy} onclick={retract} data-testid="message-retract-confirm">{t('retract')}</button>
    {/snippet}
  </Dialog>
{/if}
<Dialog bind:open={deletingLegacyDraft} title={t('legacyDeleteDraft')} icon="trash" tone="danger" testid="legacy-draft-delete-dialog">
  <p class="text-body text-muted">{t('legacyDeleteDraftHint', { name: channel?.name ?? '' })}</p>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} disabled={busy} onclick={() => (deletingLegacyDraft = false)}>{t('cancel')}</button>
    <button class={buttonClass('danger')} disabled={busy} onclick={deleteLegacyDraft} data-testid="legacy-draft-delete-confirm">{t('remove')}</button>
  {/snippet}
</Dialog>
{#if history.member}
  <Dialog bind:open={history.open} title={t('grantHistory')} description={t('historyHint')} icon="legacy" testid="history-dialog">
    <div class="flex items-end gap-2" data-testid="history-form">
      <Field label={t('fromEpoch')} class="flex-1">
        <input type="number" min="1" max={history.member.joined_epoch - 1} bind:value={history.from} class="text-ink tabular-nums" />
      </Field>
      <span class="pb-1.5 text-faint">–</span>
      <Field label={t('toEpoch')} class="flex-1">
        <input type="number" min="1" max={history.member.joined_epoch - 1} bind:value={history.to} class="text-ink tabular-nums" />
      </Field>
    </div>
    {#snippet footer()}
      <span class="flex-1"></span>
      <button class={buttonClass('ghost')} onclick={() => (history.open = false)}>{t('cancel')}</button>
      <button
        class={buttonClass('primary')}
        disabled={busy || !(history.from >= 1 && history.to >= history.from && history.to < history.member.joined_epoch)}
        onclick={grantHistory}
        data-testid="history-submit"
      >
        {t('grantHistory')}
      </button>
    {/snippet}
  </Dialog>
{/if}

<Dialog bind:open={snapshotOpen} title={workflowText('channelSnapshot')} description={workflowText('channelWarning')}>
  {#if snapshotError}<p class="text-danger">{snapshotError}</p>{/if}
  {#if snapshot}{#key snapshot.package_id}<PackageImport received={snapshot} />{/key}{/if}
</Dialog>

<FileLightbox
  bind:open={lightboxOpen}
  items={lightboxItems}
  bind:activeKey={lightboxKey}
  onDownload={(item) => {
    if (item.source.source === 'channel') void download({ seq: item.source.seq, file: { name: item.name } })
  }}
  onSave={(item) => {
    if (item.source.source === 'channel') void save({ seq: item.source.seq, file: { name: item.name } })
  }}
/>
