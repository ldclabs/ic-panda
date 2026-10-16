<script lang="ts">
  import Icon from '../components/Icon.svelte'
  import { t } from '../i18n/index.svelte'
  import { views, type View } from './nav'

  /** The view's title bar: its name, under Settings for Settings' own pages, a line on what it holds, and the command palette. */
  let { view, mod, go, onPalette }: { view: View; mod: string; go: (path: string) => void; onPalette: () => void } = $props()

  const parent = $derived(view === views.legacy ? views.settings : null)
</script>

<header class="flex h-[60px] shrink-0 items-center gap-3 border-b border-line bg-paper px-5 [-webkit-app-region:drag]">
  <span class="grid size-8 shrink-0 place-items-center rounded-lg border border-line bg-surface text-forest"><Icon name={view.icon} /></span>
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
    class="flex h-8 w-64 max-w-[40%] shrink items-center gap-2 rounded-lg border border-line bg-surface-2 ps-2.5 pe-1.5 text-[13px] text-faint outline-none transition-colors duration-150 hover:border-edge-soft hover:bg-surface focus-visible:ring-3 focus-visible:ring-focus/35 [-webkit-app-region:no-drag]"
    onclick={onPalette}
    data-testid="palette-open"
  >
    <Icon name="search" />
    <span class="min-w-0 flex-1 truncate text-start">{t('searchOrCommand')}</span>
    <kbd class="rounded border border-line bg-surface px-1.5 font-mono text-[10.5px] leading-[18px] text-faint">{mod}K</kbd>
  </button>
</header>
