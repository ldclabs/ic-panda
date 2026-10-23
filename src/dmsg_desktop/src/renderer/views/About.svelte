<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import type { AppUpdates } from '../../shared/contract'
  import Icon from '../components/Icon.svelte'
  import Fact from '../components/page/Fact.svelte'
  import Notice from '../components/page/Notice.svelte'
  import Pill from '../components/page/Pill.svelte'
  import Section from '../components/page/Section.svelte'
  import { buttonClass } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { bridge, errorText, onChange, single } from '../rpc'

  /** The app's version and its update, as the tray offers it; the main process checks and applies it. */
  let alive = true
  onDestroy(() => { alive = false })
  let info = $state<AppUpdates | null>(null)
  let error = $state('')

  const update = $derived(info?.state)
  const busy = $derived(update?.kind === 'checking' || update?.kind === 'downloading')

  const reload = single(async () => {
    if (!alive) return
    try {
      const next = await bridge(() => window.dmsg.updates())
      if (!alive) return
      info = next
      error = ''
    } catch (e) {
      if (!alive) return
      error = errorText(e)
    }
  })

  /** A check or an update; the `updates/changed` events it causes reload the section. */
  async function act(work: () => Promise<unknown>) {
    error = ''
    try {
      await bridge(work)
    } catch (e) {
      if (!alive) return
      error = errorText(e)
    }
  }

  onMount(() => {
    void reload()
    return onChange(['updates/changed'], reload)
  })
</script>

{#if error}<Notice tone="danger">{error}</Notice>{/if}
<Section title={t('aboutApp')} description={t('aboutHint')} icon="info" testid="about">
  {#snippet actions()}
    {#if update?.kind === 'ready'}
      <button class={buttonClass('primary', 'sm')} onclick={() => act(() => window.dmsg.applyUpdate())} data-testid="apply-update">
        <Icon name="download" />{t('restartToUpdate')}
      </button>
    {:else if update?.kind === 'available'}
      <button class={buttonClass('primary', 'sm')} onclick={() => act(() => window.dmsg.applyUpdate())} data-testid="apply-update">
        <Icon name="external" />{t('download')}
      </button>
    {/if}
    {#if update && update.kind !== 'off' && update.kind !== 'ready'}
      <button class={buttonClass('secondary', 'sm')} disabled={busy} aria-busy={busy} onclick={() => act(() => window.dmsg.checkUpdates())} data-testid="check-updates">
        <Icon name="sync-now" />{t('checkUpdates')}
      </button>
    {/if}
  {/snippet}
  <dl class="divide-y divide-line">
    <Fact label={t('appVersion')} testid="app-version">{info?.version ?? '-'}</Fact>
    <Fact label={t('updates')} testid="update-state">
      {#if !update}
        -
      {:else if update.kind === 'off'}
        <span class="text-muted">{t('updatesOff')}</span>
      {:else if update.kind === 'checking'}
        <span class="shimmer">{t('updateChecking')}</span>
      {:else if update.kind === 'latest'}
        <Pill tone="ok"><Icon name="check" />{t('updateLatest')}</Pill>
      {:else if update.kind === 'downloading'}
        <span class="shimmer">{t('updateDownloading', { version: update.version })}</span>
      {:else if update.kind === 'available'}
        <Pill tone="forest">{t('updateAvailable', { version: update.version })}</Pill>
      {:else if update.kind === 'ready'}
        <Pill tone="forest">{t('updateReady', { version: update.version })}</Pill>
      {:else if update.kind === 'error'}
        <Pill tone="warn">{t('updateFailed')}</Pill>
      {:else}
        <span class="text-muted">{t('updateIdle')}</span>
      {/if}
    </Fact>
  </dl>
  {#if update?.kind === 'available'}
    <Notice class="mt-3">{t('updateManualHint')}</Notice>
  {:else if update?.kind === 'ready'}
    <Notice class="mt-3">{t('updateReadyHint')}</Notice>
  {:else if update?.kind === 'error'}
    <Notice tone="warn" class="mt-3">{update.message}</Notice>
  {/if}
</Section>
