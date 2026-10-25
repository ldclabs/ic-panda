<script lang="ts">
  import { onMount } from 'svelte'
  import { display, watchDisplays, type Mode } from '../../display.svelte'
  import DisplayNotice from '../../components/DisplayNotice.svelte'
  import Icon from '../../components/Icon.svelte'
  import Checkbox from '../../components/Checkbox.svelte'
  import Dialog from '../../components/Dialog.svelte'
  import Notice from '../../components/page/Notice.svelte'
  import Select from '../../components/Select.svelte'
  import { buttonClass } from '../../components/ui'
  import { copyText } from '../../components/clipboard'
  import { t, type Label } from '../../i18n/index.svelte'
  import { call, errorText, size, when } from '../../rpc'
  import { candidates, filtered, releaseBytes, targetOf } from './storage'
  let { unlocked, linksOnly = false, sourceFile = '', onSource, onChannel }: { unlocked: boolean; linksOnly?: boolean; sourceFile?: string; onSource: (id: string) => void; onChannel: (id: string) => void } = $props()
  let report = $state<any>(null)
  let revision = 0
  $effect(() => {
    if (!unlocked) { revision++; report = null; selected = []; confirmation = null; outcome = [] }
  })
  let busy = $state(false)
  let error = $state('')
  let outcome = $state<any[]>([])
  let selected = $state<string[]>([])
  let confirmation = $state<any[] | null>(null)
  let text = $state('')
  let limit = $state(300)
  $effect(() => { text = sourceFile; limit = 300 })
  let kind = $state('')
  let phase = $state('')
  let resource = $state('')
  let before = $state('')
  let sort = $state('size')
  const all = $derived((report?.entries ?? []).filter((e: any) => !linksOnly || e.kind === 'share'))
  const rows = $derived(filtered(all, { text, kind, phase, resource, before, sort }))
  const shown = $derived(rows.slice(0, limit))
  const picked = $derived(candidates(all, selected))
  const pending = $derived((report?.entries ?? []).some((e: any) => ['deleting', 'retrying', 'releasing', 'cancelling'].includes(e.phase)))
  const phases: Record<string, Label> = { stored:'storageStored', staging:'storageUploading', unpublished:'storageUnpublished', deleting:'storageDeleting', retrying:'storageRetrying', releasing:'storageReleasing', released:'storageReleased', cancelling:'storageCancelling' }
  const kinds: Record<string, Label> = { backup:'cloudBackups', share:'shareLinks', file:'storageAttachments', avatar:'avatar', history:'storageHistory', restricted:'storageRestricted', root:'storageRecovery', envelopes:'storageRecovery', vault:'vault' }
  const label = (e: any) => t(phases[e.phase] ?? 'storageStored')
  async function refresh(mode: Mode = 'swr') {
    if (busy || !unlocked) return
    busy = true; error = ''
    const current = ++revision
    try { const next = await display('files/storage', {}, mode); if (unlocked && current === revision) report = next }
    catch (e) { if (current === revision) error = errorText(e) }
    finally { if (current === revision) busy = false }
  }
  async function act(entries: any[], action: string) {
    if (!unlocked || busy) return
    busy = true; error = ''; outcome = []; confirmation = null
    try {
      for (let start = 0; start < entries.length; start += 50) {
        const targets = entries.slice(start, start + 50).map((e) => targetOf(e, action === 'delete' && e.can_cancel ? 'cancel' : action))
        const result: any = await call('files/storage_action', { targets })
        outcome.push(...result.outcomes)
      }
      selected = []
    } catch (e) { error = errorText(e) } finally { busy = false }
    await refresh()
  }
  onMount(() => {
    const stop = watchDisplays(['storage'], refresh, 300_000)
    void refresh()
    // Only an open view with accepted cleanup work refreshes; no background permanent poll.
    // The snapshot stays fresh for minutes, so cleanup progress reads the relay.
    const timer = setInterval(() => { if (pending && !document.hidden) void refresh('fresh') }, 10000)
    return () => { stop(); clearInterval(timer) }
  })
</script>
<DisplayNotice methods={['files/storage']} />
<section class="flex h-full min-w-0 flex-col" data-testid="files-storage">
  <header class="flex flex-wrap items-center gap-3 border-b border-line px-5 py-3">
    <div class="min-w-0 flex-1"><h2 class="text-[15px] font-semibold">{t(linksOnly ? 'shareLinks' : 'storageTitle')}</h2><p class="mt-0.5 text-[12px] text-faint">{t('storageSubtitle')}</p></div>
    <button class={buttonClass('secondary')} disabled={busy || !unlocked} onclick={() => refresh('fresh')} data-testid="storage-refresh"><Icon name="sync" />{t('storageRefresh')}</button>
  </header>
  <div class="min-h-0 flex-1 overflow-y-auto p-5">
    {#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
    {#if report?.loaded}
      <dl class="mb-4 grid grid-cols-2 gap-3 border-b border-line pb-4 @min-[700px]:grid-cols-4">
        {#each [['storageUsed', report.usage.committed], ['storageReserved', report.usage.reserved], ['storagePending', report.pending_release], ['storageLimit', report.usage.lifecycle?.storage_limit]] as [key, bytes]}
          <div><dt class="min-h-9 text-[11px] text-faint">{t(key as Label)}</dt><dd class="mt-1 text-[20px] font-semibold tabular-nums">{bytes == null ? '—' : size(bytes as number)}</dd></div>
        {/each}
      </dl>
      <div class="mb-4 flex flex-wrap gap-x-5 gap-y-1 text-[12px] text-muted">{#each Object.entries(report.categories ?? {}) as [category, bytes]}<span>{t(kinds[category] ?? 'storageOther')} <b class="font-medium text-ink tabular-nums">{size(bytes as number)}</b></span>{/each}</div>
      {#if !report.reconciliation?.consistent}<Notice tone="info">{t('storageReconciling')}</Notice>{/if}
      {#if report.errors?.length}<Notice tone="info">{t('storagePartial', { n: report.errors.length })}</Notice>{/if}
      {#if outcome.length}<Notice tone={outcome.some((r) => !r.accepted) ? 'warn' : 'info'}>{t('storageAccepted', { n: outcome.filter((r) => r.accepted).length })}{#each outcome.filter((r) => !r.accepted) as failed}<p class="mt-1">{failed.target.upload_id.slice(0, 12)}… · {failed.error}</p>{/each}</Notice>{/if}
      <div class="my-4 grid gap-3">
        <input bind:value={text} placeholder={t('storageSearch')} aria-label={t('storageSearch')} class="w-full text-[13px]" />
        <div class="grid grid-cols-2 items-end gap-2 @min-[800px]:grid-cols-5">
          {#if !linksOnly}<div class="min-w-0"><p class="mb-1 text-[11px] text-faint">{t('storageType')}</p><Select bind:value={kind} label={t('storageType')} class="w-full" options={[{value:'',label:t('storageAll')}, ...Object.entries(kinds).map(([value,key]) => ({value,label:t(key)}))]} /></div>{/if}
          <div class="min-w-0"><p class="mb-1 text-[11px] text-faint">{t('storageState')}</p><Select bind:value={phase} label={t('storageState')} class="w-full" options={[{value:'',label:t('storageAll')}, ...Object.entries(phases).map(([value,key]) => ({value,label:t(key)}))]} /></div>
          <div class="min-w-0"><p class="mb-1 text-[11px] text-faint">{t('storageChannel')}</p><Select bind:value={resource} label={t('storageChannel')} class="w-full" options={[{value:'',label:t('storageAll')}, ...[...new Set<string>(all.filter((e: any) => e.owner_kind === 'channel').map((e: any) => e.resource))].map(value => ({value,label:all.find((e: any) => e.resource === value)?.channel_name ?? value.slice(0,12)+'…'}))]} /></div>
          <label class="min-w-0 text-[11px] text-faint">{t('storageBefore')}<input type="date" bind:value={before} class="mt-1 w-full min-w-0 text-[12px]" /></label>
          <div class="min-w-0"><p class="mb-1 text-[11px] text-faint">{t('storageSort')}</p><Select bind:value={sort} label={t('storageSort')} class="w-full" options={[{value:'size',label:t('storageBySize')},{value:'date',label:t('storageByDate')}]} /></div>
        </div>
      </div>
      <div class="mb-2 flex items-center gap-3 text-[12px]">
        <button class="text-forest hover:underline" onclick={() => selected = shown.filter(e => e.can_delete || e.can_cancel || e.can_revoke).map(e => e.id)}>{t('storageSelectVisible')}</button>
        <span class="flex-1 text-faint">{rows.length} · {report.refreshed_at ? when(report.refreshed_at) : ''}</span>
        {#if picked.length}<button class={buttonClass('danger','sm')} onclick={() => confirmation = picked} disabled={busy} data-testid="storage-bulk-delete">{t('storageClean', { n: picked.length, bytes: size(releaseBytes(picked)) })}</button>{/if}
      </div>
      <ul class="divide-y divide-line" data-testid="storage-list">
        {#each shown as entry (entry.id)}
          <li class="flex min-w-0 flex-wrap items-center gap-3 px-3 py-3" data-upload={entry.upload_id}>
            <Checkbox checked={selected.includes(entry.id)} disabled={busy || !(entry.can_delete || entry.can_cancel || entry.can_revoke)} onCheckedChange={(on) => selected = on ? [...selected, entry.id] : selected.filter(id => id !== entry.id)} />
            <div class="min-w-[160px] flex-1">
              <p class="truncate text-[13px] font-medium">{entry.name ?? `${t('storageEncrypted')} · ${entry.upload_id?.slice(0,12)}…`}</p>
              <p class="mt-0.5 text-[11.5px] text-faint">{t(kinds[entry.kind] ?? 'storageOther')} · {entry.channel_name ?? (entry.owner_kind === 'channel' ? entry.resource.slice(0,12)+'…' : t('storagePersonal'))} · {t('storagePayer', { payer: entry.payer === report.account ? t('storageMe') : entry.payer?.slice(0,12)+'…' })}</p>
              {#if entry.share}<p class="mt-0.5 text-[11.5px] text-faint">{t(entry.share.state === 'revoked' ? 'linkStateRevoked' : entry.share.state === 'expired' ? 'linkStateExpired' : entry.share.state === 'exhausted' ? 'storageExhausted' : 'linkStateActive')} · {t('linkDownloads',{n:entry.share.downloads ?? 0})} · {t('linkExpires',{time:when(entry.share.expires_at)})}</p>{/if}
              {#if entry.metadata_error}<p class="text-[11.5px] text-warn">{t('storageMetadataUnavailable')}</p>{/if}
            </div>
            <div class="min-w-[90px] text-end"><p class="text-[13px] font-medium tabular-nums">{size(entry.bytes ?? 0)}</p><p class="mt-0.5 text-[11.5px] text-faint" data-testid="storage-phase">{label(entry)}</p></div>
            <div class="flex flex-wrap justify-end gap-1">
              {#if entry.source_file && entry.source_available}<button class={buttonClass('ghost','xs')} onclick={() => onSource(entry.source_file)}>{t('storageSource')}</button>{/if}
              {#if entry.owner_kind === 'channel'}<button class={buttonClass('ghost','xs')} onclick={() => onChannel(entry.resource)}>{t('storageChannel')}</button>{/if}
              {#if entry.url && entry.share?.state === 'active'}<button class={buttonClass('ghost','xs')} onclick={() => copyText(entry.url)}>{t('linkCopy')}</button>{/if}
              {#if entry.recoverable && ['staging','unpublished'].includes(entry.phase)}<button class={buttonClass('secondary','xs')} disabled={busy} onclick={() => act([entry],'recover')}>{t('storageResume')}</button>{/if}
              {#if ['deleting','retrying','releasing','cancelling'].includes(entry.phase)}<button class={buttonClass('secondary','xs')} disabled={busy} onclick={() => act([entry],'retry')}>{t('storageRetry')}</button>{/if}
              {#if entry.can_delete || entry.can_cancel || entry.can_revoke}<button class={buttonClass('ghost','xs','text-danger')} disabled={busy} onclick={() => confirmation=[entry]} data-testid="storage-delete">{t(entry.can_cancel ? 'cancel' : entry.can_revoke ? 'linkRevoke' : 'deleteCloudCopy')}</button>{/if}
            </div>
          </li>
        {/each}
      </ul>
      {#if shown.length < rows.length}<button class={buttonClass('secondary','sm','mt-3')} onclick={() => limit += 300}>{t('moreRows',{n:rows.length-shown.length})}</button>{/if}
      {#if !rows.length}<p class="py-12 text-center text-[13px] text-faint">{t('storageEmpty')}</p>{/if}
    {:else}<p class="py-12 text-center text-[13px] text-faint">{t('loading')}</p>{/if}
  </div>
</section>
<Dialog bind:open={() => !!confirmation, next => !next && (confirmation=null)} title={t('storageConfirmTitle')} icon="cloud-off" tone="danger" testid="storage-confirm">
  <Notice tone="warn">{t('storageConfirmHint')}</Notice>
  <p class="mt-3 text-[13px]">{t('storageClean', { n: confirmation?.length ?? 0, bytes:size(releaseBytes(confirmation ?? [])) })}</p>
  {#snippet footer()}<button class={buttonClass('ghost')} onclick={() => confirmation=null}>{t('cancel')}</button><button class={buttonClass('danger')} disabled={busy} onclick={() => confirmation && act(confirmation,'delete')} data-testid="storage-confirm-delete">{t('deleteCloudCopy')}</button>{/snippet}
</Dialog>
