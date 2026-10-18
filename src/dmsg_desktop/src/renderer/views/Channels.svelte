<script lang="ts">
  import { DropdownMenu } from 'bits-ui'
  import { onMount } from 'svelte'
  import { localeDir } from '../../shared/locales'
  import Avatar from '../components/Avatar.svelte'
  import Dialog from '../components/Dialog.svelte'
  import Icon from '../components/Icon.svelte'
  import Locked from '../components/page/Locked.svelte'
  import { toast } from '../components/toast.svelte'
  import Tooltip from '../components/Tooltip.svelte'
  import { buttonClass, cn, menuContentClass, menuItemClass } from '../components/ui'
  import { locale, t } from '../i18n/index.svelte'
  import { bridge, call, closed, code, errorText, onChange, single } from '../rpc'
  import { kindLabel, powers, roleLabel, statusLabel } from './channels/channels'
  import Members from './channels/Members.svelte'
  import NewChannel from './channels/NewChannel.svelte'
  import Packet from './channels/Packet.svelte'
  import Thread, { type Outgoing } from './channels/Thread.svelte'
  import Transfer from './channels/Transfer.svelte'

  /**
   * Channels: the list, the open channel's timeline and composer, and its
   * members in a drawer. Creating, joining, transfers and history grants
   * open in dialogs. Each change comes back through the daemon's
   * `channel/changed`.
   */
  let { status }: { status: any } = $props()

  let channels = $state<any[]>([])
  let selected = $state<string | null>(null)
  let channel = $state<any>(null)
  let messages = $state<any[]>([])
  let files = $state<any[]>([])
  let query = $state('')
  let busy = $state(false)
  let error = $state('')
  /** Unsent text and attachment per channel, and messages on their way. */
  let drafts = $state<Record<string, string>>({})
  let attached = $state<Record<string, { id: string; name: string } | null>>({})
  let outbox = $state<Record<string, Outgoing[]>>({})
  let membersOpen = $state(stored() !== 'closed')
  let creating = $state(false)
  let createTab = $state<'create' | 'join'>('create')
  let settling = $state(false)
  let transfer = $state<{ open: boolean; kind: 'owner' | 'sponsor' }>({ open: false, kind: 'owner' })
  let history = $state<{ open: boolean; member: any; from: number; to: number }>({ open: false, member: null, from: 1, to: 1 })
  let leaving = $state(false)
  let sent = 0

  /** How many messages a channel opens with, its latest; later ones are added as they arrive. */
  const PAGE = 200

  const unlocked = $derived(status?.state === 'unlocked')
  const me = $derived<string | undefined>(status?.account)
  const ledger = $derived(channel?.ledger)
  const can = $derived(powers(ledger, me))
  const shown = $derived(channels.filter((c) => !query.trim() || c.name.toLowerCase().includes(query.trim().toLowerCase())))

  function stored(): string | null {
    try {
      return localStorage.getItem('dmsg.members')
    } catch {
      return null
    }
  }

  $effect(() => {
    try {
      localStorage.setItem('dmsg.members', membersOpen ? 'open' : 'closed')
    } catch {
      /* kept until the window closes */
    }
  })

  const fail = (e: unknown) => toast(() => errorText(e))

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

  /** A channel and its latest messages; the one already shown only gains the messages after its last. */
  async function open(id: string) {
    selected = id
    const last = channel?.id === id ? (messages.at(-1)?.seq ?? 0) : null
    const [next, list] =
      last === null
        ? await call('channel/show', { id }).then(async (next) => [next, await call<any[]>('channel/messages', { id, after: Math.max(0, next.message_cursor - PAGE), limit: PAGE })])
        : await Promise.all([call('channel/show', { id }), call<any[]>('channel/messages', { id, after: last, limit: PAGE })])
    if (selected !== id) return
    const shown = channel?.id === id ? messages : []
    const after = shown.at(-1)?.seq ?? 0
    channel = next
    messages = [...shown, ...list.filter((m: any) => m.seq > after)]
  }

  const pick = (id: string) => attempt(() => open(id))

  /** The library's files that can go with a message, read when the attachment menu opens. */
  async function attachments() {
    const all = await call<any[]>('files/search', { query: '' }).catch(() => [])
    files = all.filter((f) => f.status !== 'cloud_only' && f.status !== 'too_large').slice(0, 20)
  }

  const read = () => attempt(() => call('channel/sync', { id: channel.id }))

  /** A message stays in the outbox until the timeline holds it. */
  async function deliver(id: string, item: Outgoing) {
    try {
      if (item.file) await call('channel/send_file', { id, file: item.file.id, text: item.text })
      else await call('channel/send', { id, text: item.text })
      await reload()
      outbox[id] = (outbox[id] ?? []).filter((o) => o.key !== item.key)
    } catch (e) {
      const waiting = (outbox[id] ?? []).find((o) => o.key === item.key)
      if (waiting) waiting.error = code(e)
    }
  }

  function send() {
    const id = channel.id
    const text = (drafts[id] ?? '').trim()
    const file = attached[id] ?? undefined
    if (!text && !file) return
    const item: Outgoing = { key: ++sent, text, file }
    outbox[id] = [...(outbox[id] ?? []), item]
    drafts[id] = ''
    attached[id] = null
    void deliver(id, item)
  }

  function retry(item: Outgoing) {
    const waiting = outbox[channel.id]?.find((o) => o.key === item.key)
    if (waiting) waiting.error = undefined
    void deliver(channel.id, item)
  }

  const discard = (item: Outgoing) => (outbox[channel.id] = (outbox[channel.id] ?? []).filter((o) => o.key !== item.key))

  /** A new file enters the library through the system dialog, then goes with the next message. */
  const importFile = () =>
    attempt(async () => {
      const added: any = await bridge(() => window.dmsg.filesAdd('root'))
      const file = added?.files[0]?.file
      if (file) attached[channel.id] = { id: file.id, name: file.name }
    })

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

  const control = (action: Record<string, unknown>) => attempt(() => call('channel/control', { id: channel.id, action }))
  const rotate = () => attempt(() => call('channel/rotate', { id: channel.id }))

  /** A new epoch now: who joins later reads nothing before it. */
  async function rekey() {
    const view: any = await call('channel/rekey', { id: channel.id })
    toast(() => t('rekeyed', { epoch: view.ledger?.epoch }))
  }
  const reactivate = () => attempt(() => call('channel/reactivate', { id: channel.id }))

  const leave = () =>
    attempt(async () => {
      await call('channel/leave', { id: channel.id })
      leaving = false
      toast(() => t('left'))
    })

  /** An invitation, after a new epoch when what was written stays from the invited account. */
  async function invite(account: string, fresh: boolean): Promise<string | null> {
    try {
      if (fresh) await rekey()
      return JSON.stringify(await call('channel/invite', { id: channel.id, account }))
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

  /** A channel created or joined opens once the list holds it. */
  async function opened(id: string) {
    selected = id
    await reload()
  }

  onMount(() => {
    void reload()
    return onChange(['channel/changed', 'session/changed', 'sync/changed'], reload)
  })
</script>

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
                <DropdownMenu.Trigger {...props} class={buttonClass('primary', 'md', 'size-8 px-0')} aria-label={t('newChannel')} data-testid="channel-new">
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
      {#if error}<p class="px-4 text-[12.5px] text-danger" data-testid="channel-error">{error}</p>{/if}
      <ul class="min-h-0 flex-1 overflow-y-auto px-2 pb-3" data-testid="channel-list">
        {#each shown as item (item.id)}
          {@const draft = drafts[item.id]?.trim()}
          <li>
            <button
              class={cn(
                'grid w-full grid-cols-[auto_1fr_auto] items-center gap-2.5 rounded-lg px-2 py-2 text-start outline-none transition-colors duration-100 focus-visible:ring-3 focus-visible:ring-focus/35',
                selected === item.id ? 'bg-pale' : 'hover:bg-wash'
              )}
              aria-current={selected === item.id ? 'true' : undefined}
              onclick={() => pick(item.id)}
            >
              <Avatar id={item.id} name={item.name} />
              <span class="min-w-0">
                <span class="block truncate text-[13.5px] leading-snug font-medium">{item.name}</span>
                {#if draft}
                  <span class="block truncate text-[12px] leading-snug text-warn">{t('draft')}: {draft}</span>
                {:else}
                  <span class="block truncate text-[12px] leading-snug text-faint">
                    {item.ledger ? `${t(kindLabel[item.ledger.type] ?? 'collaboration')} · ${t('memberCount', { n: Object.keys(item.ledger.members).length })}` : '…'}
                  </span>
                {/if}
              </span>
              {#if item.ledger && item.ledger.status !== 'active'}
                <span class={cn('rounded px-1 text-[10.5px] font-medium', item.ledger.status === 'archived' ? 'bg-wash text-muted' : 'bg-warn-pale text-warn')}>
                  {t(statusLabel[item.ledger.status] ?? 'channelArchived')}
                </span>
              {/if}
            </button>
          </li>
        {:else}
          <li class="grid justify-items-center gap-2 px-4 py-10 text-center">
            <span class="grid size-11 place-items-center rounded-xl border border-dashed border-edge-soft text-faint [&_svg]:size-5"><Icon name="channels" /></span>
            <p class="text-[13px] text-muted">{query ? t('noMatch', { query }) : t('noChannels')}</p>
            {#if unlocked && !query}
              <button class={buttonClass('secondary', 'sm')} onclick={() => ((createTab = 'create'), (creating = true))}>{t('newChannel')}</button>
            {/if}
          </li>
        {/each}
      </ul>
    </section>

    <section class="flex min-w-0 flex-1 flex-col">
      {#if channel}
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
            </div>
            {#if ledger}
              <p class="truncate text-[12px] text-faint">
                {t(kindLabel[ledger.type] ?? 'collaboration')} · {t('memberCount', { n: Object.keys(ledger.members).length })}{me && ledger.members[me]
                  ? ` · ${t(roleLabel[ledger.members[me].role] ?? 'roleMember')}`
                  : ''}
              </p>
            {/if}
          </div>
          <Tooltip content={t('read')}>
            {#snippet children(props)}
              <button {...props} class={buttonClass('ghost', 'md', 'size-8 px-0')} aria-label={t('read')} disabled={busy} onclick={read} data-testid="channel-read">
                <Icon name="sync-now" />
              </button>
            {/snippet}
          </Tooltip>
          {#if ledger}
            <Tooltip content={t('members')}>
              {#snippet children(props)}
                <button
                  {...props}
                  class={buttonClass('ghost', 'md', cn('size-8 px-0', membersOpen && 'bg-wash text-ink'))}
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
          {#if unlocked && can.member}
            <DropdownMenu.Root dir={localeDir(locale())}>
              <Tooltip content={t('manageChannel')}>
                {#snippet children(props)}
                  <DropdownMenu.Trigger {...props} class={buttonClass('ghost', 'md', 'size-8 px-0')} aria-label={t('manageChannel')} data-testid="channel-manage">
                    <Icon name="more" />
                  </DropdownMenu.Trigger>
                {/snippet}
              </Tooltip>
              <DropdownMenu.Portal>
                <DropdownMenu.Content align="end" sideOffset={6} class={cn(menuContentClass, 'min-w-56')} data-testid="channel-manage-menu">
                  {#if can.rotates && ledger.status === 'active'}
                    <DropdownMenu.Item class={menuItemClass} onSelect={() => attempt(rekey)} data-testid="channel-rekey">
                      <span class="flex items-center gap-2"><Icon name="epoch" class="text-faint" />{t('rekey')}</span>
                    </DropdownMenu.Item>
                    <DropdownMenu.Separator class="my-1 h-px bg-line" />
                  {/if}
                  {#if can.owner}
                    <DropdownMenu.Item class={menuItemClass} onSelect={() => (transfer = { open: true, kind: 'owner' })} data-testid="owner-transfer">
                      <span class="flex items-center gap-2"><Icon name="owner" class="text-faint" />{t('transferOwner')}…</span>
                    </DropdownMenu.Item>
                    <DropdownMenu.Item class={menuItemClass} onSelect={() => (transfer = { open: true, kind: 'sponsor' })}>
                      <span class="flex items-center gap-2"><Icon name="coins" class="text-faint" />{t('transferSponsor')}…</span>
                    </DropdownMenu.Item>
                    {#if ledger.status !== 'archived'}
                      <DropdownMenu.Separator class="my-1 h-px bg-line" />
                      <DropdownMenu.Item class={cn(menuItemClass, 'text-danger')} onSelect={() => control({ type: 'archive' })} data-testid="channel-archive">
                        <span class="flex items-center gap-2"><Icon name="archive" />{t('archive')}</span>
                      </DropdownMenu.Item>
                    {/if}
                  {:else}
                    <DropdownMenu.Item class={cn(menuItemClass, 'text-danger')} onSelect={() => (leaving = true)} data-testid="channel-leave">
                      <span class="flex items-center gap-2"><Icon name="leave" />{t('leave')}…</span>
                    </DropdownMenu.Item>
                  {/if}
                </DropdownMenu.Content>
              </DropdownMenu.Portal>
            </DropdownMenu.Root>
          {/if}
        </header>

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
            <Icon name="leave" class="shrink-0" /><span>{t('notMember')}</span>
          </div>
        {/if}

        <Thread
          {channel}
          {messages}
          outbox={outbox[channel.id] ?? []}
          {files}
          {me}
          writes={unlocked && can.writes}
          bind:draft={() => drafts[channel.id] ?? '', (text) => (drafts[channel.id] = text)}
          bind:attachment={() => attached[channel.id] ?? null, (file) => (attached[channel.id] = file)}
          onSend={send}
          onRetry={retry}
          onDiscard={discard}
          onImport={importFile}
          onAttach={attachments}
          onDownload={download}
          onSave={save}
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
          {ledger}
          {me}
          owner={unlocked && can.owner}
          manages={unlocked && can.manages}
          rotates={unlocked && can.rotates}
          {busy}
          onControl={control}
          onHistory={(member) => (history = { open: true, member, from: 1, to: member.joined_epoch - 1 })}
          onInvite={invite}
          onClose={() => (membersOpen = false)}
        />
      </aside>
    {/if}
  {/if}
</div>

<NewChannel bind:open={creating} bind:tab={createTab} onOpened={opened} />
<Packet bind:open={settling} />
{#if channel}
  <Transfer bind:open={transfer.open} kind={transfer.kind} {channel} {me} />
  <Dialog bind:open={leaving} title={t('leaveTitle', { name: channel.name })} description={t('leaveHint')} icon="leave" tone="danger" testid="channel-leave-dialog">
    <p class="text-[13px] text-muted">{t('left')}</p>
    {#snippet footer()}
      <span class="flex-1"></span>
      <button class={buttonClass('ghost')} onclick={() => (leaving = false)}>{t('cancel')}</button>
      <button class={buttonClass('danger')} disabled={busy} onclick={leave} data-testid="channel-leave-confirm">{t('leave')}</button>
    {/snippet}
  </Dialog>
{/if}
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
