<script lang="ts">
  import IconClose from '$lib/components/icons/IconClose.svelte'
  import { getModalStore } from '@skeletonlabs/skeleton'
  import { type SvelteComponent } from 'svelte'

  // Props

  interface Props {
    /** Exposes parent props to this component. */
    parent: SvelteComponent
    imageUrl: string
    imageName: string
  }

  let { parent, imageUrl, imageName }: Props = $props()

  const modalStore = getModalStore()
</script>

{#if $modalStore[0]}
  <!-- This is a hack to fix the focus issue -->
  <button class="hidden">x</button>
  <div class="card bg-surface-50-900-token relative w-dvw">
    <button
      class="z-1 btn btn-icon absolute right-1 top-1 text-neutral-500 *:scale-125 hover:scale-110 max-md:right-2 max-md:top-2"
      onclick={parent['onClose']}
    >
      <IconClose />
    </button>
    <img class="m-auto p-4" src={imageUrl} alt={imageName} />
  </div>
{/if}
