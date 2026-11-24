<script lang="ts">
  import type { Glyph } from '../glyphs'
  import Icon from '../Icon.svelte'
  import { cn } from '../ui'

  /** A view's parts in a bar under its title; the chosen one is underlined. `onPick` lets a route own the choice. */
  let {
    value = $bindable(''),
    items,
    label,
    testid = 'tab',
    onPick
  }: { value?: string | null; items: { value: string; label: string; icon?: Glyph; count?: number }[]; label: string; testid?: string; onPick?: (value: string) => void } = $props()
</script>

<nav class="flex shrink-0 flex-wrap gap-x-1 border-b border-line px-6" aria-label={label}>
  {#each items as item (item.value)}
    <button
      class={cn(
        'relative flex h-10 shrink-0 items-center gap-1.5 px-2.5 text-body outline-none transition-colors duration-150 focus-visible:text-ink focus-visible:underline',
        value === item.value ? 'font-medium text-ink' : 'text-muted hover:text-ink'
      )}
      aria-current={value === item.value ? 'page' : undefined}
      onclick={() => (onPick ? onPick(item.value) : (value = item.value))}
      data-testid={`${testid}-${item.value}`}
    >
      {#if item.icon}<Icon name={item.icon} class={value === item.value ? 'text-forest' : 'text-faint'} />{/if}
      {item.label}
      {#if item.count}<span class="rounded-full bg-wash px-1.5 text-caption leading-4 text-ink-2 tabular-nums">{item.count}</span>{/if}
      {#if value === item.value}<span class="absolute inset-x-2 bottom-0 h-0.5 rounded-full bg-forest"></span>{/if}
    </button>
  {/each}
</nav>
