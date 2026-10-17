<script lang="ts">
  import AppearanceSwitcher from '../components/AppearanceSwitcher.svelte'
  import Icon from '../components/Icon.svelte'
  import Tooltip from '../components/Tooltip.svelte'
  import { buttonClass, cn } from '../components/ui'
  import LocaleSwitcher from '../i18n/LocaleSwitcher.svelte'
  import { sessionLabel, t } from '../i18n/index.svelte'
  import { when } from '../rpc'
  import { lock, nextStep, sync } from './session'

  /**
   * The gate rail at the foot of the sidebar: whether the device is locked
   * and when the idle lock comes, what has not reached the relay yet,
   * background online while it is on (PRD §6–§7 ask for both to stay in
   * view), and the daemon connection.
   */
  let {
    status,
    connected,
    connectionError,
    collapsed = false,
    mod
  }: { status: any; connected: boolean; connectionError?: string; collapsed?: boolean; mod: string } = $props()

  let now = $state(Date.now())
  let syncing = $state(false)

  $effect(() => {
    if (!status?.lock_at) return
    now = Date.now()
    const timer = setInterval(() => (now = Date.now()), 1000)
    return () => clearInterval(timer)
  })

  const session = $derived<string>(status?.state ?? '')
  const unlocked = $derived(session === 'unlocked')
  /** A login, an unlock or the setup: what opens the device from here. */
  const step = $derived(nextStep(status))
  const left = $derived(unlocked && status?.lock_at ? Math.max(0, status.lock_at - now) : null)
  const clock = $derived.by(() => {
    if (left === null) return ''
    const seconds = Math.ceil(left / 1000)
    return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`
  })
  const share = $derived(left !== null && status?.idle_lock_ms ? Math.min(1, left / status.idle_lock_ms) : 1)
  const pending = $derived<number>(status?.pending_sync ?? 0)
  const syncText = $derived(syncing ? t('syncing') : pending ? t('syncPending', { n: pending }) : t('synced'))
  const syncHint = $derived(status?.last_sync?.at ? `${t('sync')} · ${when(status.last_sync.at)}` : t('sync'))

  async function syncNow() {
    if (syncing) return
    syncing = true
    await sync()
    syncing = false
  }
</script>

{#snippet ring()}
  <span
    class="size-3.5 shrink-0 rounded-full"
    style:background="conic-gradient(var(--forest) {share}turn, var(--line) 0)"
    style:mask="radial-gradient(circle 4.5px, transparent 97%, #000 100%)"
    aria-hidden="true"
  ></span>
{/snippet}

{#if collapsed}
  <div class="flex flex-col items-center gap-1 pb-1" data-testid="gate-rail">
    <Tooltip
      content={left !== null ? `${sessionLabel(session)} · ${t('locksIn', { time: clock })}` : step ? `${sessionLabel(session)} · ${t(step.label)}` : sessionLabel(session)}
      side="right"
    >
      {#snippet children(props)}
        <button
          {...props}
          class={buttonClass('ghost', 'sm', 'relative size-8 px-0')}
          aria-label={sessionLabel(session)}
          onclick={() => (unlocked ? lock() : step?.run())}
          data-testid="gate-toggle"
        >
          <Icon name={unlocked ? 'unlock' : 'lock'} />
        </button>
      {/snippet}
    </Tooltip>
    {#if status?.account}
      <Tooltip content={`${syncText} · ${syncHint}`} side="right">
        {#snippet children(props)}
          <button {...props} class={buttonClass('ghost', 'sm', 'relative size-8 px-0')} aria-label={syncText} onclick={syncNow}>
            <Icon name="sync" />
            {#if pending}<span class="absolute top-1.5 end-1.5 size-1.5 rounded-full bg-gem"></span>{/if}
          </button>
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
    <LocaleSwitcher />
    <AppearanceSwitcher />
    <Tooltip content={connected ? t('connected') : connectionError || t('disconnected')} side="right">
      {#snippet children(props)}
        <span {...props} class="grid size-8 place-items-center"><span class={cn('size-2 rounded-full', connected ? 'bg-ok' : 'bg-danger')}></span></span>
      {/snippet}
    </Tooltip>
  </div>
{:else}
  <div class="grid grid-cols-1 gap-2 rounded-xl border border-line bg-surface py-2.5 ps-3 pe-2 text-[12.5px] shadow-xs" data-testid="gate-rail">
    <div class="flex min-h-6 items-center gap-2">
      <Icon name={unlocked ? 'unlock' : 'lock'} class="text-faint" />
      <b class="font-semibold" data-testid="gate-state">{session ? sessionLabel(session) : '-'}</b>
      {#if left !== null}
        <Tooltip content={t('locksIn', { time: clock })}>
          {#snippet children(props)}
            <span {...props} class="ms-auto inline-flex items-center gap-1.5 font-mono text-[11.5px] text-faint tabular-nums">{@render ring()}{clock}</span>
          {/snippet}
        </Tooltip>
      {:else if step}
        <button class={buttonClass(session === 'logged_out' ? 'primary' : 'secondary', 'xs', 'ms-auto')} onclick={step.run} data-testid="gate-unlock">
          {t(step.label)}
        </button>
      {/if}
    </div>
    {#if status?.account}
      <Tooltip content={syncHint}>
        {#snippet children(props)}
          <button
            {...props}
            class="-my-0.5 flex min-h-6 items-center gap-2 rounded-md text-start text-ink-2 outline-none hover:text-ink focus-visible:ring-3 focus-visible:ring-focus/35"
            onclick={syncNow}
            data-testid="gate-sync"
          >
            <Icon name="sync" class="text-faint" />
            <span class={cn(syncing && 'shimmer')}>{syncText}</span>
          </button>
        {/snippet}
      </Tooltip>
    {/if}
    {#if status?.background_online}
      <div class="flex min-h-6 items-center gap-2 text-gem" data-testid="gate-background">
        <Icon name="background" />
        <span>{t('backgroundOnline')} · {t('on')}{status?.resident ? ` · ${t('deviceKeyResident')}` : ''}</span>
      </div>
    {/if}
    <div class="flex items-center gap-1 border-t border-line pt-2">
      <span class={cn('size-[7px] shrink-0 rounded-full', connected ? 'bg-ok' : 'bg-danger')}></span>
      <span class={cn('ms-1 min-w-0 flex-1 truncate text-[11.5px]', connected ? 'text-faint' : 'text-danger')}>{connected ? t('connected') : t('disconnected')}</span>
      <LocaleSwitcher />
      <AppearanceSwitcher />
      {#if unlocked}
        <Tooltip content={`${t('lock')} · ${mod}L`}>
          {#snippet children(props)}
            <button {...props} class={buttonClass('ghost', 'sm', 'size-7 px-0')} aria-label={t('lock')} onclick={lock} data-testid="gate-lock">
              <Icon name="lock" />
            </button>
          {/snippet}
        </Tooltip>
      {/if}
    </div>
    {#if !connected && connectionError}
      <p class="text-[11.5px] break-words whitespace-pre-wrap text-muted" data-testid="connection-error">{connectionError}</p>
    {/if}
  </div>
{/if}
