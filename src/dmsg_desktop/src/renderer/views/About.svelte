<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import type { AppUpdates } from '../../shared/contract'
  import Icon from '../components/Icon.svelte'
  import Checkbox from '../components/Checkbox.svelte'
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
  let acting = $state(false)
  let changing = $state(false)
  let beta = $state(false)
  let channelDirty = $state(false)
  let channelError = $state('')

  const update = $derived(info?.state)
  const busy = $derived(acting || info?.applying || update?.kind === 'checking' || update?.kind === 'downloading')
  const waitingStable = $derived(info?.channel === 'stable' && info.version.includes('-'))

  const reload = single(async () => {
    if (!alive) return
    try {
      const next = await bridge(() => window.dmsg.updates())
      if (!alive) return
      info = next
      if (!changing && !channelDirty) beta = next.channel === 'beta'
      error = ''
    } catch (e) {
      if (!alive) return
      error = errorText(e)
    }
  })

  /** A check or an update; the `updates/changed` events it causes reload the section. */
  async function act(work: () => Promise<unknown>) {
    if (busy) return
    acting = true
    error = ''
    try {
      await bridge(work)
    } catch (e) {
      if (!alive) return
      error = errorText(e)
    } finally { if (alive) acting = false }
  }

  async function saveChannel() {
    if (changing || info?.applying) return
    changing = true
    channelError = ''
    try {
      const next = await bridge(() => window.dmsg.setUpdateChannel(beta ? 'beta' : 'stable'))
      if (!alive) return
      info = next
      beta = next.channel === 'beta'
      channelDirty = false
    } catch (e) {
      if (alive) channelError = errorText(e)
    } finally { if (alive) changing = false }
  }

  onMount(() => {
    void reload()
    return onChange(['updates/changed'], reload)
  })
</script>

{#if error}<Notice failure={error} tone="danger">{error}</Notice>{/if}
<Section title={t('aboutApp')} description={t('aboutHint')} icon="info" testid="about">
  <div class="mb-3 flex flex-wrap gap-2">
    {#if update?.kind === 'ready'}
      <button class={buttonClass('primary', 'sm')} disabled={busy} onclick={() => act(() => window.dmsg.applyUpdate())} data-testid="apply-update">
        <Icon name="download" />{t('restartToUpdate')}
      </button>
    {:else if update?.kind === 'available' && !info?.homebrew}
      <button class={buttonClass('primary', 'sm')} disabled={busy} onclick={() => act(() => window.dmsg.applyUpdate())} data-testid="apply-update">
        <Icon name="external" />{t('download')}
      </button>
    {/if}
    {#if update && update.kind !== 'off' && update.kind !== 'ready'}
      <button class={buttonClass('secondary', 'sm')} disabled={busy} aria-busy={busy} onclick={() => act(() => window.dmsg.checkUpdates())} data-testid="check-updates">
        <Icon name="sync-now" />{t('checkUpdates')}
      </button>
    {/if}
  </div>
  <dl>
    <Fact label={t('appVersion')} testid="app-version"><bdi dir="ltr">{info?.version ?? '-'}</bdi></Fact>
    <Fact label={t('updateChannel')} testid="update-channel">{info ? (info.channel === 'beta' ? 'Beta' : t('updateStable')) : '-'}</Fact>
    <Fact label={t('updates')} testid="update-state">
      {#if !update}
        -
      {:else if update.kind === 'off'}
        <span class="text-muted">{t('updatesOff')}</span>
      {:else if update.kind === 'checking'}
        <span class="shimmer">{t('updateChecking')}</span>
      {:else if update.kind === 'latest'}
        <Pill tone={waitingStable ? 'warn' : 'ok'}><Icon name={waitingStable ? 'time' : 'check'} />{t(waitingStable ? 'updateWaitingStable' : 'updateLatest')}</Pill>
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
  <div class="mt-4 grid gap-2" aria-busy={changing}>
    <Checkbox checked={beta} label={t('betaUpdates')} descriptionId="beta-updates-hint" testid="beta-updates"
      disabled={!info || changing || info.applying || (info.homebrew && !beta)}
      onCheckedChange={(next) => { beta = next; channelDirty = true; void saveChannel() }}>
      {t('betaUpdates')}
    </Checkbox>
    <p id="beta-updates-hint" class="text-xs text-muted">{t('betaUpdatesHint')}</p>
    {#if channelError}
      <Notice failure={channelError} tone="danger" testid="update-channel-error">{channelError}</Notice>
      <button class={buttonClass('secondary', 'sm', 'justify-self-start')} disabled={changing} onclick={() => void saveChannel()} data-testid="retry-update-channel">{t('retry')}</button>
    {/if}
  </div>
  {#if info?.homebrew}
    <Notice class="mt-3" testid="update-homebrew">{t('updateHomebrew')}</Notice>
  {:else if update?.kind === 'available'}
    <Notice class="mt-3">{t('updateManualHint')}</Notice>
  {:else if update?.kind === 'ready'}
    <Notice class="mt-3">{t('updateReadyHint')}</Notice>
  {:else if update?.kind === 'error'}
    <Notice tone="warn" class="mt-3">{update.message}</Notice>
  {/if}
</Section>
