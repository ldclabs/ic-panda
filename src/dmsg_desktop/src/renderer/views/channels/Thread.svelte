<script lang="ts" module>
  /** A message on its way, as the daemon's outbox holds it, and whether this window is sending it now. */
  export interface Outgoing {
    message_id: string
    draft?: { text?: string; reply_to?: { seq: number } }
    file?: { name: string; size: number }
    sending?: boolean
    error?: string
  }
</script>

<script lang="ts">
  import { DropdownMenu } from 'bits-ui'
  import { tick } from 'svelte'
  import { localeDir } from '../../../shared/locales'
  import Avatar from '../../components/Avatar.svelte'
  import { copyText } from '../../components/clipboard'
  import Icon from '../../components/Icon.svelte'
  import Tooltip from '../../components/Tooltip.svelte'
  import { buttonClass, cn, menuContentClass, menuItemClass } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { size, when } from '../../rpc'
  import { bytes, days, MAX_TEXT, packetStep, receiptOf } from './channels'
  import { label } from './names.svelte'

  /**
   * A channel's messages by day and the composer. A message shows as
   * sending until the relay stores it, and stays in the outbox with a retry
   * if the daemon refused it. Messages answer, edit and withdraw earlier
   * ones; a hidden one folds away on this device. Enter sends, Shift+Enter
   * starts a line, Escape leaves a reply or an edit and ↑ edits the last
   * message of this account. Files come from the library, the system
   * dialog, a drop or a paste; images show in place. The timeline follows
   * new messages only while it is at its end, which also marks them read.
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
    highlight,
    images,
    draft = $bindable(''),
    attachments = $bindable<{ id: string; name: string }[]>([]),
    reply = $bindable<any>(null),
    editing = $bindable<any>(null),
    onSend,
    onRetry,
    onDiscard,
    onImport,
    onAttach,
    onBrowse,
    onDrop,
    onDownload,
    onSave,
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
    highlight: number | null
    images: Record<number, string>
    draft?: string
    attachments?: { id: string; name: string }[]
    reply?: any
    editing?: any
    onSend: () => void
    onRetry: (item: Outgoing) => void
    onDiscard: (item: Outgoing) => void
    onImport: () => void
    onAttach: () => void
    onBrowse: () => void
    onDrop: (files: File[]) => void
    onDownload: (message: any) => void
    onSave: (message: any) => void
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
  /** Whether the timeline shows its end, so new messages keep it there. */
  let atEnd = true

  const ledger = $derived(channel?.ledger)
  const groups = $derived(days(messages))
  const bySeq = $derived(new Map(messages.map((m) => [m.seq, m])))
  const dayFormat = $derived(new Intl.DateTimeFormat(locale(), { weekday: 'long', month: 'long', day: 'numeric' }))
  const timeFormat = $derived(new Intl.DateTimeFormat(locale(), { hour: '2-digit', minute: '2-digit' }))
  const used = $derived(bytes(draft))
  const tooLong = $derived(used > MAX_TEXT)

  function dayLabel(at: number): string {
    const today = new Date()
    const date = new Date(at)
    const diff = Math.round((new Date(today.toDateString()).getTime() - new Date(date.toDateString()).getTime()) / 86_400_000)
    if (diff < 2) return new Intl.RelativeTimeFormat(locale(), { numeric: 'auto' }).format(-diff, 'day').replace(/^\p{L}/u, (c) => c.toLocaleUpperCase(locale()))
    return dayFormat.format(date)
  }

  /** A message continues the one before it when the same account wrote both within five minutes. */
  const follows = (list: any[], i: number) => i > 0 && list[i - 1].account === list[i].account && list[i].stored_at - list[i - 1].stored_at < 300_000 && !list[i].reply_to

  const nearEnd = () => !scroller || scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 80

  function onScroll() {
    atEnd = nearEnd()
    if (atEnd) read()
  }

  /** The last message is read once the timeline shows it to someone looking. */
  function read() {
    const last = messages.at(-1)
    if (last && document.hasFocus()) onRead(last.seq)
  }

  // New messages and sends keep the timeline at its end when it was there.
  $effect(() => {
    void messages.length
    void outbox.length
    if (!atEnd) return
    void tick().then(() => {
      if (scroller) scroller.scrollTop = scroller.scrollHeight
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

  function onKey(event: KeyboardEvent) {
    if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
      event.preventDefault()
      if (!tooLong) onSend()
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
  const editable = (message: any) => !!message.text && !message.retracted && !message.unreadable && !message.packet

  function edit(message: any) {
    reply = null
    editing = message
    draft = message.text
    void tick().then(() => input?.focus())
  }

  function answer(message: any) {
    editing = null
    reply = message
    void tick().then(() => input?.focus())
  }

  function leave() {
    if (editing) draft = ''
    reply = editing = null
  }

  function onPaste(event: ClipboardEvent) {
    const pasted = [...(event.clipboardData?.files ?? [])]
    if (!pasted.length) return
    event.preventDefault()
    onDrop(pasted)
  }

  function dropped(event: DragEvent) {
    event.preventDefault()
    dragging = false
    const list = [...(event.dataTransfer?.files ?? [])]
    if (list.length && writes) onDrop(list)
  }

  const snippet = (message: any) => (message.retracted ? t('retracted') : message.text || message.file?.name || '')
  const isImage = (message: any) => /^image\/(png|jpeg|gif|webp)$/.test(message.file?.mime ?? '') && message.file.size <= 4 * 1024 * 1024

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

{#snippet author(account: string)}
  <b class="text-[13px] font-semibold" title={account}>
    {#if account === me}{t('you')}{:else}<bdi>{label(account)}</bdi>{/if}
  </b>
{/snippet}

{#snippet quote(target: any, ref: any)}
  <div class="mb-1 flex max-w-lg items-baseline gap-1.5 border-s-2 border-edge-soft ps-2 text-[12px] text-faint" data-testid="message-quote">
    <Icon name="reply" class="size-3 shrink-0" />
    {#if target}
      <span class="shrink-0 font-medium text-muted">{target.account === me ? t('you') : label(target.account)}</span>
      <span class="truncate">{snippet(target)}</span>
    {:else}
      <span>{t('quoteMissing', { seq: ref.seq })}</span>
    {/if}
  </div>
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
  {#if packet}
    <div class="mt-1.5 flex max-w-md items-center gap-3 rounded-xl border border-line bg-surface px-3 py-2.5 shadow-xs" data-testid="message-packet">
      <span class="grid size-9 shrink-0 place-items-center rounded-lg bg-pale text-forest"><Icon name={packet.kind === 'owner' ? 'owner' : 'coins'} /></span>
      <span class="min-w-0 flex-1">
        <span class="block text-[13px] font-medium">{packet.kind === 'owner' ? t('transferOwner') : t('transferSponsor')}</span>
        <span class="block truncate text-[11.5px] text-faint">
          → {packet.target === me ? t('you') : label(packet.target)} · {message.packet.acceptance ? t('packetAcceptedShort') : t('packetProposed')}
        </span>
      </span>
      {#if packet.step === 'accept' || packet.step === 'commit'}
        <button class={buttonClass('primary', 'sm')} onclick={() => onPacket(message, packet.step as 'accept' | 'commit')} data-testid="packet-{packet.step}">
          {packet.step === 'accept' ? t('acceptTransfer') : t('commitTransfer')}
        </button>
      {:else if packet.step === 'done'}
        <span class="inline-flex items-center gap-1 text-[11.5px] text-forest"><Icon name="check" class="size-3.5" />{t('transferDone')}</span>
      {/if}
    </div>
  {/if}
{/snippet}

<div
  bind:this={scroller}
  class={cn('relative min-h-0 flex-1 overflow-y-auto py-3', dragging && 'bg-pale/40 ring-2 ring-forest/40 ring-inset')}
  data-testid="channel-messages"
  role="log"
  onscroll={onScroll}
  ondragover={(e) => {
    if (!writes) return
    e.preventDefault()
    dragging = true
  }}
  ondragleave={() => (dragging = false)}
  ondrop={dropped}
>
  {#if more}
    <div class="flex justify-center pb-2">
      <button class={buttonClass('ghost', 'xs')} onclick={earlier} data-testid="channel-earlier"><Icon name="history" />{t('earlierMessages')}</button>
    </div>
  {/if}
  {#each groups as group (group.day)}
    <div class="sticky top-0 z-[1] flex justify-center py-1.5">
      <span class="rounded-full border border-line bg-paper px-2.5 py-0.5 text-[11px] font-medium text-faint">{dayLabel(group.at)}</span>
    </div>
    {#each group.messages as message, i (message.seq)}
      {@const compact = follows(group.messages, i)}
      {#if message.hidden}
        <div class="flex items-center gap-2 px-6 py-1 text-[11.5px] text-faint" data-seq={message.seq} data-testid="message-hidden">
          <Icon name="eye-off" class="size-3.5" />{t('hiddenMessage')}
          <button class="underline-offset-2 hover:text-ink hover:underline" onclick={() => onHide(message, false)}>{t('show')}</button>
        </div>
      {:else}
        <article
          class={cn('group relative flex gap-3 px-6 hover:bg-wash/60', compact ? 'py-0.5' : 'pt-2.5 pb-0.5', highlight === message.seq && 'bg-warn-pale/60')}
          data-seq={message.seq}
        >
          {#if compact}
            <time class="w-8 shrink-0 pt-0.5 text-end text-[10.5px] text-faint opacity-0 group-hover:opacity-100">{timeFormat.format(message.stored_at)}</time>
          {:else}
            <Avatar id={message.account} name={label(message.account)} />
          {/if}
          <div class="min-w-0 flex-1">
            {#if !compact}
              <div class="flex flex-wrap items-baseline gap-x-2">
                {@render author(message.account)}
                {#if message.agent}
                  <span class="inline-flex items-center gap-1 rounded bg-wash px-1 text-[10.5px] text-ink-2" title={message.agent.id} data-testid="message-agent">
                    <Icon name="agents" class="size-3" />{t('viaAgent', { name: message.agent.name })}
                  </span>
                {/if}
                <time class="text-[11px] text-faint" title={when(message.stored_at)}>{timeFormat.format(message.stored_at)}</time>
                {#if mine(message)}{@render ticks(message)}{/if}
              </div>
            {/if}
            {#if message.reply_to}{@render quote(bySeq.get(message.reply_to.seq), message.reply_to)}{/if}
            {#if message.unreadable}
              <p class="inline-flex items-center gap-1.5 text-[13px] text-faint italic" data-testid="message-unreadable"><Icon name="warning" class="size-3.5 text-warn" />{t('unreadableMessage')}</p>
            {:else if message.retracted}
              <p class="text-[13px] text-faint italic" data-testid="message-retracted">{t('retracted')}</p>
            {:else}
              {#if message.text}
                <p class="text-[13.5px] leading-relaxed whitespace-pre-wrap select-text [overflow-wrap:anywhere]">
                  {message.text}{#if message.edited_at}<span class="ms-1 text-[11px] text-faint" title={when(message.edited_at)} data-testid="message-edited">({t('edited')})</span>{/if}
                </p>
              {/if}
              {#if message.file}
                {#if isImage(message) && images[message.seq]}
                  <button class="mt-1.5 block max-w-sm overflow-hidden rounded-xl border border-line" onclick={() => onDownload(message)} title={message.file.name}>
                    <img src={images[message.seq]} alt={message.file.name} class="max-h-72 w-auto object-contain" data-testid="message-image" />
                  </button>
                {/if}
                <div
                  {@attach isImage(message) && preview(message)}
                  class="mt-1.5 flex max-w-sm items-center gap-3 rounded-xl border border-line bg-surface px-3 py-2 shadow-xs"
                >
                  <span class="grid size-9 shrink-0 place-items-center rounded-lg bg-surface-2 text-faint"><Icon name={isImage(message) ? 'image' : 'file'} /></span>
                  <span class="min-w-0 flex-1">
                    <span class="block truncate text-[13px] font-medium">{message.file.name}</span>
                    <span class="block text-[11.5px] text-faint">{size(message.file.size)}</span>
                  </span>
                  <Tooltip content={t('saveToLibrary')}>
                    {#snippet children(props)}
                      <button {...props} class={buttonClass('ghost', 'sm', 'size-8 px-0')} aria-label={t('saveToLibrary')} onclick={() => onSave(message)} data-testid="message-save">
                        <Icon name="folder-add" />
                      </button>
                    {/snippet}
                  </Tooltip>
                  <Tooltip content={t('download')}>
                    {#snippet children(props)}
                      <button {...props} class={buttonClass('ghost', 'sm', 'size-8 px-0')} aria-label={t('download')} onclick={() => onDownload(message)} data-testid="message-download">
                        <Icon name="download" />
                      </button>
                    {/snippet}
                  </Tooltip>
                </div>
              {/if}
              {#if message.packet}{@render packetCard(message)}{/if}
            {/if}
          </div>
          {#if !message.unreadable}
            <div class="absolute end-4 -top-3 hidden items-center gap-0.5 rounded-lg border border-line bg-paper p-0.5 shadow-xs group-focus-within:flex group-hover:flex" data-testid="message-actions">
              {#if writes && !message.retracted}
                <Tooltip content={t('replyAction')}>
                  {#snippet children(props)}
                    <button {...props} class={buttonClass('ghost', 'xs', 'size-6 px-0')} aria-label={t('replyAction')} onclick={() => answer(message)} data-testid="message-reply"><Icon name="reply" /></button>
                  {/snippet}
                </Tooltip>
              {/if}
              {#if message.text && !message.retracted}
                <Tooltip content={t('copy')}>
                  {#snippet children(props)}
                    <button {...props} class={buttonClass('ghost', 'xs', 'size-6 px-0')} aria-label={t('copy')} onclick={() => copyText(message.text)}><Icon name="copy" /></button>
                  {/snippet}
                </Tooltip>
              {/if}
              <DropdownMenu.Root dir={localeDir(locale())}>
                <DropdownMenu.Trigger class={buttonClass('ghost', 'xs', 'size-6 px-0')} aria-label={t('moreActions')} data-testid="message-more"><Icon name="more" /></DropdownMenu.Trigger>
                <DropdownMenu.Portal>
                  <DropdownMenu.Content align="end" sideOffset={4} class={cn(menuContentClass, 'min-w-44')}>
                    {#if writes && mine(message) && editable(message)}
                      <DropdownMenu.Item class={menuItemClass} onSelect={() => edit(message)} data-testid="message-edit">
                        <span class="flex items-center gap-2"><Icon name="edit" class="text-faint" />{t('edit')}</span>
                      </DropdownMenu.Item>
                    {/if}
                    {#if writes && mine(message) && !message.retracted}
                      <DropdownMenu.Item class={cn(menuItemClass, 'text-danger')} onSelect={() => onRetract(message)} data-testid="message-retract">
                        <span class="flex items-center gap-2"><Icon name="restore" />{t('retract')}…</span>
                      </DropdownMenu.Item>
                    {/if}
                    <DropdownMenu.Item class={menuItemClass} onSelect={() => onHide(message, true)} data-testid="message-hide">
                      <span class="flex items-center gap-2"><Icon name="eye-off" class="text-faint" />{t('hideHere')}</span>
                    </DropdownMenu.Item>
                  </DropdownMenu.Content>
                </DropdownMenu.Portal>
              </DropdownMenu.Root>
            </div>
          {/if}
        </article>
      {/if}
    {/each}
  {:else}
    {#if !outbox.length}
      <div class="grid h-full place-items-center px-8 text-center">
        <div class="grid justify-items-center gap-2">
          <span class="grid size-11 place-items-center rounded-xl border border-dashed border-edge-soft text-faint [&_svg]:size-5"><Icon name="channels" /></span>
          <p class="text-[13px] text-muted">{t('noMessages')}</p>
        </div>
      </div>
    {/if}
  {/each}
  {#each outbox as item (item.message_id)}
    <article class="flex gap-3 px-6 pt-2.5 pb-0.5" data-testid="message-outgoing" data-state={item.sending ? 'sending' : 'failed'}>
      <Avatar id={me ?? ''} name={label(me)} class="opacity-60" />
      <div class="min-w-0 flex-1">
        <div class="flex items-baseline gap-2">
          {@render author(me ?? '')}
          {#if item.sending}
            <span class="shimmer text-[11px]">{t('sending')}</span>
          {:else}
            <span class="inline-flex items-center gap-1 text-[11px] text-danger"><Icon name="warning" class="size-3" />{t('notSent')}{item.error ? ` · ${item.error}` : ''}</span>
          {/if}
        </div>
        {#if item.draft?.text}<p class={cn('text-[13.5px] leading-relaxed whitespace-pre-wrap [overflow-wrap:anywhere]', item.sending && 'text-muted')}>{item.draft.text}</p>{/if}
        {#if item.file}<p class="mt-1 inline-flex items-center gap-1.5 text-[12px] text-faint"><Icon name="attach" class="size-3.5" />{item.file.name}</p>{/if}
        {#if !item.sending}
          <div class="mt-1 flex gap-1.5">
            <button class={buttonClass('secondary', 'xs')} onclick={() => onRetry(item)} data-testid="outgoing-retry">{t('retry')}</button>
            <button class={buttonClass('ghost', 'xs')} onclick={() => onDiscard(item)} data-testid="outgoing-discard">{t('discard')}</button>
          </div>
        {/if}
      </div>
    </article>
  {/each}
</div>

{#if writes}
  <div class="border-t border-line px-4 pt-2.5 pb-3">
    {#if reply || editing}
      <div class="mb-2 flex items-center gap-2 rounded-lg bg-surface-2 px-2.5 py-1.5 text-[12px] text-muted" data-testid="composer-mode">
        <Icon name={editing ? 'edit' : 'reply'} class="size-3.5 shrink-0" />
        <span class="shrink-0 font-medium">{editing ? t('editing') : `${t('replyingTo')} ${reply.account === me ? t('you') : label(reply.account)}`}</span>
        <span class="min-w-0 flex-1 truncate">{snippet(editing ?? reply)}</span>
        <button class={buttonClass('ghost', 'xs', 'size-5 px-0')} aria-label={t('cancel')} onclick={leave}><Icon name="close" /></button>
      </div>
    {/if}
    {#if attachments.length && !editing}
      <div class="mb-2 flex flex-wrap gap-1.5">
        {#each attachments as file (file.id)}
          <div class="inline-flex max-w-full items-center gap-1.5 rounded-lg border border-line bg-surface ps-2 pe-0.5 py-0.5 text-[12px]" data-testid="channel-attached">
            <Icon name="attach" class="size-3.5 shrink-0 text-faint" />
            <span class="truncate">{file.name}</span>
            <button class={buttonClass('ghost', 'xs', 'size-5 px-0')} aria-label={t('remove')} onclick={() => (attachments = attachments.filter((f) => f.id !== file.id))}><Icon name="close" /></button>
          </div>
        {/each}
      </div>
    {/if}
    <div class="flex items-end gap-1.5 rounded-xl border border-edge bg-surface p-1.5 transition-[border-color,box-shadow] duration-150 focus-within:border-forest focus-within:ring-3 focus-within:ring-forest/18">
      {#if !editing}
        <DropdownMenu.Root dir={localeDir(locale())} onOpenChange={(open) => open && onAttach()}>
          <Tooltip content={t('attachFile')}>
            {#snippet children(props)}
              <DropdownMenu.Trigger {...props} class={buttonClass('ghost', 'md', 'size-8 px-0')} aria-label={t('attachFile')} data-testid="channel-attachment">
                <Icon name="attach" />
              </DropdownMenu.Trigger>
            {/snippet}
          </Tooltip>
          <DropdownMenu.Portal>
            <DropdownMenu.Content side="top" align="start" sideOffset={6} class={cn(menuContentClass, 'w-72')} data-testid="channel-attachment-menu">
              {#if files.length}
                <DropdownMenu.Group>
                  <DropdownMenu.GroupHeading class="px-2.5 pt-1.5 pb-1 text-[11px] font-medium text-faint">{t('fromLibrary')}</DropdownMenu.GroupHeading>
                  {#each files as file (file.id)}
                    <DropdownMenu.Item
                      class={menuItemClass}
                      data-value={file.id}
                      onSelect={() => !attachments.some((a) => a.id === file.id) && (attachments = [...attachments, { id: file.id, name: file.name }])}
                    >
                      <span class="flex min-w-0 items-center gap-2"><Icon name="file" class="shrink-0 text-faint" /><span class="truncate">{file.name}</span></span>
                      <span class="shrink-0 text-[11px] text-faint">{size(file.size)}</span>
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
        bind:value={draft}
        rows="1"
        class="max-h-40 min-h-8 flex-1 resize-none border-0 bg-transparent px-1 py-1.5 text-[13.5px] shadow-none [field-sizing:content] focus-visible:shadow-none"
        placeholder={t('messageTo', { name: channel.name })}
        onkeydown={onKey}
        onpaste={onPaste}
        data-testid="channel-text"
      ></textarea>
      <button class={buttonClass('primary', 'md')} disabled={tooLong || (!draft.trim() && !attachments.length)} onclick={onSend} data-testid="channel-send">
        <Icon name={editing ? 'check' : 'send'} class="rtl:-scale-x-100" />{editing ? t('save') : t('send')}
      </button>
    </div>
    <p class="mt-1.5 flex gap-2 px-1 text-[11px] text-faint">
      <span class="min-w-0 flex-1">{t('composerHint')}</span>
      {#if used > MAX_TEXT * 0.8}<span class={cn('tabular-nums', tooLong && 'text-danger')} data-testid="composer-count">{used} / {MAX_TEXT}</span>{/if}
    </p>
  </div>
{/if}
