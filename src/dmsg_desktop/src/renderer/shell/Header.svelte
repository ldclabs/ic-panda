<script lang="ts">
  import type { Snippet } from 'svelte'
  import Icon from '../components/Icon.svelte'
  import { cn } from '../components/ui'
  import { t } from '../i18n/index.svelte'
  import { views, type View } from './nav'

  /**
   * The view's title bar: its name, under Settings for Settings' own pages, a
   * line on what it holds, and the command palette. With the sidebar
   * collapsed it reaches over the rail's top and starts with the window's
   * buttons, where the sidebar showed them. On macOS it leaves the traffic
   * lights their room at the window's top left, its end in right-to-left text.
   */
  let {
    view,
    mod,
    collapsed,
    windowNav,
    go,
    onPalette
  }: { view: View; mod: string; collapsed: boolean; windowNav: Snippet; go: (path: string) => void; onPalette: () => void } = $props()

  const parent = $derived(view === views.legacy ? views.settings : null)
</script>

<header
  class={cn(
    'flex h-[60px] shrink-0 items-center gap-3 border-b border-line bg-paper px-5 [-webkit-app-region:drag] lights:rtl:pl-[84px]',
    collapsed && '-ms-16 ps-4.5 lights:ltr:ps-[82px]'
  )}
>
  {#if collapsed}{@render windowNav()}{/if}
  <Icon name={view.icon} class="size-5 text-forest" />
  <div class="min-w-0 flex-1">
    <div class="flex min-w-0 items-center gap-1.5 text-[14.5px] leading-tight font-semibold">
      {#if parent}
        <button class="shrink-0 font-medium text-faint hover:text-ink [-webkit-app-region:no-drag]" onclick={() => go(parent.path)}>{t(parent.label)}</button>
        <span class="text-faint" aria-hidden="true">/</span>
      {/if}
      <h1 class="truncate" data-testid="view-title">{t(view.label)}</h1>
    </div>
    <p class="mt-0.5 truncate text-xs text-faint">{t(view.detail)}</p>
  </div>
  <button
    class="flex h-8 w-64 max-w-[40%] shrink items-center gap-2 rounded-lg bg-wash ps-2.5 pe-1.5 text-[13px] text-muted outline-none transition-colors duration-150 hover:bg-pale focus-visible:ring-3 focus-visible:ring-focus/35 [-webkit-app-region:no-drag]"
    onclick={onPalette}
    data-testid="palette-open"
  >
    <Icon name="search" class="size-4.5" />
    <span class="min-w-0 flex-1 truncate text-start">{t('searchOrCommand')}</span>
    <kbd class="rounded border border-line bg-surface px-1.5 font-mono text-[10.5px] leading-[18px] text-faint">{mod}K</kbd>
  </button>
</header>
