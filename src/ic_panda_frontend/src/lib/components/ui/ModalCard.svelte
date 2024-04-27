<script lang="ts">
  import IconClose from '$lib/components/icons/IconClose.svelte'
  import { getModalStore } from '@skeletonlabs/skeleton'
  import { type SvelteComponent } from 'svelte'

  // Props
  /** Exposes parent props to this component. */
  export let parent: SvelteComponent
  export let width: string = 'w-full'

  const modalStore = getModalStore()
</script>

{#if $modalStore[0]}
  <!-- This is a hack to fix the focus issue -->
  <button class="hidden"></button>
  <div
    class="card relative mt-12 {width} max-w-[420px] space-y-4 rounded-3xl bg-white p-6 shadow-xl max-md:mt-8"
  >
    <button
      class="z-1 btn btn-icon absolute right-2 top-2 text-gray/30 *:scale-125 hover:scale-110 max-md:right-2 max-md:top-2"
      on:click={parent['onClose']}
    >
      <IconClose />
    </button>
    {#if $modalStore[0].title}
      <header class="!mt-0 text-center text-xl font-bold">
        {$modalStore[0].title}
      </header>
    {/if}
    <slot {parent} />
  </div>
{/if}
