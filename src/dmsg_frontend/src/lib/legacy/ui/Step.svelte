<script lang="ts">
  import type { Snippet } from 'svelte'

  /** One numbered step of a migration panel. */
  let {
    n,
    title,
    done = false,
    children
  }: { n: number; title: string; done?: boolean; children: Snippet } = $props()
</script>

<li class="grid grid-cols-[36px_minmax(0,1fr)] gap-x-4">
  <span
    class={[
      'grid size-9 place-items-center rounded-full border font-mono text-[13px] font-semibold',
      done
        ? 'border-[var(--action)] bg-[var(--action)] text-white'
        : 'border-[var(--line)] bg-[var(--canvas)] text-[var(--action)]'
    ]}
    aria-hidden="true"
    >{#if done}<svg
        viewBox="0 0 24 24"
        class="size-4"
        fill="none"
        stroke="currentColor"
        stroke-width="3"><path d="M5 12.5l4.5 4.5L19 7.5" /></svg
      >{:else}{n}{/if}</span
  >
  <div class="grid min-w-0 content-start gap-3 pt-1.5">
    <h3 class="text-[19px]!">{title}</h3>
    {@render children()}
  </div>
</li>
