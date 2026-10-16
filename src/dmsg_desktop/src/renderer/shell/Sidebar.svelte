<script lang="ts">
  import Icon from '../components/Icon.svelte'
  import Tooltip from '../components/Tooltip.svelte'
  import { buttonClass, cn } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import AccountSwitcher from './AccountSwitcher.svelte'
  import GateRail from './GateRail.svelte'
  import { entryOf, groups, views, type View } from './nav'

  /**
   * The main window's sidebar: the account at the top, the views in the
   * groups of PRD §4, and the gate rail at the foot. Collapsed, it keeps the
   * icons and names them in tooltips. A migration that was started and not
   * imported shows a notice until it is dismissed.
   */
  let {
    route,
    status,
    connected,
    connectionError,
    collapsed = $bindable(false),
    inset,
    mod,
    legacyNotice,
    onDismissLegacy,
    go
  }: {
    route: string
    status: any
    connected: boolean
    connectionError?: string
    collapsed?: boolean
    inset: boolean
    mod: string
    legacyNotice: boolean
    onDismissLegacy: () => void
    go: (path: string) => void
  } = $props()

  const entry = $derived(entryOf(route))
</script>

{#snippet item(view: View)}
  {@const active = entry === view}
  <Tooltip content={view.key ? `${t(view.label)} · ${mod}${view.key}` : t(view.label)} side="right" disabled={!collapsed}>
    {#snippet children(props)}
      <button
        {...props}
        class={cn(
          'flex items-center gap-2.5 rounded-lg text-start text-[13.5px] leading-snug text-ink-2 outline-none transition-colors duration-150 hover:bg-wash focus-visible:ring-3 focus-visible:ring-focus/35',
          collapsed ? 'relative size-9 justify-center' : 'min-h-8 px-2.5 py-1.5',
          active && 'bg-surface font-medium text-ink shadow-xs ring-1 ring-line hover:bg-surface'
        )}
        aria-current={active ? 'page' : undefined}
        aria-label={collapsed ? t(view.label) : undefined}
        onclick={() => go(view.path)}
      >
        <Icon name={view.icon} class={active ? 'text-forest' : 'text-faint'} />
        {#if !collapsed}<span class="min-w-0 flex-1">{t(view.label)}</span>{/if}
        {#if view === views.approvals && status?.approvals_waiting}
          <span
            class={cn(
              'inline-flex items-center gap-1.5 text-[11.5px] font-semibold text-gem tabular-nums',
              collapsed && 'absolute end-0.5 top-0.5 gap-1 text-[10px] leading-none'
            )}
            data-testid="waiting-count"><i class="size-[7px] rotate-45 rounded-[1px] bg-gem"></i>{status.approvals_waiting}</span
          >
        {/if}
      </button>
    {/snippet}
  </Tooltip>
{/snippet}

<aside
  class={cn(
    'flex min-h-0 shrink-0 flex-col border-e border-line px-2.5 pb-2.5 transition-[width] duration-200 ease-calm',
    inset ? 'bg-paper-dim/55' : 'bg-paper-dim',
    collapsed ? 'w-16 items-center' : 'w-[236px]'
  )}
>
  <!-- The window's drag area; on macOS the traffic lights sit at its start. -->
  <div class={cn('flex h-[52px] shrink-0 items-center gap-2 [-webkit-app-region:drag]', collapsed ? 'justify-center' : 'ps-2.5')}>
    {#if !inset && !collapsed}
      <!-- The Gate, as on dmsg.net: a post and a dome, one request waiting between them. -->
      <svg class="h-5 w-[17px] fill-forest" viewBox="0 0 42 48" aria-hidden="true">
        <mask id="brand-cut" maskUnits="userSpaceOnUse" x="0" y="0" width="42" height="48">
          <rect width="42" height="48" fill="#fff" />
          <path d="M12 17.5 18.5 24 12 30.5 5.5 24Z" stroke="#000" stroke-width="5" stroke-linejoin="round" />
        </mask>
        <g mask="url(#brand-cut)">
          <path d="M4 0h4.5A1.5 1.5 0 0 1 10 1.5v45A1.5 1.5 0 0 1 8.5 48H4a4 4 0 0 1-4-4V4a4 4 0 0 1 4-4Z" />
          <path d="M15.5 0H18a24 24 0 0 1 0 48h-2.5a1.5 1.5 0 0 1-1.5-1.5v-45A1.5 1.5 0 0 1 15.5 0Z" />
        </g>
        <path class="fill-gem" d="M12 17.5 18.5 24 12 30.5 5.5 24Z" />
      </svg>
      <span class="text-lg leading-none font-extrabold tracking-[-0.04em]">dMsg</span>
    {/if}
    <Tooltip content={collapsed ? t('expandSidebar') : t('collapseSidebar')} side="right">
      {#snippet children(props)}
        <button
          {...props}
          class={buttonClass('ghost', 'sm', cn('size-7 px-0 [-webkit-app-region:no-drag]', !collapsed && 'ms-auto'))}
          aria-label={collapsed ? t('expandSidebar') : t('collapseSidebar')}
          aria-expanded={!collapsed}
          onclick={() => (collapsed = !collapsed)}
          data-testid="sidebar-toggle"
        >
          <Icon name={collapsed ? 'sidebar-unfold' : 'sidebar-fold'} />
        </button>
      {/snippet}
    </Tooltip>
  </div>
  <AccountSwitcher {status} {collapsed} />
  {#if legacyNotice && !collapsed}
    <div class="mt-2 grid gap-1.5 rounded-xl bg-warn-pale p-2.5 text-[12.5px] text-warn" data-testid="legacy-notice">
      <div class="flex items-start gap-2">
        <Icon name="legacy" class="mt-0.5" />
        <span class="min-w-0 flex-1">{t('legacyNotice')}</span>
        <button class={buttonClass('ghost', 'xs', '-me-1 -mt-0.5 size-6 px-0 text-warn')} aria-label={t('dismiss')} onclick={onDismissLegacy}>
          <Icon name="close" />
        </button>
      </div>
      <button class={buttonClass('secondary', 'xs', 'justify-self-start border-transparent')} onclick={() => go(views.legacy.path)}>{t('open')}</button>
    </div>
  {/if}
  <nav class={cn('mt-1 flex min-h-0 flex-col gap-0.5 overflow-x-hidden overflow-y-auto', collapsed && 'items-center')} aria-label={t('views')}>
    {#each groups as group (group.label)}
      {#if collapsed}
        <div class="mx-auto my-2 h-px w-6 bg-line"></div>
      {:else}
        <div class="px-2.5 pt-3.5 pb-1 text-[11px] font-medium text-faint">{t(group.label)}</div>
      {/if}
      {#each group.views as view (view.path)}{@render item(view)}{/each}
    {/each}
    <div class="h-3"></div>
    {@render item(views.settings)}
  </nav>
  <div class={cn('mt-auto pt-3', collapsed ? 'w-full' : '')}>
    <GateRail {status} {connected} {connectionError} {collapsed} {mod} />
  </div>
</aside>
