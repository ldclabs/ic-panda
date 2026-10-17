<script lang="ts" module>
  /** A message on its way: sending, or refused and kept for a retry. */
  export interface Outgoing {
    key: number
    text: string
    file?: { id: string; name: string }
    error?: string
  }
</script>

<script lang="ts">
  import { DropdownMenu } from 'bits-ui'
  import { tick } from 'svelte'
  import { localeDir } from '../../../shared/locales'
  import Avatar from '../../components/Avatar.svelte'
  import Icon from '../../components/Icon.svelte'
  import Tooltip from '../../components/Tooltip.svelte'
  import { buttonClass, cn, menuContentClass, menuItemClass } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { size, when } from '../../rpc'
  import { shortId } from '../../shell/session'
  import { days } from './channels'

  /**
   * A channel's messages by day and the composer. A message shows as
   * sending until the relay stores it (its sequence number), and stays with
   * a retry if the daemon refused it. Enter sends; Shift+Enter starts a line.
   * The library's files for an attachment are read when its menu opens.
   */
  let {
    channel,
    messages,
    outbox,
    files,
    me,
    writes,
    draft = $bindable(''),
    attachment = $bindable<{ id: string; name: string } | null>(null),
    onSend,
    onRetry,
    onDiscard,
    onImport,
    onAttach,
    onDownload,
    onSave
  }: {
    channel: any
    messages: any[]
    outbox: Outgoing[]
    files: any[]
    me: string | undefined
    writes: boolean
    draft?: string
    attachment?: { id: string; name: string } | null
    onSend: () => void
    onRetry: (item: Outgoing) => void
    onDiscard: (item: Outgoing) => void
    onImport: () => void
    onAttach: () => void
    onDownload: (message: any) => void
    onSave: (message: any) => void
  } = $props()

  let scroller = $state<HTMLElement | null>(null)

  const groups = $derived(days(messages))
  const dayFormat = $derived(new Intl.DateTimeFormat(locale(), { weekday: 'long', month: 'long', day: 'numeric' }))
  const timeFormat = $derived(new Intl.DateTimeFormat(locale(), { hour: '2-digit', minute: '2-digit' }))

  function dayLabel(at: number): string {
    const today = new Date()
    const date = new Date(at)
    const diff = Math.round((new Date(today.toDateString()).getTime() - new Date(date.toDateString()).getTime()) / 86_400_000)
    if (diff < 2) return new Intl.RelativeTimeFormat(locale(), { numeric: 'auto' }).format(-diff, 'day').replace(/^\p{L}/u, (c) => c.toLocaleUpperCase(locale()))
    return dayFormat.format(date)
  }

  /** A message continues the one before it when the same account wrote both within five minutes. */
  const follows = (list: any[], i: number) => i > 0 && list[i - 1].account === list[i].account && list[i].stored_at - list[i - 1].stored_at < 300_000

  // New messages and sends keep the timeline at its end.
  $effect(() => {
    void messages.length
    void outbox.length
    void tick().then(() => scroller && (scroller.scrollTop = scroller.scrollHeight))
  })

  function onKey(event: KeyboardEvent) {
    if (event.key === 'Enter' && !event.shiftKey && !event.isComposing) {
      event.preventDefault()
      onSend()
    }
  }
</script>

{#snippet author(account: string)}
  <b class="text-[13px] font-semibold">
    {#if account === me}{t('you')}{:else}<bdi dir="ltr" class="font-mono text-[12px]" title={account}>{shortId(account)}</bdi>{/if}
  </b>
{/snippet}

<div bind:this={scroller} class="min-h-0 flex-1 overflow-y-auto py-3" data-testid="channel-messages">
  {#each groups as group (group.day)}
    <div class="sticky top-0 z-[1] flex justify-center py-1.5">
      <span class="rounded-full border border-line bg-paper px-2.5 py-0.5 text-[11px] font-medium text-faint">{dayLabel(group.at)}</span>
    </div>
    {#each group.messages as message, i (message.seq)}
      {@const compact = follows(group.messages, i)}
      <article class={cn('group flex gap-3 px-6 hover:bg-wash/60', compact ? 'py-0.5' : 'pt-2.5 pb-0.5')} data-seq={message.seq}>
        {#if compact}
          <time class="w-8 shrink-0 pt-0.5 text-end text-[10.5px] text-faint opacity-0 group-hover:opacity-100">{timeFormat.format(message.stored_at)}</time>
        {:else}
          <Avatar id={message.account} />
        {/if}
        <div class="min-w-0 flex-1">
          {#if !compact}
            <div class="flex items-baseline gap-2">
              {@render author(message.account)}
              <time class="text-[11px] text-faint" title={when(message.stored_at)}>{timeFormat.format(message.stored_at)}</time>
              <Tooltip content={t('storedBy', { seq: message.seq })}>
                {#snippet children(props)}
                  <span {...props} class="text-faint opacity-0 group-hover:opacity-100"><Icon name="check" class="size-3" /></span>
                {/snippet}
              </Tooltip>
            </div>
          {/if}
          {#if message.text}<p class="text-[13.5px] leading-relaxed whitespace-pre-wrap select-text [overflow-wrap:anywhere]">{message.text}</p>{/if}
          {#if message.file}
            <div class="mt-1.5 flex max-w-sm items-center gap-3 rounded-xl border border-line bg-surface px-3 py-2 shadow-xs">
              <span class="grid size-9 shrink-0 place-items-center rounded-lg bg-surface-2 text-faint"><Icon name="file" /></span>
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
        </div>
      </article>
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
  {#each outbox as item (item.key)}
    <article class="flex gap-3 px-6 pt-2.5 pb-0.5" data-testid="message-outgoing" data-state={item.error ? 'failed' : 'sending'}>
      <Avatar id={me ?? ''} class="opacity-60" />
      <div class="min-w-0 flex-1">
        <div class="flex items-baseline gap-2">
          {@render author(me ?? '')}
          {#if item.error}
            <span class="inline-flex items-center gap-1 text-[11px] text-danger"><Icon name="warning" class="size-3" />{t('notSent')} · {item.error}</span>
          {:else}
            <span class="shimmer text-[11px]">{t('sending')}</span>
          {/if}
        </div>
        {#if item.text}<p class={cn('text-[13.5px] leading-relaxed whitespace-pre-wrap [overflow-wrap:anywhere]', !item.error && 'text-muted')}>{item.text}</p>{/if}
        {#if item.file}<p class="mt-1 inline-flex items-center gap-1.5 text-[12px] text-faint"><Icon name="attach" class="size-3.5" />{item.file.name}</p>{/if}
        {#if item.error}
          <div class="mt-1 flex gap-1.5">
            <button class={buttonClass('secondary', 'xs')} onclick={() => onRetry(item)}>{t('retry')}</button>
            <button class={buttonClass('ghost', 'xs')} onclick={() => onDiscard(item)}>{t('discard')}</button>
          </div>
        {/if}
      </div>
    </article>
  {/each}
</div>

{#if writes}
  <div class="border-t border-line px-4 pt-2.5 pb-3">
    {#if attachment}
      <div class="mb-2 inline-flex max-w-full items-center gap-1.5 rounded-lg border border-line bg-surface ps-2 pe-0.5 py-0.5 text-[12px]" data-testid="channel-attached">
        <Icon name="attach" class="size-3.5 shrink-0 text-faint" />
        <span class="truncate">{attachment.name}</span>
        <button class={buttonClass('ghost', 'xs', 'size-5 px-0')} aria-label={t('remove')} onclick={() => (attachment = null)}><Icon name="close" /></button>
      </div>
    {/if}
    <div class="flex items-end gap-1.5 rounded-xl border border-edge bg-surface p-1.5 transition-[border-color,box-shadow] duration-150 focus-within:border-forest focus-within:ring-3 focus-within:ring-forest/18">
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
                  <DropdownMenu.Item class={menuItemClass} data-value={file.id} onSelect={() => (attachment = { id: file.id, name: file.name })}>
                    <span class="flex min-w-0 items-center gap-2"><Icon name="file" class="shrink-0 text-faint" /><span class="truncate">{file.name}</span></span>
                    <span class="shrink-0 text-[11px] text-faint">{size(file.size)}</span>
                  </DropdownMenu.Item>
                {/each}
              </DropdownMenu.Group>
              <DropdownMenu.Separator class="my-1 h-px bg-line" />
            {/if}
            <DropdownMenu.Item class={menuItemClass} onSelect={onImport}>
              <span class="flex items-center gap-2"><Icon name="upload" class="text-faint" />{t('addFiles')}</span>
            </DropdownMenu.Item>
          </DropdownMenu.Content>
        </DropdownMenu.Portal>
      </DropdownMenu.Root>
      <textarea
        bind:value={draft}
        rows="1"
        class="max-h-40 min-h-8 flex-1 resize-none border-0 bg-transparent px-1 py-1.5 text-[13.5px] shadow-none [field-sizing:content] focus-visible:shadow-none"
        placeholder={t('messageTo', { name: channel.name })}
        onkeydown={onKey}
        data-testid="channel-text"
      ></textarea>
      <button class={buttonClass('primary', 'md')} disabled={!draft.trim() && !attachment} onclick={onSend} data-testid="channel-send">
        <Icon name="send" class="rtl:-scale-x-100" />{t('send')}
      </button>
    </div>
    <p class="mt-1.5 px-1 text-[11px] text-faint">{t('composerHint')}</p>
  </div>
{/if}
