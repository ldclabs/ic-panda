<script lang="ts">
  import type { NoticeStatus } from '../../shared/notifications'
  import Brand from './Brand.svelte'
  import type { Snippet } from 'svelte'
  import Icon from '../components/Icon.svelte'
  import Tooltip from '../components/Tooltip.svelte'
  import { iconButtonClass, buttonClass, cn } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import AccountSwitcher from './AccountSwitcher.svelte'
  import GateRail from './GateRail.svelte'
  import { entryOf, groups, views, type View } from './nav'

  /**
   * The main window's sidebar: the window's buttons in the title bar's line,
   * the account, the views in the groups of PRD §4, and the gate rail at the
   * foot. Collapsed, it keeps the icons and names them in tooltips, and the
   * title bar covers its top with the window's buttons. A migration that was
   * started and not imported shows a notice until it is dismissed.
   */
  let {
    route,
    status,
    collapsed,
    inset,
    mod,
    legacyNotice,
    reminderAccounts,
    onDismissLegacy,
    go,
    enter,
    windowNav
  }: {
    route: string
    status: any
    collapsed: boolean
    inset: boolean
    mod: string
    reminderAccounts: NoticeStatus['accounts']
    legacyNotice: boolean
    onDismissLegacy: () => void
    go: (path: string) => void
    enter: (path: string) => void
    windowNav: Snippet
  } = $props()

  const entry = $derived(entryOf(route))
</script>

{#snippet item(view: View)}
  {@const active = entry === view}
  <Tooltip content={view.key ? `${t(view.label)} · ${mod}${view.key}` : t(view.label)} side="right">
    {#snippet children(props)}
      <button
        {...props}
        class={cn(
          'flex items-center gap-2.5 rounded-lg text-start text-[13.5px] leading-snug text-ink-2 outline-none transition-colors duration-150 hover:bg-wash focus-visible:ring-3 focus-visible:ring-focus/35',
          collapsed ? 'relative size-9 justify-center' : 'min-h-8 px-2.5 py-1.5',
          active && 'bg-pale font-semibold text-ink hover:bg-pale'
        )}
        aria-current={active ? 'page' : undefined}
        aria-label={collapsed ? t(view.label) : undefined}
        onclick={() => enter(view.path)}
      >
        <Icon name={view.icon} class={cn('size-4.5', active ? 'text-forest' : 'text-faint')} />
        {#if !collapsed}<span class="min-w-0 flex-1">{t(view.label)}</span>{/if}
        {#if view === views.channels && status?.channels_unread}
          <span
            class={cn(
              'min-w-4.5 rounded-full bg-forest px-1.5 text-center text-[10.5px] leading-[18px] font-semibold text-on-forest tabular-nums',
              collapsed && 'absolute -end-1 -top-1 min-w-4 px-1 text-[9.5px] leading-4'
            )}
            data-testid="channels-unread">{status.channels_unread > 99 ? '99+' : status.channels_unread}</span
          >
        {/if}
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
  data-testid="sidebar"
>
  <!-- The window's drag area in the title bar's line, centred like the title
       bar above its rule; on macOS the traffic lights sit at its start.
       Collapsed, the title bar covers it. -->
  <div class="flex h-[60px] shrink-0 items-center gap-2 ps-2 pb-px [-webkit-app-region:drag] lights:ltr:ps-[72px]">
    {#if !collapsed}
      {#if !inset}
        <Brand />
        <span class="me-auto text-lg leading-none font-extrabold tracking-[-0.04em]">dMsg</span>
      {/if}
      {@render windowNav()}
    {/if}
  </div>
  <AccountSwitcher {status} {collapsed} {reminderAccounts} />
  {#if legacyNotice && !collapsed}
    <div class="mt-2 grid gap-1.5 rounded-xl bg-warn-pale p-2.5 text-[12.5px] text-warn" data-testid="legacy-notice">
      <div class="flex items-start gap-2">
        <Icon name="legacy" class="mt-0.5" />
        <span class="min-w-0 flex-1">{t('legacyNotice')}</span>
        <button class={iconButtonClass('ghost', 'md', '-mt-0.5   text-warn')} aria-label={t('dismiss')} onclick={onDismissLegacy}>
          <Icon name="close" />
        </button>
      </div>
      <button class={buttonClass('secondary', 'xs', 'justify-self-start border-transparent')} onclick={() => go(views.legacy.path)}>{t('open')}</button>
    </div>
  {/if}
  <nav class={cn('-mx-2.5 mt-1 flex min-h-0 flex-col gap-0.5 self-stretch overflow-x-hidden overflow-y-auto px-2.5', collapsed && 'items-center')} aria-label={t('views')}>
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
  <div class={cn('mt-auto shrink-0 pt-3', collapsed ? 'w-full' : '')}>
    <GateRail {status} {collapsed} {mod} />
  </div>
</aside>
