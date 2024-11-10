<script lang="ts">
  import IconClose from '$lib/components/icons/IconClose.svelte'
  import { getModalStore } from '@skeletonlabs/skeleton'
  import { type SvelteComponent } from 'svelte'

  // Props

  interface Props {
    /** Exposes parent props to this component. */
    parent: SvelteComponent
    width?: string
    children?: import('svelte').Snippet<[any]>
  }

  let { parent, width = 'w-full', children }: Props = $props()

  const modalStore = getModalStore()

  const children_render = $derived(children)
</script>

{#if $modalStore[0]}
  <!-- This is a hack to fix the focus issue -->
  <button class="hidden">x</button>
  <div
    class="card relative lg:mt-12 {width} max-w-[420px] space-y-4 rounded-3xl bg-white p-6 shadow-xl md:mt-8"
  >
    <button
      class="z-1 btn btn-icon absolute right-2 top-2 text-neutral-500 *:scale-125 hover:scale-110 max-md:right-2 max-md:top-2"
      onclick={parent['onClose']}
    >
      <IconClose />
    </button>
    {#if $modalStore[0].title}
      <header class="!mt-0 text-center text-xl font-bold">
        {$modalStore[0].title}
      </header>
    {/if}
    {@render children_render?.({ parent })}
  </div>
{/if}
