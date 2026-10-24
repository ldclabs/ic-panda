<script lang="ts">
  import { onMount } from 'svelte'
  import Dialog from '../../components/Dialog.svelte'
  import Icon from '../../components/Icon.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import { buttonClass } from '../../components/ui'
  import { t } from '../../i18n/index.svelte'
  import { bridge, call, errorText, onChange, single, when } from '../../rpc'

  let { entry, onDeleted }: { entry: any; onDeleted: () => void } = $props()
  let report = $state<any>(null)
  let messages = $state<any[]>([])
  let error = $state('')
  let busy = $state(false)
  let deleting = $state(false)

  async function attempt(work: () => Promise<void>) {
    busy = true
    error = ''
    try { await work() } catch (e) { error = errorText(e) } finally { busy = false }
  }

  const load = single(() => attempt(async () => {
    const first = report === null
    report = await call('legacy/report', { id: entry.archive, channel: entry.source, limit: first ? 100 : 0 })
    if (first) messages = report.messages
  }))

  const more = () => attempt(async () => {
    const next: any = await call('legacy/report', { id: entry.archive, channel: entry.source, offset: messages.length, limit: 100 })
    messages = [...messages, ...next.messages]
    report = next
  })

  const save = (file: any) => attempt(async () => {
    await bridge(() => window.dmsg.legacyFile(entry.archive, file.source, file.name))
  })

  const remove = () => attempt(async () => {
    await call('legacy/delete_channel', { id: entry.archive, channel: entry.source })
    deleting = false
    onDeleted()
  })

  onMount(() => {
    void load()
    return onChange(['sync/changed', 'legacy/changed'], load)
  })
</script>

<header class="flex min-h-14 items-center gap-3 border-b border-line px-5 py-2">
  <Icon name="legacy" class="text-forest" />
  <h2 class="min-w-0 flex-1 truncate text-[15px] font-semibold" data-testid="legacy-channel-title">{entry.name || entry.source}</h2>
  <Pill>{t('legacyHistory')}</Pill>
  <button class={buttonClass('danger', 'sm')} disabled={busy} onclick={() => (deleting = true)} data-testid="legacy-channel-delete">{t('legacyDeleteChannel')}</button>
</header>
<div class="min-h-0 flex-1 space-y-4 overflow-y-auto p-5" data-testid="legacy-channel-history">
  <Notice>{t('legacyHistoryHint')}</Notice>
  {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
  {#if report}
    <div class="flex flex-wrap items-center gap-2 text-[12px] text-muted">
      <span>{t('legacyReadable', { n: report.total_messages })}</span>
      {#if report.channel.unreadable}<span class="text-warn">{t('legacyUnreadable', { n: report.channel.unreadable })}</span>{/if}
      <Pill tone={report.synced ? 'ok' : 'neutral'}>{t(report.synced ? 'synced' : 'legacyWaitingSync')}</Pill>
    </div>
    {#if report.channel.unreadable}<Notice tone="warn">{t('legacyPartialHint')}</Notice>{/if}
    <ul class="divide-y divide-line rounded-xl border border-line" data-testid="legacy-channel-messages">
      {#each messages as message (message.source)}
        <li class="px-4 py-3">
          <p class="mb-1 font-mono text-[11px] break-all text-faint">{message.author} · {message.time ? when(Number(message.time)) : '-'}{message.system ? ` · ${t('legacySystem')}` : ''}</p>
          <p class="text-[13px] whitespace-pre-wrap [overflow-wrap:anywhere]">{message.deleted ? t('legacyDeleted') : message.text}</p>
          {#if message.file}
            {@const file = report.files.find((f: any) => f.source === message.file.source)}
            <button class={buttonClass('secondary', 'xs', 'mt-2')} disabled={busy || !file?.verified} onclick={() => save(message.file)}><Icon name="file" />{message.file.name}</button>
          {/if}
        </li>
      {/each}
    </ul>
    {#if messages.length < report.total_messages}<button class={buttonClass('ghost', 'sm')} disabled={busy} onclick={more}>{t('more')}</button>{/if}
  {/if}
</div>

<Dialog bind:open={deleting} title={t('legacyDeleteChannel')} icon="trash" tone="danger" testid="legacy-channel-delete-dialog">
  <p class="text-[13px] text-muted">{t('legacyDeleteChannelHint', { name: entry.name || entry.source })}</p>
  {#snippet footer()}
    <span class="flex-1"></span>
    <button class={buttonClass('ghost')} disabled={busy} onclick={() => (deleting = false)}>{t('cancel')}</button>
    <button class={buttonClass('danger')} disabled={busy} onclick={remove} data-testid="legacy-channel-delete-confirm">{t('remove')}</button>
  {/snippet}
</Dialog>
