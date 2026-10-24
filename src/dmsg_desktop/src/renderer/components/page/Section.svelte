<script lang="ts">
  import type { Snippet } from 'svelte'
  import type { Glyph } from '../glyphs'
  import Icon from '../Icon.svelte'
  import { cn } from '../ui'

  /**
   * A titled group on a page: what it holds in a line, its actions on the
   * right, and its content. `flush` lets a list run to the edges.
   */
  let {
    title,
    description,
    icon,
    flush = false,
    testid,
    class: className,
    actions,
    children
  }: {
    title: string
    description?: string
    icon?: Glyph
    flush?: boolean
    testid?: string
    class?: string
    actions?: Snippet
    children: Snippet
  } = $props()
</script>

<section class={cn('min-w-0', className)} data-testid={testid}>
  <header class="flex flex-wrap items-start gap-x-2.5 gap-y-2 px-5 pb-3">
    {#if icon}
      <span class="grid h-8 w-5 shrink-0 place-items-center text-forest [&_svg]:size-5"><Icon name={icon} /></span>
    {/if}
    <div class="min-w-0 flex-1">
      <h2 class="text-[14px] leading-8 font-semibold">{title}</h2>
      {#if description}<p class="-mt-1 text-[12.5px] leading-normal text-muted">{description}</p>{/if}
    </div>
    {#if actions}<div class="flex shrink-0 flex-wrap items-center justify-end gap-1.5 pt-0.5">{@render actions()}</div>{/if}
  </header>
  <div class={cn(!flush && 'px-5')}>
    {@render children()}
  </div>
</section>
