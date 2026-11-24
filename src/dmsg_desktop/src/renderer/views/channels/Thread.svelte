<script lang="ts" module>
  export type { Outgoing } from '../../message-delivery'
</script>

<script lang="ts">
  import { ContextMenu, DropdownMenu } from 'bits-ui'
  import { onMount, tick, untrack } from 'svelte'
  import { uncertainSend, type Outgoing } from '../../message-delivery'
  import { localeDir } from '../../../shared/locales'
  import ParticipantAvatar from './ParticipantAvatar.svelte'
  import { copyText } from '../../components/clipboard'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import Empty from '../../components/page/Empty.svelte'
  import LinkedText from '../../components/LinkedText.svelte'
  import TargetMenu from '../../components/TargetMenu.svelte'
  import { toast } from '../../components/toast.svelte'
  import Tooltip from '../../components/Tooltip.svelte'
  import { iconButtonClass, buttonClass, cn, menuContentClass, menuItemClass } from '../../components/ui'
  import { enter } from '../../components/motion'
  import { locale, t } from '../../i18n/index.svelte'
  import { bridge, errorText, size, when } from '../../rpc'
  import { remember, remembered } from '../../view-memory'
  import { validPrincipal, walletRoute } from '../wallet/model'
  import { w } from '../wallet/text'
  import { workflowText } from '../secrets/workflow-text'
  import { bytes, days, firstUnread, MAX_TEXT, packetStep, receiptOf } from './channels'
  import { label as accountLabel } from './names.svelte'

  /**
   * A channel's messages by day and the composer. A message shows as
   * sending until the relay stores it, and stays in the outbox with a retry
   * if the daemon refused it. Messages answer, edit and withdraw earlier
   * ones; a hidden one folds away on this device. Enter sends, Shift+Enter
   * starts a line, Escape leaves a reply or an edit and ↑ edits the last
   * message of this account, and typing anywhere in the timeline goes to the
   * composer. Files come from the library, the system dialog, a drop or a
   * paste; images show in place and open large. The timeline follows new
   * messages only while it is at its end, which also marks them read; it opens
   * where it was left, else at what is new, under a line. One per channel: the
   * parent keys it, so nothing of one channel's position reaches another.
   */
  let {
    channel,
    messages,
    outbox,
    files,
    me,
    writes,
    receipts,
    more,
    loadingEarlier = false,
    highlight,
    images,
    draft = $bindable(''),
    attachments = $bindable<{ id: string; name: string }[]>([]),
    reply = $bindable<any>(null),
    editing = $bindable<{ message: any; text: string } | null>(null),
    onSend,
    onRetry,
    onDiscard,
    onImport,
    onAttach,
    onBrowse,
    onDrop,
    onPaste,
    history = false,
    onLatest,
    onDownload,
    onSave,
    onSnapshot,
    onEarlier,
    onRetract,
    onHide,
    onPreview,
    onPacket,
    onRead
  }: {
    channel: any
    messages: any[]
    outbox: Outgoing[]
    files: any[]
    me: string | undefined
    writes: boolean
    receipts: any
    more: boolean
    loadingEarlier?: boolean
    highlight: number | null
    images: Record<number, string>
    draft?: string
    attachments?: { id: string; name: string }[]
    reply?: any
    editing?: { message: any; text: string } | null
    onSend: () => void
    onRetry: (item: Outgoing) => void
    onDiscard: (item: Outgoing) => void
    onImport: () => void
    onAttach: () => void
    onBrowse: () => void
    onDrop: (files: File[]) => void
    onPaste: (files: File[]) => void
    /** The timeline shows a search result's surroundings, not the latest messages. */
    history?: boolean
    onLatest: () => Promise<boolean>
    onDownload: (message: any) => void
    onSave: (message: any) => void
    onSnapshot: (message: any) => void
    onEarlier: () => Promise<void>
    onRetract: (message: any) => void
    onHide: (message: any, hidden: boolean) => void
    onPreview: (message: any) => void
    onPacket: (message: any, step: 'accept' | 'commit') => void
    onRead: (seq: number) => void
  } = $props()

  let scroller = $state<HTMLElement | null>(null)
  let input = $state<HTMLTextAreaElement | null>(null)
  let dragging = $state(false)
  /** Where this channel was left; none is at its end, or at what is new. */
  const place = untrack(() => `channels.scroll.${channel.id}`)
  const left = remembered<number>(place)
  /** Whether the timeline shows its end, so new messages keep it there. */
  let atEnd = $state(left === undefined)
  /** What this account had read as the channel opened, for the line above what is new. */
  const readAt = untrack(() => channel?.read ?? 0)
  const unreadAt = untrack(() => (channel?.unread ? firstUnread(messages, readAt, me) : null))
  let large = $state<any>(null)

  const ledger = $derived(channel?.ledger)
  const groups = $derived(days(messages))
  const bySeq = $derived(new Map(messages.map((m) => [m.seq, m])))
  const dayFormat = $derived(new Intl.DateTimeFormat(locale(), { weekday: 'long', month: 'long', day: 'numeric' }))
  const timeFormat = $derived(new Intl.DateTimeFormat(locale(), { hour: '2-digit', minute: '2-digit' }))
  const composerText = $derived(editing?.text ?? draft)
  const used = $derived(bytes(composerText))
  const tooLong = $derived(used > MAX_TEXT)
  const canSend = $derived(writes && !tooLong && (editing
    ? !!editing.text.trim() && editing.text !== editing.message.text
    : !!draft.trim() || attachments.length > 0))

  function dayLabel(at: number): string {
    const today = new Date()
    const date = new Date(at)
    const diff = Math.round((new Date(today.toDateString()).getTime() - new Date(date.toDateString()).getTime()) / 86_400_000)
    if (diff < 2) return new Intl.RelativeTimeFormat(locale(), { numeric: 'auto' }).format(-diff, 'day').replace(/^\p{L}/u, (c) => c.toLocaleUpperCase(locale()))
    return dayFormat.format(date)
  }

  /** Another's message stored after the thread opened fades in; the ones already there show at once. */
  const opened = Date.now()

  /** A message continues the one before it when the same account wrote both within five minutes. */
  const follows = (list: any[], i: number) => i > 0 && list[i - 1].account === list[i].account && list[i].stored_at - list[i - 1].stored_at < 300_000 && !list[i].reply_to

  const nearEnd = () => !scroller || scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 80

  function onScroll() {
    atEnd = nearEnd()
    remember(place, atEnd || !scroller ? null : scroller.scrollHeight - scroller.scrollTop)
    if (atEnd) read()
  }

  const toEnd = () => {
    if (scroller) scroller.scrollTop = scroller.scrollHeight
  }

  /** The newest messages, from anywhere: a search result's surroundings first load them. */
  async function latest() {
    if (history && !await onLatest()) return
    atEnd = true
    remember(place, null)
    await tick()
    toEnd()
    read()
  }

  /** The channel opens where it was left, else at the line above what is new, else at its end. */
  onMount(() => {
    void tick().then(() => {
      if (!scroller) return
      if (left !== undefined) scroller.scrollTop = scroller.scrollHeight - left
      else if (unreadAt !== null) scroller.querySelector('[data-testid="unread-divider"]')?.scrollIntoView({ block: 'start' })
      else toEnd()
      atEnd = nearEnd()
      read()
      if (writes && document.hasFocus() && !document.querySelector('[aria-modal="true"]')) input?.focus({ preventScroll: true })
    })
  })

  /** A letter typed with nothing focused, or the timeline focused, belongs in the composer. */
  function onWindowKey(event: KeyboardEvent) {
    if (!input || event.key.length !== 1 || event.key === ' ' || event.metaKey || event.ctrlKey || event.altKey || event.isComposing) return
    const target = event.target as Element | null
    if (target !== document.body && !scroller?.contains(target)) return
    if (document.querySelector('[aria-modal="true"]')) return
    input.focus()
  }

  /** The last message is read once the timeline shows it to someone looking. */
  function read() {
    const last = messages.at(-1)
    if (last && atEnd && document.hasFocus() && document.visibilityState === 'visible' && !document.querySelector('[aria-modal="true"]')) onRead(last.seq)
  }

  // New messages and sends keep the timeline at its end when it was there.
  $effect(() => {
    void messages.length
    void outbox.length
    if (!atEnd) return
    void tick().then(() => {
      if (!atEnd) return
      toEnd()
      read()
    })
  })

  // A search result opens at its message.
  $effect(() => {
    if (highlight === null) return
    void tick().then(() => scroller?.querySelector(`[data-seq="${highlight}"]`)?.scrollIntoView({ block: 'center' }))
  })

  /** Earlier messages go above without moving what is shown. */
  async function earlier() {
    const before = scroller?.scrollHeight ?? 0
    await onEarlier()
    await tick()
    if (scroller) scroller.scrollTop += scroller.scrollHeight - before
  }

  function send() {
    if (!canSend) return
    // An explicit send follows its own pending row even when the person was
    // reading older history. Incoming messages still respect their position.
    atEnd = true
    remember(place, null)
    onSend()
    void tick().then(() => {
      toEnd()
      input?.focus()
    })
  }

  function onKey(event: KeyboardEvent) {
    if (event.isComposing) return
    if (event.key === 'Enter' && !event.shiftKey) {
      event.preventDefault()
      send()
    } else if (event.key === 'Escape' && (reply || editing)) {
      event.preventDefault()
      leave()
    } else if (event.key === 'ArrowUp' && !draft && !reply && !editing) {
      const last = [...messages].reverse().find((m) => mine(m) && editable(m))
      if (last) {
        event.preventDefault()
        edit(last)
      }
    }
  }

  const mine = (message: any) => message.account === me
  const label = (account: string | undefined) => accountLabel(account, false)
  const editable = (message: any) => !!message.text && !message.retracted && !message.unreadable && !message.packet

  function edit(message: any) {
    editing = { message, text: message.text }
    void tick().then(() => input?.focus())
  }

  function answer(message: any) {
    editing = null
    reply = message
    void tick().then(() => input?.focus())
  }

  function leave() {
    if (editing) editing = null
    else reply = null
    void tick().then(() => input?.focus())
  }

  function pasteFiles(event: ClipboardEvent) {
    if (editing) return
    const pasted = [...(event.clipboardData?.files ?? [])]
    if (!pasted.length) return
    event.preventDefault()
    onPaste(pasted)
  }

  function dropped(event: DragEvent) {
    event.preventDefault()
    dragging = false
    const list = [...(event.dataTransfer?.files ?? [])]
    if (list.length && writes && !editing) onDrop(list)
  }

  /** What a person selected in the message, else its whole text. */
  const copyMessage = (message: any) => copyText(getSelection()?.toString() || message.text)
  const copyImage = (message: any) => void bridge(() => window.dmsg.copyImage(images[message.seq])).catch((error) => toast(() => errorText(error)))
  const messageOf = (element: Element) => {
    const seq = Number(element.closest('[data-seq]')?.getAttribute('data-seq'))
    return messages.find((message) => message.seq === seq && !message.unreadable && !message.hidden) ?? null
  }

  const snippet = (message: any) => (message.retracted ? t('retracted') : message.text || message.file?.name || '')
  const isImage = (message: any) => !message.file_deleted && /^image\/(png|jpeg|gif|webp)$/.test(message.file?.mime ?? '') && message.file.size <= 4 * 1024 * 1024

  /** An image is read once its message comes on screen, not every image of the page at once. */
  const preview = (message: any) => (node: Element) => {
    const observer = new IntersectionObserver((entries) => {
      if (!entries.some((e) => e.isIntersecting)) return
      observer.disconnect()
      if (!(message.seq in images)) onPreview(message)
    })
    observer.observe(node)
    return () => observer.disconnect()
  }
</script>

{#snippet messageItems(Item: any, message: any)}
  {#if writes && !message.retracted}
    <Item class={menuItemClass} onSelect={() => answer(message)} data-testid="menu-reply">
      <span class="flex items-center gap-2"><Icon name="reply" class="text-faint" />{t('replyAction')}</span>
    </Item>
  {/if}
  {#if message.text && !message.retracted}
    <Item class={menuItemClass} onSelect={() => copyMessage(message)} data-testid="menu-copy">
      <span class="flex items-center gap-2"><Icon name="copy" class="text-faint" />{t('copy')}</span>
    </Item>
  {/if}
  {#if message.file && !message.file_deleted && !message.retracted && message.file.mime !== 'application/vnd.dmsg.secret-package'}
    <Item class={menuItemClass} onSelect={() => onSave(message)} data-testid="menu-save">
      <span class="flex items-center gap-2"><Icon name="folder-add" class="text-faint" />{t('saveToLibrary')}</span>
    </Item>
    <Item class={menuItemClass} onSelect={() => onDownload(message)} data-testid="menu-download">
      <span class="flex items-center gap-2"><Icon name="download" class="text-faint" />{t('download')}</span>
    </Item>
  {/if}
  {#if writes && mine(message) && editable(message)}
    <Item class={menuItemClass} onSelect={() => edit(message)} data-testid="message-edit">
      <span class="flex items-center gap-2"><Icon name="edit" class="text-faint" />{t('edit')}</span>
    </Item>
  {/if}
  {#if writes && mine(message) && !message.retracted}
    <Item class={cn(menuItemClass, 'text-danger')} onSelect={() => onRetract(message)} data-testid="message-retract">
      <span class="flex items-center gap-2"><Icon name="restore" />{t('retract')}…</span>
    </Item>
  {/if}
  <Item class={menuItemClass} onSelect={() => onHide(message, true)} data-testid="message-hide">
    <span class="flex items-center gap-2"><Icon name="eye-off" class="text-faint" />{t('hideHere')}</span>
  </Item>
{/snippet}

{#snippet author(account: string)}
  <b class="text-body font-semibold" title={account}>
    {#if account === me}{t('you')}{:else}<bdi>{label(account)}</bdi>{/if}
  </b>
{/snippet}

{#snippet quote(target: any, ref: any)}
  <div class="mb-1 flex max-w-lg items-baseline gap-1.5 border-s-2 border-edge-soft ps-2 text-callout text-muted" data-testid="message-quote">
    <Icon name="reply" class="size-3 shrink-0" />
    {#if target}
      <span class="shrink-0 font-medium text-muted">{target.account === me ? t('you') : label(target.account)}</span>
      <span class="truncate">{snippet(target)}</span>
    {:else}
      <span>{t('quoteMissing', { seq: ref.seq })}</span>
    {/if}
  </div>
{/snippet}

{#snippet meta(message: any)}
  <span class="ms-2 inline-flex items-center gap-1 align-baseline text-caption whitespace-nowrap text-faint">
    <time title={when(message.stored_at)}>{timeFormat.format(message.stored_at)}</time>
    {#if mine(message) && !message.retracted}{@render ticks(message)}{/if}
  </span>
{/snippet}

{#snippet ticks(message: any)}
  {@const receipt = receiptOf(message.seq, receipts, ledger, me)}
  {@const all = receipt.others > 0 && receipt.delivered === receipt.others}
  <Tooltip content={[t('storedBy', { seq: message.seq }), receipt.others ? t('deliveredTo', { n: receipt.delivered, of: receipt.others }) : '', receipt.read ? t('readBy', { n: receipt.read }) : ''].filter(Boolean).join(' · ')}>
    {#snippet children(props)}
      <span {...props} class={cn('inline-flex', receipt.read ? 'text-forest' : 'text-faint')} data-testid="message-receipt">
        <Icon name={all ? 'delivered' : 'check'} class="size-3.5" />
      </span>
    {/snippet}
  </Tooltip>
{/snippet}

{#snippet packetCard(message: any)}
  {@const packet = packetStep(message.packet, ledger, channel?.billing, me)}
  {#if message.packet?.format==='dmsg-wallet-event/1' && validPrincipal(message.packet.wallet??'')}
    <div class="mt-1.5 max-w-md rounded-xl border border-line bg-surface p-3" data-testid="message-wallet-event">
      <p class="text-body font-semibold">{message.packet.name}</p>
      <p class="mt-1 text-callout">{message.packet.proposal!=null?`#${message.packet.proposal} · `:''}{w(message.packet.status??'proposals')}</p>
      <p class="mt-1 text-caption text-faint">{w('channelHint')}</p>
      <a class="mt-2 inline-flex text-callout font-medium text-forest underline" href={`#${walletRoute(message.packet.wallet,Number.isSafeInteger(message.packet.proposal)?message.packet.proposal:undefined)}`}>{w('details')}</a>
    </div>
  {:else if packet}
    <div class="mt-1.5 flex max-w-md items-center gap-3 rounded-xl border border-line bg-surface px-3 py-2.5" data-testid="message-packet">
      <span class="grid size-9 shrink-0 place-items-center rounded-lg bg-pale text-forest"><Icon name={packet.kind === 'owner' ? 'owner' : 'coins'} /></span>
      <span class="min-w-0 flex-1">
        <span class="block text-body font-medium">{packet.kind === 'owner' ? t('transferOwner') : t('transferSponsor')}</span>
        <span class="block truncate text-caption text-faint">
          → {packet.target === me ? t('you') : label(packet.target)} · {message.packet.acceptance ? t('packetAcceptedShort') : t('packetProposed')}
        </span>
      </span>
      {#if packet.step === 'accept' || packet.step === 'commit'}
        <button class={buttonClass('primary', 'sm')} onclick={() => onPacket(message, packet.step as 'accept' | 'commit')} data-testid="packet-{packet.step}">
          {packet.step === 'accept' ? t('acceptTransfer') : t('commitTransfer')}
        </button>
      {:else if packet.step === 'done'}
        <span class="inline-flex items-center gap-1 text-caption text-forest"><Icon name="check" class="size-3.5" />{t('transferDone')}</span>
      {/if}
    </div>
  {/if}
{/snippet}

<svelte:window onkeydown={onWindowKey} />

<div class="relative flex min-h-0 flex-1 flex-col">
<TargetMenu find={messageOf} testid="message-menu">
  {#snippet menu(message)}{@render messageItems(ContextMenu.Item, message)}{/snippet}
<div
  bind:this={scroller}
  class={cn('relative min-h-0 flex-1 overflow-y-auto py-3', dragging && 'bg-pale/40 ring-2 ring-forest/40 ring-inset')}
  data-testid="channel-messages"
  role="log"
  onscroll={onScroll}
  ondragover={(e) => {
    if (!writes || editing || !e.dataTransfer?.types.includes('Files')) return
    e.preventDefault()
    dragging = true
  }}
  ondragleave={() => (dragging = false)}
  ondrop={dropped}
>
  {#if more}
    <div class="flex justify-center pb-2">
      <button class={buttonClass('ghost', 'xs')} disabled={loadingEarlier} onclick={earlier} data-testid="channel-earlier"><Icon name="history" />{t('earlierMessages')}</button>
    </div>
  {/if}
  {#each groups as group, g (group.day)}
    <!-- Days out of sight skip layout until they come near; the last two stay
         laid out, so the timeline's end is where it says it is. -->
    <section class={g < groups.length - 2 ? '[content-visibility:auto] [contain-intrinsic-size:auto_600px]' : ''}>
    <div class="sticky top-0 z-[1] flex justify-center py-1.5">
      <span class="rounded-full border border-line bg-paper px-2.5 py-0.5 text-caption font-medium text-faint">{dayLabel(group.at)}</span>
    </div>
    {#each group.messages as message, i (message.seq)}
      {@const compact = follows(group.messages, i) && message.seq !== unreadAt}
      {#if message.seq === unreadAt}
        <div class="flex items-center gap-3 px-6 py-1.5 text-caption font-medium text-forest" data-testid="unread-divider">
          <span class="h-px flex-1 bg-forest/40"></span>{t('newMessages')}<span class="h-px flex-1 bg-forest/40"></span>
        </div>
      {/if}
      {#if message.hidden}
        <div class="flex items-center gap-2 px-6 py-1 text-caption text-faint" data-seq={message.seq} data-testid="message-hidden">
          <Icon name="eye-off" class="size-3.5" />{t('hiddenMessage')}
          <button class="underline-offset-2 hover:text-ink hover:underline" onclick={() => onHide(message, false)}>{t('show')}</button>
        </div>
      {:else}
        {@const own = mine(message)}
        <article
          class={cn('group relative flex gap-2.5 px-4', own && 'flex-row-reverse', compact ? 'pt-0.5' : 'pt-3', highlight === message.seq && 'bg-warn-pale/60', !own && message.stored_at > opened && 'animate-in fade-in-0 duration-150')}
          data-seq={message.seq}
          data-mine={own}
          dir="ltr"
        >
          {#if !own}
            {#if compact}<span class="w-8 shrink-0"></span>{:else}<ParticipantAvatar account={message.account} />{/if}
          {/if}
          <div class={cn('flex min-w-0 flex-col', own ? 'max-w-[min(80%,calc(100%-44px))] items-end' : 'max-w-[calc(100%-44px)] items-start')}>
            {#if !compact && (!own || message.agent)}
              <div class="mb-0.5 flex flex-wrap items-baseline gap-x-2 px-1">
                {#if !own}{@render author(message.account)}{/if}
                {#if message.agent}
                  <span class="inline-flex items-center gap-1 rounded bg-wash px-1 text-caption text-ink-2" title={message.agent.id} data-testid="message-agent">
                    <Icon name="agents" class="size-3" />{t('viaAgent', { name: message.agent.name })}
                  </span>
                {/if}
              </div>
            {/if}
            {#if message.unreadable}
              {#if message.reply_to}{@render quote(bySeq.get(message.reply_to.seq), message.reply_to)}{/if}
              <p class="inline-flex items-center gap-1.5 px-1 text-body text-faint italic" data-testid="message-unreadable"><Icon name="warning" class="size-3.5 text-warn" />{t('unreadableMessage')}</p>
            {:else if message.retracted}
              <p class="px-1 text-body text-faint italic" data-testid="message-retracted">{t('retracted')}{@render meta(message)}</p>
            {:else}
              {#if message.text || message.reply_to}
                <div
                  class={cn('max-w-[60ch] min-w-0 rounded-2xl px-3 py-1.5', own ? 'bubble-mine' : 'border border-line bg-surface', compact && (own ? 'rounded-tr-md' : 'rounded-tl-md'))}
                  data-testid="message-bubble"
                >
                  {#if message.reply_to}{@render quote(bySeq.get(message.reply_to.seq), message.reply_to)}{/if}
                  {#if message.text}
                    <p dir="auto" class="text-start text-body leading-relaxed whitespace-pre-wrap select-text [overflow-wrap:anywhere]">
                      <span><LinkedText text={message.text} /></span>{#if message.edited_at}<span class="ms-1 text-caption text-faint" title={when(message.edited_at)} data-testid="message-edited">({t('edited')})</span>{/if}{@render meta(message)}
                    </p>
                  {:else}{@render meta(message)}{/if}
                </div>
              {:else}
                <p class="px-1">{@render meta(message)}</p>
              {/if}
              {#if message.file_deleted}<p class="mt-2 text-callout text-muted" data-testid="attachment-deleted">{t('storageAttachmentDeleted')}</p>
              {:else if message.file}
                {#if isImage(message) && images[message.seq]}
                  <button class="mt-1.5 block max-w-sm cursor-zoom-in overflow-hidden rounded-xl border border-line" onclick={() => (large = message)} title={message.file.name} data-testid="message-image-open">
                    <img src={images[message.seq]} alt={message.file.name} class="max-h-72 w-auto object-contain" data-testid="message-image" />
                  </button>
                {/if}
                <div
                  {@attach isImage(message) && preview(message)}
                  class="mt-1.5 flex max-w-sm items-center gap-3 rounded-xl border border-line bg-surface px-3 py-2"
                >
                  <span class="grid size-9 shrink-0 place-items-center rounded-lg bg-surface-2 text-faint"><Icon name={isImage(message) ? 'image' : 'file'} /></span>
                  <span class="min-w-0 flex-1">
                    <span class="block truncate text-body font-medium">{message.file.name}</span>
                    <span class="block text-caption text-faint">{size(message.file.size)}</span>
                  </span>
                  {#if message.file.mime==='application/vnd.dmsg.secret-package'}
                    <button class={buttonClass('secondary','sm')} onclick={()=>onSnapshot(message)} data-testid="message-snapshot">{workflowText('channelSnapshot')}</button>
                  {:else}
                  <Tooltip content={t('saveToLibrary')}>
                    {#snippet children(props)}
                      <button {...props} class={iconButtonClass('ghost', 'md')} aria-label={t('saveToLibrary')} onclick={() => onSave(message)} data-testid="message-save">
                        <Icon name="folder-add" />
                      </button>
                    {/snippet}
                  </Tooltip>
                  <Tooltip content={t('download')}>
                    {#snippet children(props)}
                      <button {...props} class={iconButtonClass('ghost', 'md')} aria-label={t('download')} onclick={() => onDownload(message)} data-testid="message-download">
                        <Icon name="download" />
                      </button>
                    {/snippet}
                  </Tooltip>
                  {/if}
                </div>
              {/if}
              {#if message.packet}{@render packetCard(message)}{/if}
            {/if}
          </div>
          {#if !message.unreadable}
            <div class="pointer-events-none absolute -top-2.5 flex items-center gap-0.5 rounded-lg bg-surface p-0.5 opacity-0 shadow-pop group-data-[mine=true]:left-4 group-data-[mine=false]:right-4 group-focus-within:pointer-events-auto group-focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100" data-testid="message-actions" dir={localeDir(locale())}>
              {#if writes && !message.retracted}
                <Tooltip content={t('replyAction')}>
                  {#snippet children(props)}
                    <button {...props} class={iconButtonClass('ghost', 'md')} aria-label={t('replyAction')} onclick={() => answer(message)} data-testid="message-reply"><Icon name="reply" /></button>
                  {/snippet}
                </Tooltip>
              {/if}
              {#if message.text && !message.retracted}
                <Tooltip content={t('copy')}>
                  {#snippet children(props)}
                    <button {...props} class={iconButtonClass('ghost', 'md')} aria-label={t('copy')} onclick={() => copyText(message.text)}><Icon name="copy" /></button>
                  {/snippet}
                </Tooltip>
              {/if}
              <DropdownMenu.Root dir={localeDir(locale())}>
                <DropdownMenu.Trigger class={iconButtonClass('ghost', 'md')} aria-label={t('moreActions')} data-testid="message-more"><Icon name="more" /></DropdownMenu.Trigger>
                <DropdownMenu.Portal>
                  <DropdownMenu.Content align="end" sideOffset={4} class={cn(menuContentClass, 'min-w-44')}>
                    {@render messageItems(DropdownMenu.Item, message)}
                  </DropdownMenu.Content>
                </DropdownMenu.Portal>
              </DropdownMenu.Root>
            </div>
          {/if}
        </article>
      {/if}
    {/each}
    </section>
  {:else}
    {#if !outbox.length && !more}
      <div class="grid h-full place-items-center"><Empty icon="channels" title={t('noMessages')} /></div>
    {/if}
  {/each}
  {#each outbox as item (item.message_id)}
    <article class="flex flex-row-reverse gap-2.5 px-4 pt-3" in:enter dir="ltr" data-testid="message-outgoing" data-mine="true" data-state={item.queued ? 'queued' : item.sending ? 'sending' : 'failed'}>
      <div class="flex min-w-0 max-w-[min(80%,calc(100%-44px))] flex-col items-end">
        <div class="mb-0.5 flex flex-wrap items-baseline justify-end gap-2 px-1">
          {#if item.queued}
            <span class="text-caption text-muted">{t('mailQueued')}</span>
          {:else if item.sending}
            <span class="shimmer text-caption">{t('sending')}</span>
          {:else if uncertainSend(item)}
            <span class="inline-flex items-center gap-1 text-caption text-warn"><Icon name="history" class="size-3" />{t('waiting')}</span>
          {:else}
            <span class="inline-flex items-center gap-1 text-caption text-danger"><Icon name="warning" class="size-3" />{t('notSent')}{item.error ? ` · ${item.error}` : ''}</span>
          {/if}
        </div>
        {#if item.draft?.text}<p dir="auto" class={cn('bubble-mine max-w-[60ch] rounded-2xl px-3 py-1.5 text-start text-body leading-relaxed whitespace-pre-wrap select-text [overflow-wrap:anywhere]', item.sending || item.queued ? 'opacity-70' : '')}>{item.draft.text}</p>{/if}
        {#if item.file}<p class="mt-1 inline-flex items-center gap-1.5 text-callout text-muted"><Icon name="attach" class="size-3.5" />{item.file.name}</p>{/if}
        {#if !item.sending && !item.queued}
          <div class="mt-1 flex gap-1.5">
            <button class={buttonClass('subtle', 'xs')} onclick={() => onRetry(item)} data-testid="outgoing-retry">{t(uncertainSend(item) ? 'reconcileNow' : 'retry')}</button>
            {#if !uncertainSend(item)}<button class={buttonClass('ghost', 'xs')} onclick={() => onDiscard(item)} data-testid="outgoing-discard">{t('discard')}</button>{/if}
          </div>
        {/if}
      </div>
    </article>
  {/each}
</div>
</TargetMenu>
{#if !atEnd || history}
  <button
    class={buttonClass('secondary', 'sm', 'absolute end-5 bottom-3 z-[2] rounded-full shadow-pop')}
    onclick={latest}
    data-testid="channel-latest"
  >
    <Icon name="arrow-down" />{t('jumpToLatest')}{#if channel?.unread}<span class="rounded-full bg-forest px-1.5 text-caption leading-4 font-semibold text-on-forest tabular-nums">{channel.unread > 99 ? '99+' : channel.unread}</span>{/if}
  </button>
{/if}
</div>

{#if large}
  <Dialog bind:open={() => !!large, (open) => !open && (large = null)} title={large.file.name} icon="image" wide testid="message-image-large">
    <img src={images[large.seq]} alt={large.file.name} class="mx-auto max-h-[64vh] w-auto rounded-lg object-contain" />
    {#snippet footer()}
      <span class="min-w-0 flex-1 truncate text-callout text-muted">{size(large.file.size)}</span>
      <button class={buttonClass('ghost')} onclick={() => copyImage(large)} data-testid="image-copy"><Icon name="copy" />{t('copy')}</button>
      <button class={buttonClass('ghost')} onclick={() => onSave(large)}><Icon name="folder-add" />{t('saveToLibrary')}</button>
      <button class={buttonClass('primary')} onclick={() => onDownload(large)} data-testid="image-download"><Icon name="download" />{t('download')}</button>
    {/snippet}
  </Dialog>
{/if}

{#if writes}
  <div class="border-t border-line px-4 pt-2.5 pb-3">
    {#if reply || editing}
      <div class="mb-2 flex items-center gap-2 rounded-lg bg-surface-2 px-2.5 py-1.5 text-callout text-muted" data-testid="composer-mode">
        <Icon name={editing ? 'edit' : 'reply'} class="size-3.5 shrink-0" />
        <span class="shrink-0 font-medium">{editing ? t('editing') : `${t('replyingTo')} ${reply.account === me ? t('you') : label(reply.account)}`}</span>
        <span class="min-w-0 flex-1 truncate">{snippet(editing?.message ?? reply)}</span>
        <button class={buttonClass('ghost', 'xs', 'size-6 px-0')} aria-label={t('cancel')} onclick={leave}><Icon name="close" /></button>
      </div>
    {/if}
    {#if attachments.length && !editing}
      <div class="mb-2 flex flex-wrap gap-1.5">
        {#each attachments as file (file.id)}
          <div class="inline-flex max-w-full items-center gap-1.5 rounded-lg border border-line bg-surface ps-2 pe-0.5 py-0.5 text-callout" data-testid="channel-attached">
            <Icon name="attach" class="size-3.5 shrink-0 text-faint" />
            <span class="truncate">{file.name}</span>
            <button class={buttonClass('ghost', 'xs', 'size-6 px-0')} aria-label={t('remove')} onclick={() => (attachments = attachments.filter((f) => f.id !== file.id))}><Icon name="close" /></button>
          </div>
        {/each}
      </div>
    {/if}
    <div class="flex items-end gap-1.5 rounded-xl border border-edge bg-surface p-1.5 transition-[border-color,box-shadow] duration-150 focus-within:border-forest focus-within:ring-3 focus-within:ring-forest/18">
      {#if !editing}
        <DropdownMenu.Root dir={localeDir(locale())} onOpenChange={(open) => open && onAttach()}>
          <Tooltip content={t('attachFile')}>
            {#snippet children(props)}
              <DropdownMenu.Trigger {...props} class={iconButtonClass('ghost', 'md')} aria-label={t('attachFile')} data-testid="channel-attachment">
                <Icon name="attach" />
              </DropdownMenu.Trigger>
            {/snippet}
          </Tooltip>
          <DropdownMenu.Portal>
            <DropdownMenu.Content side="top" align="start" sideOffset={6} class={cn(menuContentClass, 'w-72')} data-testid="channel-attachment-menu">
              {#if files.length}
                <DropdownMenu.Group>
                  <DropdownMenu.GroupHeading class="px-2.5 pt-1.5 pb-1 text-caption font-medium text-faint">{t('fromLibrary')}</DropdownMenu.GroupHeading>
                  {#each files as file (file.id)}
                    <DropdownMenu.Item
                      class={menuItemClass}
                      data-value={file.id}
                      onSelect={() => !attachments.some((a) => a.id === file.id) && (attachments = [...attachments, { id: file.id, name: file.name }])}
                    >
                      <span class="flex min-w-0 items-center gap-2"><Icon name="file" class="shrink-0 text-faint" /><span class="truncate">{file.name}</span></span>
                      <span class="shrink-0 text-caption text-faint">{size(file.size)}</span>
                    </DropdownMenu.Item>
                  {/each}
                </DropdownMenu.Group>
                <DropdownMenu.Separator class="my-1 h-px bg-line" />
              {/if}
              <DropdownMenu.Item class={menuItemClass} onSelect={onBrowse} data-testid="channel-browse">
                <span class="flex items-center gap-2"><Icon name="search" class="text-faint" />{t('searchLibrary')}…</span>
              </DropdownMenu.Item>
              <DropdownMenu.Item class={menuItemClass} onSelect={onImport}>
                <span class="flex items-center gap-2"><Icon name="upload" class="text-faint" />{t('addFiles')}</span>
              </DropdownMenu.Item>
            </DropdownMenu.Content>
          </DropdownMenu.Portal>
        </DropdownMenu.Root>
      {/if}
      <textarea
        bind:this={input}
        bind:value={() => composerText, (text) => { if (editing) editing.text = text; else draft = text }}
        rows="1"
        class="max-h-40 min-h-8 flex-1 resize-none border-0 bg-transparent px-1 py-1.5 text-body shadow-none [field-sizing:content] focus-visible:shadow-none"
        placeholder={t('messageTo', { name: channel.name })}
        onkeydown={onKey}
        onpaste={pasteFiles}
        data-testid="channel-text"
      ></textarea>
      <button class={buttonClass('primary', 'md')} disabled={!canSend} onclick={send} data-testid="channel-send">
        <Icon name={editing ? 'check' : 'send'} class="rtl:-scale-x-100" />{editing ? t('save') : t('send')}
      </button>
    </div>
    <p class="mt-1.5 flex gap-2 px-1 text-caption text-faint">
      <span class="min-w-0 flex-1">{t('composerHint')}</span>
      {#if used > MAX_TEXT * 0.8}<span class={cn('tabular-nums', tooLong && 'text-danger')} data-testid="composer-count">{used} / {MAX_TEXT}</span>{/if}
    </p>
  </div>
{/if}
