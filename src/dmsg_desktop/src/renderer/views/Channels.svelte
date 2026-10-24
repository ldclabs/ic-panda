<script lang="ts">
  import Notice from '../components/page/Notice.svelte'
  import { DropdownMenu } from 'bits-ui'
  import { onMount } from 'svelte'
  import { localeDir } from '../../shared/locales'
  import Avatar from '../components/Avatar.svelte'
  import Dialog from '../components/Dialog.svelte'
  import Icon from '../components/Icon.svelte'
  import Locked from '../components/page/Locked.svelte'
  import { toast } from '../components/toast.svelte'
  import Tooltip from '../components/Tooltip.svelte'
  import { iconButtonClass, buttonClass, cn, menuContentClass, menuItemClass } from '../components/ui'
  import { locale, t } from '../i18n/index.svelte'
  import { bridge, call, closed, code, errorText, onChange, planAction, single } from '../rpc'
  import Access from './channels/Access.svelte'
  import { arrange, kindLabel, powers, roleLabel, statusLabel } from './channels/channels'
  import LibraryPicker from './channels/LibraryPicker.svelte'
  import LegacyHistory from './channels/LegacyHistory.svelte'
  import Members from './channels/Members.svelte'
  import { accountOf, label } from './channels/names.svelte'
  import NewChannel from './channels/NewChannel.svelte'
  import Packet from './channels/Packet.svelte'
  import Thread, { type Outgoing } from './channels/Thread.svelte'
  import Transfer from './channels/Transfer.svelte'
  import PackageImport from './secrets/PackageImport.svelte'
  import { workflowText } from './secrets/workflow-text'

  /**
   * Channels: the list, latest activity first with what is unread, the open
   * channel's timeline and composer, and its members in a drawer. Creating,
   * joining, transfers, history grants, access and confirmations open in
   * dialogs; a `dmsg://` invitation link opens the join dialog with it, and a
   * profile or a letter opens a conversation with its person. Each change
   * comes back through the daemon's `channel/changed`.
   */
  let { status, route, go }: { status: any; route: string; go: (path: string) => void } = $props()

  let channels = $state<any[]>([])
  let legacyChannels = $state<any[]>([])
  let legacySelected = $state<any>(null)
  let legacyError = $state('')
  let legacyRequested: string | null = null
  let selected = $state<string | null>(null)
  let channel = $state<any>(null)
  let messages = $state<any[]>([])
  /** Earlier messages exist above the first one shown. */
  let more = $state(false)
  let files = $state<any[]>([])
  let query = $state('')
  let busy = $state(false)
  let error = $state('')
  /** Unsent text and attachments per channel; the text is kept by the daemon too. */
  let drafts = $state<Record<string, string>>({})
  let attached = $state<Record<string, { id: string; name: string }[]>>({})
  let reply = $state<any>(null)
  let editing = $state<any>(null)
  /** The daemon's outbox of the open channel, the messages this window sends now, and why the last try failed. */
  let outbox = $state<Outgoing[]>([])
  let sending = $state<Outgoing[]>([])
  let failures = $state<Record<string, string>>({})
  let receipts = $state<any>(null)
  let images = $state<Record<number, string>>({})
  let search = $state<{ open: boolean; query: string; results: any[] | null }>({ open: false, query: '', results: null })
  let highlight = $state<number | null>(null)
  let membersOpen = $state(stored('dmsg.members') !== 'closed')
  let showOther = $state(stored('dmsg.channels-other') === 'open')
  let showLegacy = $state(false)
  let readReceipts = $state(stored('dmsg.read-receipts') === 'on')
  let creating = $state(false)
  let createTab = $state<'create' | 'join'>('create')
  let joinText = $state('')
  let peer = $state<{ account: string; name?: string } | null>(null)
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
  const listed = $derived(arrange(channels.filter((c) => !query.trim() || c.name.toLowerCase().includes(query.trim().toLowerCase())), me))
  const oldChannels = $derived(legacyChannels.filter((c) => !query.trim() || `${c.name} ${c.source}`.toLowerCase().includes(query.trim().toLowerCase())))
  const shownOutbox = $derived([
    ...sending,
    ...outbox.filter((o) => !sending.some((s) => s.message_id === o.message_id)).map((o) => ({ ...o, error: failures[o.message_id] }))
  ])
  const transferring = $derived(channel?.billing?.lifecycle === 'transferring')
  const shortTime = $derived(new Intl.DateTimeFormat(locale(), { hour: '2-digit', minute: '2-digit' }))
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
  $effect(() => store('dmsg.read-receipts', readReceipts ? 'on' : 'off'))

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
      if (selected && !channels.some((c) => c.id === selected)) selected = channel = null
      if (selected) await open(selected)
    } catch (e) {
      channels = []
      error = closed(e, status) ? 'LOCKED' : errorText(e)
    }
  })

  function pickLegacy(entry: any) {
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
  async function open(id: string) {
    legacySelected = null
    const switching = channel?.id !== id
    selected = id
    const from = switching ? null : Math.min((messages.at(-PAGE)?.seq ?? 1) - 1, channel.message_cursor)
    const next: any = await call('channel/show', { id })
    const [list, box] = await Promise.all([
      from === null
        ? call<any[]>('channel/messages', { id, before: Number.MAX_SAFE_INTEGER, limit: PAGE })
        : call<any[]>('channel/messages', { id, after: from, limit: 2 * PAGE }),
      call<Outgoing[]>('channel/outbox', { id }).catch(() => [])
    ])
    if (selected !== id) return
    if (switching) {
      messages = list
      more = list.length === PAGE
      reply = editing = null
      images = {}
      highlight = null
      search = { open: false, query: '', results: null }
      drafts[id] ??= next.draft ?? ''
      marked = 0
      void loadReceipts(id)
      // Show local history immediately, then catch up this channel without
      // waiting for its place in the background pass.
      if (next.ledger?.members?.[me ?? '']) void call('channel/sync', { id }).catch(fail)
    } else {
      // A local send can be acknowledged before earlier incoming messages
      // arrive. Merge by sequence so those gaps appear in the right place.
      const changed = new Map([...messages, ...list].map((m: any) => [m.seq, m]))
      messages = [...changed.values()].sort((a, b) => a.seq - b.seq)
    }
    channel = next
    outbox = box
  }

  let receiptsAt = 0
  async function loadReceipts(id: string) {
    receiptsAt = Date.now()
    const loaded = await call('channel/receipts', { id }).catch(() => null)
    if (selected === id) receipts = loaded
  }

  const pick = (id: string) => attempt(() => open(id))

  /** The library's latest files, read when the attachment menu opens. */
  async function attachments() {
    const all = await call<any[]>('files/search', { query: '' }).catch(() => [])
    files = all.filter((f) => f.status !== 'cloud_only' && f.status !== 'too_large').slice(0, 8)
  }

  const read = () => attempt(() => call('channel/sync', { id: channel.id }))

  // The composer's text outlives the window: the daemon keeps it a moment after typing stops.
  $effect(() => {
    const id = channel?.id
    if (!id) return
    const text = drafts[id] ?? ''
    if (text === (channel.draft ?? '')) return
    const timer = setTimeout(() => void call('channel/draft', { id, text }).then(() => channel?.id === id && (channel.draft = text)), 600)
    return () => clearTimeout(timer)
  })

  /** Send one message; it stays in the daemon's outbox until the relay stores it. */
  async function deliver(id: string, item: Outgoing, params: Record<string, unknown>, file?: { id: string; name: string }) {
    sending = [...sending, { ...item, sending: true }]
    try {
      if (file) {
        const [source, version] = file.id.split('@')
        await call('channel/send_file', { id, file: source, version, message_id: item.message_id, ...params })
      } else await call('channel/send', { id, message_id: item.message_id, ...params })
      delete failures[item.message_id]
    } catch (e) {
      failures[item.message_id] = code(e)
    } finally {
      sending = sending.filter((o) => o.message_id !== item.message_id)
      await reload()
    }
  }

  async function send() {
    const id = channel.id
    const text = (drafts[id] ?? '').trim()
    const files = attached[id] ?? []
    if (editing) {
      const target = editing
      editing = null
      drafts[id] = ''
      if (text && text !== target.text) await deliver(id, { message_id: random(), draft: { text } }, { text, edit_of: ref(target) })
      return
    }
    if (!text && !files.length) return
    const replyTo = reply ? ref(reply) : null
    drafts[id] = ''
    attached[id] = []
    reply = null
    // A text goes with the first file; each further file goes in a message of its own.
    if (!files.length) await deliver(id, { message_id: random(), draft: { text } }, { text, reply_to: replyTo })
    for (const [i, file] of files.entries()) {
      const words = i === 0 ? text : ''
      await deliver(id, { message_id: random(), draft: { text: words }, file: { name: file.name, size: 0 } }, { text: words, reply_to: i === 0 ? replyTo : null }, file)
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
      delete failures[item.message_id]
      await reload()
    })

  /** New files enter the library through the system dialog or a drop, then go with the next message. */
  async function addFiles(dropped?: File[]) {
    const id = channel.id
    await attempt(async () => {
      const added: any = await bridge(() => window.dmsg.filesAdd('root', dropped))
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
    images[message.seq] = ''
    const result: any = await call('channel/preview', { id: channel.id, seq: message.seq }).catch(() => null)
    if (result?.url) images[message.seq] = result.url
  }

  const hide = (message: any, hidden: boolean) =>
    attempt(async () => {
      const changed = await call('channel/hide', { id: channel.id, seq: message.seq, hidden })
      messages = messages.map((m) => (m.seq === message.seq ? changed : m))
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
  /** The messages shown through `seq` are read; a read receipt goes only when chosen (C04). */
  function markRead(seq: number) {
    seq = Math.min(seq, channel?.message_cursor ?? 0)
    if (!channel || seq <= Math.max(channel.read ?? 0, marked)) return
    marked = seq
    const id = channel.id
    void call<any>('channel/mark', { id, through: seq, receipt: readReceipts }).then((view) => {
      channels = channels.map((c) => (c.id === id ? { ...c, read: view.read, unread: view.unread } : c))
      if (channel?.id === id) channel = { ...channel, read: view.read, unread: view.unread }
    })
  }

  async function earlier() {
    const first = messages[0]?.seq
    if (!first) return
    const list = await call<any[]>('channel/messages', { id: channel.id, before: first, limit: PAGE })
    messages = [...list, ...messages]
    more = list.length === PAGE
  }

  async function find() {
    const asked = search.query.trim()
    if (!asked) return
    const results = await call<any[]>('channel/search', { id: channel.id, query: asked }).catch((e) => {
      fail(e)
      return []
    })
    if (search.query.trim() === asked) search.results = results
  }

  /** A search result opens with the messages around it. */
  async function jump(seq: number) {
    const list = await call<any[]>('channel/messages', { id: channel.id, after: Math.max(0, seq - 30), limit: PAGE })
    messages = list
    more = seq > 30
    highlight = seq
    search = { ...search, open: false }
  }

  const settings = (change: Record<string, unknown>) => attempt(() => call('channel/settings', { id: channel.id, ...change }))
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
      peer = { account: direct, name: params.get('name') ?? label(direct) }
      creating = true
    } else if (target) void pick(target)
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
        if (channel && Date.now() - receiptsAt > 10_000) void loadReceipts(channel.id)
      }),
      onChange(['legacy/changed', 'session/changed', 'sync/changed'], readLegacy)
    ]
    return () => offs.forEach((off) => off())
  })
</script>

{#snippet item(entry: any)}
  {@const draft = (drafts[entry.id] ?? entry.draft)?.trim()}
  {@const at = entry.last?.stored_at}
  <li>
    <button
      class={cn(
        'grid w-full grid-cols-[auto_1fr_auto] items-center gap-2.5 rounded-lg px-2 py-2 text-start outline-none transition-colors duration-100 focus-visible:ring-3 focus-visible:ring-focus/35',
        selected === entry.id ? 'bg-pale' : 'hover:bg-wash'
      )}
      aria-current={selected === entry.id ? 'true' : undefined}
      onclick={() => pick(entry.id)}
      data-testid="channel-item"
    >
      <Avatar id={entry.id} name={entry.name} />
      <span class="min-w-0">
        <span class="flex min-w-0 items-center gap-1">
          <span class={cn('truncate text-[13.5px] leading-snug', entry.unread && !entry.muted ? 'font-semibold' : 'font-medium')}>{entry.name}</span>
          {#if entry.muted}<Icon name="eye-off" class="size-3 shrink-0 text-faint" />{/if}
        </span>
        {#if draft}
          <span class="block truncate text-[12px] leading-snug text-warn">{t('draft')}: {draft}</span>
        {:else if entry.last}
          <span class="block truncate text-[12px] leading-snug text-faint" data-testid="channel-preview">
            {entry.last.account === me ? t('you') : label(entry.last.account)}: {entry.last.text || '…'}
          </span>
        {:else}
          <span class="block truncate text-[12px] leading-snug text-faint">
            {entry.ledger ? `${t(kindLabel[entry.ledger.type] ?? 'collaboration')} · ${t('memberCount', { n: Object.keys(entry.ledger.members).length })}` : '…'}
          </span>
        {/if}
      </span>
      <span class="grid justify-items-end gap-0.5">
        {#if at}<span class="text-[10.5px] text-faint tabular-nums">{Date.now() - at < 86_400_000 ? shortTime.format(at) : shortDay.format(at)}</span>{/if}
        {#if entry.ledger && entry.ledger.status !== 'active'}
          <span class={cn('rounded px-1 text-[10.5px] font-medium', entry.ledger.status === 'archived' ? 'bg-wash text-muted' : 'bg-warn-pale text-warn')}>
            {t(statusLabel[entry.ledger.status] ?? 'channelArchived')}
          </span>
        {:else if entry.unread}
          <span
            class={cn('min-w-4.5 rounded-full px-1.5 text-center text-[10.5px] leading-[18px] font-semibold tabular-nums', entry.muted ? 'bg-wash text-muted' : 'bg-forest text-on-forest')}
            data-testid="channel-unread">{entry.unread > 99 ? '99+' : entry.unread}</span
          >
        {/if}
      </span>
    </button>
  </li>
{/snippet}

<div class="@container relative flex h-full min-h-0">
  {#if error === 'LOCKED'}
    <Locked {status} />
  {:else}
    <section class="flex w-[248px] shrink-0 flex-col border-e border-line @min-[1100px]:w-[284px]">
      <div class="flex items-center gap-1.5 p-3 pb-2">
        <label class="relative min-w-0 flex-1">
          <Icon name="search" class="pointer-events-none absolute start-2.5 top-1/2 -translate-y-1/2 text-faint" />
          <input bind:value={query} type="search" class="ps-8" placeholder={t('searchChannels')} aria-label={t('searchChannels')} />
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
                <DropdownMenu.Separator class="my-1 h-px bg-line" />
                <DropdownMenu.Item class={menuItemClass} onSelect={() => (settling = true)} data-testid="channel-packet-open">
                  <span class="flex items-center gap-2"><Icon name="transfer" class="text-faint" />{t('transferPacket')}…</span>
                </DropdownMenu.Item>
              </DropdownMenu.Content>
            </DropdownMenu.Portal>
          </DropdownMenu.Root>
        {/if}
      </div>
      {#if error}<Notice failure={error} tone="danger" testid="channel-error">{error}</Notice>{/if}
      <ul class="min-h-0 flex-1 overflow-y-auto px-2 pb-3" data-testid="channel-list">
        {#each listed.live as entry (entry.id)}
          {@render item(entry)}
        {:else}
          {#if !listed.other.length && !oldChannels.length}
            <li class="grid justify-items-center gap-2 px-4 py-10 text-center">
              <span class="grid size-11 place-items-center text-faint [&_svg]:size-6"><Icon name="channels" /></span>
              <p class="text-[13px] text-muted">{query ? t('noMatch', { query }) : t('noChannels')}</p>
              {#if unlocked && !query}
                <button class={buttonClass('secondary', 'sm')} onclick={() => ((createTab = 'create'), (creating = true))}>{t('newChannel')}</button>
              {/if}
            </li>
          {/if}
        {/each}
        {#if listed.other.length}
          <li class="px-2 pt-3">
            <button class="flex w-full items-center gap-1 text-[11.5px] font-medium text-faint hover:text-ink" onclick={() => (showOther = !showOther)} data-testid="channel-other-toggle">
              <Icon name={showOther ? 'chevron-down' : 'chevron-end'} class="size-3.5" />{t('otherChannels', { n: listed.other.length })}
            </button>
          </li>
          {#if showOther}
            {#each listed.other as entry (entry.id)}
              {@render item(entry)}
            {/each}
          {/if}
        {/if}
        {#if oldChannels.length}
          <li class="px-2 pt-4 pb-1">
            <button class="flex w-full items-center gap-1 text-[11.5px] font-medium text-faint hover:text-ink" aria-expanded={showLegacy} onclick={() => (showLegacy = !showLegacy)} data-testid="legacy-channel-toggle">
              <Icon name={showLegacy ? 'chevron-down' : 'chevron-end'} class="size-3.5" />{t('legacyHistory')} · {oldChannels.length}
            </button>
          </li>
          {#if showLegacy}
            {#each oldChannels as entry (`${entry.archive}:${entry.source}`)}
              <li>
                <button class={cn('flex w-full items-center gap-2 rounded-lg px-2 py-2 text-start hover:bg-wash', legacySelected?.archive === entry.archive && legacySelected?.source === entry.source && 'bg-pale')} onclick={() => pickLegacy(entry)} data-testid="legacy-channel-item">
                  <Icon name="legacy" class="shrink-0 text-faint" />
                  <span class="min-w-0">
                    <b class="block truncate text-[13px] font-medium">{entry.name || entry.source}</b>
                    <span class="block text-[11px] text-faint">{t('legacyReadable', { n: entry.messages })}{entry.unreadable ? ` · ${t('legacyUnreadable', { n: entry.unreadable })}` : ''}</span>
                  </span>
                </button>
              </li>
            {/each}
          {/if}
        {/if}
        {#if legacyError}<li class="px-2 py-2 text-[12px] text-danger">{legacyError}</li>{/if}
      </ul>
    </section>

    <section class="flex min-w-0 flex-1 flex-col">
      {#if legacySelected}
        {#key `${legacySelected.archive}:${legacySelected.source}`}
          <LegacyHistory entry={legacySelected} onDeleted={() => { legacySelected = null; void readLegacy() }} />
        {/key}
      {:else if channel}
        <header class="flex min-h-14 shrink-0 items-center gap-3 border-b border-line px-5 py-2">
          <Avatar id={channel.id} name={channel.name} size="lg" />
          <div class="min-w-0 flex-1">
            <div class="flex min-w-0 items-center gap-2">
              <h2 class="truncate text-[15px] leading-snug font-semibold" data-testid="channel-title">{channel.name}</h2>
              {#if ledger}
                <span class="inline-flex shrink-0 items-center gap-1 rounded-md border border-line bg-surface px-1.5 text-[11px] leading-5 text-muted" title={t('epochN', { n: ledger.epoch })}>
                  <Icon name="epoch" class="size-3" />{ledger.epoch}
                </span>
              {/if}
              {#if channel.muted}<Icon name="eye-off" class="size-3.5 shrink-0 text-faint" />{/if}
            </div>
            {#if ledger}
              <p class="truncate text-[12px] text-faint">
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
                aria-label={t('searchMessages')}
                aria-pressed={search.open}
                onclick={() => (search = { ...search, open: !search.open })}
                data-testid="channel-search-toggle"
              >
                <Icon name="search" />
              </button>
            {/snippet}
          </Tooltip>
          <Tooltip content={t('read')}>
            {#snippet children(props)}
              <button {...props} class={iconButtonClass('ghost', 'md')} aria-label={t('read')} disabled={busy} onclick={read} data-testid="channel-read">
                <Icon name="sync-now" />
              </button>
            {/snippet}
          </Tooltip>
          {#if ledger}
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
                  <DropdownMenu.CheckboxItem checked={channel.muted} onCheckedChange={(on) => settings({ muted: on })} class={menuItemClass} data-testid="channel-mute">
                    {#snippet children({ checked })}
                      <span class="flex items-center gap-2"><Icon name="eye-off" class="text-faint" />{t('muteChannel')}</span>
                      {#if checked}<Icon name="check" class="size-3.5" />{/if}
                    {/snippet}
                  </DropdownMenu.CheckboxItem>
                  <DropdownMenu.CheckboxItem checked={readReceipts} onCheckedChange={(on) => (readReceipts = on)} class={menuItemClass} data-testid="channel-receipts">
                    {#snippet children({ checked })}
                      <span class="flex items-center gap-2"><Icon name="delivered" class="text-faint" />{t('sendReadReceipts')}</span>
                      {#if checked}<Icon name="check" class="size-3.5" />{/if}
                    {/snippet}
                  </DropdownMenu.CheckboxItem>
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
          <div class="border-b border-line px-5 py-2" data-testid="channel-search">
            <label class="relative block">
              <Icon name="search" class="pointer-events-none absolute start-2.5 top-1/2 -translate-y-1/2 text-faint" />
              <input
                bind:value={search.query}
                type="search"
                class="ps-8"
                placeholder={t('searchMessages')}
                onkeydown={(e) => e.key === 'Enter' && find()}
                data-testid="channel-search-query"
              />
            </label>
            {#if search.results}
              <ul class="mt-1.5 max-h-56 overflow-y-auto" data-testid="channel-search-results">
                {#each search.results as hit (hit.seq)}
                  <li>
                    <button class="flex w-full items-baseline gap-2 rounded-md px-2 py-1 text-start text-[12.5px] hover:bg-wash" onclick={() => jump(hit.seq)}>
                      <span class="shrink-0 font-medium">{hit.account === me ? t('you') : label(hit.account)}</span>
                      <span class="min-w-0 flex-1 truncate text-muted">{hit.text || hit.file?.name}</span>
                      <span class="shrink-0 text-[11px] text-faint">{shortDay.format(hit.stored_at)}</span>
                    </button>
                  </li>
                {:else}
                  <li class="px-2 py-1.5 text-[12.5px] text-faint">{t('noMatch', { query: search.query })}</li>
                {/each}
              </ul>
            {/if}
          </div>
        {/if}

        {#if transferring}
          <div class="flex items-center gap-2.5 border-b border-warn/30 bg-warn-pale px-5 py-2 text-[12.5px] text-ink-2" data-testid="channel-transferring">
            <Icon name="coins" class="shrink-0 text-warn" />
            <span class="min-w-0 flex-1">{t('sponsorUnderway', { name: label(channel.billing?.transition?.to_payer) })}</span>
            {#if can.owner && unlocked}<button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={cancelSponsor} data-testid="sponsor-cancel">{t('cancelTransfer')}</button>{/if}
          </div>
        {/if}
        {#if ledger?.status === 'rotation_required'}
          <div class="flex items-center gap-2.5 border-b border-warn/30 bg-warn-pale px-5 py-2 text-[12.5px] text-ink-2">
            <Icon name="epoch" class="shrink-0 text-warn" />
            <span class="min-w-0 flex-1" data-testid="channel-rotation">{t(can.rotates ? 'rotationRequired' : 'rotationWaiting')}</span>
            {#if unlocked && can.rotates}<button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={rotate}>{t('rotate')}</button>{/if}
          </div>
        {:else if ledger?.status === 'archived'}
          <div class="flex items-center gap-2.5 border-b border-line bg-surface-2 px-5 py-2 text-[12.5px] text-muted">
            <Icon name="archive" class="shrink-0" />
            <span class="min-w-0 flex-1">{t('archivedChannel')}</span>
            {#if can.owner && unlocked}
              <button class={buttonClass('secondary', 'xs')} disabled={busy} onclick={reactivate} data-testid="channel-reactivate">
                <Icon name="unarchive" />{t('reactivate')}
              </button>
            {/if}
          </div>
        {:else if ledger && !can.member}
          <div class="flex items-center gap-2.5 border-b border-line bg-surface-2 px-5 py-2 text-[12.5px] text-muted">
            <Icon name="leave" class="shrink-0" /><span class="min-w-0 flex-1">{t('notMember')}</span>
            {#if unlocked && !channel.hidden}<button class={buttonClass('secondary', 'xs')} onclick={() => settings({ hidden: true })}>{t('hideFromList')}</button>{/if}
          </div>
        {/if}

        <Thread
          {channel}
          {messages}
          outbox={shownOutbox}
          {files}
          {me}
          {receipts}
          {more}
          {highlight}
          {images}
          writes={unlocked && can.writes}
          bind:draft={() => drafts[channel.id] ?? '', (text) => (drafts[channel.id] = text)}
          bind:attachments={() => attached[channel.id] ?? [], (list) => (attached[channel.id] = list)}
          bind:reply
          bind:editing
          onSend={send}
          onRetry={retry}
          onDiscard={discard}
          onImport={() => addFiles()}
          onAttach={attachments}
          onBrowse={() => (picking = true)}
          onDrop={(dropped) => addFiles(dropped)}
          onDownload={download}
          onSave={save}
          onSnapshot={receiveSnapshot}
          onEarlier={earlier}
          onRetract={(message) => (retracting = message)}
          onHide={hide}
          onPreview={preview}
          onPacket={settlePacket}
          onRead={markRead}
        />
      {:else}
        <div class="grid flex-1 place-items-center p-8 text-center text-[13px] text-faint">
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

<NewChannel bind:open={creating} bind:tab={createTab} bind:invitation={joinText} bind:peer onOpened={opened} />
<Packet bind:open={settling} />
<LibraryPicker
  bind:open={picking}
  onPick={(list) => channel && (attached[channel.id] = [...(attached[channel.id] ?? []), ...list.filter((f) => !(attached[channel.id] ?? []).some((a) => a.id === f.id))])}
/>
{#if channel}
  <Transfer bind:open={transfer.open} kind={transfer.kind} {channel} {me} onSent={reload} />
  <Access bind:open={access} {channel} manages={unlocked && can.manages} onControl={control} />
  <Dialog bind:open={leaving} title={t('leaveTitle', { name: channel.name })} description={t('leaveHint')} icon="leave" tone="danger" testid="channel-leave-dialog">
    <p class="text-[13px] text-muted">{t('leaveBody')}</p>
    {#snippet footer()}
      <span class="flex-1"></span>
      <button class={buttonClass('ghost')} onclick={() => (leaving = false)}>{t('cancel')}</button>
      <button class={buttonClass('danger')} disabled={busy} onclick={leave} data-testid="channel-leave-confirm">{t('leave')}</button>
    {/snippet}
  </Dialog>
  <Dialog bind:open={archiving} title={t('archiveTitle', { name: channel.name })} description={t('archiveHint')} icon="archive" tone="danger" testid="channel-archive-dialog">
    <p class="text-[13px] text-muted">{t('archiveBody')}</p>
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
    <input bind:value={renaming.name} class="text-[13.5px]" data-testid="channel-rename-input" />
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
    <p class="line-clamp-3 text-[13px] text-muted">{retracting.text || retracting.file?.name}</p>
    {#snippet footer()}
      <span class="flex-1"></span>
      <button class={buttonClass('ghost')} onclick={() => (retracting = null)}>{t('cancel')}</button>
      <button class={buttonClass('danger')} disabled={busy} onclick={retract} data-testid="message-retract-confirm">{t('retract')}</button>
    {/snippet}
  </Dialog>
{/if}
<Dialog bind:open={deletingLegacyDraft} title={t('legacyDeleteDraft')} icon="trash" tone="danger" testid="legacy-draft-delete-dialog">
  <p class="text-[13px] text-muted">{t('legacyDeleteDraftHint', { name: channel?.name ?? '' })}</p>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} disabled={busy} onclick={() => (deletingLegacyDraft = false)}>{t('cancel')}</button>
    <button class={buttonClass('danger')} disabled={busy} onclick={deleteLegacyDraft} data-testid="legacy-draft-delete-confirm">{t('remove')}</button>
  {/snippet}
</Dialog>
{#if history.member}
  <Dialog bind:open={history.open} title={t('grantHistory')} description={t('historyHint')} icon="legacy" testid="history-dialog">
    <div class="flex items-end gap-2" data-testid="history-form">
      <label class="grid flex-1 gap-1 text-[12px] text-faint">
        {t('fromEpoch')}
        <input type="number" min="1" max={history.member.joined_epoch - 1} bind:value={history.from} class="text-ink tabular-nums" />
      </label>
      <span class="pb-1.5 text-faint">–</span>
      <label class="grid flex-1 gap-1 text-[12px] text-faint">
        {t('toEpoch')}
        <input type="number" min="1" max={history.member.joined_epoch - 1} bind:value={history.to} class="text-ink tabular-nums" />
      </label>
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
  {#if snapshot}{#key snapshot.package_id}<PackageImport received={snapshot} open={id=>{snapshotOpen=false;go(`/approval/${id}`)}} />{/key}{/if}
</Dialog>
