<script lang="ts">
  import type { Glyph } from '../glyphs'
  import Icon from '../Icon.svelte'
  import { cn } from '../ui'

  /** A view's parts in a bar under its title; the chosen one is underlined. */
  let {
    value = $bindable(''),
    items,
    label,
    testid = 'tab'
  }: { value?: string; items: { value: string; label: string; icon?: Glyph; count?: number }[]; label: string; testid?: string } = $props()
</script>

<nav class="flex shrink-0 gap-1 overflow-x-auto border-b border-line px-6 [scrollbar-width:none]" aria-label={label}>
  {#each items as item (item.value)}
    <button
      class={cn(
        'relative flex h-10 shrink-0 items-center gap-1.5 px-2.5 text-[13px] outline-none transition-colors duration-150 focus-visible:text-ink focus-visible:underline',
        value === item.value ? 'font-medium text-ink' : 'text-muted hover:text-ink'
      )}
      aria-current={value === item.value ? 'page' : undefined}
      onclick={() => (value = item.value)}
      data-testid={`${testid}-${item.value}`}
    >
      {#if item.icon}<Icon name={item.icon} class={value === item.value ? 'text-forest' : 'text-faint'} />{/if}
      {item.label}
      {#if item.count}<span class="rounded-full bg-wash px-1.5 text-[10.5px] leading-4 text-ink-2 tabular-nums">{item.count}</span>{/if}
      {#if value === item.value}<span class="absolute inset-x-2 -bottom-px h-0.5 rounded-full bg-forest"></span>{/if}
    </button>
  {/each}
</nav>
