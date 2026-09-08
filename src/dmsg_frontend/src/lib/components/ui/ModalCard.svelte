<script lang="ts">
  import IconClose from '$lib/components/icons/IconClose.svelte'
  import { getModalStore } from '$lib/ui/stores'
  import { type SvelteComponent } from 'svelte'

  // Props

  interface Props {
    /** Exposes parent props to this component. */
    parent: SvelteComponent
    width?: string
    cardClass?: string
    showTitle?: boolean
    children?: import('svelte').Snippet<[any]>
  }

  let {
    parent,
    width = 'w-full',
    cardClass = '',
    showTitle = true,
    children
  }: Props = $props()

  const modalStore = getModalStore()

  // ModalHost only renders the visible modal, so the head of the stack is
  // always this one.
  const title = $derived($modalStore[0]?.title)
  const children_render = $derived(children)
</script>

<div
  class="card relative lg:mt-12 {width} max-w-[420px] space-y-4 rounded-3xl bg-white p-6 shadow-xl md:mt-8 {cardClass}"
>
  <button
    class="btn btn-icon absolute top-2 right-2 z-1 text-neutral-500 *:scale-125 hover:scale-110 max-md:top-2 max-md:right-2"
    aria-label="Close dialog"
    onclick={parent['onClose']}
  >
    <IconClose />
  </button>
  {#if title && showTitle}
    <header class="!mt-0 text-center text-xl font-bold">
      {title}
    </header>
  {/if}
  {@render children_render?.({ parent })}
</div>
