<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import Icon from '../components/Icon.svelte'
  import Notice from '../components/page/Notice.svelte'
  import { t } from '../i18n/index.svelte'
  import { call, onChange, size, when } from '../rpc'
  import { localOnlyRisk, type LocalOnly } from './files/files'

  /**
   * What only this device holds: file versions without a cloud backup,
   * changes not yet synced, and a sync that stopped succeeding. Shown where
   * losing or giving up this device would lose them; `always` shows the
   * figures even when nothing is at risk, `compact` fits a side list.
   */
  let { always = false, compact = false, testid }: { always?: boolean; compact?: boolean; testid?: string } = $props()

  let value = $state<LocalOnly | null>(null)
  let alive = true
  onDestroy(() => { alive = false })
  const risk = $derived(value ? localOnlyRisk(value, Date.now()) : null)
  const lines = $derived.by(() => {
    if (!value || !risk) return []
    const shown: string[] = []
    if (risk.unbacked) shown.push(t('localOnlyUnbacked', { n: value.unbacked_versions, size: size(value.unbacked_bytes) }))
    if (risk.pending) shown.push(t('localOnlyPending', { n: value.pending_sync }))
    if (risk.never) shown.push(t('localOnlyNever'))
    else if (risk.stale) shown.push(t('localOnlyStale', { when: when(value.last_sync_ok_at) }))
    if (!risk.any) shown.push(t('localOnlyClear', { n: value.versions, size: size(value.bytes), when: when(value.last_sync_ok_at) }))
    return shown
  })

  async function load() {
    const next = await call<LocalOnly>('files/local_only').catch(() => null)
    if (alive) value = next
  }

  onMount(() => {
    void load()
    return onChange(['files/changed', 'sync/changed', 'session/changed'], () => void load())
  })
</script>

{#if risk && (risk.any || (always && !compact))}
  {#if compact}
    <p class="flex items-start gap-1.5 border-t border-line px-3.5 py-2 text-[11px] text-warn" data-testid={testid}>
      <Icon name="warning" class="mt-px size-3.5 shrink-0" /><span>{lines.join(' ')}</span>
    </p>
  {:else}
    <Notice tone={risk.any ? 'warn' : 'info'} icon={risk.any ? 'warning' : 'cloud-up'} {testid}>
      {#each lines as line (line)}<p>{line}</p>{/each}
    </Notice>
  {/if}
{/if}
