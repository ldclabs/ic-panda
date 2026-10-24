<script lang="ts">
  import Icon from '../components/Icon.svelte'
  import Tooltip from '../components/Tooltip.svelte'
  import { iconButtonClass, buttonClass, cn } from '../components/ui'
  import { sessionLabel, t } from '../i18n/index.svelte'
  import { when } from '../rpc'
  import { lock, nextStep, sync, unlocking } from './session'

  /**
   * The gate rail at the foot of the sidebar: the step that opens the device
   * when it is not unlocked, what has not reached the relay yet, background
   * online while it is on (PRD §6–§7 ask for both to stay in view), the Lock
   * button while it is unlocked, and the daemon connection.
   */
  let {
    status,
    connected,
    connectionError,
    collapsed = false,
    mod
  }: { status: any; connected: boolean; connectionError?: string; collapsed?: boolean; mod: string } = $props()

  let requested = $state(false)
  const syncing = $derived(requested || !!status?.syncing)

  const session = $derived<string>(status?.state ?? '')
  const unlocked = $derived(session === 'unlocked')
  /** A login, an unlock or the setup: what opens the device from here. */
  const step = $derived(nextStep(status))
  const pending = $derived<number>(status?.pending_sync ?? 0)
  const failed = $derived(!syncing && !!status?.last_sync?.error)
  const syncText = $derived(syncing ? t('syncing') : failed ? t('syncFailed') : pending ? t('syncPending', { n: pending }) : status?.last_sync?.at ? t('accountSynced') : t('sync'))
  const syncHint = $derived(status?.last_sync?.at ? `${t('sync')} · ${when(status.last_sync.at)}${failed ? ` · ${status.last_sync.error}` : ''}` : t('sync'))

  async function syncNow() {
    if (syncing) return
    requested = true
    await sync()
    requested = false
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
          <Icon name={$unlocking ? 'sync' : unlocked ? 'unlock' : 'lock'} class={$unlocking ? 'motion-safe:animate-spin' : ''} />
        </button>
      {/snippet}
    </Tooltip>
    {#if status?.account}
      <Tooltip content={`${syncText} · ${syncHint}`} side="right">
        {#snippet children(props)}
          <button {...props} class={iconButtonClass('ghost', 'md', 'relative')} aria-label={syncText} disabled={syncing || $unlocking} onclick={syncNow}>
            <Icon name="sync" class={cn(syncing && 'motion-safe:animate-spin', failed && 'text-danger')} />
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
    <Tooltip content={connected ? t('connected') : connectionError || t('disconnected')} side="right">
      {#snippet children(props)}
        <span {...props} class="grid size-8 place-items-center"><span class={cn('size-2 rounded-full', connected ? 'bg-ok' : 'bg-danger')}></span></span>
      {/snippet}
    </Tooltip>
  </div>
{:else}
  <div class="grid grid-cols-1 gap-2 border-t border-line py-2.5 ps-3 pe-2 text-[12.5px]" data-testid="gate-rail">
    {#if step}
      <div class="flex min-h-6 items-center gap-2">
        <Icon name="lock" class="text-faint" />
        <b class="font-semibold" data-testid="gate-state">{session ? sessionLabel(session) : '-'}</b>
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
            class="-my-0.5 flex min-h-6 items-center gap-2 rounded-md text-start text-ink-2 outline-none hover:text-ink focus-visible:ring-3 focus-visible:ring-focus/35"
            onclick={syncNow}
            disabled={syncing || $unlocking}
            data-testid="gate-sync"
          >
            <Icon name="sync" class={cn('text-faint', syncing && 'motion-safe:animate-spin', failed && 'text-danger')} />
            <span role="status" class={cn(syncing && 'shimmer', failed && 'text-danger')}>{syncText}</span>
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
    <div class="flex items-center gap-1">
      <span class={cn('size-[7px] shrink-0 rounded-full', connected ? 'bg-ok' : 'bg-danger')}></span>
      <span class={cn('ms-1 min-w-0 flex-1 truncate text-[11.5px]', connected ? 'text-faint' : 'text-danger')}>{connected ? t('connected') : t('disconnected')}</span>
      {#if unlocked}
        <Tooltip content={`${t('lock')} · ${mod}L`}>
          {#snippet children(props)}
            <button {...props} class={iconButtonClass('ghost', 'md')} aria-label={t('lock')} onclick={lock} data-testid="gate-lock">
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
