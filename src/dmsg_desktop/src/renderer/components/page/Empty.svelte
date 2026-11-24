<script lang="ts">
  import type { Snippet } from 'svelte'
  import type { Glyph } from '../glyphs'
  import Icon from '../Icon.svelte'
  import { cn } from '../ui'

  /** An empty place: its mark in Forest, what would be here in one sentence, and the next steps side by side. */
  let {
    icon,
    title,
    description,
    testid,
    class: className,
    actions
  }: { icon: Glyph; title: string; description?: string; testid?: string; class?: string; actions?: Snippet } = $props()
</script>

<div class={cn('grid justify-items-center gap-1.5 px-6 py-12 text-center', className)} data-testid={testid}>
  <Icon name={icon} class="mb-1.5 size-10 text-forest" />
  <p class="text-title-3 font-semibold">{title}</p>
  {#if description}<p class="max-w-xs text-body text-muted">{description}</p>{/if}
  {#if actions}<div class="mt-3 flex flex-wrap justify-center gap-2">{@render actions()}</div>{/if}
</div>
