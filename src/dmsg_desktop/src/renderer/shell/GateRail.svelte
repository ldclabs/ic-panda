<script lang="ts">
  import { onMount } from 'svelte'
  import Progress from '../components/Progress.svelte'
  import Icon from '../components/Icon.svelte'
  import Tooltip from '../components/Tooltip.svelte'
  import { iconButtonClass, buttonClass, cn } from '../components/ui'
  import { sessionLabel, t } from '../i18n/index.svelte'
  import { bridge, onChange, size, when } from '../rpc'
  import { syncShown, syncState } from '../sync-state'
  import { lock, nextStep, sync, unlocking } from './session'

  /**
   * The gate rail at the foot of the sidebar: the step that opens the device
   * when it is not unlocked, what has not reached the relay yet, background
   * online while it is on (PRD §6–§7 ask for both to stay in view) and the
   * Lock button while it is unlocked. A lost daemon covers the whole window.
   */
  let { status, collapsed = false, mod }: { status: any; collapsed?: boolean; mod: string } = $props()

  let requested = $state(false)
  let now = $state(Date.now())
  /** A downloaded update waits for a restart, here in view rather than only in About. */
  let updateReady = $state(false)
  const readUpdate = () => bridge(() => window.dmsg.updates()).then((info) => (updateReady = info.state?.kind === 'ready'), () => {})
  onMount(() => {
    const timer = setInterval(() => { now = Date.now() }, 1000)
    void readUpdate()
    const off = onChange(['updates/changed'], readUpdate)
    return () => { clearInterval(timer); off() }
  })
  let syncingSince = $state<number | undefined>()
  $effect(() => {
    if (!status?.syncing) syncingSince = undefined
    else if (syncingSince === undefined) syncingSince = Date.now()
  })
  const syncing = $derived(syncShown(status, requested, syncingSince, now))

  const session = $derived<string>(status?.state ?? '')
  const unlocked = $derived(session === 'unlocked')
  /** A login, an unlock or the setup: what opens the device from here. */
  const step = $derived(nextStep(status))
  const presentation = $derived(syncState({ ...status, syncing }, now))
  const pending = $derived(presentation.pending)
  const failed = $derived(presentation.failed)
  const transient = $derived(presentation.transient)
  const syncText = $derived(t(presentation.label, { n: pending }))
  const showPending = $derived(pending > 0 && presentation.label !== 'syncPending')
  const lastSuccess = $derived(presentation.okAt ? t('syncLastSuccess', { time: when(presentation.okAt) }) : t('localOnlyNever'))
  const retryText = $derived(presentation.retryAt ? t('syncRetryAt', { time: when(presentation.retryAt) }) : '')
  const syncHint = $derived([syncText, presentation.initial && syncing ? t(presentation.stage) : '', showPending ? t('syncPending', { n: pending }) : '', lastSuccess, retryText, t('syncScope')].filter(Boolean).join(' · '))

  async function syncNow() {
    if (syncing || presentation.coolingDown) return
    requested = true
    try { await sync() } finally { requested = false }
  }
</script>

{#if collapsed}
  <div class="flex flex-col items-center gap-1 pb-1" data-testid="gate-rail">
    <Tooltip
      content={step ? `${sessionLabel(session)} · ${t(step.label)}` : sessionLabel(session)}
      side="right"
    >
      {#snippet children(props)}
        <button
          {...props}
          class={iconButtonClass('ghost', 'md', 'relative')}
          aria-label={sessionLabel(session)}
          disabled={$unlocking}
          aria-busy={$unlocking}
          onclick={() => (unlocked ? lock() : step?.run())}
          data-testid="gate-toggle"
        >
          <Icon name={$unlocking ? 'pending' : unlocked ? 'unlock' : 'lock'} class={$unlocking ? 'motion-safe:animate-spin' : ''} />
        </button>
      {/snippet}
    </Tooltip>
    {#if status?.account}
      <Tooltip content={syncHint} side="right">
        {#snippet children(props)}
          <button {...props} class={iconButtonClass('ghost', 'md', 'relative')} aria-label={syncText} disabled={syncing || $unlocking || presentation.paused || presentation.coolingDown} onclick={syncNow} data-testid="gate-sync">
            <Icon name={syncing ? 'pending' : 'sync'} class={cn(syncing && 'motion-safe:animate-spin', failed && (transient ? 'text-warn' : 'text-danger'))} />
            {#if pending}<span class="absolute top-1.5 end-1.5 size-1.5 rounded-full bg-gem"></span>{/if}
          </button>
        {/snippet}
      </Tooltip>
    {/if}
    {#if updateReady}
      <Tooltip content={t('restartToUpdate')} side="right">
        {#snippet children(props)}
          <button {...props} class={iconButtonClass('ghost', 'md', 'text-forest')} aria-label={t('restartToUpdate')} onclick={() => void window.dmsg.applyUpdate()} data-testid="gate-update"><Icon name="download" /></button>
        {/snippet}
      </Tooltip>
    {/if}
    {#if status?.background_online}
      <Tooltip content={`${t('backgroundOnline')} · ${t('on')}`} side="right">
        {#snippet children(props)}
          <span {...props} class="grid size-8 place-items-center text-gem"><Icon name="background" /></span>
        {/snippet}
      </Tooltip>
    {/if}
  </div>
{:else}
  <div class="grid grid-cols-1 gap-2 border-t border-line py-2.5 ps-3 pe-2 text-[12.5px]" data-testid="gate-rail">
    {#if step}
      <!-- A long state moves the step to its own line instead of squeezing both. -->
      <div class="flex min-h-6 flex-wrap items-center gap-2">
        <span class="flex min-w-0 items-center gap-2">
          <Icon name="lock" class="shrink-0 text-faint" />
          <b class="font-semibold" data-testid="gate-state">{session ? sessionLabel(session) : '-'}</b>
        </span>
        <button class={buttonClass(session === 'logged_out' ? 'primary' : 'secondary', 'xs', 'ms-auto')} disabled={$unlocking} aria-busy={$unlocking} onclick={step.run} data-testid="gate-unlock">
          {$unlocking ? t('unlocking') : t(step.label)}
        </button>
      </div>
    {/if}
    {#if status?.account}
      <Tooltip content={syncHint}>
        {#snippet children(props)}
          <button
            {...props}
            class={buttonClass('ghost', 'xs', '-my-0.5 h-auto min-h-6 justify-start gap-2 px-0 text-start whitespace-normal')}
            onclick={syncNow}
            disabled={syncing || $unlocking || presentation.paused || presentation.coolingDown}
            data-testid="gate-sync"
          >
            <Icon name={syncing ? 'pending' : 'sync'} class={cn('shrink-0 text-faint', syncing && 'motion-safe:animate-spin', failed && (transient ? 'text-warn' : 'text-danger'))} />
            <span role="status" class={cn(syncing && 'shimmer', failed && (transient ? 'text-warn' : 'text-danger'))}>{syncText}</span>
          </button>
        {/snippet}
      </Tooltip>
      {#if presentation.initial && !presentation.paused}
        <div class="grid gap-1.5 ps-6 text-[11.5px] text-faint" data-testid="gate-sync-initial">
          {#if syncing}
            <Progress value={presentation.progress?.total > 0 ? presentation.progress.entries : null} max={presentation.progress?.total || 100} label={t(presentation.stage)} testid="gate-sync-progress" />
          {/if}
          {#if presentation.progress?.total > 0}<span class="tabular-nums"><bdi dir="ltr">{presentation.progress.entries} / {presentation.progress.total}</bdi></span>{/if}
          {#if presentation.progress?.downloaded_bytes > 0}<span><Icon name="cloud-down" class="inline size-3" /> {size(presentation.progress.downloaded_bytes)}</span>{/if}
        </div>
      {/if}
      {#if failed || showPending || retryText}
        <div class="grid gap-0.5 ps-6 text-[11.5px] text-faint" data-testid="gate-sync-detail">
          {#if showPending}<span data-testid="gate-sync-pending">{t('syncPending', { n: pending })}</span>{/if}
          {#if failed}<span data-testid="gate-sync-success">{lastSuccess}</span>{/if}
          {#if retryText}<span data-testid="gate-sync-retry">{retryText}</span>{/if}
          {#if status?.last_sync?.upload_error === 'QUOTA_EXCEEDED'}
            <button class={buttonClass('ghost', 'xs', 'h-auto justify-start px-0 text-start whitespace-normal')} onclick={() => (location.hash = '/plan')} data-testid="gate-sync-storage">{t('openPlan')}</button>
          {/if}
        </div>
      {/if}
    {/if}
    {#if status?.background_online}
      <div class="flex min-h-6 items-center gap-2 text-gem" data-testid="gate-background">
        <Icon name="background" />
        <span>{t('backgroundOnline')} · {t('on')}{status?.resident ? ` · ${t('deviceKeyResident')}` : ''}</span>
      </div>
    {/if}
    {#if updateReady}
      <button class={buttonClass('ghost', 'xs', '-my-0.5 h-auto min-h-6 justify-start gap-2 px-0 text-start text-forest')} onclick={() => void window.dmsg.applyUpdate()} data-testid="gate-update">
        <Icon name="download" class="shrink-0" /><span class="min-w-0 flex-1 truncate">{t('restartToUpdate')}</span>
      </button>
    {/if}
    {#if unlocked}
      <button class={buttonClass('ghost', 'xs', '-my-0.5 h-auto min-h-6 justify-start gap-2 px-0 text-start')} onclick={lock} data-testid="gate-lock">
        <Icon name="lock" class="shrink-0 text-faint" /><span class="min-w-0 flex-1 truncate">{t('lock')}</span>
        <kbd class="font-mono text-[10.5px] font-normal text-faint">{mod}L</kbd>
      </button>
    {/if}
  </div>
{/if}
