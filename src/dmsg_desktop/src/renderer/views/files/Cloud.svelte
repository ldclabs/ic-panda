<script lang="ts">
  import Checkbox from '../../components/Checkbox.svelte'
  import Icon from '../../components/Icon.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Pill from '../../components/page/Pill.svelte'
  import Tooltip from '../../components/Tooltip.svelte'
  import { buttonClass, cn } from '../../components/ui'
  import { locale, t } from '../../i18n/index.svelte'
  import { ago, size, when } from '../../rpc'
  import { fileIcon } from './files'

  /**
   * The account's backups in the cloud, from every device: what the relay
   * lists, read on request. A backup is fetched into the library, checked,
   * or deleted for good; nothing here happens on its own.
   */
  let {
    entries,
    usage,
    unlocked,
    busy,
    loaded,
    onRefresh,
    onFetch,
    onVerify,
    onDelete
  }: {
    entries: any[]
    usage: any
    unlocked: boolean
    busy: boolean
    loaded: boolean
    onRefresh: () => void
    onFetch: (upload: string) => void
    onVerify: (upload: string) => void
    onDelete: (uploads: string[]) => void
  } = $props()

  let picked = $state<string[]>([])

  const dateFormat = $derived(new Intl.DateTimeFormat(locale(), { dateStyle: 'medium', timeStyle: 'short' }))
  const groups = $derived.by(() => {
    const map = new Map<string, any[]>()
    for (const entry of entries) {
      const key = '/' + (entry.path ?? []).join('/')
      map.set(key, [...(map.get(key) ?? []), entry])
    }
    return [...map].sort(([a], [b]) => a.localeCompare(b, locale()))
  })

  // What was deleted leaves the selection.
  $effect(() => {
    picked = picked.filter((id) => entries.some((e) => e.upload_id === id))
  })

  function toggle(id: string, on: boolean) {
    picked = on ? [...new Set([...picked, id])] : picked.filter((p) => p !== id)
  }
</script>

<section class="flex h-full min-w-0 flex-col" data-testid="files-cloud">
  <header class="flex flex-wrap items-center gap-2 border-b border-line px-5 py-3">
    <div class="min-w-0 flex-1">
      <h2 class="text-[15px] font-semibold">{t('cloudBackups')}</h2>
      {#if usage}
        <p class="text-[12px] text-faint">
          {t('cloudUsageLine', { backups: usage.backups, bytes: size(usage.backup_bytes) })}
          {#if usage.storage_limit}· {t('cloudRoom', { used: size(usage.reserved + usage.committed), limit: size(usage.storage_limit) })}{/if}
        </p>
      {/if}
    </div>
    {#if picked.length}
      <span class="text-[12px] text-faint">{t('selectedCount', { n: picked.length })}</span>
      <button class={buttonClass('danger', 'md')} disabled={busy || !unlocked} onclick={() => onDelete(picked)} data-testid="files-cloud-delete">
        <Icon name="cloud-off" />{t('deleteCloudCopy')}
      </button>
    {/if}
    <button class={buttonClass('secondary', 'md')} disabled={busy || !unlocked} onclick={onRefresh} data-testid="files-cloud-refresh">
      <Icon name="sync" class={cn(busy && 'animate-spin')} />{t('refreshCloud')}
    </button>
  </header>

  <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4">
    {#if !entries.length}
      {#if loaded}
        <div class="grid justify-items-center gap-2 px-4 py-14 text-center" data-testid="files-cloud-empty">
          <span class="grid size-11 place-items-center rounded-xl border border-dashed border-edge-soft text-faint [&_svg]:size-5"><Icon name="cloud-down" /></span>
          <b class="text-[13.5px] font-semibold">{t('cloudEmpty')}</b>
          <p class="max-w-sm text-[12.5px] text-muted">{t('cloudEmptyHint')}</p>
        </div>
      {/if}
    {:else}
      <div class="grid gap-4">
        {#each groups as [path, list] (path)}
          <div>
            <h3 class="mb-1.5 flex items-center gap-1.5 px-1 text-[12px] font-medium text-faint"><Icon name="folder" class="size-3.5" />{path === '/' ? t('library') : path}</h3>
            <ul class="divide-y divide-line rounded-xl border border-line bg-surface shadow-xs">
              {#each list as entry (entry.upload_id)}
                <li class="flex items-center gap-3 px-3 py-2.5" data-upload={entry.upload_id}>
                  <Checkbox checked={picked.includes(entry.upload_id)} onCheckedChange={(on) => toggle(entry.upload_id, on)} disabled={!unlocked} testid="files-cloud-pick" />
                  <span class="grid size-8 shrink-0 place-items-center rounded-lg border border-line bg-surface-2 text-faint"><Icon name={entry.needs_root ? 'cloud-off' : fileIcon(entry.mime)} /></span>
                  <div class="min-w-0 flex-1">
                    <div class="flex flex-wrap items-center gap-1.5">
                      <span class="truncate text-[13.5px] font-medium">{entry.name ?? entry.upload_id.slice(0, 12) + '…'}</span>
                      {#if entry.local}<Pill tone="ok"><Icon name="drive" />{t('hereToo')}</Pill>{/if}
                      {#if entry.needs_root}<Pill tone="warn"><Icon name="warning" />{t('needsRoot')}</Pill>{/if}
                    </div>
                    <div class="truncate text-[11.5px] text-faint" title={when(entry.committed_at)}>
                      {entry.size != null ? `${size(entry.size)} · ` : ''}{size(entry.bytes)} · {t('fromDevice', { device: String(entry.device).slice(0, 8) })} · {entry.committed_at ? dateFormat.format(entry.committed_at) : ago(entry.created_at)}
                      {#if entry.tags?.length}· {entry.tags.map((tag: string) => `#${tag}`).join(' ')}{/if}
                    </div>
                  </div>
                  {#if unlocked}
                    <Tooltip content={t('fetchFile')}>
                      {#snippet children(props)}
                        <button {...props} class={buttonClass('secondary', 'sm')} disabled={busy || entry.needs_root || entry.local} onclick={() => onFetch(entry.upload_id)} data-testid="files-cloud-fetch">
                          <Icon name="cloud-down" />{t('fetchFile')}
                        </button>
                      {/snippet}
                    </Tooltip>
                    <Tooltip content={t('verifyFile')}>
                      {#snippet children(props)}
                        <button {...props} class={buttonClass('ghost', 'sm', 'size-7 px-0')} aria-label={t('verifyFile')} disabled={busy || entry.needs_root} onclick={() => onVerify(entry.upload_id)}>
                          <Icon name="verified" />
                        </button>
                      {/snippet}
                    </Tooltip>
                  {/if}
                </li>
              {/each}
            </ul>
          </div>
        {/each}
      </div>
    {/if}
    <Notice tone="info" class="mt-4" icon="info">{t('deleteCloudHint')}</Notice>
  </div>
</section>
